//! Weft relay, protocol v1 (`docs/protocol.md`, sections 4 and 5).
//!
//! The relay authenticates each device with a signed challenge, then forwards
//! envelopes between devices of the same group when the recipient is
//! connected. It reads the clear envelope header to route messages and never
//! decrypts anything. Groups are built by pairing, through control frames.

mod dispatch;
mod groups;
mod pairing;
mod store;

use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use weft_protocol::{
    AUTH_ACCEPTED, AuthResponse, Challenge, ControlFrame, HEADER_LEN, MAX_CLIPBOARD_TEXT_LEN,
    NONCE_LEN, TAG_LEN, verify_auth,
};

use crate::dispatch::RateLimits;

use crate::groups::Groups;
use crate::pairing::Pairing;
use crate::store::Store;

/// Smallest valid envelope: header, nonce and tag (empty plaintext).
pub const MIN_ENVELOPE_LEN: usize = HEADER_LEN + NONCE_LEN + TAG_LEN;
/// Largest valid envelope: a clipboard text of the maximum size.
pub const MAX_ENVELOPE_LEN: usize = HEADER_LEN + NONCE_LEN + MAX_CLIPBOARD_TEXT_LEN + TAG_LEN;
/// How long a device has to answer the challenge.
pub const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
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
}

impl Config {
    /// A configuration with the durations of the protocol.
    pub fn new(relay_name: impl Into<String>, db_path: impl Into<PathBuf>) -> Config {
        Config {
            relay_name: relay_name.into(),
            db_path: db_path.into(),
            pairing_slot_ttl: PAIRING_TTL,
            pending_join_ttl: PAIRING_TTL,
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
}

/// Serves the relay on `listener` until the process stops.
pub async fn serve(listener: TcpListener, config: Config) -> io::Result<()> {
    serve_with_shutdown(listener, config, std::future::pending()).await
}

/// Serves the relay on `listener` until `shutdown` completes, then closes
/// the connections of the authenticated devices and returns.
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
    });
    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1", get(upgrade))
        .with_state(relay.clone());
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown.await;
            // Dropping the senders closes every authenticated connection.
            relay.state.lock().unwrap().peers.clear();
        })
        .await
}

async fn upgrade(ws: WebSocketUpgrade, State(relay): State<Arc<Relay>>) -> Response {
    ws.max_message_size(MAX_ENVELOPE_LEN)
        .max_frame_size(MAX_ENVELOPE_LEN)
        .on_upgrade(move |socket| handle(socket, relay))
}

async fn handle(mut socket: WebSocket, relay: Arc<Relay>) {
    let Some(device_id) = authenticate(&mut socket, &relay.name).await else {
        let _ = socket.send(Message::Close(None)).await;
        return;
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
        relay_frames(&mut socket, &relay, &device_id, &mut inbox).await;
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
/// either side closes or a newer connection replaces this one.
async fn relay_frames(
    socket: &mut WebSocket,
    relay: &Relay,
    device_id: &DeviceId,
    inbox: &mut mpsc::Receiver<Bytes>,
) {
    let mut limits = RateLimits::new();
    loop {
        tokio::select! {
            incoming = socket.recv() => match incoming {
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
        }
    }
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
