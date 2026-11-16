//! End-to-end tests of authentication and envelope forwarding.

mod common;

use std::time::Duration;

use futures_util::SinkExt;
use tokio_tungstenite::tungstenite::Message;
use weft_protocol::{
    AuthResponse, Challenge, ControlError, ControlFrame, Identity, Kind, MAX_FILE_CHUNK_DATA_LEN,
    MAX_PACKET_LEN, encode_file_chunk, sign_auth,
};
use weft_relay::{MAX_ENVELOPE_LEN, MIN_ENVELOPE_LEN};

use common::{
    Device, Event, assert_closed, clipboard, connect, connect_authenticated, envelope, next_event,
    open_from, pair, seeded_identity, send, start_relay,
};

#[tokio::test]
async fn forwards_envelope_unchanged() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    pair(&mut a, &mut b).await;

    let frame = clipboard(&a.identity, &b.identity, "hello from A", 1);
    a.send(frame.clone()).await;

    let received = b.recv().await;
    assert_eq!(received, frame);
    let (header, plaintext) = open_from(&b.identity, &a.identity, &received);
    assert_eq!(header.sender, a.id());
    assert_eq!(plaintext, b"hello from A");
}

#[tokio::test]
async fn forwards_envelope_of_unknown_kind() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    pair(&mut a, &mut b).await;

    // The relay routes on the header without checking the kind, so kinds
    // added later need no relay update.
    let mut frame = clipboard(&a.identity, &b.identity, "from the future", 1);
    frame[1] = 0x7F;
    a.send(frame.clone()).await;
    assert_eq!(b.recv().await, frame);
}

#[tokio::test]
async fn forwards_largest_packet_and_file_chunk() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    pair(&mut a, &mut b).await;

    let packet = vec![b'a'; MAX_PACKET_LEN];
    let chunk = encode_file_chunk(&[9; 16], 0, &vec![0xab; MAX_FILE_CHUNK_DATA_LEN]).unwrap();
    for (counter, (kind, plaintext)) in [(Kind::Packet, packet), (Kind::FileChunk, chunk)]
        .into_iter()
        .enumerate()
    {
        let frame = envelope(
            &a.identity,
            &b.identity,
            kind,
            &plaintext,
            counter as u64 + 1,
        );
        assert!(frame.len() <= MAX_ENVELOPE_LEN);
        a.send(frame.clone()).await;
        let received = b.recv().await;
        assert_eq!(received, frame);
        assert_eq!(open_from(&b.identity, &a.identity, &received).1, plaintext);
    }
}

#[tokio::test]
async fn drops_envelope_with_forged_sender() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    let mut c = Device::connect(&url).await;
    // All three share a group, so only the forged sender can get it dropped.
    pair(&mut a, &mut b).await;
    pair(&mut a, &mut c).await;

    // A sends an envelope claiming to come from C, then a genuine one.
    a.send(clipboard(&c.identity, &b.identity, "forged", 1))
        .await;
    let genuine = clipboard(&a.identity, &b.identity, "genuine", 1);
    a.send(genuine.clone()).await;

    assert_eq!(b.recv().await, genuine);
    assert_eq!(
        next_event(&mut b.ws, Duration::from_millis(300)).await,
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
async fn closes_on_text_frame() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    a.ws.send(Message::Text("hello".into())).await.unwrap();
    assert_closed(&mut a.ws).await;
}

#[tokio::test]
async fn drops_too_short_frame() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    pair(&mut a, &mut b).await;

    // A valid header with a truncated body, then a genuine envelope.
    let mut short = clipboard(&a.identity, &b.identity, "", 1);
    short.truncate(MIN_ENVELOPE_LEN - 1);
    a.send(short).await;
    let genuine = clipboard(&a.identity, &b.identity, "genuine", 2);
    a.send(genuine.clone()).await;

    assert_eq!(b.recv().await, genuine);
}

#[tokio::test]
async fn drops_unknown_and_malformed_frames() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    pair(&mut a, &mut b).await;

    // Unknown first bytes, an empty frame, a reserved control byte, a
    // truncated control frame and an error frame, which only the relay sends.
    for junk in [
        vec![0x00, 1, 2, 3],
        vec![0x02; 120],
        vec![],
        vec![0xB7; 17],
        vec![0xB0; 5],
        ControlFrame::Error(ControlError::NotAllowed).encode(),
    ] {
        a.send(junk).await;
    }
    // Nothing was answered, and the connection still works.
    a.settle().await;
    a.assert_delivers(&mut b, "still here").await;
}

#[tokio::test]
async fn new_connection_replaces_older_one() {
    let url = start_relay().await;
    let mut a1 = Device::connect_as(&url, seeded_identity(1)).await;
    let mut a2 = Device::connect_as(&url, seeded_identity(1)).await;
    assert_closed(&mut a1.ws).await;

    let mut b = Device::connect(&url).await;
    pair(&mut a2, &mut b).await;
    b.assert_delivers(&mut a2, "to the new connection").await;
}
