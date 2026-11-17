//! The client library against a real relay: pairing, group changes,
//! exchanging clipboard text, replay protection and the state file.

mod common;

use common::*;
use weft_cli::session::Session;
use weft_cli::state::ReplayError;
use weft_cli::{DropReason, Error, Event, State, hex};
use weft_protocol::{
    ControlError, ControlFrame, Header, Kind, PairRequest, PairingCode, VERSION, random_message_id,
    random_pairing_id, random_pairing_secret, seal, session_keys,
};

#[tokio::test]
async fn paired_devices_exchange_text_and_persist_counters() {
    timed(async {
        let url = start_relay().await;
        let (a_dev, b_dev) = (TestDevice::new(&url), TestDevice::new(&url));
        let mut a = a_dev.connect().await;
        let mut b = b_dev.connect().await;
        pair(&mut a, &mut b).await;
        assert_eq!(trusted(&a), vec![b.device_id()]);
        assert_eq!(trusted(&b), vec![a.device_id()]);

        assert_eq!(a.send_text("hello from A").await.unwrap(), 1);
        expect_text(&mut b, &a, "hello from A").await;
        assert_eq!(b.send_text("hello from B ✓").await.unwrap(), 1);
        expect_text(&mut a, &b, "hello from B ✓").await;
        a.send_text("second").await.unwrap();
        expect_text(&mut b, &a, "second").await;

        // The Welcome and A's name took counters 1 and 2 from A to B, B's
        // name counter 1 from B to A.
        let a_state = a_dev.load();
        let b_state = b_dev.load();
        assert_eq!(a_state.peer(&b.device_id()).unwrap().send_counter, 4);
        assert_eq!(b_state.peer(&a.device_id()).unwrap().recv_highest, 4);
        assert_eq!(b_state.peer(&a.device_id()).unwrap().send_counter, 2);
        assert_eq!(a_state.peer(&b.device_id()).unwrap().recv_highest, 2);

        // A fresh process reads the counters back and carries on from there.
        a.close().await;
        let mut a = a_dev.connect().await;
        a.send_text("after reload").await.unwrap();
        expect_text(&mut b, &a, "after reload").await;
        assert_eq!(a_dev.load().peer(&b.device_id()).unwrap().send_counter, 5);
    })
    .await;
}

#[tokio::test]
async fn third_device_is_announced_to_the_group() {
    timed(async {
        let url = start_relay().await;
        let devices: Vec<_> = (0..3).map(|_| TestDevice::new(&url)).collect();
        let mut a = devices[0].connect().await;
        let mut b = devices[1].connect().await;
        let mut c = devices[2].connect().await;
        pair(&mut a, &mut b).await;
        pair(&mut a, &mut c).await;

        assert_eq!(
            next_event(&mut b).await,
            Event::DeviceAdded {
                by: a.device_id(),
                device: c.device_id(),
            }
        );
        // C sent its name to every member once welcomed; B sent its name to
        // C once told about it.
        expect_name(&mut b, &c).await;
        expect_name(&mut c, &b).await;
        assert_eq!(trusted(&b), sorted(vec![a.device_id(), c.device_id()]));
        assert_eq!(trusted(&c), sorted(vec![a.device_id(), b.device_id()]));

        assert_eq!(b.send_text("B to all").await.unwrap(), 2);
        expect_text(&mut c, &b, "B to all").await;
        expect_text(&mut a, &b, "B to all").await;
        assert_eq!(c.send_text("C to all").await.unwrap(), 2);
        expect_text(&mut b, &c, "C to all").await;
        expect_text(&mut a, &c, "C to all").await;
    })
    .await;
}

/// An envelope from `from` to `to` with a chosen counter.
fn envelope(from: &Session, to: &Session, counter: u64, text: &str) -> Vec<u8> {
    let keys = session_keys(from.identity(), &to.device_id(), &to.identity().kx_pk()).unwrap();
    let header = Header {
        version: VERSION,
        kind: Kind::ClipboardText,
        sender: from.device_id(),
        recipient: to.device_id(),
        message_id: random_message_id(),
        counter,
    };
    seal(&header, text.as_bytes(), &keys.tx).unwrap()
}

#[tokio::test]
async fn replayed_and_invalid_counters_are_dropped() {
    timed(async {
        let url = start_relay().await;
        let (a_dev, b_dev) = (TestDevice::new(&url), TestDevice::new(&url));
        let mut a = a_dev.connect().await;
        let mut b = b_dev.connect().await;
        pair(&mut a, &mut b).await;
        let a_id = a.device_id();
        let dropped = |reason| Event::Dropped {
            from: Some(a_id),
            reason: DropReason::Replay(reason),
        };

        let frame = envelope(&a, &b, 100, "once");
        a.send_frame(frame.clone()).await.unwrap();
        a.send_frame(frame.clone()).await.unwrap();
        expect_text(&mut b, &a, "once").await;
        assert_eq!(next_event(&mut b).await, dropped(ReplayError::Duplicate));

        a.send_frame(envelope(&a, &b, 0, "zero")).await.unwrap();
        assert_eq!(next_event(&mut b).await, dropped(ReplayError::ZeroCounter));

        a.send_frame(envelope(&a, &b, 2000, "far ahead"))
            .await
            .unwrap();
        expect_text(&mut b, &a, "far ahead").await;
        a.send_frame(envelope(&a, &b, 900, "too old"))
            .await
            .unwrap();
        assert_eq!(next_event(&mut b).await, dropped(ReplayError::TooOld));
        // Inside the window and never seen: accepted.
        let late = envelope(&a, &b, 1500, "late");
        a.send_frame(late.clone()).await.unwrap();
        expect_text(&mut b, &a, "late").await;

        // The replay state is on disk: a restarted receiver still refuses.
        b.close().await;
        let mut b = b_dev.connect().await;
        a.send_frame(late).await.unwrap();
        assert_eq!(next_event(&mut b).await, dropped(ReplayError::Duplicate));
        a.send_frame(frame).await.unwrap();
        assert_eq!(next_event(&mut b).await, dropped(ReplayError::TooOld));
    })
    .await;
}

#[tokio::test]
async fn pair_request_with_a_wrong_mac_is_ignored() {
    timed(async {
        let url = start_relay().await;
        let devices: Vec<_> = (0..3).map(|_| TestDevice::new(&url)).collect();
        let mut a = devices[0].connect().await;
        let mut b = devices[1].connect().await;
        let mut mallory = devices[2].connect().await;

        let code = a.open_pairing().await.unwrap();
        // Right pairing id, but a MAC made without the secret, then garbage.
        let mut request = PairRequest::new(
            &random_pairing_secret(),
            code.pairing_id,
            mallory.identity().sign_pk(),
            mallory.identity().kx_pk(),
        );
        for mac in [request.mac, [0; 32]] {
            request.mac = mac;
            mallory
                .send_frame(ControlFrame::PairRequest(request.clone()).encode())
                .await
                .unwrap();
            mallory.settle().await.unwrap();
            let waited = a.wait_pair_request(&code, QUIET).await;
            assert!(matches!(waited, Err(Error::Timeout(_))), "{waited:?}");
            // A reopened the slot the relay closed when forwarding the request.
            a.settle().await.unwrap();
            assert!(trusted(&a).is_empty());
            expect_nothing(&mut mallory).await;
        }

        // The code still works for the device that really scanned it.
        let (added, joined) = tokio::join!(a.wait_pair_request(&code, STEP), b.join(&code, STEP),);
        assert_eq!(added.unwrap().device(), b.device_id());
        assert_eq!(joined.unwrap(), vec![a.device_id()]);
        assert_eq!(trusted(&a), vec![b.device_id()]);
        expect_nothing(&mut mallory).await;
    })
    .await;
}

#[tokio::test]
async fn join_reports_relay_errors() {
    timed(async {
        let url = start_relay().await;
        let (a_dev, b_dev) = (TestDevice::new(&url), TestDevice::new(&url));
        let a = a_dev.connect().await;
        let mut b = b_dev.connect().await;
        // A code for a slot that was never opened.
        let code = PairingCode {
            relay_url: url.clone(),
            pairing_id: random_pairing_id(),
            sign_pk: a.identity().sign_pk(),
            kx_pk: a.identity().kx_pk(),
            secret: random_pairing_secret(),
        };
        let joined = b.join(&code, STEP).await;
        assert!(
            matches!(joined, Err(Error::Relay(ControlError::UnknownPairing))),
            "{joined:?}"
        );
        assert!(trusted(&b).is_empty());
    })
    .await;
}

#[tokio::test]
async fn relay_name_must_match_the_host() {
    timed(async {
        let url = start_relay_named("relay.example").await;
        let device = TestDevice::new(&url);
        let connected = Session::connect(device.dir.clone()).await;
        assert!(
            matches!(
                connected,
                Err(Error::Protocol(weft_protocol::Error::RelayNameMismatch))
            ),
            "{:?}",
            connected.err()
        );

        // The host is compared lowercased and without its port.
        let url = start_relay_named("localhost").await;
        let device = TestDevice::new(&url.replace("127.0.0.1", "LocalHost"));
        device.connect().await;
    })
    .await;
}

#[tokio::test]
async fn removed_device_no_longer_receives() {
    timed(async {
        let url = start_relay().await;
        let devices: Vec<_> = (0..3).map(|_| TestDevice::new(&url)).collect();
        let mut a = devices[0].connect().await;
        let mut b = devices[1].connect().await;
        let mut c = devices[2].connect().await;
        pair(&mut a, &mut b).await;
        pair(&mut a, &mut c).await;
        next_event(&mut b).await; // C added
        expect_name(&mut b, &c).await;
        expect_name(&mut c, &b).await;

        let prefix = &hex(&c.device_id())[..6];
        let removed = a.remove(prefix).await.unwrap();
        assert_eq!(removed.device_id(), c.device_id());
        assert_eq!(
            next_event(&mut b).await,
            Event::DeviceRemoved {
                by: a.device_id(),
                device: c.device_id(),
            }
        );
        assert_eq!(trusted(&a), vec![b.device_id()]);
        assert_eq!(trusted(&b), vec![a.device_id()]);

        assert_eq!(a.send_text("A without C").await.unwrap(), 1);
        expect_text(&mut b, &a, "A without C").await;
        assert_eq!(b.send_text("B without C").await.unwrap(), 1);
        expect_text(&mut a, &b, "B without C").await;
        expect_nothing(&mut c).await;

        // C still trusts A and B, but the relay no longer routes for it.
        assert_eq!(c.send_text("C left").await.unwrap(), 2);
        expect_nothing(&mut a).await;
        expect_nothing(&mut b).await;
    })
    .await;
}

#[tokio::test]
async fn state_file_is_private_and_never_holds_the_pairing_secret() {
    timed(async {
        let url = start_relay().await;
        let (a_dev, b_dev) = (TestDevice::new(&url), TestDevice::new(&url));
        let mut a = a_dev.connect().await;
        let mut b = b_dev.connect().await;

        let code = a.open_pairing().await.unwrap();
        let (added, joined) = tokio::join!(a.wait_pair_request(&code, STEP), b.join(&code, STEP),);
        added.unwrap();
        joined.unwrap();
        expect_name(&mut a, &b).await;
        expect_name(&mut b, &a).await;
        a.send_text("x").await.unwrap();
        expect_text(&mut b, &a, "x").await;

        let encoded = code.encode().unwrap();
        let secret_hex = hex(&code.secret);
        for device in [&a_dev, &b_dev] {
            let file = device.dir.file();
            let contents = std::fs::read_to_string(&file).unwrap();
            assert!(!contents.contains(&secret_hex));
            assert!(!contents.contains(&encoded[weft_protocol::PAIRING_CODE_PREFIX.len()..]));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(&file).unwrap().permissions().mode();
                assert_eq!(mode & 0o777, 0o600, "{}", file.display());
                let dir_mode = std::fs::metadata(device.dir.path())
                    .unwrap()
                    .permissions()
                    .mode();
                assert_eq!(dir_mode & 0o777, 0o700);
            }
            // Only the state and its lock: no temporary file left behind.
            let mut names: Vec<_> = std::fs::read_dir(device.dir.path())
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect();
            names.sort();
            assert_eq!(names, ["state.json", "state.lock"]);
        }

        // `init` does not overwrite an identity unless forced.
        let fresh = State::generate(&url).unwrap();
        assert!(matches!(
            a_dev.dir.create(&fresh, false),
            Err(Error::AlreadyInitialised(_))
        ));
        assert_eq!(a_dev.load().identity().device_id(), a.device_id());
        a_dev.dir.create(&fresh, true).unwrap();
        assert_eq!(
            a_dev.load().identity().device_id(),
            fresh.identity().device_id()
        );
    })
    .await;
}
