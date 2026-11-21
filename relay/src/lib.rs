//! Weft relay, protocol v1 (`docs/protocol.md`, sections 4 and 5).
//!
//! The relay authenticates each device with a signed challenge, then forwards
//! envelopes between devices of the same group when the recipient is
//! connected. It reads the clear envelope header to route messages and never
//! decrypts anything. Groups are built by pairing, through control frames.

mod dispatch;
mod groups;
mod limits;
mod pairing;
mod store;

use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Bytes;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
use weft_protocol::{
    AUTH_ACCEPTED, AuthResponse, Challenge, ControlFrame, HEADER_LEN, MAX_CLIPBOARD_TEXT_LEN,
    MAX_FILE_CHUNK_DATA_LEN, MAX_PACKET_LEN, NONCE_LEN, TAG_LEN, TRANSFER_ID_LEN, verify_auth,
};

use crate::dispatch::RateLimits;

use crate::groups::Groups;
use crate::limits::{Limiter, Refusal, client_ip};
use crate::pairing::Pairing;
use crate::store::Store;

/// Smallest valid envelope: header, nonce and tag (empty plaintext).
pub const MIN_ENVELOPE_LEN: usize = HEADER_LEN + NONCE_LEN + TAG_LEN;
/// Largest valid envelope: a clipboard text or a packet of the maximum size
/// (a full file chunk is smaller).
pub const MAX_ENVELOPE_LEN: usize = HEADER_LEN + NONCE_LEN + MAX_CLIPBOARD_TEXT_LEN + TAG_LEN;
const _: () = assert!(MAX_PACKET_LEN <= MAX_CLIPBOARD_TEXT_LEN);
const _: () = assert!(TRANSFER_ID_LEN + 8 + MAX_FILE_CHUNK_DATA_LEN <= MAX_CLIPBOARD_TEXT_LEN);
/// How long a device has to answer the challenge.
pub const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
/// Default for [`Config::idle_timeout`]. Clients ping at least every 30 s.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
/// Default for [`Config::max_connections`].
pub const MAX_CONNECTIONS: usize = 2000;
/// Default for [`Config::max_connections_per_ip`].
pub const MAX_CONNECTIONS_PER_IP: usize = 20;
/// How long open connections get to close once shutdown starts.
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);
/// How long a pairing slot stays open, and how long the slot owner then has
/// to confirm the device that asked to join.
pub const PAIRING_TTL: Duration = Duration::from_secs(5 * 60);
/// Pairing slots one connection may open per minute.
pub const PAIR_OPEN_PER_MINUTE: usize = 10;
/// Pair requests one connection may send per minute.
pub const PAIR_REQUEST_PER_MINUTE: usize = 20;
/// Default for [`Config::offline_ttl`].
pub const OFFLINE_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// Default for [`Config::max_offline_per_device`].
pub const MAX_OFFLINE_PER_DEVICE: usize = 64;
/// Default for [`Config::max_groups`]: the global ceiling on stored groups.
pub const MAX_GROUPS: usize = 100_000;
/// Default for [`Config::group_inactive_ttl`]: a group whose members are all
/// idle this long is pruned (`docs/protocol.md`, section 6).
pub const GROUP_INACTIVE_TTL: Duration = Duration::from_secs(90 * 24 * 60 * 60);
/// How often the relay sweeps for inactive groups to prune.
const GROUP_SWEEP_INTERVAL: Duration = Duration::from_secs(60 * 60);
const RATE_WINDOW: Duration = Duration::from_secs(60);
/// Frames queued for one device before new ones are dropped.
const OUTBOX_CAPACITY: usize = 64;

pub(crate) type DeviceId = [u8; 16];

/// What the relay needs to start.
#[derive(Debug, Clone)]
pub struct Config {
    /// The name devices sign in their authentication response; at most 255
    /// bytes of UTF-8.
    pub relay_name: String,
    /// SQLite database holding the groups and offline envelopes. `:memory:`
    /// keeps them in memory.
    pub db_path: PathBuf,
    /// How long a pairing slot stays open.
    pub pairing_slot_ttl: Duration,
    /// How long a slot owner has to confirm a device that asked to join.
    pub pending_join_ttl: Duration,
    /// Open WebSocket connections the relay accepts in total; beyond that,
    /// upgrades are refused with HTTP 503.
    pub max_connections: usize,
    /// Open WebSocket connections one client address may hold (one /64 for
    /// IPv6); beyond that, upgrades are refused with HTTP 429.
    pub max_connections_per_ip: usize,
    /// Take the client address from the last `X-Forwarded-For` entry, as
    /// appended by a reverse proxy. Only set it when the relay is reachable
    /// through that proxy alone.
    pub trust_proxy: bool,
    /// An authenticated connection silent for half this long is pinged, and
    /// closed if it stays silent for the whole of it.
    pub idle_timeout: Duration,
    /// How long queued group and device-name envelopes are kept for offline
    /// recipients.
    pub offline_ttl: Duration,
    /// Queued offline envelopes kept per recipient; beyond that, the oldest is
    /// evicted.
    pub max_offline_per_device: usize,
    /// Largest number of groups the relay keeps at once; a new group beyond
    /// this is refused, so an abusive client cannot grow the state without
    /// bound.
    pub max_groups: usize,
    /// A group whose members have all been idle this long is pruned, along
    /// with anything queued for them.
    pub group_inactive_ttl: Duration,
    /// Header carrying the real client address, set by a trusted edge proxy
    /// (e.g. `cf-connecting-ip`). Read before `X-Forwarded-For`.
    pub client_ip_header: Option<String>,
    /// Addresses whose forwarded headers are trusted. Empty trusts any peer,
    /// which is only safe when the relay is unreachable except through its
    /// proxy.
    pub trusted_proxies: Vec<IpAddr>,
    /// Optional operator contact shown on `GET /`.
    pub contact: Option<String>,
}

impl Config {
    /// A configuration with the durations of the protocol.
    pub fn new(relay_name: impl Into<String>, db_path: impl Into<PathBuf>) -> Config {
        Config {
            relay_name: relay_name.into(),
            db_path: db_path.into(),
            pairing_slot_ttl: PAIRING_TTL,
            pending_join_ttl: PAIRING_TTL,
            max_connections: MAX_CONNECTIONS,
            max_connections_per_ip: MAX_CONNECTIONS_PER_IP,
            trust_proxy: false,
            idle_timeout: IDLE_TIMEOUT,
            offline_ttl: OFFLINE_TTL,
            max_offline_per_device: MAX_OFFLINE_PER_DEVICE,
            max_groups: MAX_GROUPS,
            group_inactive_ttl: GROUP_INACTIVE_TTL,
            client_ip_header: None,
            trusted_proxies: Vec::new(),
            contact: None,
        }
    }
}

/// A live, authenticated connection. Dropping `outbox` closes it.
struct Peer {
    conn_id: u64,
    outbox: mpsc::Sender<Bytes>,
}

/// Everything connections share. One lock keeps routing, groups and pairing
/// consistent with each other; it is never held across an `await`.
pub(crate) struct Shared {
    peers: HashMap<DeviceId, Peer>,
    groups: Groups,
    pairing: Pairing,
}

struct Relay {
    name: String,
    index_html: Arc<str>,
    state: Mutex<Shared>,
    next_conn_id: AtomicU64,
    limiter: Arc<Limiter>,
    trust_proxy: bool,
    client_ip_header: Option<String>,
    trusted_proxies: Vec<IpAddr>,
    idle_timeout: Duration,
    offline_ttl: Duration,
    max_offline_per_device: usize,
    group_inactive_ttl: Duration,
    /// Set to `true` when the relay shuts down.
    stopping: watch::Sender<bool>,
}

/// Serves the relay on `listener` until the process stops.
pub async fn serve(listener: TcpListener, config: Config) -> io::Result<()> {
    serve_with_shutdown(listener, config, std::future::pending()).await
}

/// Serves the relay on `listener` until `shutdown` completes. It then stops
/// accepting connections, closes the open ones with a close frame and
/// returns once they are closed, or after [`SHUTDOWN_GRACE`].
pub async fn serve_with_shutdown(
    listener: TcpListener,
    config: Config,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    Challenge::new(config.relay_name.as_str(), [0; 32])
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
    let max_groups = config.max_groups;
    let groups = Store::open(&config.db_path)
        .and_then(|store| Groups::load(store, max_groups))
        .map_err(io::Error::other)?;
    let index_html = Arc::<str>::from(render_index_html(
        &config.relay_name,
        config.contact.as_deref(),
    ));
    let relay = Arc::new(Relay {
        name: config.relay_name,
        index_html,
        state: Mutex::new(Shared {
            peers: HashMap::new(),
            groups,
            pairing: Pairing::new(config.pairing_slot_ttl, config.pending_join_ttl),
        }),
        next_conn_id: AtomicU64::new(1),
        limiter: Arc::new(Limiter::new(
            config.max_connections,
            config.max_connections_per_ip,
        )),
        trust_proxy: config.trust_proxy,
        client_ip_header: config.client_ip_header,
        trusted_proxies: config.trusted_proxies,
        idle_timeout: config.idle_timeout,
        offline_ttl: config.offline_ttl,
        max_offline_per_device: config.max_offline_per_device,
        group_inactive_ttl: config.group_inactive_ttl,
        stopping: watch::channel(false).0,
    });
    let app = Router::new()
        .route("/", get(index))
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1", get(upgrade))
        .with_state(relay.clone());

    // Prune groups whose members have all been idle past the TTL, until the
    // relay shuts down.
    tokio::spawn({
        let relay = relay.clone();
        async move {
            let mut stopping = relay.stopping.subscribe();
            loop {
                tokio::select! {
                    () = tokio::time::sleep(GROUP_SWEEP_INTERVAL) => relay.sweep_inactive_groups(),
                    () = stopped(&mut stopping) => break,
                }
            }
        }
    });

    let signal = {
        let relay = relay.clone();
        async move {
            shutdown.await;
            tracing::info!("shutting down");
            // Every connection handler watches this and closes its socket.
            relay.stopping.send_replace(true);
        }
    };
    // Upgraded connections are not tracked by axum: wait for their slots.
    let server = async {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(signal)
        .await?;
        relay.limiter.drained().await;
        io::Result::Ok(())
    };
    let grace_elapsed = async {
        stopped(&mut relay.stopping.subscribe()).await;
        tokio::time::sleep(SHUTDOWN_GRACE).await;
    };
    tokio::select! {
        result = server => result,
        () = grace_elapsed => {
            tracing::warn!("connections still open after the grace period, stopping anyway");
            Ok(())
        }
    }
}

impl Relay {
    /// Prunes every group whose members have all been idle past the TTL and
    /// are all currently disconnected.
    fn sweep_inactive_groups(&self) {
        let now = now_ms();
        let ttl = self.group_inactive_ttl;
        let mut state = self.state.lock().unwrap();
        let stale = state
            .groups
            .inactive_unconnected_ids(now, ttl, |device| state.peers.contains_key(device));
        if stale.is_empty() {
            return;
        }
        let pruned = stale.len();
        for group in stale {
            if let Err(err) = state.groups.remove_group(group, now) {
                tracing::error!(%err, group, "could not prune inactive group");
            }
        }
        tracing::info!(pruned, "pruned inactive groups");
    }
}

async fn index(State(relay): State<Arc<Relay>>) -> Html<String> {
    Html(relay.index_html.to_string())
}

async fn upgrade(
    ws: WebSocketUpgrade,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(relay): State<Arc<Relay>>,
) -> Response {
    let ip = client_ip(
        peer,
        &headers,
        relay.trust_proxy,
        relay.client_ip_header.as_deref(),
        &relay.trusted_proxies,
    );
    let slot = match relay.limiter.acquire(ip) {
        Ok(slot) => slot,
        Err(Refusal::Full) => {
            tracing::debug!(%ip, "connection refused: relay full");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        Err(Refusal::TooManyFromAddress) => {
            tracing::debug!(%ip, "connection refused: too many from this address");
            return StatusCode::TOO_MANY_REQUESTS.into_response();
        }
    };
    ws.max_message_size(MAX_ENVELOPE_LEN)
        .max_frame_size(MAX_ENVELOPE_LEN)
        // The slot is released when the connection ends, or when the
        // upgrade fails and the callback is dropped.
        .on_upgrade(move |socket| async move {
            handle(socket, relay).await;
            drop(slot);
        })
}

async fn handle(mut socket: WebSocket, relay: Arc<Relay>) {
    let mut stopping = relay.stopping.subscribe();
    let authenticated = tokio::select! {
        device_id = authenticate(&mut socket, &relay.name) => Some(device_id),
        () = stopped(&mut stopping) => None,
    };
    let device_id = match authenticated {
        Some(Some(device_id)) => device_id,
        Some(None) => {
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
        None => {
            let _ = socket.send(going_away()).await;
            return;
        }
    };

    let conn_id = relay.next_conn_id.fetch_add(1, Ordering::Relaxed);
    let (outbox, mut inbox) = mpsc::channel(OUTBOX_CAPACITY);
    // Replacing the entry drops the older connection's sender, which closes it.
    let queued = {
        let mut state = relay.state.lock().unwrap();
        state.peers.insert(device_id, Peer { conn_id, outbox });
        // Keep the device's group from being pruned while it is in use.
        state.groups.touch(&device_id, now_ms());
        match state
            .groups
            .load_offline(&device_id, now_ms(), relay.offline_ttl)
        {
            Ok(queued) => queued,
            Err(err) => {
                tracing::error!(%err, "could not load offline envelopes");
                Vec::new()
            }
        }
    };
    tracing::debug!(
        device = %short_id(&device_id),
        queued = queued.len(),
        "device connected"
    );
    // Accept only once registered, so the device is reachable as soon as it
    // sees 0xA2.
    let accepted = Bytes::from_static(&[AUTH_ACCEPTED]);
    if socket.send(Message::Binary(accepted)).await.is_ok()
        && drain_offline(&mut socket, &relay, &mut inbox, &mut stopping, queued).await
    {
        relay_frames(&mut socket, &relay, &device_id, &mut inbox, &mut stopping).await;
    }

    let mut state = relay.state.lock().unwrap();
    if state
        .peers
        .get(&device_id)
        .is_some_and(|peer| peer.conn_id == conn_id)
    {
        state.peers.remove(&device_id);
    }
    drop(state);
    tracing::debug!(device = %short_id(&device_id), "device disconnected");
}

/// Sends non-expired offline envelopes right after `0xA2` and before any live
/// frame from `inbox`, deleting each row once sent.
async fn drain_offline(
    socket: &mut WebSocket,
    relay: &Relay,
    inbox: &mut mpsc::Receiver<Bytes>,
    stopping: &mut watch::Receiver<bool>,
    queued: Vec<(i64, Bytes)>,
) -> bool {
    for (row_id, frame) in queued {
        if inbox.is_closed() {
            let _ = socket.send(Message::Close(None)).await;
            return false;
        }
        let sent = tokio::select! {
            result = socket.send(Message::Binary(frame)) => result.is_ok(),
            () = stopped(stopping) => {
                let _ = socket.send(going_away()).await;
                return false;
            }
        };
        if !sent {
            return false;
        }
        if let Err(err) = relay.state.lock().unwrap().groups.delete_offline(row_id) {
            tracing::error!(%err, "could not delete delivered offline envelope");
        }
    }
    true
}

/// Handles the device's frames and delivers the ones addressed to it, until
/// either side closes, a newer connection replaces this one, the device
/// stays silent too long or the relay shuts down.
async fn relay_frames(
    socket: &mut WebSocket,
    relay: &Relay,
    device_id: &DeviceId,
    inbox: &mut mpsc::Receiver<Bytes>,
    stopping: &mut watch::Receiver<bool>,
) {
    let mut limits = RateLimits::new();
    let mut last_heard = Instant::now();
    let mut pinged = false;
    loop {
        // Ping a silent device halfway through the idle timeout: its pong
        // keeps a live connection open even if it sends nothing else.
        let deadline = if pinged {
            last_heard + relay.idle_timeout
        } else {
            last_heard + relay.idle_timeout / 2
        };
        tokio::select! {
            incoming = socket.recv() => {
                last_heard = Instant::now();
                pinged = false;
                match incoming {
                Some(Ok(Message::Binary(frame))) => {
                    if let Some(error) = relay.dispatch(device_id, frame, &mut limits) {
                        let reply = ControlFrame::Error(error).encode();
                        if socket.send(Message::Binary(reply.into())).await.is_err() {
                            break;
                        }
                    }
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                Some(Ok(Message::Text(_))) => {
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                }
            },
            outgoing = inbox.recv() => match outgoing {
                Some(frame) => {
                    if socket.send(Message::Binary(frame)).await.is_err() {
                        break;
                    }
                }
                None => {
                    tracing::debug!(device = %short_id(device_id), "replaced by a newer connection");
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
            },
            () = tokio::time::sleep_until(deadline) => {
                if pinged {
                    tracing::debug!(device = %short_id(device_id), "idle timeout");
                    let _ = socket.send(close(close_code::NORMAL, "idle timeout")).await;
                    break;
                }
                pinged = true;
                if socket.send(Message::Ping(Bytes::new())).await.is_err() {
                    break;
                }
            },
            () = stopped(stopping) => {
                let _ = socket.send(going_away()).await;
                break;
            },
        }
    }
}

/// Completes once the relay is shutting down.
async fn stopped(stopping: &mut watch::Receiver<bool>) {
    // The sender lives as long as the relay, so this only fails once the
    // relay is gone, which is a shutdown too.
    let _ = stopping.wait_for(|stopping| *stopping).await;
}

fn close(code: u16, reason: &'static str) -> Message {
    Message::Close(Some(CloseFrame {
        code,
        reason: reason.into(),
    }))
}

/// The close frame sent to every connection when the relay shuts down.
fn going_away() -> Message {
    close(close_code::AWAY, "relay shutting down")
}

/// Sends the challenge and waits for a valid response. Returns the
/// authenticated device id, or `None` if the connection must be closed.
/// The caller sends the `0xA2` acceptance.
async fn authenticate(socket: &mut WebSocket, relay_name: &str) -> Option<DeviceId> {
    let mut nonce = [0u8; 32];
    getrandom::fill(&mut nonce).ok()?;
    let challenge = Challenge::new(relay_name, nonce).ok()?;
    socket
        .send(Message::Binary(challenge.encode().into()))
        .await
        .ok()?;

    let frame = tokio::time::timeout(AUTH_TIMEOUT, async {
        loop {
            match socket.recv().await? {
                Ok(Message::Ping(_) | Message::Pong(_)) => continue,
                Ok(Message::Binary(frame)) => return Some(frame),
                _ => return None,
            }
        }
    })
    .await
    .ok()??;

    let response = AuthResponse::decode(&frame).ok()?;
    let device_id = match verify_auth(&challenge, &response) {
        Ok(device_id) => device_id,
        Err(err) => {
            tracing::debug!(%err, "authentication refused");
            return None;
        }
    };
    Some(device_id)
}

/// First 4 bytes of a device id in hex, for logs.
pub(crate) fn short_id(device_id: &DeviceId) -> String {
    device_id[..4].iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

pub(crate) fn duration_ms(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn render_index_html(relay_name: &str, contact: Option<&str>) -> String {
    let name = escape_html(relay_name);
    let contact_block = match contact.map(str::trim).filter(|s| !s.is_empty()) {
        Some(contact) => format!(
            "\n        <dt>Operator contact</dt>\n        <dd>{}</dd>",
            escape_html(contact)
        ),
        None => String::new(),
    };
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Weft relay — {name}</title>
  <style>
    :root {{
      color-scheme: light dark;
      --bg: #F6EFE6;
      --fg: #17130F;
      --muted: #574E46;
      --line: rgba(23, 19, 15, 0.12);
      --code-bg: rgba(23, 19, 15, 0.06);
      --accent: #F26A21;
      --link: #B54308;
      --sans: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
      --mono: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    }}
    @media (prefers-color-scheme: dark) {{
      :root {{
        --bg: #17130F;
        --fg: #F6EFE6;
        --muted: #B9ADA0;
        --line: rgba(246, 239, 230, 0.14);
        --code-bg: rgba(246, 239, 230, 0.08);
        --link: #F58442;
      }}
    }}
    *, *::before, *::after {{ box-sizing: border-box; }}
    html, body {{
      margin: 0;
      padding: 0;
      overflow-x: clip;
      background: var(--bg);
      color: var(--fg);
      font-family: var(--sans);
      font-size: 16px;
      line-height: 1.6;
    }}
    main {{
      max-width: 640px;
      margin: 0 auto;
      padding: 48px 24px 64px;
    }}
    header {{
      padding-bottom: 24px;
      border-bottom: 2px solid var(--accent);
    }}
    h1 {{
      margin: 0 0 8px;
      font-size: 28px;
      font-weight: 600;
      line-height: 1.25;
      overflow-wrap: break-word;
    }}
    .lead {{
      margin: 0;
      color: var(--muted);
    }}
    h2 {{
      margin: 32px 0 12px;
      font-size: 18px;
      font-weight: 600;
      line-height: 1.4;
    }}
    p {{
      margin: 0 0 12px;
    }}
    ul {{
      margin: 0;
      padding-left: 20px;
    }}
    li {{
      margin-bottom: 8px;
    }}
    dl {{
      margin: 16px 0 0;
      display: grid;
      grid-template-columns: minmax(140px, auto) minmax(0, 1fr);
      gap: 8px 16px;
      padding-top: 16px;
      border-top: 1px solid var(--line);
    }}
    dt {{
      color: var(--muted);
      font-weight: 500;
    }}
    dd {{
      margin: 0;
      overflow-wrap: break-word;
    }}
    code {{
      font-family: var(--mono);
      font-size: 14px;
      padding: 2px 6px;
      border-radius: 4px;
      background: var(--code-bg);
    }}
    a {{
      color: var(--link);
      text-underline-offset: 4px;
    }}
    a:focus-visible {{
      outline: 2px solid var(--accent);
      outline-offset: 2px;
      border-radius: 4px;
    }}
    @media (max-width: 480px) {{
      main {{ padding: 32px 20px 48px; }}
      dl {{ grid-template-columns: minmax(0, 1fr); gap: 4px; }}
      dd {{ margin-bottom: 8px; }}
    }}
  </style>
</head>
<body>
  <main>
    <header>
      <h1>Weft relay</h1>
      <p class="lead">End-to-end encrypted transport for paired devices on <code>{name}</code>.</p>
      <dl>
        <dt>Relay name</dt>
        <dd><code>{name}</code></dd>
        <dt>WebSocket endpoint</dt>
        <dd><code>/v1</code></dd>{contact_block}
      </dl>
    </header>

    <section>
      <h2>End-to-end encryption</h2>
      <p>Clipboard text, files, notifications, device names, and control payloads are encrypted on each device with libsodium (XChaCha20-Poly1305, X25519 session keys, Ed25519 identities) before reaching the relay. This server routes ciphertext headers and cannot decrypt or read any payload.</p>
    </section>

    <section>
      <h2>Data and retention</h2>
      <ul>
        <li><strong>Group membership:</strong> device identifiers and their routing group are stored in SQLite so only paired devices can exchange frames.</li>
        <li><strong>Offline queue:</strong> encrypted group-change and device-name envelopes for offline group members are held for up to 30 days (capped per device) and deleted as soon as delivered.</li>
        <li><strong>State deletion:</strong> removing a device or leaving a group immediately deletes its group record and any envelopes queued for it on the relay.</li>
        <li><strong>Operational logs:</strong> transient connection and error diagnostics without message content, with zero analytics or tracking.</li>
      </ul>
    </section>

    <section>
      <h2>Source, self-hosting, and notices</h2>
      <p>Weft is open source and self-hostable. Protocol documentation, server source code, and contact details for abuse or Digital Services Act notices are available at <a href="https://github.com/fraustiz/weft">github.com/fraustiz/weft</a>.</p>
    </section>
  </main>
</body>
</html>
"#
    )
}
