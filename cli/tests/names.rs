//! Device names (kind `0x06`): exchanged when devices start trusting each
//! other, sent again on rename, ignored from untrusted senders.

mod common;

use common::*;
use weft_cli::{DropReason, Error, Event};

#[tokio::test]
async fn two_devices_exchange_names_on_pairing() {
    timed(async {
        let url = start_relay().await;
        let a_dev = TestDevice::named(&url, "Laptop");
        let b_dev = TestDevice::named(&url, "Téléphone de test");
        let mut a = a_dev.connect().await;
        let mut b = b_dev.connect().await;
        // `pair` checks the DeviceName events and the saved names.
        pair(&mut a, &mut b).await;
        assert_eq!(name_of(&a, &b.device_id()).unwrap(), "Téléphone de test");
        assert_eq!(name_of(&b, &a.device_id()).unwrap(), "Laptop");
        expect_nothing(&mut a).await;
        expect_nothing(&mut b).await;
    })
    .await;
}

#[tokio::test]
async fn three_devices_all_learn_each_other_names() {
    timed(async {
        let url = start_relay().await;
        let a_dev = TestDevice::named(&url, "A laptop");
        let b_dev = TestDevice::named(&url, "B phone");
        let c_dev = TestDevice::named(&url, "C tablet");
        let mut a = a_dev.connect().await;
        let mut b = b_dev.connect().await;
        let mut c = c_dev.connect().await;
        pair(&mut a, &mut b).await;
        pair(&mut a, &mut c).await;

        // B learns about C from A, then gets C's name (sent by C once
        // welcomed) and answers with its own (sent once told about C).
        assert_eq!(
            next_event(&mut b).await,
            Event::DeviceAdded {
                by: a.device_id(),
                device: c.device_id(),
            }
        );
        expect_name(&mut b, &c).await;
        expect_name(&mut c, &b).await;

        let names = |session: &weft_cli::Session| {
            let mut names: Vec<String> = session
                .state_dir()
                .load()
                .unwrap()
                .devices
                .iter()
                .map(|device| device.name.clone().unwrap())
                .collect();
            names.sort();
            names
        };
        assert_eq!(names(&a), ["B phone", "C tablet"]);
        assert_eq!(names(&b), ["A laptop", "C tablet"]);
        assert_eq!(names(&c), ["A laptop", "B phone"]);
        for session in [&mut a, &mut b, &mut c] {
            expect_nothing(session).await;
        }
    })
    .await;
}

#[tokio::test]
async fn rename_reaches_every_trusted_device() {
    timed(async {
        let url = start_relay().await;
        let devices: Vec<_> = ["A", "B", "C"]
            .iter()
            .map(|name| TestDevice::named(&url, name))
            .collect();
        let mut a = devices[0].connect().await;
        let mut b = devices[1].connect().await;
        let mut c = devices[2].connect().await;
        pair(&mut a, &mut b).await;
        pair(&mut a, &mut c).await;
        next_event(&mut b).await; // C added
        expect_name(&mut b, &c).await;
        expect_name(&mut c, &b).await;

        assert_eq!(b.rename("B renamed ✓").await.unwrap(), 2);
        assert_eq!(devices[1].load().name(), "B renamed ✓");
        // `expect_name` also checks the old name was overwritten on disk.
        expect_name(&mut a, &b).await;
        expect_name(&mut c, &b).await;
        assert_eq!(name_of(&a, &b.device_id()).unwrap(), "B renamed ✓");

        // Names do not change trust.
        assert_eq!(trusted(&a), sorted(vec![b.device_id(), c.device_id()]));
        assert_eq!(trusted(&c), sorted(vec![a.device_id(), b.device_id()]));
    })
    .await;
}

#[tokio::test]
async fn name_from_an_untrusted_sender_is_ignored() {
    timed(async {
        let url = start_relay().await;
        let devices: Vec<_> = ["A", "B", "C"]
            .iter()
            .map(|name| TestDevice::named(&url, name))
            .collect();
        let mut a = devices[0].connect().await;
        let mut b = devices[1].connect().await;
        let mut c = devices[2].connect().await;
        pair(&mut a, &mut b).await;
        pair(&mut a, &mut c).await;
        next_event(&mut b).await; // C added
        expect_name(&mut b, &c).await;
        expect_name(&mut c, &b).await;

        // B stops trusting C on its own; the relay still routes between them.
        devices[1]
            .dir
            .update(|state| state.untrust(&c.device_id()))
            .unwrap();
        c.rename("C renamed").await.unwrap();
        expect_name(&mut a, &c).await;
        assert_eq!(
            next_event(&mut b).await,
            Event::Dropped {
                from: Some(c.device_id()),
                reason: DropReason::UnknownSender,
            }
        );
        assert_eq!(name_of(&b, &c.device_id()), None);
        assert_eq!(trusted(&b), vec![a.device_id()]);
        assert!(
            !std::fs::read_to_string(devices[1].dir.file())
                .unwrap()
                .contains("C renamed")
        );
    })
    .await;
}

#[tokio::test]
async fn rename_rejects_invalid_names() {
    timed(async {
        let url = start_relay().await;
        let a_dev = TestDevice::named(&url, "A");
        let b_dev = TestDevice::named(&url, "B");
        let mut a = a_dev.connect().await;
        let mut b = b_dev.connect().await;
        pair(&mut a, &mut b).await;

        let long = "x".repeat(65);
        for (name, expected) in [
            ("", weft_protocol::Error::DeviceNameLength(0)),
            (&long[..], weft_protocol::Error::DeviceNameLength(65)),
            (
                "tab\there",
                weft_protocol::Error::DeviceNameControlCharacter,
            ),
            (
                "del\u{7f}",
                weft_protocol::Error::DeviceNameControlCharacter,
            ),
            ("c1\u{9f}", weft_protocol::Error::DeviceNameControlCharacter),
        ] {
            let result = a.rename(name).await;
            assert!(
                matches!(&result, Err(Error::Protocol(err)) if *err == expected),
                "{name:?}: {result:?}"
            );
        }
        assert_eq!(a_dev.load().name(), "A");
        expect_nothing(&mut b).await;
        assert_eq!(name_of(&b, &a.device_id()).unwrap(), "A");

        // Exactly 64 bytes is fine.
        let max = "é".repeat(32);
        assert_eq!(a.rename(&max).await.unwrap(), 1);
        expect_name(&mut b, &a).await;
    })
    .await;
}
