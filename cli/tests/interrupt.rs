//! `Session::next_event_or` and `Session::next_queued_event`: waiting for the
//! connection and for something else at once, without losing a frame.

mod common;

use std::time::Duration;

use common::*;
use tokio::sync::mpsc;
use weft_cli::{Error, Event, Next};

#[tokio::test]
async fn an_interrupt_returns_control_without_losing_a_frame() {
    timed(async {
        let url = start_relay().await;
        let (a_dev, b_dev) = (TestDevice::new(&url), TestDevice::new(&url));
        let mut a = a_dev.connect().await;
        let mut b = b_dev.connect().await;
        pair(&mut a, &mut b).await;

        // Nothing arrives: the interrupt wins and its output comes back.
        let (requests, mut inbox) = mpsc::unbounded_channel();
        requests.send("first").unwrap();
        let next = b.next_event_or(Some(STEP), inbox.recv()).await.unwrap();
        assert_eq!(next, Next::Interrupted(Some("first")));

        // A frame sent while the caller was busy elsewhere is delivered by
        // the next call, whatever interrupted the previous one.
        a.send_text("sent meanwhile").await.unwrap();
        let next = b
            .next_event_or(Some(STEP), std::future::pending::<()>())
            .await
            .unwrap();
        assert_eq!(
            next,
            Next::Event(Event::Clipboard {
                from: a.device_id(),
                text: "sent meanwhile".into(),
            })
        );

        // An interrupt that completes while the wait is in progress.
        let later = tokio::time::sleep(Duration::from_millis(50));
        let next = b.next_event_or(Some(STEP), later).await.unwrap();
        assert_eq!(next, Next::Interrupted(()));

        // Interrupted many times while frames arrive: every frame comes out
        // once, in order.
        let texts: Vec<String> = (0..20).map(|n| format!("text {n}")).collect();
        for text in &texts {
            a.send_text(text).await.unwrap();
        }
        let mut received = Vec::new();
        while received.len() < texts.len() {
            requests.send("again").unwrap();
            match b.next_event_or(Some(STEP), inbox.recv()).await.unwrap() {
                Next::Event(Event::Clipboard { text, .. }) => received.push(text),
                Next::Interrupted(request) => assert_eq!(request, Some("again")),
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(received, texts);

        // The timeout still reports an idle connection, for the keepalive.
        while inbox.try_recv().is_ok() {}
        let next = b.next_event_or(Some(QUIET), inbox.recv()).await.unwrap();
        assert_eq!(next, Next::Idle);
        b.settle().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn events_are_handled_as_with_next_event() {
    timed(async {
        let url = start_relay().await;
        let a_dev = TestDevice::named(&url, "Test PC");
        let b_dev = TestDevice::named(&url, "Test phone");
        let c_dev = TestDevice::named(&url, "Test tablet");
        let mut a = a_dev.connect().await;
        let mut b = b_dev.connect().await;
        let mut c = c_dev.connect().await;
        let never = std::future::pending::<()>;
        pair(&mut a, &mut b).await;

        a.send_text("hello").await.unwrap();
        assert_eq!(
            b.next_event_or(Some(STEP), never()).await.unwrap(),
            Next::Event(Event::Clipboard {
                from: a.device_id(),
                text: "hello".into(),
            })
        );

        // B is told about C through `next_event_or`: it trusts C and sends
        // it its name before returning, as `next_event` does.
        pair(&mut a, &mut c).await;
        assert_eq!(
            b.next_event_or(Some(STEP), never()).await.unwrap(),
            Next::Event(Event::DeviceAdded {
                by: a.device_id(),
                device: c.device_id(),
            })
        );
        assert_eq!(trusted(&b), sorted(vec![a.device_id(), c.device_id()]));
        // C's name reaches B, and the name B sent reaches C.
        assert_eq!(
            b.next_event_or(Some(STEP), never()).await.unwrap(),
            Next::Event(Event::DeviceName {
                from: c.device_id(),
                name: "Test tablet".into(),
            })
        );
        expect_name(&mut c, &b).await;
        assert_eq!(name_of(&b, &c.device_id()).as_deref(), Some("Test tablet"));
    })
    .await;
}

#[tokio::test]
async fn frames_kept_while_pairing_are_handled_from_the_queue() {
    timed(async {
        let url = start_relay().await;
        let (a_dev, b_dev) = (TestDevice::new(&url), TestDevice::new(&url));
        let mut a = a_dev.connect().await;
        let mut b = b_dev.connect().await;
        pair(&mut a, &mut b).await;
        assert_eq!(a.next_queued_event().await.unwrap(), None);

        // Text arrives while A waits for a third device.
        let code = a.open_pairing().await.unwrap();
        b.send_text("during pairing").await.unwrap();
        let waited = a.wait_pair_request(&code, QUIET).await;
        assert!(matches!(waited, Err(Error::Timeout(_))), "{waited:?}");
        assert_eq!(
            a.next_queued_event().await.unwrap(),
            Some(Event::Clipboard {
                from: b.device_id(),
                text: "during pairing".into(),
            })
        );
        assert_eq!(a.next_queued_event().await.unwrap(), None);

        // The slot is still open for the device that uses the code.
        let c_dev = TestDevice::new(&url);
        let mut c = c_dev.connect().await;
        let (added, joined) = tokio::join!(a.wait_pair_request(&code, STEP), c.join(&code, STEP));
        assert_eq!(added.unwrap().device(), c.device_id());
        joined.unwrap();
    })
    .await;
}

#[tokio::test]
async fn queued_lan_frame_precedes_a_later_relay_frame() {
    timed(async {
        let url = start_relay().await;
        let (a_dev, b_dev) = (TestDevice::new(&url), TestDevice::new(&url));
        let mut a = a_dev.connect().await;
        let mut b = b_dev.connect().await;
        pair(&mut a, &mut b).await;

        let direct = a
            .seal_for(b.device_id(), weft_protocol::Kind::ClipboardText, b"LAN")
            .unwrap();
        a.send_text("relay fallback").await.unwrap();
        // Keep the relay frame read ahead so both paths are ready.
        a.settle().await.unwrap();
        b.settle().await.unwrap();

        let first = b
            .next_event_with_lan(
                Some(STEP),
                std::future::pending(),
                std::future::ready(direct.clone()),
            )
            .await
            .unwrap();
        assert_eq!(first, Next::Interrupted(direct.clone()));
        assert_eq!(
            b.receive_frame(&direct).await.unwrap(),
            Event::Clipboard {
                from: a.device_id(),
                text: "LAN".into(),
            }
        );
        assert_eq!(
            b.next_event_or(Some(STEP), std::future::pending::<()>())
                .await
                .unwrap(),
            Next::Event(Event::Clipboard {
                from: a.device_id(),
                text: "relay fallback".into(),
            })
        );
    })
    .await;
}
