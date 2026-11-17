//! End-to-end tests of offline envelope delivery (`docs/protocol.md`,
//! sections 4 and 5, « Offline delivery »).

mod common;

use std::path::PathBuf;
use std::time::Duration;

use weft_protocol::{
    ControlFrame, DEVICE_ID_LEN, Identity, Kind, MAX_DEVICE_NAME_LEN, MAX_WELCOME_DEVICES,
    PEER_KEYS_LEN, PeerKeys, VERSION, decode_device_added, decode_device_name,
    decode_device_removed, decode_welcome, encode_device_added, encode_device_name,
    encode_device_removed, encode_file_chunk, encode_welcome,
};
use weft_relay::{Config, MIN_ENVELOPE_LEN};

use common::{
    Device, Event, RELAY_NAME, TestRelay, assert_closed, clipboard, envelope, invite,
    memory_config, next_event, open_from, pair, seeded_identity, start_relay, start_relay_with,
};

const QUIET: Duration = Duration::from_millis(120);

fn peer_keys(identity: &Identity) -> PeerKeys {
    PeerKeys {
        sign_pk: identity.sign_pk(),
        kx_pk: identity.kx_pk(),
    }
}

/// Builds an unencrypted-looking raw frame with a valid route header (`version`,
/// `kind`, `sender`, `recipient`) and exact `frame_len`, to test relay length
/// checks directly.
fn raw_envelope(sender: &Identity, recipient: &Identity, kind: u8, frame_len: usize) -> Vec<u8> {
    assert!(frame_len >= MIN_ENVELOPE_LEN);
    let mut frame = vec![0u8; frame_len];
    frame[0] = VERSION;
    frame[1] = kind;
    frame[2..18].copy_from_slice(&sender.device_id());
    frame[18..34].copy_from_slice(&recipient.device_id());
    frame
}

async fn disconnect(mut device: Device) {
    device.ws.close(None).await.unwrap();
    assert_closed(&mut device.ws).await;
}

/// A database file under the build directory, removed when the test ends.
struct TempDb(PathBuf);

impl TempDb {
    fn new(name: &str) -> TempDb {
        let file = format!("{name}-{}.sqlite3", std::process::id());
        let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(file);
        let _ = std::fs::remove_file(&path);
        TempDb(path)
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[tokio::test]
async fn invitation_while_member_is_offline_delivers_group_and_names_in_order() {
    let url = start_relay().await;
    let a = Device::connect_as(&url, seeded_identity(10)).await;
    let mut b = Device::connect_as(&url, seeded_identity(20)).await;
    let mut a = a;
    pair(&mut a, &mut b).await;
    disconnect(a).await;

    // B invites C while A is offline.
    let mut c = Device::connect_as(&url, seeded_identity(30)).await;
    invite(&mut c, &mut b).await;

    let a_id = seeded_identity(10);
    let c_id = seeded_identity(30);

    // B tells offline A about C, and sends C the Welcome listing A.
    let added_c = envelope(
        &b.identity,
        &a_id,
        Kind::DeviceAdded,
        &encode_device_added(&peer_keys(&c_id)),
        1,
    );
    b.send(added_c.clone()).await;
    let welcome_for_c = envelope(
        &b.identity,
        &c_id,
        Kind::Welcome,
        &encode_welcome(&[peer_keys(&a_id)]).unwrap(),
        1,
    );
    b.send(welcome_for_c.clone()).await;
    assert_eq!(c.recv().await, welcome_for_c);
    b.settle().await;

    // C sends its DeviceName to offline A, then goes offline too.
    let name_c = envelope(
        &c.identity,
        &a_id,
        Kind::DeviceName,
        &encode_device_name("Desktop").unwrap(),
        1,
    );
    c.send(name_c.clone()).await;
    c.settle().await;
    disconnect(c).await;

    // A reconnects: receives DeviceAdded(C) then DeviceName("Desktop") in FIFO order.
    let mut a = Device::connect_as(&url, seeded_identity(10)).await;
    let first = a.recv().await;
    let second = a.recv().await;
    assert_eq!(first, added_c);
    assert_eq!(second, name_c);

    let (h1, p1) = open_from(&a.identity, &b.identity, &first);
    assert_eq!(h1.kind, Kind::DeviceAdded);
    assert_eq!(decode_device_added(&p1).unwrap(), peer_keys(&c_id));

    let (h2, p2) = open_from(&a.identity, &c_id, &second);
    assert_eq!(h2.kind, Kind::DeviceName);
    assert_eq!(decode_device_name(&p2).unwrap(), "Desktop");

    // A answers with its own DeviceName while C is still offline.
    let name_a = envelope(
        &a.identity,
        &c_id,
        Kind::DeviceName,
        &encode_device_name("Phone").unwrap(),
        1,
    );
    a.send(name_a.clone()).await;
    a.settle().await;

    // C reconnects: receives DeviceName("Phone"), and A and C now exchange clipboard text.
    let mut c = Device::connect_as(&url, seeded_identity(30)).await;
    let from_a = c.recv().await;
    assert_eq!(from_a, name_a);
    let (h3, p3) = open_from(&c.identity, &a.identity, &from_a);
    assert_eq!(h3.kind, Kind::DeviceName);
    assert_eq!(decode_device_name(&p3).unwrap(), "Phone");

    a.assert_delivers(&mut c, "clip from A to C").await;
    c.assert_delivers(&mut a, "clip from C to A").await;

    // Reconnecting again delivers nothing: queued rows were deleted on send.
    disconnect(c).await;
    let mut c = Device::connect_as(&url, seeded_identity(30)).await;
    assert_eq!(next_event(&mut c.ws, QUIET).await, Event::Timeout);
}

#[tokio::test]
async fn welcome_is_queued_for_offline_recipient_up_to_maximum_members() {
    let url = start_relay().await;
    let mut a = Device::connect_as(&url, seeded_identity(1)).await;
    let mut b = Device::connect_as(&url, seeded_identity(2)).await;
    pair(&mut a, &mut b).await;
    disconnect(b).await;

    let b_id = seeded_identity(2);
    let empty_welcome = envelope(
        &a.identity,
        &b_id,
        Kind::Welcome,
        &encode_welcome(&[]).unwrap(),
        1,
    );
    let max_members: Vec<PeerKeys> = (0..MAX_WELCOME_DEVICES)
        .map(|i| peer_keys(&seeded_identity(50 + i as u8 * 2)))
        .collect();
    let full_welcome = envelope(
        &a.identity,
        &b_id,
        Kind::Welcome,
        &encode_welcome(&max_members).unwrap(),
        2,
    );
    a.send(empty_welcome.clone()).await;
    a.send(full_welcome.clone()).await;
    a.settle().await;

    let mut b = Device::connect_as(&url, seeded_identity(2)).await;
    let got_empty = b.recv().await;
    let got_full = b.recv().await;
    assert_eq!(
        decode_welcome(&open_from(&b.identity, &a.identity, &got_empty).1).unwrap(),
        Vec::<PeerKeys>::new()
    );
    assert_eq!(
        decode_welcome(&open_from(&b.identity, &a.identity, &got_full).1).unwrap(),
        max_members
    );
}

#[tokio::test]
async fn removal_delivers_to_offline_member_and_wipes_removed_recipients_queue() {
    let url = start_relay().await;
    let mut a = Device::connect_as(&url, seeded_identity(10)).await;
    let mut b = Device::connect_as(&url, seeded_identity(20)).await;
    let mut c = Device::connect_as(&url, seeded_identity(30)).await;
    pair(&mut a, &mut b).await;
    pair(&mut a, &mut c).await;

    let a_id = seeded_identity(10);
    let c_id = seeded_identity(30);
    disconnect(a).await;
    disconnect(c).await;

    // Queue an envelope for offline C before removing C.
    let queued_for_c = envelope(
        &b.identity,
        &c_id,
        Kind::DeviceName,
        &encode_device_name("Laptop").unwrap(),
        1,
    );
    b.send(queued_for_c).await;
    b.settle().await;

    // B removes C on the relay and notifies offline A.
    b.send_control(ControlFrame::GroupRemove {
        device_id: c_id.device_id(),
    })
    .await;
    let removed_c = envelope(
        &b.identity,
        &a_id,
        Kind::DeviceRemoved,
        &encode_device_removed(&c_id.device_id()),
        1,
    );
    b.send(removed_c.clone()).await;
    b.settle().await;

    // C's queued envelope was wiped when C left the group.
    let mut c = Device::connect_as(&url, seeded_identity(30)).await;
    assert_eq!(next_event(&mut c.ws, QUIET).await, Event::Timeout);

    // Offline A receives DeviceRemoved(C) on reconnect.
    let mut a = Device::connect_as(&url, seeded_identity(10)).await;
    let got = a.recv().await;
    assert_eq!(got, removed_c);
    let (header, plaintext) = open_from(&a.identity, &b.identity, &got);
    assert_eq!(header.kind, Kind::DeviceRemoved);
    assert_eq!(decode_device_removed(&plaintext).unwrap(), c_id.device_id());
    assert_eq!(next_event(&mut a.ws, QUIET).await, Event::Timeout);
}

#[tokio::test]
async fn drops_non_queued_kinds_and_malformed_lengths_when_recipient_is_offline() {
    let url = start_relay().await;
    let mut a = Device::connect_as(&url, seeded_identity(10)).await;
    let mut b = Device::connect_as(&url, seeded_identity(20)).await;
    pair(&mut a, &mut b).await;
    disconnect(b).await;

    let b_id = seeded_identity(20);

    // Valid envelopes of kinds that are never stored offline.
    a.send(clipboard(&a.identity, &b_id, "not stored", 1)).await;
    a.send(envelope(&a.identity, &b_id, Kind::Ping, b"", 2))
        .await;
    a.send(envelope(
        &a.identity,
        &b_id,
        Kind::Packet,
        br#"{"type":"ping"}"#,
        3,
    ))
    .await;
    a.send(envelope(
        &a.identity,
        &b_id,
        Kind::FileChunk,
        &encode_file_chunk(&[7; 16], 0, b"payload").unwrap(),
        4,
    ))
    .await;
    // Unknown future kind.
    a.send(raw_envelope(&a.identity, &b_id, 0x7F, MIN_ENVELOPE_LEN + 8))
        .await;

    // Malformed lengths for 0x03..=0x06.
    for (kind, bad_len) in [
        (Kind::Welcome as u8, MIN_ENVELOPE_LEN),
        (Kind::Welcome as u8, MIN_ENVELOPE_LEN + 2),
        (
            Kind::Welcome as u8,
            MIN_ENVELOPE_LEN + 1 + (MAX_WELCOME_DEVICES + 1) * PEER_KEYS_LEN,
        ),
        (
            Kind::DeviceAdded as u8,
            MIN_ENVELOPE_LEN + PEER_KEYS_LEN - 1,
        ),
        (
            Kind::DeviceAdded as u8,
            MIN_ENVELOPE_LEN + PEER_KEYS_LEN + 1,
        ),
        (
            Kind::DeviceRemoved as u8,
            MIN_ENVELOPE_LEN + DEVICE_ID_LEN - 1,
        ),
        (
            Kind::DeviceRemoved as u8,
            MIN_ENVELOPE_LEN + DEVICE_ID_LEN + 1,
        ),
        (Kind::DeviceName as u8, MIN_ENVELOPE_LEN),
        (
            Kind::DeviceName as u8,
            MIN_ENVELOPE_LEN + MAX_DEVICE_NAME_LEN + 1,
        ),
    ] {
        a.send(raw_envelope(&a.identity, &b_id, kind, bad_len))
            .await;
    }

    // One valid queued envelope at the end proves all earlier ones were dropped.
    let valid_name = envelope(
        &a.identity,
        &b_id,
        Kind::DeviceName,
        &encode_device_name("Anchor").unwrap(),
        5,
    );
    a.send(valid_name.clone()).await;
    a.settle().await;

    let mut b = Device::connect_as(&url, seeded_identity(20)).await;
    assert_eq!(b.recv().await, valid_name);
    assert_eq!(next_event(&mut b.ws, QUIET).await, Event::Timeout);
}

#[tokio::test]
async fn deduplicates_device_name_per_sender_and_enforces_cap_and_ttl() {
    // 1. DeviceName deduplication per (sender, recipient) pair.
    let url = start_relay().await;
    let mut a = Device::connect_as(&url, seeded_identity(10)).await;
    let mut b = Device::connect_as(&url, seeded_identity(20)).await;
    let mut c = Device::connect_as(&url, seeded_identity(30)).await;
    pair(&mut a, &mut b).await;
    pair(&mut a, &mut c).await;
    disconnect(b).await;

    let b_id = seeded_identity(20);
    let added = envelope(
        &a.identity,
        &b_id,
        Kind::DeviceAdded,
        &encode_device_added(&peer_keys(&seeded_identity(99))),
        1,
    );
    a.send(envelope(
        &a.identity,
        &b_id,
        Kind::DeviceName,
        &encode_device_name("Old A").unwrap(),
        2,
    ))
    .await;
    a.send(added.clone()).await;
    a.settle().await;

    let name_c = envelope(
        &c.identity,
        &b_id,
        Kind::DeviceName,
        &encode_device_name("Peer C").unwrap(),
        1,
    );
    c.send(name_c.clone()).await;
    c.settle().await;

    let latest_a = envelope(
        &a.identity,
        &b_id,
        Kind::DeviceName,
        &encode_device_name("Latest A").unwrap(),
        3,
    );
    a.send(latest_a.clone()).await;
    a.settle().await;

    let mut b = Device::connect_as(&url, seeded_identity(20)).await;
    assert_eq!(b.recv().await, added);
    assert_eq!(b.recv().await, name_c);
    assert_eq!(b.recv().await, latest_a);
    assert_eq!(next_event(&mut b.ws, QUIET).await, Event::Timeout);

    // 2. Per-device cap evicts the oldest queued envelopes.
    let capped_url = start_relay_with(Config {
        max_offline_per_device: 3,
        ..memory_config()
    })
    .await;
    let mut a = Device::connect_as(&capped_url, seeded_identity(10)).await;
    let mut b = Device::connect_as(&capped_url, seeded_identity(20)).await;
    pair(&mut a, &mut b).await;
    disconnect(b).await;

    let mut sent = Vec::new();
    for i in 1..=5u8 {
        let frame = envelope(
            &a.identity,
            &b_id,
            Kind::DeviceRemoved,
            &encode_device_removed(&[i; 16]),
            u64::from(i),
        );
        a.send(frame.clone()).await;
        sent.push(frame);
    }
    a.settle().await;

    let mut b = Device::connect_as(&capped_url, seeded_identity(20)).await;
    assert_eq!(b.recv().await, sent[2]);
    assert_eq!(b.recv().await, sent[3]);
    assert_eq!(b.recv().await, sent[4]);
    assert_eq!(next_event(&mut b.ws, QUIET).await, Event::Timeout);

    // 3. TTL expiry discards stale envelopes.
    let ttl_url = start_relay_with(Config {
        offline_ttl: Duration::from_millis(80),
        ..memory_config()
    })
    .await;
    let mut a = Device::connect_as(&ttl_url, seeded_identity(10)).await;
    let mut b = Device::connect_as(&ttl_url, seeded_identity(20)).await;
    pair(&mut a, &mut b).await;
    disconnect(b).await;

    a.send(envelope(
        &a.identity,
        &b_id,
        Kind::DeviceName,
        &encode_device_name("Expired").unwrap(),
        1,
    ))
    .await;
    a.settle().await;
    tokio::time::sleep(Duration::from_millis(120)).await;

    let fresh = envelope(
        &a.identity,
        &b_id,
        Kind::DeviceRemoved,
        &encode_device_removed(&[42; 16]),
        2,
    );
    a.send(fresh.clone()).await;
    a.settle().await;

    let mut b = Device::connect_as(&ttl_url, seeded_identity(20)).await;
    assert_eq!(b.recv().await, fresh);
    assert_eq!(next_event(&mut b.ws, QUIET).await, Event::Timeout);
}

#[tokio::test]
async fn migrates_v0_3_database_and_persists_offline_envelopes_across_restart() {
    let db = TempDb::new("offline-v0-3-migration");
    let a_id = seeded_identity(10);
    let b_id = seeded_identity(20);
    let c_id = seeded_identity(30);

    // Seed a v0.3.0 SQLite database that only has `group_members`.
    {
        let conn = rusqlite::Connection::open(&db.0).unwrap();
        conn.execute_batch(
            "CREATE TABLE group_members (
                 device_id BLOB PRIMARY KEY,
                 group_id  INTEGER NOT NULL
             ) STRICT;",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO group_members (device_id, group_id) VALUES (?1, 1), (?2, 1)",
            (a_id.device_id(), b_id.device_id()),
        )
        .unwrap();
    }

    let relay = TestRelay::start(Config::new(RELAY_NAME, &db.0)).await;
    let mut a = Device::connect_as(&relay.url, seeded_identity(10)).await;
    let added_c = envelope(
        &a.identity,
        &b_id,
        Kind::DeviceAdded,
        &encode_device_added(&peer_keys(&c_id)),
        1,
    );
    let name_a = envelope(
        &a.identity,
        &b_id,
        Kind::DeviceName,
        &encode_device_name("Workstation").unwrap(),
        2,
    );
    a.send(added_c.clone()).await;
    a.send(name_a.clone()).await;
    a.settle().await;

    relay.shutdown().await;
    assert_closed(&mut a.ws).await;

    // Restart on the same database: both the v0.3.0 group and queued envelopes survive.
    let relay = TestRelay::start(Config::new(RELAY_NAME, &db.0)).await;
    let mut b = Device::connect_as(&relay.url, seeded_identity(20)).await;
    assert_eq!(b.recv().await, added_c);
    assert_eq!(b.recv().await, name_a);
    assert_eq!(next_event(&mut b.ws, QUIET).await, Event::Timeout);

    let mut a = Device::connect_as(&relay.url, seeded_identity(10)).await;
    a.assert_delivers(&mut b, "group intact after migration")
        .await;
    relay.shutdown().await;
}
