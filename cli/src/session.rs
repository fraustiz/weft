//! A connection to the relay, authenticated as this device
//! (`docs/protocol.md`, sections 3 to 5).
//!
//! A [`Session`] owns the WebSocket and reads the state from its
//! [`StateDir`]: every change (trust, counters, replay state) goes through
//! [`StateDir::update`], so it is on disk before anything depends on it.

use std::collections::VecDeque;
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
    Kind, MAX_CLIPBOARD_TEXT_LEN, MAX_GROUP_SIZE, MAX_RELAY_URL_LEN, PairRequest, PairingCode,
    PeerKeys, VERSION, check_relay_name, classify, decode_device_added, decode_device_removed,
    decode_welcome, device_id_from_sign_pk, encode_device_added, encode_device_removed,
    encode_welcome, open, random_message_id, random_pairing_id, random_pairing_secret, seal,
    session_keys, sign_auth, verify_pair_request,
};

use crate::state::{ReplayError, State, StateDir, TrustedDevice};
use crate::{Error, Result, short_id};

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// How long the TCP, TLS and WebSocket handshakes, then the relay
/// authentication, may take.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a pairing code stays valid (spec §5).
pub const PAIRING_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// How long a joining device waits for the Welcome envelope.
pub const JOIN_TIMEOUT: Duration = Duration::from_secs(30);
/// How long the relay has to answer a WebSocket ping.
pub const SETTLE_TIMEOUT: Duration = Duration::from_secs(10);
/// Idle time after which `listen` checks the connection is still alive.
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
const MIN_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

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
    identity: Identity,
    ws: Ws,
    /// Frames read ahead (while waiting for a pong or a pairing message)
    /// and not handled yet.
    pending: VecDeque<Vec<u8>>,
}

impl Session {
    /// Loads the state from `dir`, connects to its relay and authenticates.
    pub async fn connect(dir: StateDir) -> Result<Session> {
        let state = dir.load()?;
        let identity = state.identity();
        let ws = connect_ws(&state.relay_url, &identity).await?;
        tracing::debug!(relay = %state.relay_url, device = %short_id(&identity.device_id()), "connected");
        Ok(Session {
            dir,
            state,
            identity,
            ws,
            pending: VecDeque::new(),
        })
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
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

    /// Seals `plaintext` for `peer` and sends it. The counter is incremented
    /// and saved before the envelope leaves.
    async fn send_envelope(
        &mut self,
        peer: &TrustedDevice,
        kind: Kind,
        plaintext: &[u8],
    ) -> Result<()> {
        let peer_id = peer.device_id();
        let keys = session_keys(&self.identity, &peer_id, &peer.kx_pk)?;
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
        let frame = seal(&header, plaintext, &keys.tx)?;
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

    /// Waits for the next frame and handles it. Returns `None` if `timeout`
    /// passes first; `None` as the timeout waits forever.
    pub async fn next_event(&mut self, timeout: Option<Duration>) -> Result<Option<Event>> {
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        match self.next_frame(deadline).await? {
            Some(frame) => self.handle_frame(&frame).map(Some),
            None => Ok(None),
        }
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
        let identity = &self.identity;
        let (event, state) = self.dir.update(|state| {
            let Some(device) = state.device(&sender) else {
                return dropped(DropReason::UnknownSender);
            };
            let Ok(keys) = session_keys(identity, &sender, &device.kx_pk) else {
                return dropped(DropReason::Undecryptable);
            };
            let Ok((header, plaintext)) = open(frame, &keys.rx) else {
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

    /// Waits for a valid pair request for `code`, then adds the device to
    /// the group: trust, `0xB2`, Welcome to it and Device added to the other
    /// members. Requests with another pairing id or a wrong MAC are ignored
    /// (with a warning) and the wait goes on. Returns the new device's id.
    pub async fn wait_pair_request(
        &mut self,
        code: &PairingCode,
        timeout: Duration,
        name: Option<String>,
    ) -> Result<[u8; 16]> {
        let deadline = Instant::now() + timeout;
        let mut deferred = Vec::new();
        let result = self
            .wait_pair_request_until(code, deadline, &mut deferred)
            .await;
        self.requeue(deferred);
        self.accept_join(result?, name).await
    }

    async fn wait_pair_request_until(
        &mut self,
        code: &PairingCode,
        deadline: Instant,
        deferred: &mut Vec<Vec<u8>>,
    ) -> Result<PairRequest> {
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
                            // The relay closed the slot when it forwarded the
                            // request: reopen it so the code keeps working.
                            let pairing_id = code.pairing_id;
                            self.send_frame(ControlFrame::PairOpen { pairing_id }.encode())
                                .await?;
                        } else if request.sign_pk == self.identity.sign_pk() {
                            tracing::warn!("ignored a pair request from this device");
                        } else {
                            return Ok(request);
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

    async fn accept_join(
        &mut self,
        request: PairRequest,
        name: Option<String>,
    ) -> Result<[u8; 16]> {
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
            state.trust(keys, name);
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
        self.send_envelope(&new_device, Kind::Welcome, &encode_welcome(&members)?)
            .await?;
        for other in &others {
            self.send_envelope(other, Kind::DeviceAdded, &encode_device_added(&keys))
                .await?;
        }
        self.settle().await?;
        Ok(joiner)
    }

    /// Joins the group of the device that showed `code`: sends the pair
    /// request, waits for its Welcome, then trusts it and the members it
    /// lists. Devices trusted before (a previous group) are dropped, since
    /// the relay takes this device out of its previous group. Nothing is
    /// saved unless the Welcome arrives. Returns the trusted devices.
    pub async fn join(
        &mut self,
        code: &PairingCode,
        timeout: Duration,
        name: Option<String>,
    ) -> Result<Vec<[u8; 16]>> {
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
        let result = self.wait_welcome(code, deadline, &mut deferred).await;
        self.requeue(deferred);
        let (header, members) = result?;

        let owner = PeerKeys {
            sign_pk: code.sign_pk,
            kx_pk: code.kx_pk,
        };
        let owner_id = owner.device_id();
        let own_sign_pk = self.identity.sign_pk();
        let (dropped, state) = self.dir.update(|state| {
            let before: Vec<TrustedDevice> = std::mem::take(&mut state.devices);
            state.trust(owner, name);
            for member in members {
                if member.sign_pk != own_sign_pk {
                    state.trust(member, None);
                }
            }
            // Checked in `wait_welcome`, under the same keys.
            let _ = state
                .peer_mut(&owner_id)
                .accept(header.counter, &header.message_id);
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
        Ok(self.state.devices.iter().map(|d| d.device_id()).collect())
    }

    async fn wait_welcome(
        &mut self,
        code: &PairingCode,
        deadline: Instant,
        deferred: &mut Vec<Vec<u8>>,
    ) -> Result<(Header, Vec<PeerKeys>)> {
        let owner_id = device_id_from_sign_pk(&code.sign_pk);
        let keys = session_keys(&self.identity, &owner_id, &code.kx_pk)?;
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
                .filter(|member| state.trust(*member, None))
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
                state.trust(device, None);
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

fn is_fatal(err: &Error) -> bool {
    matches!(
        err,
        Error::Io { .. }
            | Error::InvalidState(..)
            | Error::NotInitialised(_)
            | Error::InvalidUrl(..)
            | Error::Protocol(weft_protocol::Error::RelayNameMismatch)
    )
}
