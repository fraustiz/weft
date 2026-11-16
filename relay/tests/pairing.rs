//! End-to-end tests of groups and pairing (`docs/protocol.md`, section 5).

mod common;

use std::path::PathBuf;
use std::time::Duration;

use weft_protocol::{
    ControlError, ControlFrame, Identity, MAX_GROUP_SIZE, PairRequest, random_pairing_id,
    random_pairing_secret,
};
use weft_relay::{Config, PAIR_OPEN_PER_MINUTE};

use common::{
    Device, RELAY_NAME, TestRelay, assert_closed, clipboard, memory_config, pair, request_join,
    seeded_identity, start_relay, start_relay_with,
};

/// Short enough to wait for in a test, long enough for the steps before it.
const SHORT_TTL: Duration = Duration::from_millis(300);

#[tokio::test]
async fn pairing_lets_envelopes_flow_both_ways() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    let (pairing_id, secret) = (random_pairing_id(), random_pairing_secret());

    a.open_slot(pairing_id).await;
    let request = b.pair_request(&secret, pairing_id);
    b.send(request.clone()).await;
    assert_eq!(a.recv().await, request);

    // Not confirmed yet: this envelope is dropped. B's first frame is then
    // the one sent after the confirmation.
    a.send(clipboard(&a.identity, &b.identity, "too early", 1))
        .await;
    a.send_control(ControlFrame::GroupAdd { device_id: b.id() })
        .await;
    a.assert_delivers(&mut b, "hello from A").await;
    b.assert_delivers(&mut a, "hello from B").await;
}

#[tokio::test]
async fn refuses_pair_request_with_another_device_key() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    let other = Identity::generate();
    let (pairing_id, secret) = (random_pairing_id(), random_pairing_secret());
    a.open_slot(pairing_id).await;

    let forged = PairRequest::new(&secret, pairing_id, other.sign_pk(), other.kx_pk());
    b.send_control(ControlFrame::PairRequest(forged)).await;
    b.expect_error(ControlError::NotAllowed).await;

    // Nothing was forwarded and the slot is still open: A's first frame is
    // B's genuine request.
    let genuine = b.pair_request(&secret, pairing_id);
    b.send(genuine.clone()).await;
    assert_eq!(a.recv().await, genuine);
}

#[tokio::test]
async fn refuses_unknown_pairing_id() {
    let url = start_relay().await;
    let mut b = Device::connect(&url).await;
    b.send(b.pair_request(&random_pairing_secret(), random_pairing_id()))
        .await;
    b.expect_error(ControlError::UnknownPairing).await;
}

#[tokio::test]
async fn pairing_slot_is_single_use() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    let mut c = Device::connect(&url).await;
    let (pairing_id, secret) = (random_pairing_id(), random_pairing_secret());
    a.open_slot(pairing_id).await;

    let request = b.pair_request(&secret, pairing_id);
    b.send(request.clone()).await;
    assert_eq!(a.recv().await, request);

    c.send(c.pair_request(&secret, pairing_id)).await;
    c.expect_error(ControlError::UnknownPairing).await;
    b.send(request).await;
    b.expect_error(ControlError::UnknownPairing).await;
    // A got nothing more.
    a.settle().await;
}

#[tokio::test]
async fn new_slot_replaces_the_previous_one() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    let secret = random_pairing_secret();
    let (first, second) = (random_pairing_id(), random_pairing_id());
    a.open_slot(first).await;
    a.open_slot(second).await;

    b.send(b.pair_request(&secret, first)).await;
    b.expect_error(ControlError::UnknownPairing).await;

    let request = b.pair_request(&secret, second);
    b.send(request.clone()).await;
    assert_eq!(a.recv().await, request);
}

#[tokio::test]
async fn refuses_pair_request_when_owner_is_offline() {
    let url = start_relay().await;
    let mut a = Device::connect_as(&url, seeded_identity(10)).await;
    let mut b = Device::connect(&url).await;
    let (pairing_id, secret) = (random_pairing_id(), random_pairing_secret());
    a.open_slot(pairing_id).await;
    a.ws.close(None).await.unwrap();
    assert_closed(&mut a.ws).await;

    // The relay forgets a connection before it drops its socket, so A is
    // known to be offline once its connection is seen closed.
    let request = b.pair_request(&secret, pairing_id);
    b.send(request.clone()).await;
    b.expect_error(ControlError::UnknownPairing).await;

    // The slot survives: once A is back, the request goes through.
    let mut a = Device::connect_as(&url, seeded_identity(10)).await;
    b.send(request.clone()).await;
    assert_eq!(a.recv().await, request);
}

#[tokio::test]
async fn refuses_group_add_without_pair_request() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    let mut c = Device::connect(&url).await;

    // No request at all.
    a.send_control(ControlFrame::GroupAdd { device_id: b.id() })
        .await;
    a.expect_error(ControlError::NotAllowed).await;

    // B asked A, which lets neither C add B, nor B add A, nor A add C.
    request_join(&mut a, &mut b).await;
    c.send_control(ControlFrame::GroupAdd { device_id: b.id() })
        .await;
    c.expect_error(ControlError::NotAllowed).await;
    b.send_control(ControlFrame::GroupAdd { device_id: a.id() })
        .await;
    b.expect_error(ControlError::NotAllowed).await;
    a.send_control(ControlFrame::GroupAdd { device_id: c.id() })
        .await;
    a.expect_error(ControlError::NotAllowed).await;

    // A pending join is used up by the confirmation.
    a.send_control(ControlFrame::GroupAdd { device_id: b.id() })
        .await;
    a.settle().await;
    a.send_control(ControlFrame::GroupAdd { device_id: b.id() })
        .await;
    a.expect_error(ControlError::NotAllowed).await;
    a.assert_delivers(&mut b, "paired").await;
}

#[tokio::test]
async fn third_device_joins_and_outsider_is_kept_out() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    let mut c = Device::connect(&url).await;
    let mut d = Device::connect(&url).await;
    pair(&mut a, &mut b).await;
    pair(&mut a, &mut c).await;

    a.assert_delivers(&mut c, "A to C").await;
    c.assert_delivers(&mut a, "C to A").await;
    b.assert_delivers(&mut c, "B to C").await;
    c.assert_delivers(&mut b, "C to B").await;

    // D is in no group; then D is in a group, but not A's.
    d.send_undeliverable(&a).await;
    let mut e = Device::connect(&url).await;
    pair(&mut d, &mut e).await;
    d.send_undeliverable(&a).await;
    a.send_undeliverable(&d).await;
    b.assert_delivers(&mut a, "B to A").await;
    e.assert_delivers(&mut d, "E to D").await;
}

#[tokio::test]
async fn joining_another_group_leaves_the_previous_one() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    let mut c = Device::connect(&url).await;
    let mut d = Device::connect(&url).await;
    pair(&mut a, &mut b).await;
    pair(&mut a, &mut c).await;

    // C moves to D's new group.
    pair(&mut d, &mut c).await;
    c.send_undeliverable(&a).await;
    a.send_undeliverable(&c).await;
    d.assert_delivers(&mut c, "D to C").await;
    b.assert_delivers(&mut a, "B to A").await;
}

#[tokio::test]
async fn group_remove_follows_membership() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    let mut c = Device::connect(&url).await;
    let mut stranger = Device::connect(&url).await;
    pair(&mut a, &mut b).await;
    pair(&mut a, &mut c).await;

    // A stranger cannot remove a member.
    stranger
        .send_control(ControlFrame::GroupRemove { device_id: c.id() })
        .await;
    stranger.expect_error(ControlError::NotAllowed).await;
    a.assert_delivers(&mut c, "C is still a member").await;

    // B removes another member, C: nothing flows to or from C any more.
    b.send_control(ControlFrame::GroupRemove { device_id: c.id() })
        .await;
    b.settle().await;
    c.send_undeliverable(&a).await;
    a.send_undeliverable(&c).await;
    // C cannot remove its former group's members either.
    c.send_control(ControlFrame::GroupRemove { device_id: a.id() })
        .await;
    c.expect_error(ControlError::NotAllowed).await;
    a.assert_delivers(&mut b, "A and B remain").await;
    b.assert_delivers(&mut a, "B to A").await;
    // The relay only reads the header: an envelope readdressed to its own
    // sender is routed like any other, as long as the sender has a group.
    let mut to_myself = clipboard(&b.identity, &a.identity, "to myself", 2);
    to_myself[18..34].copy_from_slice(&b.id());
    b.send(to_myself.clone()).await;
    assert_eq!(b.recv().await, to_myself);

    // A removes itself. B would be alone: the group is deleted.
    a.send_control(ControlFrame::GroupRemove { device_id: a.id() })
        .await;
    a.settle().await;
    a.send_undeliverable(&b).await;
    b.send_undeliverable(&a).await;
    // A member of a group can send to itself; B no longer can. The relay
    // delivers in order, so the pair request below would come second if the
    // envelope had been accepted.
    b.send(to_myself).await;
    let mut d = Device::connect(&url).await;
    pair(&mut b, &mut d).await;
    b.assert_delivers(&mut d, "new group").await;
}

#[tokio::test]
async fn removing_itself_without_a_group_is_harmless() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    a.send_control(ControlFrame::GroupRemove { device_id: a.id() })
        .await;
    // No error.
    a.settle().await;
}

#[tokio::test]
async fn refuses_device_beyond_group_size() {
    let url = start_relay().await;
    let mut members = vec![Device::connect(&url).await];
    // Each device is added by the previous one, so no connection comes near
    // its pairing quota.
    while members.len() < MAX_GROUP_SIZE {
        let mut joiner = Device::connect(&url).await;
        pair(members.last_mut().unwrap(), &mut joiner).await;
        members.push(joiner);
    }
    let (first, rest) = members.split_first_mut().unwrap();
    let last = rest.last_mut().unwrap();
    first.assert_delivers(last, "first to last").await;

    let mut extra = Device::connect(&url).await;
    request_join(last, &mut extra).await;
    let add = ControlFrame::GroupAdd {
        device_id: extra.id(),
    };
    last.send_control(add.clone()).await;
    last.expect_error(ControlError::GroupFull).await;
    last.send_undeliverable(&extra).await;

    // Once there is room, the same pending join can be confirmed.
    last.send_control(ControlFrame::GroupRemove {
        device_id: first.id(),
    })
    .await;
    last.send_control(add).await;
    last.assert_delivers(&mut extra, "room was made").await;
}

#[tokio::test]
async fn rate_limits_pair_open() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    for _ in 0..PAIR_OPEN_PER_MINUTE {
        a.send_control(ControlFrame::PairOpen {
            pairing_id: random_pairing_id(),
        })
        .await;
    }
    // None of these was refused.
    a.settle().await;

    let pairing_id = random_pairing_id();
    a.send_control(ControlFrame::PairOpen { pairing_id }).await;
    a.expect_error(ControlError::RateLimited).await;

    // The refused slot was not opened, and the quota is per connection.
    let mut b = Device::connect(&url).await;
    b.send(b.pair_request(&random_pairing_secret(), pairing_id))
        .await;
    b.expect_error(ControlError::UnknownPairing).await;
    b.open_slot(random_pairing_id()).await;
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
async fn groups_survive_a_restart() {
    let db = TempDb::new("groups-survive-a-restart");
    let relay = TestRelay::start(Config::new(RELAY_NAME, &db.0)).await;
    let mut a = Device::connect_as(&relay.url, seeded_identity(20)).await;
    let mut b = Device::connect_as(&relay.url, seeded_identity(30)).await;
    let mut c = Device::connect_as(&relay.url, seeded_identity(40)).await;
    pair(&mut a, &mut b).await;
    pair(&mut a, &mut c).await;
    // A removal must survive too.
    a.send_control(ControlFrame::GroupRemove { device_id: c.id() })
        .await;
    a.settle().await;

    relay.shutdown().await;
    assert_closed(&mut a.ws).await;
    assert_closed(&mut b.ws).await;

    let relay = TestRelay::start(Config::new(RELAY_NAME, &db.0)).await;
    let mut a = Device::connect_as(&relay.url, seeded_identity(20)).await;
    let mut b = Device::connect_as(&relay.url, seeded_identity(30)).await;
    let mut c = Device::connect_as(&relay.url, seeded_identity(40)).await;
    c.send_undeliverable(&a).await;
    a.assert_delivers(&mut b, "A to B after restart").await;
    b.assert_delivers(&mut a, "B to A after restart").await;
    relay.shutdown().await;
}

#[tokio::test]
async fn pairing_slot_expires() {
    let config = Config {
        pairing_slot_ttl: SHORT_TTL,
        ..memory_config()
    };
    let url = start_relay_with(config).await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    let (pairing_id, secret) = (random_pairing_id(), random_pairing_secret());
    a.open_slot(pairing_id).await;

    tokio::time::sleep(SHORT_TTL + Duration::from_millis(50)).await;
    b.send(b.pair_request(&secret, pairing_id)).await;
    b.expect_error(ControlError::UnknownPairing).await;
    // Nothing reached A.
    a.settle().await;
}

#[tokio::test]
async fn pending_join_expires() {
    let config = Config {
        pending_join_ttl: SHORT_TTL,
        ..memory_config()
    };
    let url = start_relay_with(config).await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    request_join(&mut a, &mut b).await;

    tokio::time::sleep(SHORT_TTL + Duration::from_millis(50)).await;
    a.send_control(ControlFrame::GroupAdd { device_id: b.id() })
        .await;
    a.expect_error(ControlError::NotAllowed).await;
    a.send_undeliverable(&b).await;
    b.settle().await;
}
