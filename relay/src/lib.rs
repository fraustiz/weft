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
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
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
    /// SQLite database holding the groups. `:memory:` keeps them in memory.
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
struct Shared {
    peers: HashMap<DeviceId, Peer>,
    groups: Groups,
    pairing: Pairing,
}

struct Relay {
    name: String,
    state: Mutex<Shared>,
    next_conn_id: AtomicU64,
    limiter: Arc<Limiter>,
    trust_proxy: bool,
    idle_timeout: Duration,
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
    let groups = Store::open(&config.db_path)
        .and_then(Groups::load)
        .map_err(io::Error::other)?;
    let relay = Arc::new(Relay {
        name: config.relay_name,
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
        idle_timeout: config.idle_timeout,
        stopping: watch::channel(false).0,
    });
    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1", get(upgrade))
        .with_state(relay.clone());

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

async fn upgrade(
    ws: WebSocketUpgrade,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(relay): State<Arc<Relay>>,
) -> Response {
    let ip = client_ip(peer, &headers, relay.trust_proxy);
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
    relay
        .state
        .lock()
        .unwrap()
        .peers
        .insert(device_id, Peer { conn_id, outbox });
    tracing::info!(device = %short_id(&device_id), "device connected");
    // Accept only once registered, so the device is reachable as soon as it
    // sees 0xA2.
    let accepted = Bytes::from_static(&[AUTH_ACCEPTED]);
    if socket.send(Message::Binary(accepted)).await.is_ok() {
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
    tracing::info!(device = %short_id(&device_id), "device disconnected");
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
                    tracing::info!(device = %short_id(device_id), "replaced by a newer connection");
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
            },
            () = tokio::time::sleep_until(deadline) => {
                if pinged {
                    tracing::info!(device = %short_id(device_id), "idle timeout");
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
