//! Helpers shared by the end-to-end tests: start the relay on an ephemeral
//! port and talk to it with a WebSocket client, as a device would.

// Each test binary uses its own subset of these helpers.
#![allow(dead_code)]

use std::io;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use weft_protocol::{
    AUTH_ACCEPTED, Challenge, ControlError, ControlFrame, Header, Identity, Kind, PairRequest,
    open, random_pairing_id, random_pairing_secret, seal, session_keys, sign_auth,
};
use weft_relay::Config;

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub const RELAY_NAME: &str = "relay.test";
pub const STEP: Duration = Duration::from_secs(5);

/// A relay with the protocol durations and groups kept in memory.
pub fn memory_config() -> Config {
    Config::new(RELAY_NAME, ":memory:")
}

/// A relay running in the background, which can be shut down.
pub struct TestRelay {
    pub url: String,
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<io::Result<()>>,
}

impl TestRelay {
    pub async fn start(config: Config) -> TestRelay {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown, stopped) = oneshot::channel::<()>();
        let task = tokio::spawn(weft_relay::serve_with_shutdown(listener, config, async {
            let _ = stopped.await;
        }));
        TestRelay {
            url: format!("ws://{addr}/v1"),
            shutdown,
            task,
        }
    }

    /// Stops the relay and waits until it has returned.
    pub async fn shutdown(self) {
        self.shutdown.send(()).unwrap();
        tokio::time::timeout(STEP, self.task)
            .await
            .expect("relay did not shut down")
            .unwrap()
            .unwrap();
    }
}

/// Starts a relay that runs until the test ends, and returns its url.
pub async fn start_relay() -> String {
    start_relay_with(memory_config()).await
}

pub async fn start_relay_with(config: Config) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(weft_relay::serve(listener, config));
    format!("ws://{addr}/v1")
}

/// What the client sees next on a connection.
#[derive(Debug, PartialEq)]
pub enum Event {
    Binary(Vec<u8>),
    Closed,
    Timeout,
}

pub async fn next_event(ws: &mut Ws, wait: Duration) -> Event {
    let read = async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Binary(frame))) => return Event::Binary(frame.to_vec()),
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return Event::Closed,
                Some(Ok(other)) => panic!("unexpected message from relay: {other:?}"),
            }
        }
    };
    tokio::time::timeout(wait, read)
        .await
        .unwrap_or(Event::Timeout)
}

pub async fn recv_binary(ws: &mut Ws) -> Vec<u8> {
    match next_event(ws, STEP).await {
        Event::Binary(frame) => frame,
        other => panic!("expected a binary frame, got {other:?}"),
    }
}

pub async fn assert_closed(ws: &mut Ws) {
    assert_eq!(next_event(ws, STEP).await, Event::Closed);
}

pub async fn send(ws: &mut Ws, frame: Vec<u8>) {
    ws.send(Message::Binary(frame.into())).await.unwrap();
}

/// Opens a connection and reads the relay challenge.
pub async fn connect(url: &str) -> (Ws, Challenge) {
    let (mut ws, _) = tokio::time::timeout(STEP, tokio_tungstenite::connect_async(url))
        .await
        .expect("connect timed out")
        .expect("connect failed");
    let challenge = Challenge::decode(&recv_binary(&mut ws).await).expect("invalid challenge");
    (ws, challenge)
}

/// Connects and authenticates `identity`, asserting the relay accepts it.
pub async fn connect_authenticated(url: &str, identity: &Identity) -> Ws {
    let (mut ws, challenge) = connect(url).await;
    assert_eq!(challenge.relay_name(), RELAY_NAME);
    send(&mut ws, sign_auth(identity, &challenge).encode().to_vec()).await;
    assert_eq!(recv_binary(&mut ws).await, [AUTH_ACCEPTED]);
    ws
}

/// Waits until the relay has handled every frame sent so far on `ws`.
///
/// The relay handles the frames of a connection in order and answers a ping
/// when it reads it, so the pong can only come after the earlier frames were
/// handled. Panics if a binary frame arrives in the meantime: read expected
/// frames before calling this.
pub async fn settle(ws: &mut Ws) {
    ws.send(Message::Ping(b"settle".as_slice().into()))
        .await
        .unwrap();
    let pong = async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Pong(_))) => return,
                Some(Ok(Message::Ping(_))) => continue,
                other => panic!("unexpected message while waiting for a pong: {other:?}"),
            }
        }
    };
    tokio::time::timeout(STEP, pong)
        .await
        .expect("no pong from the relay");
}

fn header(sender: &Identity, recipient: &Identity, counter: u64) -> Header {
    let mut message_id = [0u8; 16];
    message_id[8..].copy_from_slice(&counter.to_be_bytes());
    Header {
        version: weft_protocol::VERSION,
        kind: Kind::ClipboardText,
        sender: sender.device_id(),
        recipient: recipient.device_id(),
        message_id,
        counter,
    }
}

pub fn clipboard(sender: &Identity, recipient: &Identity, text: &str, counter: u64) -> Vec<u8> {
    let keys = session_keys(sender, &recipient.device_id(), &recipient.kx_pk()).unwrap();
    seal(
        &header(sender, recipient, counter),
        text.as_bytes(),
        &keys.tx,
    )
    .unwrap()
}

pub fn open_from(recipient: &Identity, sender: &Identity, frame: &[u8]) -> (Header, Vec<u8>) {
    let keys = session_keys(recipient, &sender.device_id(), &sender.kx_pk()).unwrap();
    open(frame, &keys.rx).unwrap()
}

/// A fixed identity, for a device that connects more than once.
pub fn seeded_identity(seed: u8) -> Identity {
    Identity::from_seeds([seed; 32], [seed.wrapping_add(1); 32])
}

/// A connected, authenticated device.
pub struct Device {
    pub identity: Identity,
    pub ws: Ws,
}

impl Device {
    /// Connects a device with a fresh identity.
    pub async fn connect(url: &str) -> Device {
        Device::connect_as(url, Identity::generate()).await
    }

    pub async fn connect_as(url: &str, identity: Identity) -> Device {
        let ws = connect_authenticated(url, &identity).await;
        Device { identity, ws }
    }

    pub fn id(&self) -> [u8; 16] {
        self.identity.device_id()
    }

    pub async fn send(&mut self, frame: Vec<u8>) {
        send(&mut self.ws, frame).await;
    }

    pub async fn send_control(&mut self, frame: ControlFrame) {
        self.send(frame.encode()).await;
    }

    pub async fn recv(&mut self) -> Vec<u8> {
        recv_binary(&mut self.ws).await
    }

    pub async fn settle(&mut self) {
        settle(&mut self.ws).await;
    }

    /// Asserts the next frame is the relay error `expected`.
    pub async fn expect_error(&mut self, expected: ControlError) {
        let frame = self.recv().await;
        assert_eq!(
            ControlFrame::decode(&frame).ok(),
            Some(ControlFrame::Error(expected))
        );
    }

    /// Opens the pairing slot `pairing_id` and waits until it is open.
    pub async fn open_slot(&mut self, pairing_id: [u8; 16]) {
        self.send_control(ControlFrame::PairOpen { pairing_id })
            .await;
        self.settle().await;
    }

    /// The encoded pair request this device sends for the code
    /// (`pairing_id`, `secret`).
    pub fn pair_request(&self, secret: &[u8; 32], pairing_id: [u8; 16]) -> Vec<u8> {
        let request = PairRequest::new(
            secret,
            pairing_id,
            self.identity.sign_pk(),
            self.identity.kx_pk(),
        );
        ControlFrame::PairRequest(request).encode()
    }

    /// Asserts that `text`, sent in an envelope, reaches `recipient`
    /// unchanged and readable.
    pub async fn assert_delivers(&mut self, recipient: &mut Device, text: &str) {
        let frame = clipboard(&self.identity, &recipient.identity, text, 1);
        self.send(frame.clone()).await;
        let received = recipient.recv().await;
        assert_eq!(received, frame);
        let (_, plaintext) = open_from(&recipient.identity, &self.identity, &received);
        assert_eq!(plaintext, text.as_bytes());
    }

    /// Sends an envelope the relay must drop, and waits until it has been
    /// handled. A later frame for `recipient` then proves it was not
    /// delivered, since frames are delivered in order.
    pub async fn send_undeliverable(&mut self, recipient: &Device) {
        self.send(clipboard(
            &self.identity,
            &recipient.identity,
            "must not arrive",
            1,
        ))
        .await;
        self.settle().await;
    }
}

/// First half of a pairing: `owner` opens a slot, `joiner` asks to join and
/// `owner` receives the request unchanged.
pub async fn request_join(owner: &mut Device, joiner: &mut Device) {
    let (pairing_id, secret) = (random_pairing_id(), random_pairing_secret());
    owner.open_slot(pairing_id).await;
    let request = joiner.pair_request(&secret, pairing_id);
    joiner.send(request.clone()).await;
    assert_eq!(owner.recv().await, request);
}

/// Puts `joiner` in the group of `owner` through a full pairing.
pub async fn pair(owner: &mut Device, joiner: &mut Device) {
    request_join(owner, joiner).await;
    let device_id = joiner.id();
    owner
        .send_control(ControlFrame::GroupAdd { device_id })
        .await;
    owner.settle().await;
}
