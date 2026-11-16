//! End-to-end tests: start the relay on an ephemeral port and talk to it with
//! a WebSocket client, as a device would.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use weft_protocol::{
    AUTH_ACCEPTED, AuthResponse, Challenge, Header, Identity, Kind, open, seal, session_keys,
    sign_auth,
};
use weft_relay::{MAX_ENVELOPE_LEN, MIN_ENVELOPE_LEN};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

const RELAY_NAME: &str = "relay.test";
const STEP: Duration = Duration::from_secs(5);

async fn start_relay() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(weft_relay::serve(listener, RELAY_NAME.to_owned()));
    format!("ws://{addr}/v1")
}

/// What the client sees next on a connection.
#[derive(Debug, PartialEq)]
enum Event {
    Binary(Vec<u8>),
    Closed,
    Timeout,
}

async fn next_event(ws: &mut Ws, wait: Duration) -> Event {
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

async fn recv_binary(ws: &mut Ws) -> Vec<u8> {
    match next_event(ws, STEP).await {
        Event::Binary(frame) => frame,
        other => panic!("expected a binary frame, got {other:?}"),
    }
}

async fn assert_closed(ws: &mut Ws) {
    assert_eq!(next_event(ws, STEP).await, Event::Closed);
}

async fn send(ws: &mut Ws, frame: Vec<u8>) {
    ws.send(Message::Binary(frame.into())).await.unwrap();
}

/// Opens a connection and reads the relay challenge.
async fn connect(url: &str) -> (Ws, Challenge) {
    let (mut ws, _) = tokio::time::timeout(STEP, tokio_tungstenite::connect_async(url))
        .await
        .expect("connect timed out")
        .expect("connect failed");
    let challenge = Challenge::decode(&recv_binary(&mut ws).await).expect("invalid challenge");
    (ws, challenge)
}

/// Connects and authenticates `identity`, asserting the relay accepts it.
async fn connect_authenticated(url: &str, identity: &Identity) -> Ws {
    let (mut ws, challenge) = connect(url).await;
    assert_eq!(challenge.relay_name(), RELAY_NAME);
    send(&mut ws, sign_auth(identity, &challenge).encode().to_vec()).await;
    assert_eq!(recv_binary(&mut ws).await, [AUTH_ACCEPTED]);
    ws
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

fn clipboard(sender: &Identity, recipient: &Identity, text: &str, counter: u64) -> Vec<u8> {
    let keys = session_keys(sender, &recipient.device_id(), &recipient.kx_pk()).unwrap();
    seal(
        &header(sender, recipient, counter),
        text.as_bytes(),
        &keys.tx,
    )
    .unwrap()
}

fn open_from(recipient: &Identity, sender: &Identity, frame: &[u8]) -> (Header, Vec<u8>) {
    let keys = session_keys(recipient, &sender.device_id(), &sender.kx_pk()).unwrap();
    open(frame, &keys.rx).unwrap()
}

#[tokio::test]
async fn forwards_envelope_unchanged() {
    let url = start_relay().await;
    let (a, b) = (Identity::generate(), Identity::generate());
    let mut ws_a = connect_authenticated(&url, &a).await;
    let mut ws_b = connect_authenticated(&url, &b).await;

    let frame = clipboard(&a, &b, "hello from A", 1);
    send(&mut ws_a, frame.clone()).await;

    let received = recv_binary(&mut ws_b).await;
    assert_eq!(received, frame);
    let (header, plaintext) = open_from(&b, &a, &received);
    assert_eq!(header.sender, a.device_id());
    assert_eq!(plaintext, b"hello from A");
}

#[tokio::test]
async fn drops_envelope_with_forged_sender() {
    let url = start_relay().await;
    let (a, b, c) = (
        Identity::generate(),
        Identity::generate(),
        Identity::generate(),
    );
    let mut ws_a = connect_authenticated(&url, &a).await;
    let mut ws_b = connect_authenticated(&url, &b).await;

    // A sends an envelope claiming to come from C, then a genuine one.
    send(&mut ws_a, clipboard(&c, &b, "forged", 1)).await;
    let genuine = clipboard(&a, &b, "genuine", 1);
    send(&mut ws_a, genuine.clone()).await;

    assert_eq!(recv_binary(&mut ws_b).await, genuine);
    assert_eq!(
        next_event(&mut ws_b, Duration::from_millis(300)).await,
        Event::Timeout
    );
}

#[tokio::test]
async fn refuses_bad_signature() {
    let url = start_relay().await;
    let a = Identity::generate();
    let (mut ws, challenge) = connect(&url).await;
    let mut response = sign_auth(&a, &challenge);
    response.signature[0] ^= 0x01;
    send(&mut ws, response.encode().to_vec()).await;
    assert_closed(&mut ws).await;
}

#[tokio::test]
async fn refuses_signature_for_another_relay_name() {
    let url = start_relay().await;
    let a = Identity::generate();
    let (mut ws, challenge) = connect(&url).await;
    let other = Challenge::new("evil.example", challenge.nonce()).unwrap();
    let response: AuthResponse = sign_auth(&a, &other);
    send(&mut ws, response.encode().to_vec()).await;
    assert_closed(&mut ws).await;
}

#[tokio::test]
async fn closes_on_envelope_before_auth() {
    let url = start_relay().await;
    let (a, b) = (Identity::generate(), Identity::generate());
    let (mut ws, _) = connect(&url).await;
    send(&mut ws, clipboard(&a, &b, "too early", 1)).await;
    assert_closed(&mut ws).await;
}

#[tokio::test]
async fn closes_on_oversized_frame() {
    let url = start_relay().await;
    let a = Identity::generate();
    let mut ws = connect_authenticated(&url, &a).await;
    send(&mut ws, vec![0u8; MAX_ENVELOPE_LEN + 1]).await;
    assert_closed(&mut ws).await;
}

#[tokio::test]
async fn drops_too_short_frame() {
    let url = start_relay().await;
    let (a, b) = (Identity::generate(), Identity::generate());
    let mut ws_a = connect_authenticated(&url, &a).await;
    let mut ws_b = connect_authenticated(&url, &b).await;

    // A valid header with a truncated body, then a genuine envelope.
    let mut short = clipboard(&a, &b, "", 1);
    short.truncate(MIN_ENVELOPE_LEN - 1);
    send(&mut ws_a, short).await;
    let genuine = clipboard(&a, &b, "genuine", 2);
    send(&mut ws_a, genuine.clone()).await;

    assert_eq!(recv_binary(&mut ws_b).await, genuine);
}

#[tokio::test]
async fn new_connection_replaces_older_one() {
    let url = start_relay().await;
    let (a, b) = (Identity::generate(), Identity::generate());
    let mut ws_a1 = connect_authenticated(&url, &a).await;
    let mut ws_a2 = connect_authenticated(&url, &a).await;
    assert_closed(&mut ws_a1).await;

    let mut ws_b = connect_authenticated(&url, &b).await;
    let frame = clipboard(&b, &a, "to the new connection", 1);
    send(&mut ws_b, frame.clone()).await;
    assert_eq!(recv_binary(&mut ws_a2).await, frame);
}
