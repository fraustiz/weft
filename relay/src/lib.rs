//! Weft relay, protocol v1 (`docs/protocol.md`, section 4).
//!
//! The relay authenticates each device with a signed challenge, then forwards
//! envelopes to their recipient when it is connected. It reads the clear
//! envelope header to route messages and never decrypts anything.

use std::collections::HashMap;
use std::io;
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
    AUTH_ACCEPTED, AuthResponse, Challenge, HEADER_LEN, Header, MAX_CLIPBOARD_TEXT_LEN, NONCE_LEN,
    TAG_LEN, verify_auth,
};

/// Smallest valid envelope: header, nonce and tag (empty plaintext).
pub const MIN_ENVELOPE_LEN: usize = HEADER_LEN + NONCE_LEN + TAG_LEN;
/// Largest valid envelope: a clipboard text of the maximum size.
pub const MAX_ENVELOPE_LEN: usize = HEADER_LEN + NONCE_LEN + MAX_CLIPBOARD_TEXT_LEN + TAG_LEN;
/// How long a device has to answer the challenge.
pub const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
/// Envelopes queued for one device before new ones are dropped.
const OUTBOX_CAPACITY: usize = 64;

type DeviceId = [u8; 16];

/// A live, authenticated connection. Dropping `outbox` closes it.
struct Peer {
    conn_id: u64,
    outbox: mpsc::Sender<Bytes>,
}

struct Relay {
    name: String,
    peers: Mutex<HashMap<DeviceId, Peer>>,
    next_conn_id: AtomicU64,
}

/// Serves the relay on `listener` until the process stops.
///
/// `relay_name` is the name devices sign in their authentication response;
/// it must be at most 255 bytes of UTF-8.
pub async fn serve(listener: TcpListener, relay_name: String) -> io::Result<()> {
    Challenge::new(relay_name.as_str(), [0; 32])
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
    let relay = Arc::new(Relay {
        name: relay_name,
        peers: Mutex::new(HashMap::new()),
        next_conn_id: AtomicU64::new(1),
    });
    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1", get(upgrade))
        .with_state(relay);
    axum::serve(listener, app).await
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
        .peers
        .lock()
        .unwrap()
        .insert(device_id, Peer { conn_id, outbox });
    tracing::info!(device = %short_id(&device_id), "device connected");
    // Accept only once registered, so the device is reachable as soon as it
    // sees 0xA2.
    let accepted = Bytes::from_static(&[AUTH_ACCEPTED]);
    if socket.send(Message::Binary(accepted)).await.is_ok() {
        relay_frames(&mut socket, &relay, &device_id, &mut inbox).await;
    }

    let mut peers = relay.peers.lock().unwrap();
    if peers
        .get(&device_id)
        .is_some_and(|peer| peer.conn_id == conn_id)
    {
        peers.remove(&device_id);
    }
    drop(peers);
    tracing::info!(device = %short_id(&device_id), "device disconnected");
}

/// Forwards the device's envelopes and delivers the ones addressed to it,
/// until either side closes or a newer connection replaces this one.
async fn relay_frames(
    socket: &mut WebSocket,
    relay: &Relay,
    device_id: &DeviceId,
    inbox: &mut mpsc::Receiver<Bytes>,
) {
    loop {
        tokio::select! {
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Binary(frame))) => relay.forward(device_id, frame),
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

impl Relay {
    /// Routes one envelope from an authenticated device. Invalid frames and
    /// frames for offline recipients are dropped.
    fn forward(&self, sender: &DeviceId, frame: Bytes) {
        let from = short_id(sender);
        if frame.len() < MIN_ENVELOPE_LEN {
            tracing::debug!(%from, len = frame.len(), "dropped frame: too short");
            return;
        }
        let header = match Header::decode(&frame[..HEADER_LEN]) {
            Ok(header) => header,
            Err(err) => {
                tracing::debug!(%from, %err, "dropped frame: invalid header");
                return;
            }
        };
        if header.sender != *sender {
            tracing::debug!(%from, "dropped frame: sender is not the authenticated device");
            return;
        }
        let to = short_id(&header.recipient);
        let outbox = match self.peers.lock().unwrap().get(&header.recipient) {
            Some(peer) => peer.outbox.clone(),
            None => {
                tracing::debug!(%from, %to, "dropped frame: recipient not connected");
                return;
            }
        };
        if outbox.try_send(frame).is_err() {
            tracing::debug!(%from, %to, "dropped frame: recipient queue full");
        }
    }
}

/// First 4 bytes of a device id in hex, for logs.
fn short_id(device_id: &DeviceId) -> String {
    device_id[..4].iter().map(|b| format!("{b:02x}")).collect()
}
