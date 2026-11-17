//! Invitations (`docs/protocol.md`, section 5, « Invitation »): a device in
//! a group scans the code of a device in none and brings it into its group.

mod common;

use std::process::Stdio;
use std::time::Duration;

use common::{
    QUIET, STEP, TestDevice, expect_name, expect_nothing, expect_text, name_of, next_event, pair,
    sorted, start_relay, timed, trusted,
};
use tokio::io::{AsyncBufReadExt, BufReader};
use weft_cli::{Error, Event, PairOutcome, Session};
use weft_protocol::{ControlFrame, PairRequest, PairingCode};

/// How long an inviter waits in the tests where nobody answers.
const SHORT: Duration = Duration::from_secs(1);

/// `inviter` invites the device showing `code`, which waits for it: returns
/// both sides' results.
async fn invite(
    owner: &mut Session,
    inviter: &mut Session,
    code: &PairingCode,
) -> (PairOutcome, [u8; 16]) {
    let (outcome, invited) = tokio::join!(
        owner.wait_pair_request(code, STEP),
        inviter.invite(code, STEP)
    );
    (
        outcome.expect("owner side of the invitation"),
        invited.expect("inviter side of the invitation"),
    )
}

/// Handles events until `session` has the names of all of `from`.
async fn expect_names(session: &mut Session, from: &[&Session]) {
    let ids: Vec<[u8; 16]> = from.iter().map(|device| device.device_id()).collect();
    assert!(
        session.wait_names(&ids, STEP, |_| {}).await.unwrap(),
        "names missing"
    );
    for device in from {
        assert_eq!(
            name_of(session, &device.device_id()).as_deref(),
            Some(device.state().name())
        );
    }
}

/// Asserts that the next event is `by` announcing `device`.
async fn expect_device_added(session: &mut Session, by: &Session, device: &Session) {
    assert_eq!(
        next_event(session).await,
        Event::DeviceAdded {
            by: by.device_id(),
            device: device.device_id(),
        }
    );
}

#[tokio::test]
async fn owner_accepts_an_invitation_into_a_group() {
    timed(async {
        let url = start_relay().await;
        let a_dev = TestDevice::named(&url, "Laptop");
        let b_dev = TestDevice::named(&url, "Phone");
        let c_dev = TestDevice::named(&url, "Tablet");
        let d_dev = TestDevice::named(&url, "Desktop");
        let mut a = a_dev.connect().await;
        let mut b = b_dev.connect().await;
        let mut c = c_dev.connect().await;
        // B and C: a group of two.
        pair(&mut b, &mut c).await;

        // A shows a code; B, in a group, invites A into it.
        let code = a.open_pairing().await.unwrap();
        let (outcome, invited) = invite(&mut a, &mut b, &code).await;
        assert_eq!(invited, a.device_id());
        assert_eq!(
            outcome,
            PairOutcome::Invited {
                inviter: b.device_id(),
                devices: vec![b.device_id(), c.device_id()],
            }
        );
        assert_eq!(outcome.device(), b.device_id());
        // B had A's name before it sent the Welcome.
        assert_eq!(name_of(&b, &a.device_id()).as_deref(), Some("Laptop"));

        // C learns about A from B, and answers with its name; A sends its
        // name to C once it has the Welcome.
        expect_device_added(&mut c, &b, &a).await;
        expect_name(&mut c, &a).await;
        // A gets B's name (after the Welcome) and C's.
        expect_names(&mut a, &[&b, &c]).await;

        assert_eq!(trusted(&a), sorted(vec![b.device_id(), c.device_id()]));
        assert_eq!(trusted(&b), sorted(vec![a.device_id(), c.device_id()]));
        assert_eq!(trusted(&c), sorted(vec![a.device_id(), b.device_id()]));
        expect_nothing(&mut b).await;

        // Clipboard text flows between A and C, which never met.
        a.send_text("from the laptop").await.unwrap();
        expect_text(&mut c, &a, "from the laptop").await;
        expect_text(&mut b, &a, "from the laptop").await;
        c.send_text("from the tablet").await.unwrap();
        expect_text(&mut a, &c, "from the tablet").await;
        expect_text(&mut b, &c, "from the tablet").await;

        // A fourth device, invited by C this time: the Welcome lists two
        // members.
        let mut d = d_dev.connect().await;
        let code = d.open_pairing().await.unwrap();
        let (outcome, invited) = invite(&mut d, &mut c, &code).await;
        assert_eq!(invited, d.device_id());
        let PairOutcome::Invited { inviter, devices } = outcome else {
            panic!("not an invitation: {outcome:?}");
        };
        assert_eq!(inviter, c.device_id());
        assert_eq!(devices[0], c.device_id());
        assert_eq!(
            sorted(devices),
            sorted(vec![a.device_id(), b.device_id(), c.device_id()])
        );
        for member in [&mut a, &mut b] {
            expect_device_added(member, &c, &d).await;
            expect_name(member, &d).await;
        }
        expect_names(&mut d, &[&a, &b, &c]).await;
        assert_eq!(
            trusted(&d),
            sorted(vec![a.device_id(), b.device_id(), c.device_id()])
        );
        d.send_text("from the desktop").await.unwrap();
        for member in [&mut a, &mut b, &mut c] {
            expect_text(member, &d, "from the desktop").await;
        }
        a.send_text("to everyone").await.unwrap();
        expect_text(&mut d, &a, "to everyone").await;
    })
    .await;
}

#[tokio::test]
async fn device_in_a_group_ignores_invitations() {
    timed(async {
        let url = start_relay().await;
        let devices: Vec<_> = (0..5).map(|_| TestDevice::new(&url)).collect();
        let mut a = devices[0].connect().await;
        let mut x = devices[1].connect().await;
        let mut b = devices[2].connect().await;
        let mut c = devices[3].connect().await;
        pair(&mut a, &mut x).await;
        pair(&mut b, &mut c).await;

        // A, already in a group with X, shows a code; B invites it.
        let code = a.open_pairing().await.unwrap();
        let (waited, invited) = tokio::join!(
            a.wait_pair_request(&code, SHORT + QUIET),
            b.invite(&code, SHORT)
        );
        assert!(matches!(waited, Err(Error::Timeout(_))), "{waited:?}");
        let err = invited.unwrap_err();
        assert!(matches!(err, Error::NoAnswer), "{err:?}");
        assert!(
            err.to_string().contains("may already belong to a group"),
            "{err}"
        );
        // Nothing changed on either side.
        assert_eq!(trusted(&a), vec![x.device_id()]);
        assert_eq!(trusted(&b), vec![c.device_id()]);
        expect_nothing(&mut c).await;
        b.send_text("still B and C").await.unwrap();
        expect_text(&mut c, &b, "still B and C").await;
        expect_nothing(&mut a).await;

        // A reopened its slot: the code still works for a join.
        let mut d = devices[4].connect().await;
        let (added, joined) = tokio::join!(a.wait_pair_request(&code, STEP), d.join(&code, STEP));
        assert_eq!(
            added.unwrap(),
            PairOutcome::Joined {
                device: d.device_id()
            }
        );
        joined.unwrap();
        assert_eq!(trusted(&a), sorted(vec![x.device_id(), d.device_id()]));
    })
    .await;
}

/// A relay that changes `0xB5` into `0xB1` (or the other way round) cannot
/// turn an invitation into a join request: the MAC binds the intent.
#[tokio::test]
async fn retagged_requests_are_ignored() {
    timed(async {
        let url = start_relay().await;
        let devices: Vec<_> = (0..3).map(|_| TestDevice::new(&url)).collect();
        let mut a = devices[0].connect().await;
        let mut b = devices[1].connect().await;
        let mut c = devices[2].connect().await;
        pair(&mut b, &mut c).await;
        let code = a.open_pairing().await.unwrap();
        let (sign_pk, kx_pk) = (b.identity().sign_pk(), b.identity().kx_pk());
        let invite_mac = PairRequest::invite(&code.secret, code.pairing_id, sign_pk, kx_pk);
        let join_mac = PairRequest::new(&code.secret, code.pairing_id, sign_pk, kx_pk);

        // What A would get from a relay that flipped the first byte, built
        // here by hand: an invitation's body as a join request, and a join
        // request's body as an invitation.
        let mut flipped_invite = ControlFrame::PairInvite(invite_mac.clone()).encode();
        flipped_invite[0] = 0xB1;
        let mut flipped_join = ControlFrame::PairRequest(join_mac).encode();
        flipped_join[0] = 0xB5;
        for frame in [flipped_invite, flipped_join] {
            b.send_frame(frame).await.unwrap();
            b.settle().await.unwrap();
            let waited = a.wait_pair_request(&code, QUIET).await;
            assert!(matches!(waited, Err(Error::Timeout(_))), "{waited:?}");
            // A reopened the slot the relay closed when forwarding.
            a.settle().await.unwrap();
            assert!(trusted(&a).is_empty());
            expect_nothing(&mut b).await;
        }
        assert_eq!(trusted(&b), vec![c.device_id()]);

        // The genuine invitation still goes through.
        let (outcome, invited) = invite(&mut a, &mut b, &code).await;
        assert_eq!(invited, a.device_id());
        assert_eq!(outcome.device(), b.device_id());
        assert_eq!(trusted(&a), sorted(vec![b.device_id(), c.device_id()]));
    })
    .await;
}

#[tokio::test]
async fn inviter_without_a_group_refuses() {
    timed(async {
        let url = start_relay().await;
        let (a_dev, b_dev) = (TestDevice::new(&url), TestDevice::new(&url));
        let mut a = a_dev.connect().await;
        let mut b = b_dev.connect().await;
        let code = a.open_pairing().await.unwrap();

        let err = b.invite(&code, STEP).await.unwrap_err();
        assert!(matches!(err, Error::NoGroup), "{err:?}");
        assert!(err.to_string().contains("join"), "{err}");
        assert!(trusted(&b).is_empty());
        // Nothing was sent: A is still waiting, and its code still works.
        let waited = a.wait_pair_request(&code, QUIET).await;
        assert!(matches!(waited, Err(Error::Timeout(_))), "{waited:?}");
        let (added, joined) = tokio::join!(a.wait_pair_request(&code, STEP), b.join(&code, STEP));
        assert_eq!(added.unwrap().device(), b.device_id());
        joined.unwrap();

        // Its own code cannot be used either.
        let own = b.open_pairing().await.unwrap();
        assert!(matches!(b.invite(&own, STEP).await, Err(Error::OwnCode)));
    })
    .await;
}

/// `weft pair` on a device in no group, `weft invite` on a device in one.
#[tokio::test(flavor = "multi_thread")]
async fn invite_with_the_binary() {
    timed(async {
        let url = start_relay().await;
        let a_dev = TestDevice::named(&url, "Laptop");
        let b_dev = TestDevice::named(&url, "Phone");
        let c_dev = TestDevice::named(&url, "Tablet");
        let a_id = a_dev.load().identity().device_id();
        {
            let mut b = b_dev.connect().await;
            let mut c = c_dev.connect().await;
            pair(&mut b, &mut c).await;
            b.close().await;
            c.close().await;
        }
        let weft = |dev: &TestDevice| {
            let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_weft"));
            command
                .arg("--state")
                .arg(dev.dir.path())
                .env_remove("WEFT_STATE")
                .env("WEFT_LOG", "info")
                .stdin(Stdio::null())
                .kill_on_drop(true);
            command
        };

        // A device in no group cannot invite.
        let lone = TestDevice::new(&url);
        let code_for_nothing = {
            let mut session = a_dev.connect().await;
            let code = session.open_pairing().await.unwrap();
            session.close().await;
            code.encode().unwrap()
        };
        let output = tokio::time::timeout(
            STEP,
            weft(&lone).args(["invite", &code_for_nothing]).output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("join the other device instead"), "{stderr}");

        let mut c = c_dev.connect().await;
        let mut pair = weft(&a_dev)
            .arg("pair")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut pair_out = BufReader::new(pair.stdout.take().unwrap()).lines();
        let next_line = async |lines: &mut tokio::io::Lines<_>| -> String {
            tokio::time::timeout(STEP, lines.next_line())
                .await
                .expect("no line before the timeout")
                .unwrap()
                .expect("output closed")
        };
        let code = loop {
            let line = next_line(&mut pair_out).await;
            if line.starts_with("WEFT1:") {
                break line;
            }
        };
        let output = tokio::time::timeout(STEP, weft(&b_dev).args(["invite", &code]).output())
            .await
            .unwrap()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            weft_cli::short_id(&a_id)
        );
        // `weft pair` prints the devices it now trusts, the inviter first.
        let b_id = b_dev.load().identity().device_id();
        assert_eq!(next_line(&mut pair_out).await, weft_cli::short_id(&b_id));
        assert_eq!(
            next_line(&mut pair_out).await,
            weft_cli::short_id(&c.device_id())
        );
        let status = tokio::time::timeout(STEP, pair.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(status.success());

        // C, which was listening, now trusts A and has its name.
        assert_eq!(
            next_event(&mut c).await,
            Event::DeviceAdded {
                by: b_id,
                device: a_id,
            }
        );
        assert_eq!(
            next_event(&mut c).await,
            Event::DeviceName {
                from: a_id,
                name: "Laptop".into(),
            }
        );
        assert_eq!(
            sorted(a_dev.load().devices.iter().map(|d| d.device_id()).collect()),
            sorted(vec![b_id, c.device_id()])
        );
    })
    .await;
}
