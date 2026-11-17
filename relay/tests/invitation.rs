//! End-to-end tests of invitations (`docs/protocol.md`, section 5,
//! « Invitation »): `0xB5` is relayed like `0xB1`, `0xB4` moves the slot
//! owner into the inviter's group.

mod common;

use weft_protocol::{
    ControlError, ControlFrame, Identity, MAX_GROUP_SIZE, PairRequest, random_pairing_id,
    random_pairing_secret,
};
use weft_relay::PAIR_REQUEST_PER_MINUTE;

use common::{Device, invite, pair, request_invite, request_join, start_relay};

#[tokio::test]
async fn invitation_moves_the_slot_owner_into_the_inviters_group() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    let mut c = Device::connect(&url).await;
    pair(&mut b, &mut c).await;

    let (pairing_id, secret) = (random_pairing_id(), random_pairing_secret());
    a.open_slot(pairing_id).await;
    let invitation = b.pair_invite(&secret, pairing_id);
    b.send(invitation.clone()).await;
    // Forwarded unchanged, still an invitation.
    let received = a.recv().await;
    assert_eq!(received, invitation);
    assert!(matches!(
        ControlFrame::decode(&received).unwrap(),
        ControlFrame::PairInvite(_)
    ));

    // Not accepted yet: A is in no group.
    a.send_undeliverable(&b).await;
    a.send_control(ControlFrame::GroupJoin { device_id: b.id() })
        .await;
    a.settle().await;
    a.assert_delivers(&mut b, "A to B").await;
    b.assert_delivers(&mut a, "B to A").await;
    a.assert_delivers(&mut c, "A to C").await;
    c.assert_delivers(&mut a, "C to A").await;
    b.assert_delivers(&mut c, "B and C remain").await;

    // The slot was closed by the invitation and the pending join consumed.
    let mut d = Device::connect(&url).await;
    d.send(d.pair_request(&secret, pairing_id)).await;
    d.expect_error(ControlError::UnknownPairing).await;
    a.send_control(ControlFrame::GroupJoin { device_id: b.id() })
        .await;
    a.expect_error(ControlError::NotAllowed).await;
}

#[tokio::test]
async fn invitation_from_a_device_without_group_creates_one() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    invite(&mut a, &mut b).await;
    a.assert_delivers(&mut b, "A to B").await;
    b.assert_delivers(&mut a, "B to A").await;
}

#[tokio::test]
async fn refuses_group_join_without_invitation() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    let mut c = Device::connect(&url).await;

    // No request at all.
    a.send_control(ControlFrame::GroupJoin { device_id: b.id() })
        .await;
    a.expect_error(ControlError::NotAllowed).await;

    // B invited A: that lets neither C join B, nor B join A, nor A join C.
    request_invite(&mut a, &mut b).await;
    c.send_control(ControlFrame::GroupJoin { device_id: b.id() })
        .await;
    c.expect_error(ControlError::NotAllowed).await;
    b.send_control(ControlFrame::GroupJoin { device_id: a.id() })
        .await;
    b.expect_error(ControlError::NotAllowed).await;
    a.send_control(ControlFrame::GroupJoin { device_id: c.id() })
        .await;
    a.expect_error(ControlError::NotAllowed).await;
    a.send_undeliverable(&b).await;

    // Accepted once, then used up.
    a.send_control(ControlFrame::GroupJoin { device_id: b.id() })
        .await;
    a.settle().await;
    a.send_control(ControlFrame::GroupJoin { device_id: b.id() })
        .await;
    a.expect_error(ControlError::NotAllowed).await;
    a.assert_delivers(&mut b, "invited").await;
}

/// The relay cannot tell a join request from an invitation, so either
/// pending entry allows either answer: the intent is bound by the MAC, which
/// only the slot owner checks.
#[tokio::test]
async fn pending_join_is_the_same_for_both_frames() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    let mut c = Device::connect(&url).await;
    let mut d = Device::connect(&url).await;

    request_join(&mut a, &mut b).await;
    a.send_control(ControlFrame::GroupJoin { device_id: b.id() })
        .await;
    a.settle().await;
    b.assert_delivers(&mut a, "B to A").await;

    request_invite(&mut c, &mut d).await;
    c.send_control(ControlFrame::GroupAdd { device_id: d.id() })
        .await;
    c.settle().await;
    c.assert_delivers(&mut d, "C to D").await;
}

#[tokio::test]
async fn refuses_invitation_with_another_device_key() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    let other = Identity::generate();
    let (pairing_id, secret) = (random_pairing_id(), random_pairing_secret());
    a.open_slot(pairing_id).await;

    let forged = PairRequest::invite(&secret, pairing_id, other.sign_pk(), other.kx_pk());
    b.send_control(ControlFrame::PairInvite(forged)).await;
    b.expect_error(ControlError::NotAllowed).await;

    // Nothing was forwarded and the slot is still open.
    let genuine = b.pair_invite(&secret, pairing_id);
    b.send(genuine.clone()).await;
    assert_eq!(a.recv().await, genuine);

    // Single use, whichever frame comes next.
    let mut c = Device::connect(&url).await;
    c.send(c.pair_request(&secret, pairing_id)).await;
    c.expect_error(ControlError::UnknownPairing).await;
    c.send(c.pair_invite(&secret, pairing_id)).await;
    c.expect_error(ControlError::UnknownPairing).await;
    a.settle().await;
}

#[tokio::test]
async fn invited_device_leaves_its_previous_group() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    let mut c = Device::connect(&url).await;
    let mut x = Device::connect(&url).await;
    let mut y = Device::connect(&url).await;
    // A is in a group of three with X and Y; B is in a group with C.
    pair(&mut a, &mut x).await;
    pair(&mut a, &mut y).await;
    pair(&mut b, &mut c).await;

    invite(&mut a, &mut b).await;
    a.assert_delivers(&mut c, "A to C").await;
    a.send_undeliverable(&x).await;
    y.send_undeliverable(&a).await;
    // X and Y keep their group.
    x.assert_delivers(&mut y, "X to Y").await;

    // Invited again elsewhere, A leaves B's group; B and C remain.
    let mut d = Device::connect(&url).await;
    let mut e = Device::connect(&url).await;
    pair(&mut d, &mut e).await;
    invite(&mut a, &mut d).await;
    a.assert_delivers(&mut e, "A to E").await;
    a.send_undeliverable(&b).await;
    b.assert_delivers(&mut c, "B to C").await;
}

#[tokio::test]
async fn group_of_two_is_deleted_when_the_invited_device_leaves() {
    let url = start_relay().await;
    let mut a = Device::connect(&url).await;
    let mut x = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    pair(&mut a, &mut x).await;

    invite(&mut a, &mut b).await;
    a.assert_delivers(&mut b, "A to B").await;
    // X would be alone: its group is gone.
    x.send_undeliverable(&a).await;
    x.send_control(ControlFrame::GroupRemove { device_id: a.id() })
        .await;
    x.expect_error(ControlError::NotAllowed).await;
}

#[tokio::test]
async fn refuses_group_join_into_a_full_group() {
    let url = start_relay().await;
    let mut members = vec![Device::connect(&url).await];
    while members.len() < MAX_GROUP_SIZE {
        let mut joiner = Device::connect(&url).await;
        pair(members.last_mut().unwrap(), &mut joiner).await;
        members.push(joiner);
    }
    let (first, rest) = members.split_first_mut().unwrap();
    let last = rest.last_mut().unwrap();

    let mut extra = Device::connect(&url).await;
    request_invite(&mut extra, last).await;
    let join = ControlFrame::GroupJoin {
        device_id: last.id(),
    };
    extra.send_control(join.clone()).await;
    extra.expect_error(ControlError::GroupFull).await;
    extra.send_undeliverable(last).await;

    // Once there is room, the same pending join can be used.
    last.send_control(ControlFrame::GroupRemove {
        device_id: first.id(),
    })
    .await;
    last.settle().await;
    extra.send_control(join).await;
    extra.settle().await;
    extra.assert_delivers(last, "room was made").await;
}

#[tokio::test]
async fn invitations_share_the_pair_request_quota() {
    let url = start_relay().await;
    let mut b = Device::connect(&url).await;
    let secret = random_pairing_secret();
    for i in 0..PAIR_REQUEST_PER_MINUTE {
        let frame = if i % 2 == 0 {
            b.pair_invite(&secret, random_pairing_id())
        } else {
            b.pair_request(&secret, random_pairing_id())
        };
        b.send(frame).await;
        b.expect_error(ControlError::UnknownPairing).await;
    }
    b.send(b.pair_invite(&secret, random_pairing_id())).await;
    b.expect_error(ControlError::RateLimited).await;
    b.send(b.pair_request(&secret, random_pairing_id())).await;
    b.expect_error(ControlError::RateLimited).await;
}
