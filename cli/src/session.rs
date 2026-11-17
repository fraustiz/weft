//! A connection to the relay, authenticated as this device
//! (`docs/protocol.md`, sections 3 to 5).
//!
//! A [`Session`] owns the WebSocket and reads the state from its
//! [`StateDir`]: every change (trust, counters, replay state) goes through
//! [`StateDir::update`], so it is on disk before anything depends on it.

use std::collections::{BTreeMap, VecDeque};
use std::convert::Infallible;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::Uri;
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};
use weft_protocol::{
    AUTH_ACCEPTED, Challenge, ControlError, ControlFrame, FrameType, HEADER_LEN, Header, Identity,
    Kind, MAX_CLIPBOARD_TEXT_LEN, MAX_GROUP_SIZE, MAX_PACKET_LEN, MAX_RELAY_URL_LEN, PairRequest,
    PairingCode, PeerKeys, SessionKeys, VERSION, check_relay_name, classify, decode_device_added,
    decode_device_name, decode_device_removed, decode_file_chunk, decode_welcome,
    device_id_from_sign_pk, encode_device_added, encode_device_name, encode_device_removed,
    encode_file_chunk, encode_welcome, open, random_message_id, random_pairing_id,
    random_pairing_secret, seal, session_keys, sign_auth, verify_invite_request,
    verify_pair_request, zero_key,
};

use crate::state::{ReplayError, State, StateDir, TrustedDevice};
use crate::{Error, Result, short_id};

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// How long the TCP, TLS and WebSocket handshakes, then the relay
/// authentication, may take.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a pairing code stays valid (spec §5).
pub const PAIRING_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// How long a joining device waits for the Welcome envelope; also how long
/// an invited device waits for it, and how long an inviting device waits
/// for the invited one's name (spec §5, « Invitation »).
pub const JOIN_TIMEOUT: Duration = Duration::from_secs(30);
/// How long `weft pair` and `weft join` wait for the names of the devices
/// they just started trusting before exiting.
pub const NAME_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the relay has to answer a WebSocket ping.
pub const SETTLE_TIMEOUT: Duration = Duration::from_secs(10);
/// Idle time after which a listener checks the connection is still alive
/// ([`Session::settle`]).
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
/// Delay before the first reconnection attempt; it doubles after each
/// failure, up to [`MAX_BACKOFF`].
pub const MIN_BACKOFF: Duration = Duration::from_secs(1);
pub const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// How long the previous `rx` key is kept after an epoch transition so
/// in-flight live envelopes still decrypt cleanly.
pub const REKEY_GRACE: Duration = Duration::from_secs(30);

/// Per-peer ephemeral forward-secrecy session state (in memory only).
#[derive(Default)]
struct PeerEpoch {
    /// Active acknowledged `(epoch, keys)` used to seal and open live frames.
    epoch_keys: Option<(u64, SessionKeys)>,
    /// Derived `(epoch, keys)` waiting for `session.rekey.ack` before switching `tx`.
    staged: Option<(u64, SessionKeys)>,
    /// Previous `rx` key retained for [`REKEY_GRACE`] during transition.
    prev_rx: Option<([u8; 32], Instant)>,
    /// Live frames sealed under the active `epoch_keys`.
    sent_frames: u64,
    /// When the active `epoch_keys` took effect.
    activated_at: Option<Instant>,
}

impl PeerEpoch {
    fn expire_prev(&mut self, now: Instant) {
        if let Some((_, since)) = self.prev_rx
            && now.saturating_duration_since(since) >= REKEY_GRACE
            && let Some((mut prev, _)) = self.prev_rx.take()
        {
            zero_key(&mut prev);
        }
    }

    fn clear(&mut self) {
        if let Some((_, mut keys)) = self.epoch_keys.take() {
            keys.zero();
        }
        if let Some((_, mut keys)) = self.staged.take() {
            keys.zero();
        }
        if let Some((mut prev, _)) = self.prev_rx.take() {
            zero_key(&mut prev);
        }
        self.sent_frames = 0;
        self.activated_at = None;
    }
}

impl Drop for PeerEpoch {
    fn drop(&mut self) {
        self.clear();
    }
}

/// Where a relay url points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayEndpoint {
    /// `wss://` rather than `ws://`.
    pub tls: bool,
    /// Lowercased host, without port or IPv6 brackets: the name the relay
    /// must announce in its challenge.
    pub host: String,
}

/// Parses and checks a relay url: `ws://` or `wss://`, with a host, 1 to 255
/// bytes (it must fit in a pairing code).
pub fn relay_endpoint(url: &str) -> Result<RelayEndpoint> {
    let invalid = |why| Error::InvalidUrl(url.to_owned(), why);
    if url.is_empty() || url.len() > MAX_RELAY_URL_LEN {
        return Err(invalid("must be 1 to 255 bytes"));
    }
    let uri: Uri = url.parse().map_err(|_| invalid("not a valid url"))?;
    let tls = match uri.scheme_str() {
        Some("wss") => true,
        Some("ws") => false,
        _ => return Err(invalid("must start with ws:// or wss://")),
    };
    let host = uri
        .host()
        .map(|host| host.trim_start_matches('[').trim_end_matches(']'))
        .filter(|host| !host.is_empty())
        .ok_or_else(|| invalid("has no host"))?;
    Ok(RelayEndpoint {
        tls,
        host: host.to_ascii_lowercase(),
    })
}

/// TLS with the Mozilla root store compiled in (webpki-roots) and the
/// `ring` provider, chosen explicitly so it never depends on a process-wide
/// default.
fn tls_connector() -> Connector {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("ring supports the default protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Connector::Rustls(Arc::new(config))
}

/// How a pairing code shown by this device was used, as returned by
/// [`Session::wait_pair_request`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairOutcome {
    /// `device` sent a join request (`0xB1`): it is now in this device's
    /// group.
    Joined { device: [u8; 16] },
    /// `inviter` sent an invitation (`0xB5`): this device is now in the
    /// group of `inviter`. `devices` are the devices it now trusts: the
    /// inviter first, then the members its Welcome listed.
    Invited {
        inviter: [u8; 16],
        devices: Vec<[u8; 16]>,
    },
}

impl PairOutcome {
    /// The device at the other end of the pairing: the one that joined, or
    /// the one that invited.
    pub fn device(&self) -> [u8; 16] {
        match self {
            PairOutcome::Joined { device } => *device,
            PairOutcome::Invited { inviter, .. } => *inviter,
        }
    }

    /// The devices this device started trusting.
    pub fn new_devices(&self) -> Vec<[u8; 16]> {
        match self {
            PairOutcome::Joined { device } => vec![*device],
            PairOutcome::Invited { devices, .. } => devices.clone(),
        }
    }
}

/// A valid request received on an open pairing slot.
enum SlotRequest {
    Join(PairRequest),
    Invite(PairRequest),
}

/// How a call to [`Session::next_event_or`] ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next<T> {
    /// A frame arrived and was handled.
    Event(Event),
    /// The timeout passed first.
    Idle,
    /// The caller's future completed first, with this output. No frame was
    /// taken from the connection.
    Interrupted(T),
}

/// Something that happened on the connection, as reported by
/// [`Session::next_event`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Clipboard text from a trusted device.
    Clipboard { from: [u8; 16], text: String },
    /// A ping envelope from a trusted device.
    Ping { from: [u8; 16] },
    /// A Welcome from a trusted device; `added` are the devices newly trusted.
    Welcome {
        from: [u8; 16],
        added: Vec<[u8; 16]>,
    },
    /// A trusted device announced a new member, now trusted.
    DeviceAdded { by: [u8; 16], device: [u8; 16] },
    /// A trusted device announced a member left; it is no longer trusted.
    DeviceRemoved { by: [u8; 16], device: [u8; 16] },
    /// A trusted device announced its name, now saved as its name.
    DeviceName { from: [u8; 16], name: String },
    /// A packet from a trusted device (`docs/features.md`): UTF-8 text the
    /// sender meant as a JSON object, not parsed here.
    Packet { from: [u8; 16], json: String },
    /// A file chunk from a trusted device (`docs/features.md`, section 4).
    FileChunk {
        from: [u8; 16],
        transfer_id: [u8; 16],
        offset: u64,
        data: Vec<u8>,
    },
    /// An error frame from the relay.
    RelayError(ControlError),
    /// A frame that was dropped, and why.
    Dropped {
        from: Option<[u8; 16]>,
        reason: DropReason,
    },
}

/// Why a received frame was dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// Not a valid envelope or control frame.
    Malformed,
    /// The envelope is addressed to another device.
    NotForUs,
    /// The sender is not a trusted device.
    UnknownSender,
    /// Decryption or plaintext validation failed.
    Undecryptable,
    /// Refused by the replay protection.
    Replay(ReplayError),
    /// A valid control frame this device does not expect now.
    Unexpected,
}

impl std::fmt::Display for DropReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            DropReason::Malformed => "malformed frame",
            DropReason::NotForUs => "addressed to another device",
            DropReason::UnknownSender => "sender is not trusted",
            DropReason::Undecryptable => "decryption failed",
            DropReason::Replay(ReplayError::ZeroCounter) => "counter 0 is invalid",
            DropReason::Replay(ReplayError::TooOld) => "counter older than the replay window",
            DropReason::Replay(ReplayError::Duplicate) => "replayed message",
            DropReason::Unexpected => "unexpected control frame",
        })
    }
}

enum Read {
    Frame(Vec<u8>),
    Pong(Vec<u8>),
}

/// Reads the next binary frame or pong. Pings are answered by tungstenite.
async fn read(ws: &mut Ws) -> Result<Read> {
    loop {
        match ws.next().await {
            Some(Ok(Message::Binary(frame))) => return Ok(Read::Frame(frame.to_vec())),
            Some(Ok(Message::Pong(payload))) => return Ok(Read::Pong(payload.to_vec())),
            Some(Ok(Message::Ping(_) | Message::Text(_) | Message::Frame(_))) => continue,
            Some(Ok(Message::Close(_))) | None => return Err(Error::Closed),
            Some(Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed)) => {
                return Err(Error::Closed);
            }
            Some(Err(err)) => return Err(err.into()),
        }
    }
}

async fn read_frame(ws: &mut Ws) -> Result<Vec<u8>> {
    loop {
        if let Read::Frame(frame) = read(ws).await? {
            return Ok(frame);
        }
    }
}

/// Opens the WebSocket and authenticates `identity` (spec §4).
async fn connect_ws(url: &str, identity: &Identity) -> Result<Ws> {
    let endpoint = relay_endpoint(url)?;
    let request = url.into_client_request()?;
    let connector = if endpoint.tls {
        tls_connector()
    } else {
        Connector::Plain
    };
    let attempt = async {
        let (mut ws, _) =
            tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
                .await?;
        let challenge = Challenge::decode(&read_frame(&mut ws).await?)?;
        // Before signing anything: the relay must be the one we dialled.
        check_relay_name(&challenge, &endpoint.host)?;
        let response = sign_auth(identity, &challenge).encode().to_vec();
        ws.send(Message::Binary(response.into())).await?;
        match read_frame(&mut ws).await {
            Ok(frame) if frame == [AUTH_ACCEPTED] => Ok(ws),
            Ok(_) | Err(Error::Closed) => Err(Error::AuthRejected),
            Err(err) => Err(err),
        }
    };
    tokio::time::timeout(CONNECT_TIMEOUT, attempt)
        .await
        .map_err(|_| Error::Timeout("the relay connection"))?
}

/// An authenticated connection to the relay.
pub struct Session {
    dir: StateDir,
    state: State,
    identity: Arc<Identity>,
    ws: Ws,
    /// Frames read ahead (while waiting for a pong or a pairing message)
    /// and not handled yet.
    pending: VecDeque<Vec<u8>>,
    /// Per-peer ephemeral forward-secrecy session keys.
    ephemeral: BTreeMap<[u8; 16], PeerEpoch>,
}

impl Session {
    /// Loads the state from `dir`, connects to its relay and authenticates.
    pub async fn connect(dir: StateDir) -> Result<Session> {
        let state = dir.load()?;
        let identity = Arc::new(state.identity());
        let ws = connect_ws(&state.relay_url, &identity).await?;
        tracing::debug!(relay = %state.relay_url, device = %short_id(&identity.device_id()), "connected");
        Ok(Session {
            dir,
            state,
            identity,
            ws,
            pending: VecDeque::new(),
            ephemeral: BTreeMap::new(),
        })
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    /// Shared handle to this session's [`Identity`], for authenticating
    /// direct LAN connections without copying secret keys.
    pub fn identity_arc(&self) -> Arc<Identity> {
        self.identity.clone()
    }

    pub fn device_id(&self) -> [u8; 16] {
        self.identity.device_id()
    }

    /// The state as last read or written by this session.
    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn state_dir(&self) -> &StateDir {
        &self.dir
    }

    /// Sends one raw binary frame. Meant for tools and tests; regular
    /// messages go through [`Session::send_text`].
    pub async fn send_frame(&mut self, frame: Vec<u8>) -> Result<()> {
        self.ws.send(Message::Binary(frame.into())).await?;
        Ok(())
    }

    /// Waits until the relay has handled every frame sent so far: it handles
    /// a connection's frames in order and answers a ping when it reads it.
    /// Fails with the relay's error if it answers one in the meantime;
    /// other frames are kept for later.
    pub async fn settle(&mut self) -> Result<()> {
        let payload = random_message_id().to_vec();
        self.ws.send(Message::Ping(payload.clone().into())).await?;
        let deadline = Instant::now() + SETTLE_TIMEOUT;
        loop {
            let read = tokio::time::timeout_at(deadline, read(&mut self.ws))
                .await
                .map_err(|_| Error::Timeout("the relay to answer a ping"))??;
            match read {
                Read::Pong(pong) if pong == payload => return Ok(()),
                Read::Pong(_) => {}
                Read::Frame(frame) => {
                    if let Ok(ControlFrame::Error(code)) = ControlFrame::decode(&frame) {
                        return Err(Error::Relay(code));
                    }
                    self.pending.push_back(frame);
                }
            }
        }
    }

    /// The next frame, or `None` once `deadline` has passed.
    ///
    /// Cancel-safe: taking a kept frame does not wait, and a dropped read
    /// leaves the frame, even one partly received, in the WebSocket.
    async fn next_frame(&mut self, deadline: Option<Instant>) -> Result<Option<Vec<u8>>> {
        if let Some(frame) = self.pending.pop_front() {
            return Ok(Some(frame));
        }
        let Some(deadline) = deadline else {
            return read_frame(&mut self.ws).await.map(Some);
        };
        match tokio::time::timeout_at(deadline, read_frame(&mut self.ws)).await {
            Ok(frame) => frame.map(Some),
            Err(_) => Ok(None),
        }
    }

    /// Puts frames read ahead back in front of the queue, in order.
    fn requeue(&mut self, frames: Vec<Vec<u8>>) {
        for frame in frames.into_iter().rev() {
            self.pending.push_front(frame);
        }
    }

    /// Reloads `state.json` from disk and returns it.
    pub fn reload_state(&mut self) -> Result<&State> {
        self.state = self.dir.load()?;
        Ok(&self.state)
    }

    /// Static [`SessionKeys`] derived with the trusted device `peer_id`.
    pub fn static_session_keys(&self, peer_id: &[u8; 16]) -> Result<SessionKeys> {
        let peer = self.trusted_device(peer_id)?;
        Ok(session_keys(&self.identity, peer_id, &peer.kx_pk)?)
    }

    /// Stages derived ephemeral `keys` for `epoch` with `peer_id` so incoming
    /// frames sealed under `epoch` already open while waiting for the peer's
    /// `session.rekey.ack`.
    pub fn stage_epoch_keys(&mut self, peer_id: [u8; 16], epoch: u64, mut keys: SessionKeys) {
        if self
            .active_epoch(&peer_id)
            .is_some_and(|active| epoch <= active)
        {
            keys.zero();
            return;
        }
        let entry = self.ephemeral.entry(peer_id).or_default();
        if let Some((_, mut prev_staged)) = entry.staged.take() {
            prev_staged.zero();
        }
        entry.staged = Some((epoch, keys));
    }

    /// Promotes the staged ephemeral keys for `epoch` to the active epoch of
    /// `peer_id`, moving the previous `rx` key (or static `rx` on the first
    /// activation) into `prev_rx` for [`REKEY_GRACE`]. Returns `true` if
    /// `epoch` was staged and activated.
    pub fn activate_epoch(&mut self, peer_id: [u8; 16], epoch: u64) -> Result<bool> {
        let static_rx = self.static_session_keys(&peer_id)?.rx;
        let now = Instant::now();
        let entry = self.ephemeral.entry(peer_id).or_default();
        let matches = entry
            .staged
            .as_ref()
            .is_some_and(|(staged_epoch, _)| *staged_epoch == epoch);
        if !matches {
            return Ok(false);
        }
        let (_, new_keys) = entry.staged.take().expect("checked above");
        let prev = match entry.epoch_keys.take() {
            Some((_, mut old_keys)) => {
                let rx = old_keys.rx;
                old_keys.zero();
                rx
            }
            None => static_rx,
        };
        if let Some((mut old_prev, _)) = entry.prev_rx.take() {
            zero_key(&mut old_prev);
        }
        entry.prev_rx = Some((prev, now));
        entry.epoch_keys = Some((epoch, new_keys));
        entry.sent_frames = 0;
        entry.activated_at = Some(now);
        Ok(true)
    }

    /// Stages and immediately activates ephemeral `keys` for `epoch` with
    /// `peer_id`.
    pub fn set_epoch_keys(
        &mut self,
        peer_id: [u8; 16],
        epoch: u64,
        keys: SessionKeys,
    ) -> Result<()> {
        self.stage_epoch_keys(peer_id, epoch, keys);
        self.activate_epoch(peer_id, epoch)?;
        Ok(())
    }

    /// Active acknowledged ephemeral epoch with `peer_id`, if any.
    pub fn active_epoch(&self, peer_id: &[u8; 16]) -> Option<u64> {
        self.ephemeral
            .get(peer_id)
            .and_then(|entry| entry.epoch_keys.as_ref().map(|(epoch, _)| *epoch))
    }

    /// Staged (not yet acknowledged) ephemeral epoch with `peer_id`, if any.
    pub fn staged_epoch(&self, peer_id: &[u8; 16]) -> Option<u64> {
        self.ephemeral
            .get(peer_id)
            .and_then(|entry| entry.staged.as_ref().map(|(epoch, _)| *epoch))
    }

    /// Live frames sealed under the active epoch of `peer_id`.
    pub fn epoch_sent_frames(&self, peer_id: &[u8; 16]) -> u64 {
        self.ephemeral
            .get(peer_id)
            .map_or(0, |entry| entry.sent_frames)
    }

    /// Whether `peer_id` has an active epoch that has reached `max_frames`
    /// sent frames or `max_age` elapsed time.
    pub fn epoch_needs_rotation(
        &self,
        peer_id: &[u8; 16],
        max_age: Duration,
        max_frames: u64,
    ) -> bool {
        let Some(entry) = self.ephemeral.get(peer_id) else {
            return false;
        };
        if entry.epoch_keys.is_none() || entry.staged.is_some() {
            return false;
        }
        entry.sent_frames >= max_frames
            || entry
                .activated_at
                .is_some_and(|at| Instant::now().saturating_duration_since(at) >= max_age)
    }

    /// Drops and zeroes any ephemeral keys held for `peer_id`.
    pub fn clear_peer_epoch(&mut self, peer_id: &[u8; 16]) {
        self.ephemeral.remove(peer_id);
    }

    /// Seals `plaintext` for `peer` using static keys for offline-eligible
    /// kinds (`0x03`–`0x06`) and active `epoch_keys` (when acknowledged) for
    /// live kinds (`0x01`, `0x02`, `0x07`, `0x08`). Increments and saves the
    /// counter before returning the encoded frame.
    fn seal_device_envelope(
        &mut self,
        peer: &TrustedDevice,
        kind: Kind,
        plaintext: &[u8],
    ) -> Result<Vec<u8>> {
        let peer_id = peer.device_id();
        let mut static_keys = session_keys(&self.identity, &peer_id, &peer.kx_pk)?;
        let mut tx = static_keys.tx;
        let used_epoch = if kind.is_static() {
            false
        } else if let Some((_, epoch_keys)) = self
            .ephemeral
            .get(&peer_id)
            .and_then(|entry| entry.epoch_keys.as_ref())
        {
            tx = epoch_keys.tx;
            true
        } else {
            false
        };
        static_keys.zero();
        let (counter, state) = self.dir.update(|state| state.next_counter(&peer_id))?;
        self.state = state;
        let header = Header {
            version: VERSION,
            kind,
            sender: self.identity.device_id(),
            recipient: peer_id,
            message_id: random_message_id(),
            counter,
        };
        let frame_res = seal(&header, plaintext, &tx);
        zero_key(&mut tx);
        let frame = frame_res?;
        if used_epoch && let Some(entry) = self.ephemeral.get_mut(&peer_id) {
            entry.sent_frames += 1;
        }
        Ok(frame)
    }

    /// Seals an envelope of `kind` for the trusted device `to` and returns
    /// the wire frame without sending it, so a caller can send over an
    /// authenticated LAN stream with relay fallback.
    pub fn seal_for(&mut self, to: [u8; 16], kind: Kind, plaintext: &[u8]) -> Result<Vec<u8>> {
        if kind == Kind::Packet {
            let text =
                std::str::from_utf8(plaintext).map_err(|_| weft_protocol::Error::InvalidUtf8)?;
            check_packet_len(text)?;
        }
        let device = self.trusted_device(&to)?;
        self.seal_device_envelope(&device, kind, plaintext)
    }

    /// Seals `plaintext` for `peer` and sends it over the relay WebSocket.
    /// The counter is incremented and saved before the envelope leaves.
    async fn send_envelope(
        &mut self,
        peer: &TrustedDevice,
        kind: Kind,
        plaintext: &[u8],
    ) -> Result<()> {
        let frame = self.seal_device_envelope(peer, kind, plaintext)?;
        self.send_frame(frame).await
    }

    /// Sends `text` to every trusted device and waits until the relay has
    /// handled it. Returns how many devices it was sent to. The relay
    /// silently drops envelopes for devices that are not connected.
    pub async fn send_text(&mut self, text: &str) -> Result<usize> {
        if text.len() > MAX_CLIPBOARD_TEXT_LEN {
            return Err(weft_protocol::Error::PlaintextTooLarge(text.len()).into());
        }
        self.state = self.dir.load()?;
        let devices = self.state.devices.clone();
        for device in &devices {
            self.send_envelope(device, Kind::ClipboardText, text.as_bytes())
                .await?;
        }
        self.settle().await?;
        Ok(devices.len())
    }

    /// The trusted device `device_id`, or [`Error::UnknownDevice`].
    fn trusted_device(&self, device_id: &[u8; 16]) -> Result<TrustedDevice> {
        self.state
            .device(device_id)
            .cloned()
            .ok_or_else(|| Error::UnknownDevice(crate::hex(device_id)))
    }

    /// Sends a packet (`json`, meant to be a JSON object, 1 to 256 KiB) to
    /// the trusted device `to`. Does not wait for the relay: call
    /// [`Session::settle`] for that. Fails if `to` is not trusted.
    pub async fn send_packet(&mut self, to: [u8; 16], json: &str) -> Result<()> {
        check_packet_len(json)?;
        let device = self.trusted_device(&to)?;
        self.send_envelope(&device, Kind::Packet, json.as_bytes())
            .await
    }

    /// Sends a packet to every trusted device. Returns how many devices it
    /// was sent to. Does not wait for the relay.
    pub async fn send_packet_all(&mut self, json: &str) -> Result<usize> {
        check_packet_len(json)?;
        self.state = self.dir.load()?;
        let devices = self.state.devices.clone();
        for device in &devices {
            self.send_envelope(device, Kind::Packet, json.as_bytes())
                .await?;
        }
        Ok(devices.len())
    }

    /// Sends one file chunk (1 to 128 KiB of `data` at `offset` of the
    /// transfer `transfer_id`) to the trusted device `to`. Does not wait
    /// for the relay. Fails if `to` is not trusted.
    pub async fn send_file_chunk(
        &mut self,
        to: [u8; 16],
        transfer_id: [u8; 16],
        offset: u64,
        data: &[u8],
    ) -> Result<()> {
        let plaintext = encode_file_chunk(&transfer_id, offset, data)?;
        let device = self.trusted_device(&to)?;
        self.send_envelope(&device, Kind::FileChunk, &plaintext)
            .await
    }

    /// Sends this device's name (Device name envelope) to each of `devices`
    /// that is trusted; the others are skipped. Does not wait for the relay.
    async fn send_name(&mut self, devices: &[[u8; 16]]) -> Result<()> {
        let plaintext = encode_device_name(self.state.name())?;
        for device_id in devices {
            if *device_id == self.device_id() {
                continue;
            }
            let Some(device) = self.state.device(device_id).cloned() else {
                continue;
            };
            self.send_envelope(&device, Kind::DeviceName, &plaintext)
                .await?;
        }
        Ok(())
    }

    /// Sends this device's name to `to` if it is trusted, without waiting
    /// for the relay: a device that was offline when it changed learns it
    /// on its next connection (`docs/features.md` section 2).
    pub async fn send_name_to(&mut self, to: [u8; 16]) -> Result<()> {
        self.send_name(&[to]).await
    }

    /// Changes this device's name, saves it, then sends it to every trusted
    /// device and waits until the relay has handled it. Returns how many
    /// devices it was sent to. The name is saved even if sending fails.
    pub async fn rename(&mut self, name: &str) -> Result<usize> {
        let (result, state) = self.dir.update(|state| state.set_name(name))?;
        self.state = state;
        result?;
        let devices: Vec<[u8; 16]> = self.state.devices.iter().map(|d| d.device_id()).collect();
        self.send_name(&devices).await?;
        self.settle().await?;
        Ok(devices.len())
    }

    /// Waits for the next frame and handles it. Returns `None` if `timeout`
    /// passes first; `None` as the timeout waits forever.
    ///
    /// When the event makes this device trust another one (Welcome, Device
    /// added), it sends that device its name before returning.
    pub async fn next_event(&mut self, timeout: Option<Duration>) -> Result<Option<Event>> {
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        match self.next_frame(deadline).await? {
            Some(frame) => self.handle(&frame).await.map(Some),
            None => Ok(None),
        }
    }

    /// Like [`Session::next_event`], but also returns as soon as `interrupt`
    /// completes: the way to wait for the connection and for something else
    /// (a local request, a shutdown) at once.
    ///
    /// Unlike a `select!` around `next_event`, this never loses anything.
    /// Only the wait for a frame races with `interrupt`, and dropping that
    /// wait leaves the frame in the connection. Once a frame has been read,
    /// it is handled to the end before this returns, and `interrupt` is
    /// dropped: it must be cancel-safe as well if its output matters.
    pub async fn next_event_or<F: Future>(
        &mut self,
        timeout: Option<Duration>,
        interrupt: F,
    ) -> Result<Next<F::Output>> {
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        let frame = tokio::select! {
            frame = self.next_frame(deadline) => frame?,
            output = interrupt => return Ok(Next::Interrupted(output)),
        };
        match frame {
            Some(frame) => self.handle(&frame).await.map(Next::Event),
            None => Ok(Next::Idle),
        }
    }

    /// Handles the next frame that was read ahead and kept for later (while
    /// waiting for a pong or a pairing message), without reading from the
    /// connection. Returns `None` when there is none.
    ///
    /// Lets a caller that waits for a device in short slices of
    /// [`Session::wait_pair_request`] deliver the envelopes that arrive in
    /// the meantime, without taking a pair request off the connection.
    pub async fn next_queued_event(&mut self) -> Result<Option<Event>> {
        match self.pending.pop_front() {
            Some(frame) => self.handle(&frame).await.map(Some),
            None => Ok(None),
        }
    }

    /// Handles one externally received envelope or control frame (such as an
    /// envelope from an authenticated LAN TCP stream) through the exact same
    /// decryption and 1024-message replay window as WebSocket frames.
    pub async fn receive_frame(&mut self, frame: &[u8]) -> Result<Event> {
        self.handle(frame).await
    }

    /// Handles one frame to the end: applies it, then sends this device's
    /// name to the devices it made this one trust. Not cancel-safe: dropped
    /// half-way, the event is lost and the names may not be sent.
    async fn handle(&mut self, frame: &[u8]) -> Result<Event> {
        let event = self.handle_frame(frame)?;
        match &event {
            Event::Welcome { from, added } => {
                let mut devices = vec![*from];
                devices.extend(added);
                self.send_name(&devices).await?;
            }
            Event::DeviceAdded { device, .. } => self.send_name(&[*device]).await?,
            Event::DeviceRemoved { device, .. } => self.clear_peer_epoch(device),
            _ => {}
        }
        Ok(event)
    }

    /// Handles events until a Device name envelope has arrived from each of
    /// `devices`, or `timeout` passes. Every event is passed to `on_event`.
    /// Returns whether all the names arrived.
    pub async fn wait_names(
        &mut self,
        devices: &[[u8; 16]],
        timeout: Duration,
        mut on_event: impl FnMut(&Event),
    ) -> Result<bool> {
        let deadline = Instant::now() + timeout;
        let mut missing: Vec<[u8; 16]> = devices.to_vec();
        while !missing.is_empty() {
            let left = deadline.saturating_duration_since(Instant::now());
            let Some(event) = self.next_event(Some(left)).await? else {
                return Ok(false);
            };
            if let Event::DeviceName { from, .. } = &event {
                missing.retain(|device| device != from);
            }
            on_event(&event);
        }
        Ok(true)
    }

    fn handle_frame(&mut self, frame: &[u8]) -> Result<Event> {
        let dropped = |reason| Event::Dropped { from: None, reason };
        match classify(frame) {
            FrameType::Envelope => self.handle_envelope(frame),
            FrameType::Control => Ok(match ControlFrame::decode(frame) {
                Ok(ControlFrame::Error(code)) => Event::RelayError(code),
                Ok(_) => dropped(DropReason::Unexpected),
                Err(_) => dropped(DropReason::Malformed),
            }),
            FrameType::Unknown => Ok(dropped(DropReason::Malformed)),
        }
    }

    /// Decrypts an envelope from a trusted device, checks it against the
    /// replay state, and applies it, all in one state update.
    fn handle_envelope(&mut self, frame: &[u8]) -> Result<Event> {
        let Some(Ok(header)) = frame.get(..HEADER_LEN).map(Header::decode) else {
            return Ok(Event::Dropped {
                from: None,
                reason: DropReason::Malformed,
            });
        };
        let sender = header.sender;
        let dropped = |reason| Event::Dropped {
            from: Some(sender),
            reason,
        };
        if header.recipient != self.identity.device_id() {
            return Ok(dropped(DropReason::NotForUs));
        }
        let now = Instant::now();
        if let Some(entry) = self.ephemeral.get_mut(&sender) {
            entry.expire_prev(now);
        }
        let identity = &self.identity;
        let peer_epoch = self.ephemeral.get(&sender);
        let (event, state) = self.dir.update(|state| {
            let Some(device) = state.device(&sender) else {
                return dropped(DropReason::UnknownSender);
            };
            let Ok(mut static_keys) = session_keys(identity, &sender, &device.kx_pk) else {
                return dropped(DropReason::Undecryptable);
            };
            let opened = open_with_peer_keys(frame, header.kind, &static_keys.rx, peer_epoch, now);
            static_keys.zero();
            let Ok((header, plaintext)) = opened else {
                return dropped(DropReason::Undecryptable);
            };
            // Only recorded once authenticated, so a forged header cannot
            // move the window.
            let mut peer = state.peer(&sender).cloned().unwrap_or_default();
            if let Err(err) = peer.accept(header.counter, &header.message_id) {
                return dropped(DropReason::Replay(err));
            }
            *state.peer_mut(&sender) = peer;
            apply(state, identity, sender, header.kind, plaintext)
        })?;
        self.state = state;
        Ok(event)
    }

    /// Opens a pairing slot with a fresh pairing id and secret, and returns
    /// the code to show. The secret only leaves this process through it.
    pub async fn open_pairing(&mut self) -> Result<PairingCode> {
        self.state = self.dir.load()?;
        if self.state.devices.len() + 1 >= MAX_GROUP_SIZE {
            return Err(Error::GroupFull);
        }
        let code = PairingCode {
            relay_url: self.state.relay_url.clone(),
            pairing_id: random_pairing_id(),
            sign_pk: self.identity.sign_pk(),
            kx_pk: self.identity.kx_pk(),
            secret: random_pairing_secret(),
        };
        // Fails early if the url cannot go in a code.
        code.encode()?;
        let pairing_id = code.pairing_id;
        self.send_frame(ControlFrame::PairOpen { pairing_id }.encode())
            .await?;
        self.settle().await?;
        Ok(code)
    }

    /// Waits for a valid request for `code` and acts on it.
    ///
    /// A join request (`0xB1`) adds the device to the group: trust, `0xB2`,
    /// Welcome to it, Device added to the other members and this device's
    /// name to it.
    ///
    /// An invitation (`0xB5`) is accepted only while this device trusts no
    /// other one (spec §5, « Invitation »): trust the inviter, `0xB4`, this
    /// device's name to it, then wait up to [`JOIN_TIMEOUT`] for its
    /// Welcome, trust the members it lists and send them this device's
    /// name. A device that already trusts others ignores invitations.
    ///
    /// Requests with another pairing id or a wrong MAC, and ignored
    /// invitations, are logged and the wait goes on. `timeout` only bounds
    /// the wait for a request, not the steps after it.
    pub async fn wait_pair_request(
        &mut self,
        code: &PairingCode,
        timeout: Duration,
    ) -> Result<PairOutcome> {
        let deadline = Instant::now() + timeout;
        let mut deferred = Vec::new();
        let result = self
            .wait_pair_request_until(code, deadline, &mut deferred)
            .await;
        self.requeue(deferred);
        match result? {
            SlotRequest::Join(request) => {
                let device = self.accept_join(request).await?;
                Ok(PairOutcome::Joined { device })
            }
            SlotRequest::Invite(request) => self.accept_invite(request).await,
        }
    }

    async fn wait_pair_request_until(
        &mut self,
        code: &PairingCode,
        deadline: Instant,
        deferred: &mut Vec<Vec<u8>>,
    ) -> Result<SlotRequest> {
        loop {
            let Some(frame) = self.next_frame(Some(deadline)).await? else {
                return Err(Error::Timeout("a device to join"));
            };
            match classify(&frame) {
                FrameType::Envelope => deferred.push(frame),
                FrameType::Control => match ControlFrame::decode(&frame) {
                    Ok(ControlFrame::PairRequest(request)) => {
                        let joiner = short_id(&device_id_from_sign_pk(&request.sign_pk));
                        if request.pairing_id != code.pairing_id {
                            tracing::warn!(%joiner, "ignored a pair request for another pairing");
                        } else if !verify_pair_request(&code.secret, &request) {
                            tracing::warn!(%joiner, "ignored a pair request with an invalid MAC");
                            self.reopen_slot(code).await?;
                        } else if request.sign_pk == self.identity.sign_pk() {
                            tracing::warn!("ignored a pair request from this device");
                        } else {
                            return Ok(SlotRequest::Join(request));
                        }
                    }
                    Ok(ControlFrame::PairInvite(request)) => {
                        let inviter = short_id(&device_id_from_sign_pk(&request.sign_pk));
                        if request.pairing_id != code.pairing_id {
                            tracing::warn!(%inviter, "ignored an invitation for another pairing");
                        } else if !verify_invite_request(&code.secret, &request) {
                            tracing::warn!(%inviter, "ignored an invitation with an invalid MAC");
                            self.reopen_slot(code).await?;
                        } else if request.sign_pk == self.identity.sign_pk() {
                            tracing::warn!("ignored an invitation from this device");
                        } else if !self.dir.load()?.devices.is_empty() {
                            // Scanning the code of a device in a group must
                            // never pull it out of that group.
                            tracing::warn!(
                                %inviter,
                                "ignored an invitation: this device already belongs to a group; \
                                 the other device can join it instead"
                            );
                            self.reopen_slot(code).await?;
                        } else {
                            return Ok(SlotRequest::Invite(request));
                        }
                    }
                    Ok(ControlFrame::Error(code)) => {
                        tracing::warn!(error = %Error::Relay(code), "while waiting for a device");
                    }
                    _ => {}
                },
                FrameType::Unknown => {}
            }
        }
    }

    /// The relay closes the slot when it forwards a request: reopens it
    /// after an ignored one, so the code keeps working.
    async fn reopen_slot(&mut self, code: &PairingCode) -> Result<()> {
        let pairing_id = code.pairing_id;
        self.send_frame(ControlFrame::PairOpen { pairing_id }.encode())
            .await
    }

    /// Accepts the invitation `request` of a device this one does not trust
    /// yet, this device trusting no other one.
    async fn accept_invite(&mut self, request: PairRequest) -> Result<PairOutcome> {
        let inviter_keys = PeerKeys {
            sign_pk: request.sign_pk,
            kx_pk: request.kx_pk,
        };
        let inviter = inviter_keys.device_id();
        self.state = self.dir.update(|state| state.trust(inviter_keys))?.1;

        self.send_frame(ControlFrame::GroupJoin { device_id: inviter }.encode())
            .await?;
        if let Err(err) = self.settle().await {
            self.state = self.dir.update(|state| state.untrust(&inviter))?.1;
            return Err(match err {
                Error::Relay(ControlError::GroupFull) => Error::GroupFull,
                err => err,
            });
        }

        // In the inviter's group now. This name tells the inviter so: it
        // answers with the Welcome.
        self.send_name(&[inviter]).await?;
        let deadline = Instant::now() + JOIN_TIMEOUT;
        let mut deferred = Vec::new();
        let result = self
            .wait_welcome(&inviter_keys, deadline, &mut deferred)
            .await;
        self.requeue(deferred);
        let (header, members) = match result {
            Ok(welcome) => welcome,
            Err(err) => {
                self.leave_failed_invitation(&inviter).await?;
                return Err(match err {
                    Error::Timeout(_) => Error::NoWelcome,
                    err => err,
                });
            }
        };

        let identity = &self.identity;
        let (devices, state) = self.dir.update(|state| {
            trust_welcome(state, identity, &inviter, &header, members);
            state
                .devices
                .iter()
                .map(|d| d.device_id())
                .collect::<Vec<_>>()
        })?;
        self.state = state;
        let members: Vec<[u8; 16]> = devices
            .iter()
            .copied()
            .filter(|device| *device != inviter)
            .collect();
        self.send_name(&members).await?;
        self.settle().await?;
        let mut devices = devices;
        // The inviter first.
        devices.retain(|device| *device != inviter);
        devices.insert(0, inviter);
        Ok(PairOutcome::Invited { inviter, devices })
    }

    /// Undoes an accepted invitation whose Welcome never came: stops
    /// trusting the inviter and leaves its group on the relay, so this
    /// device is back to trusting nobody and can be invited or joined
    /// again.
    async fn leave_failed_invitation(&mut self, inviter: &[u8; 16]) -> Result<()> {
        self.state = self.dir.update(|state| state.untrust(inviter))?.1;
        let device_id = self.device_id();
        self.send_frame(ControlFrame::GroupRemove { device_id }.encode())
            .await?;
        match self.settle().await {
            Ok(()) => Ok(()),
            Err(Error::Relay(code)) => {
                tracing::warn!(error = %Error::Relay(code), "while leaving the inviter's group");
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// Invites the device that showed `code` into this device's group
    /// (spec §5, « Invitation »): trusts it, sends `0xB5`, waits up to
    /// `timeout` for its Device name (the sign that it accepted and is in
    /// the group), then sends it the Welcome, sends the other members a
    /// Device added with its keys, and sends it this device's name.
    ///
    /// This device must already be in a group: otherwise the other device
    /// should be joined instead ([`Session::join`]). A device that already
    /// belongs to a group ignores invitations: it then never answers, and
    /// this fails with [`Error::NoAnswer`]; it is no longer trusted then.
    /// Returns the invited device's id.
    pub async fn invite(&mut self, code: &PairingCode, timeout: Duration) -> Result<[u8; 16]> {
        if code.sign_pk == self.identity.sign_pk() {
            return Err(Error::OwnCode);
        }
        self.state = self.dir.load()?;
        if code.relay_url != self.state.relay_url {
            return Err(Error::RelayUrlMismatch {
                state: self.state.relay_url.clone(),
                code: code.relay_url.clone(),
            });
        }
        if self.state.devices.is_empty() {
            return Err(Error::NoGroup);
        }
        let keys = PeerKeys {
            sign_pk: code.sign_pk,
            kx_pk: code.kx_pk,
        };
        let invitee = keys.device_id();
        let was_trusted = self.state.device(&invitee).is_some();
        if !was_trusted && self.state.devices.len() + 1 >= MAX_GROUP_SIZE {
            return Err(Error::GroupFull);
        }
        // Trusted first, so its name can be read.
        self.state = self.dir.update(|state| state.trust(keys))?.1;
        let request = PairRequest::invite(
            &code.secret,
            code.pairing_id,
            self.identity.sign_pk(),
            self.identity.kx_pk(),
        );
        let deadline = Instant::now() + timeout;
        let mut deferred = Vec::new();
        let result = match self
            .send_frame(ControlFrame::PairInvite(request).encode())
            .await
        {
            Ok(()) => {
                self.wait_invitee_name(&invitee, deadline, &mut deferred)
                    .await
            }
            Err(err) => Err(err),
        };
        self.requeue(deferred);
        if let Err(err) = result {
            if !was_trusted {
                self.state = self.dir.update(|state| state.untrust(&invitee))?.1;
            }
            return Err(match err {
                Error::Timeout(_) => Error::NoAnswer,
                err => err,
            });
        }

        let invited = self.trusted_device(&invitee)?;
        let others: Vec<TrustedDevice> = self
            .state
            .devices
            .iter()
            .filter(|device| device.device_id() != invitee)
            .cloned()
            .collect();
        let members: Vec<PeerKeys> = others.iter().map(TrustedDevice::keys).collect();
        // Device added first: the relay then queues it for each member
        // before the invited device can read the Welcome and send them its
        // name, so the members trust it by the time its name arrives.
        for other in &others {
            self.send_envelope(other, Kind::DeviceAdded, &encode_device_added(&keys))
                .await?;
        }
        self.send_envelope(&invited, Kind::Welcome, &encode_welcome(&members)?)
            .await?;
        self.send_name(&[invitee]).await?;
        self.settle().await?;
        Ok(invitee)
    }

    /// Waits for the Device name of `invitee` and saves it. Fails on the
    /// first relay error (the code expired, the group is full...). Other
    /// envelopes are kept in `deferred`.
    async fn wait_invitee_name(
        &mut self,
        invitee: &[u8; 16],
        deadline: Instant,
        deferred: &mut Vec<Vec<u8>>,
    ) -> Result<()> {
        loop {
            let Some(frame) = self.next_frame(Some(deadline)).await? else {
                return Err(Error::Timeout("the invited device"));
            };
            match classify(&frame) {
                FrameType::Control => {
                    if let Ok(ControlFrame::Error(code)) = ControlFrame::decode(&frame) {
                        return Err(Error::Relay(code));
                    }
                }
                FrameType::Envelope => {
                    let Some(Ok(header)) = frame.get(..HEADER_LEN).map(Header::decode) else {
                        continue;
                    };
                    if header.sender != *invitee || header.kind != Kind::DeviceName {
                        deferred.push(frame);
                        continue;
                    }
                    match self.handle(&frame).await? {
                        Event::DeviceName { .. } => return Ok(()),
                        Event::Dropped { reason, .. } => {
                            tracing::warn!(%reason, "dropped a frame from the invited device");
                        }
                        _ => {}
                    }
                }
                FrameType::Unknown => {}
            }
        }
    }

    async fn accept_join(&mut self, request: PairRequest) -> Result<[u8; 16]> {
        let keys = PeerKeys {
            sign_pk: request.sign_pk,
            kx_pk: request.kx_pk,
        };
        let joiner = keys.device_id();
        let ((others, was_trusted), state) = self.dir.update(|state| {
            let was_trusted = state.device(&joiner).is_some();
            let others: Vec<TrustedDevice> = state
                .devices
                .iter()
                .filter(|device| device.device_id() != joiner)
                .cloned()
                .collect();
            state.trust(keys);
            (others, was_trusted)
        })?;
        self.state = state;

        self.send_frame(ControlFrame::GroupAdd { device_id: joiner }.encode())
            .await?;
        if let Err(err) = self.settle().await {
            if !was_trusted {
                self.state = self.dir.update(|state| state.untrust(&joiner))?.1;
            }
            return Err(match err {
                Error::Relay(ControlError::GroupFull) => Error::GroupFull,
                err => err,
            });
        }

        let new_device = self.state.device(&joiner).cloned().expect("trusted above");
        let members: Vec<PeerKeys> = others.iter().map(TrustedDevice::keys).collect();
        // As with invitations, queue membership changes before welcoming the
        // new device. Once welcomed it can immediately send its name to every
        // member, whose incoming relay queue must already contain DeviceAdded.
        for other in &others {
            self.send_envelope(other, Kind::DeviceAdded, &encode_device_added(&keys))
                .await?;
        }
        self.send_envelope(&new_device, Kind::Welcome, &encode_welcome(&members)?)
            .await?;
        self.send_name(&[joiner]).await?;
        self.settle().await?;
        Ok(joiner)
    }

    /// Joins the group of the device that showed `code`: sends the pair
    /// request, waits for its Welcome, then trusts it and the members it
    /// lists, and sends each of them this device's name. Devices trusted
    /// before (a previous group) are dropped, since the relay takes this
    /// device out of its previous group. Nothing is saved unless the Welcome
    /// arrives. Returns the trusted devices.
    pub async fn join(&mut self, code: &PairingCode, timeout: Duration) -> Result<Vec<[u8; 16]>> {
        if code.sign_pk == self.identity.sign_pk() {
            return Err(Error::OwnCode);
        }
        if code.relay_url != self.state.relay_url {
            return Err(Error::RelayUrlMismatch {
                state: self.state.relay_url.clone(),
                code: code.relay_url.clone(),
            });
        }
        let request = PairRequest::new(
            &code.secret,
            code.pairing_id,
            self.identity.sign_pk(),
            self.identity.kx_pk(),
        );
        self.send_frame(ControlFrame::PairRequest(request).encode())
            .await?;
        let deadline = Instant::now() + timeout;
        let mut deferred = Vec::new();
        let owner = PeerKeys {
            sign_pk: code.sign_pk,
            kx_pk: code.kx_pk,
        };
        let result = self.wait_welcome(&owner, deadline, &mut deferred).await;
        self.requeue(deferred);
        let (header, members) = result?;

        let owner_id = owner.device_id();
        let identity = &self.identity;
        let (dropped, state) = self.dir.update(|state| {
            let before: Vec<TrustedDevice> = std::mem::take(&mut state.devices);
            state.trust(owner);
            trust_welcome(state, identity, &owner_id, &header, members);
            before
                .into_iter()
                .filter(|device| state.device(&device.device_id()).is_none())
                .map(|device| device.device_id())
                .collect::<Vec<_>>()
        })?;
        self.state = state;
        for device in dropped {
            tracing::warn!(device = %short_id(&device), "no longer trusted: it belongs to the previous group");
        }
        let trusted: Vec<[u8; 16]> = self.state.devices.iter().map(|d| d.device_id()).collect();
        self.send_name(&trusted).await?;
        self.settle().await?;
        Ok(trusted)
    }

    /// Waits for the Welcome of `owner` (a device whose keys are known but
    /// that may not be trusted yet) and checks it, without applying it.
    async fn wait_welcome(
        &mut self,
        owner: &PeerKeys,
        deadline: Instant,
        deferred: &mut Vec<Vec<u8>>,
    ) -> Result<(Header, Vec<PeerKeys>)> {
        let owner_id = owner.device_id();
        let keys = session_keys(&self.identity, &owner_id, &owner.kx_pk)?;
        loop {
            let Some(frame) = self.next_frame(Some(deadline)).await? else {
                return Err(Error::Timeout("the welcome message"));
            };
            match classify(&frame) {
                FrameType::Control => {
                    if let Ok(ControlFrame::Error(code)) = ControlFrame::decode(&frame) {
                        return Err(Error::Relay(code));
                    }
                }
                FrameType::Envelope => {
                    let Some(Ok(header)) = frame.get(..HEADER_LEN).map(Header::decode) else {
                        continue;
                    };
                    if header.sender != owner_id
                        || header.recipient != self.identity.device_id()
                        || header.kind != Kind::Welcome
                    {
                        deferred.push(frame);
                        continue;
                    }
                    let Ok((header, plaintext)) = open(&frame, &keys.rx) else {
                        tracing::warn!("dropped a welcome that does not decrypt");
                        continue;
                    };
                    let peer = self.state.peer(&owner_id).cloned().unwrap_or_default();
                    if let Err(err) = peer.check(header.counter, &header.message_id) {
                        let reason = DropReason::Replay(err);
                        tracing::warn!(%reason, "dropped a welcome");
                        continue;
                    }
                    return Ok((header, decode_welcome(&plaintext)?));
                }
                FrameType::Unknown => {}
            }
        }
    }

    /// Removes the trusted device whose id starts with the hex `prefix`:
    /// `0xB3` to the relay, Device removed to the remaining members, then
    /// drops it locally. Returns the removed device.
    pub async fn remove(&mut self, prefix: &str) -> Result<TrustedDevice> {
        self.state = self.dir.load()?;
        let device = self.state.find_by_prefix(prefix)?.clone();
        let device_id = device.device_id();
        self.send_frame(ControlFrame::GroupRemove { device_id }.encode())
            .await?;
        match self.settle().await {
            Ok(()) => {}
            // Local trust is what matters; the relay copy is for routing.
            Err(Error::Relay(code)) => {
                tracing::warn!(error = %Error::Relay(code), "the relay refused the removal; removing locally anyway");
            }
            Err(err) => return Err(err),
        }
        let (_, state) = self.dir.update(|state| state.untrust(&device_id))?;
        self.state = state;
        self.clear_peer_epoch(&device_id);
        let remaining = self.state.devices.clone();
        let plaintext = encode_device_removed(&device_id);
        for other in &remaining {
            self.send_envelope(other, Kind::DeviceRemoved, &plaintext)
                .await?;
        }
        self.settle().await?;
        Ok(device)
    }

    /// Closes the connection politely.
    pub async fn close(mut self) {
        let _ = tokio::time::timeout(Duration::from_secs(2), self.ws.close(None)).await;
    }
}

/// Opens `frame` according to the envelope kind and the peer's ephemeral
/// epoch state:
/// - Offline-eligible kinds (`0x03`–`0x06`) ALWAYS open with `static_rx`.
/// - Live kinds (`0x01`, `0x02`, `0x07`, `0x08`) try `epoch_keys.rx` first,
///   then `staged.rx` (during handshake), then `prev_rx` (if within
///   [`REKEY_GRACE`]), then `static_rx` (before epoch activation).
fn open_with_peer_keys(
    frame: &[u8],
    kind: Kind,
    static_rx: &[u8; 32],
    peer_epoch: Option<&PeerEpoch>,
    now: Instant,
) -> Result<(Header, Vec<u8>), weft_protocol::Error> {
    if kind.is_static() {
        return open(frame, static_rx);
    }
    if let Some(entry) = peer_epoch {
        if let Some((_, epoch_keys)) = &entry.epoch_keys
            && let Ok(opened) = open(frame, &epoch_keys.rx)
        {
            return Ok(opened);
        }
        if let Some((_, staged_keys)) = &entry.staged
            && let Ok(opened) = open(frame, &staged_keys.rx)
        {
            return Ok(opened);
        }
        if let Some((prev_rx, since)) = &entry.prev_rx
            && now.saturating_duration_since(*since) < REKEY_GRACE
            && let Ok(opened) = open(frame, prev_rx)
        {
            return Ok(opened);
        }
        if entry.epoch_keys.is_some() {
            return Err(weft_protocol::Error::DecryptionFailed);
        }
    }
    open(frame, static_rx)
}

/// Checks a packet's length before a counter is spent on it, with the
/// errors `seal` would give.
fn check_packet_len(json: &str) -> Result<()> {
    match json.len() {
        0 => Err(weft_protocol::Error::InvalidPlaintextLength(0).into()),
        len if len > MAX_PACKET_LEN => Err(weft_protocol::Error::PlaintextTooLarge(len).into()),
        _ => Ok(()),
    }
}

/// Trusts the members a Welcome from `owner` lists (this device excepted)
/// and records the Welcome in the replay state of `owner`.
fn trust_welcome(
    state: &mut State,
    identity: &Identity,
    owner: &[u8; 16],
    header: &Header,
    members: Vec<PeerKeys>,
) {
    let own_sign_pk = identity.sign_pk();
    for member in members {
        if member.sign_pk != own_sign_pk {
            state.trust(member);
        }
    }
    // Checked in `wait_welcome`, under the same keys.
    let _ = state
        .peer_mut(owner)
        .accept(header.counter, &header.message_id);
}

/// Applies an authenticated envelope to the state and describes it.
fn apply(
    state: &mut State,
    identity: &Identity,
    sender: [u8; 16],
    kind: Kind,
    plaintext: Vec<u8>,
) -> Event {
    let malformed = Event::Dropped {
        from: Some(sender),
        reason: DropReason::Malformed,
    };
    let own_sign_pk = identity.sign_pk();
    match kind {
        Kind::ClipboardText => match String::from_utf8(plaintext) {
            Ok(text) => Event::Clipboard { from: sender, text },
            Err(_) => malformed,
        },
        Kind::Ping => Event::Ping { from: sender },
        Kind::Welcome => {
            let Ok(members) = decode_welcome(&plaintext) else {
                return malformed;
            };
            let added = members
                .into_iter()
                .filter(|member| member.sign_pk != own_sign_pk)
                .filter(|member| state.trust(*member))
                .map(|member| member.device_id())
                .collect();
            Event::Welcome {
                from: sender,
                added,
            }
        }
        Kind::DeviceAdded => {
            let Ok(device) = decode_device_added(&plaintext) else {
                return malformed;
            };
            if device.sign_pk != own_sign_pk {
                state.trust(device);
            }
            Event::DeviceAdded {
                by: sender,
                device: device.device_id(),
            }
        }
        Kind::DeviceRemoved => {
            let Ok(device) = decode_device_removed(&plaintext) else {
                return malformed;
            };
            state.untrust(&device);
            Event::DeviceRemoved { by: sender, device }
        }
        Kind::DeviceName => {
            let Ok(name) = decode_device_name(&plaintext) else {
                return malformed;
            };
            // `handle_envelope` only gets here for a trusted sender.
            state.set_device_name(&sender, name.clone());
            Event::DeviceName { from: sender, name }
        }
        Kind::Packet => match String::from_utf8(plaintext) {
            Ok(json) => Event::Packet { from: sender, json },
            Err(_) => malformed,
        },
        Kind::FileChunk => {
            let Ok(chunk) = decode_file_chunk(&plaintext) else {
                return malformed;
            };
            Event::FileChunk {
                from: sender,
                transfer_id: chunk.transfer_id,
                offset: chunk.offset,
                data: chunk.data,
            }
        }
    }
}

/// Receives until `shutdown` completes, calling `on_event` for every event,
/// and reconnects with a backoff of 1 s doubling up to 30 s whenever the
/// connection is lost. Returns early only on errors a retry cannot fix
/// (state file, relay url, relay name).
pub async fn listen(
    dir: &StateDir,
    mut on_event: impl FnMut(&Event),
    shutdown: impl Future<Output = ()>,
) -> Result<()> {
    tokio::pin!(shutdown);
    let mut backoff = MIN_BACKOFF;
    loop {
        let mut connected = false;
        tokio::select! {
            () = &mut shutdown => return Ok(()),
            result = listen_once(dir, &mut on_event, &mut connected) => {
                let Err(err) = result;
                if is_fatal(&err) {
                    return Err(err);
                }
                if connected {
                    backoff = MIN_BACKOFF;
                }
                tracing::warn!(error = %err, "connection lost; retrying in {}s", backoff.as_secs());
            }
        }
        tokio::select! {
            () = &mut shutdown => return Ok(()),
            () = tokio::time::sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn listen_once(
    dir: &StateDir,
    on_event: &mut impl FnMut(&Event),
    connected: &mut bool,
) -> Result<Infallible> {
    let mut session = Session::connect(dir.clone()).await?;
    *connected = true;
    tracing::info!(
        device = %short_id(&session.device_id()),
        devices = session.state().devices.len(),
        "listening for clipboard text"
    );
    loop {
        match session.next_event(Some(KEEPALIVE_INTERVAL)).await? {
            Some(event) => on_event(&event),
            // Idle: make sure the connection is still alive.
            None => session.settle().await?,
        }
    }
}

/// Whether `err` is one a reconnection cannot fix (state file, relay url,
/// relay name), as opposed to a lost or refused connection.
pub fn is_fatal(err: &Error) -> bool {
    matches!(
        err,
        Error::Io { .. }
            | Error::InvalidState(..)
            | Error::NotInitialised(_)
            | Error::InvalidUrl(..)
            | Error::Protocol(weft_protocol::Error::RelayNameMismatch)
    )
}

#[cfg(test)]
mod tests {
    use weft_protocol::{RekeyKeyPair, derive_epoch_keys};

    use super::*;

    #[test]
    fn static_envelopes_stay_on_static_keys_while_live_envelopes_follow_epochs() {
        let a = Identity::from_seeds([1; 32], [2; 32]);
        let b = Identity::from_seeds([3; 32], [4; 32]);
        let static_a = session_keys(&a, &b.device_id(), &b.kx_pk()).unwrap();
        let static_b = session_keys(&b, &a.device_id(), &a.kx_pk()).unwrap();

        let ephem_a = RekeyKeyPair::generate();
        let ephem_b = RekeyKeyPair::generate();
        let epoch1_a = derive_epoch_keys(
            &static_a,
            1,
            &a.device_id(),
            &b.device_id(),
            &ephem_a,
            ephem_b.public_key(),
        )
        .unwrap();
        let epoch1_b = derive_epoch_keys(
            &static_b,
            1,
            &b.device_id(),
            &a.device_id(),
            &ephem_b,
            ephem_a.public_key(),
        )
        .unwrap();

        let now = Instant::now();
        let make_frame = |kind: Kind, counter: u64, plaintext: &[u8], tx: &[u8; 32]| {
            let header = Header {
                version: VERSION,
                kind,
                sender: a.device_id(),
                recipient: b.device_id(),
                message_id: [counter as u8; 16],
                counter,
            };
            seal(&header, plaintext, tx).unwrap()
        };

        let static_text = make_frame(Kind::ClipboardText, 1, b"before", &static_a.tx);
        let epoch1_text = make_frame(Kind::ClipboardText, 2, b"after", &epoch1_a.tx);
        let static_name = make_frame(Kind::DeviceName, 3, b"Laptop", &static_a.tx);
        let wrong_epoch_name = make_frame(Kind::DeviceName, 4, b"Laptop", &epoch1_a.tx);

        // Before epoch activation: static live envelopes open cleanly.
        assert_eq!(
            open_with_peer_keys(&static_text, Kind::ClipboardText, &static_b.rx, None, now)
                .unwrap()
                .1,
            b"before"
        );

        // Epoch 1 activated with `prev_rx` holding static_b.rx for 30s.
        let mut peer_epoch = PeerEpoch {
            epoch_keys: Some((1, epoch1_b)),
            staged: None,
            prev_rx: Some((static_b.rx, now)),
            sent_frames: 0,
            activated_at: Some(now),
        };

        // Epoch-encrypted live envelope opens first.
        assert_eq!(
            open_with_peer_keys(
                &epoch1_text,
                Kind::ClipboardText,
                &static_b.rx,
                Some(&peer_epoch),
                now,
            )
            .unwrap()
            .1,
            b"after"
        );
        // In-flight static live envelope still opens during the 30s grace window.
        assert_eq!(
            open_with_peer_keys(
                &static_text,
                Kind::ClipboardText,
                &static_b.rx,
                Some(&peer_epoch),
                now + Duration::from_secs(10),
            )
            .unwrap()
            .1,
            b"before"
        );
        // Once the 30s grace window expires, static live envelopes are rejected.
        let after_grace = now + REKEY_GRACE;
        peer_epoch.expire_prev(after_grace);
        assert!(peer_epoch.prev_rx.is_none());
        assert_eq!(
            open_with_peer_keys(
                &static_text,
                Kind::ClipboardText,
                &static_b.rx,
                Some(&peer_epoch),
                after_grace,
            ),
            Err(weft_protocol::Error::DecryptionFailed)
        );
        // Offline-eligible DeviceName (0x06) STILL opens with static keys!
        assert_eq!(
            open_with_peer_keys(
                &static_name,
                Kind::DeviceName,
                &static_b.rx,
                Some(&peer_epoch),
                after_grace,
            )
            .unwrap()
            .1,
            b"Laptop"
        );
        // And a DeviceName encrypted with epoch keys is rejected.
        assert_eq!(
            open_with_peer_keys(
                &wrong_epoch_name,
                Kind::DeviceName,
                &static_b.rx,
                Some(&peer_epoch),
                after_grace,
            ),
            Err(weft_protocol::Error::DecryptionFailed)
        );
    }
}
