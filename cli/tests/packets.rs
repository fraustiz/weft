//! Packets (kind `0x07`) and file chunks (kind `0x08`) between trusted
//! devices, through `Session` (`docs/features.md`, section 1).

mod common;

use common::*;
use weft_cli::{DropReason, Error, Event, Next};
use weft_protocol::{
    Header, Kind, MAX_FILE_CHUNK_DATA_LEN, MAX_PACKET_LEN, VERSION, random_message_id, seal,
    session_keys,
};

const BATTERY: &str = r#"{"type":"battery","level":42,"charging":true,"low":false}"#;

#[tokio::test]
async fn packet_reaches_the_device_it_is_sent_to() {
    timed(async {
        let url = start_relay().await;
        let (a_dev, b_dev) = (TestDevice::new(&url), TestDevice::new(&url));
        let mut a = a_dev.connect().await;
        let mut b = b_dev.connect().await;
        pair(&mut a, &mut b).await;

        a.send_packet(b.device_id(), BATTERY).await.unwrap();
        a.settle().await.unwrap();
        let next = b
            .next_event_or(Some(STEP), std::future::pending::<()>())
            .await
            .unwrap();
        assert_eq!(
            next,
            Next::Event(Event::Packet {
                from: a.device_id(),
                json: BATTERY.to_owned(),
            })
        );

        let largest = "a".repeat(MAX_PACKET_LEN);
        b.send_packet(a.device_id(), &largest).await.unwrap();
        assert_eq!(
            next_event(&mut a).await,
            Event::Packet {
                from: b.device_id(),
                json: largest,
            }
        );
        expect_nothing(&mut a).await;
        expect_nothing(&mut b).await;
    })
    .await;
}

#[tokio::test]
async fn packet_all_reaches_every_trusted_device() {
    timed(async {
        let url = start_relay().await;
        let devices: Vec<_> = (0..3).map(|_| TestDevice::new(&url)).collect();
        let mut a = devices[0].connect().await;
        let mut b = devices[1].connect().await;
        let mut c = devices[2].connect().await;
        pair(&mut a, &mut b).await;
        pair(&mut a, &mut c).await;
        // B learns about C, then B and C exchange names.
        assert!(matches!(
            next_event(&mut b).await,
            Event::DeviceAdded { .. }
        ));
        expect_name(&mut b, &c).await;
        expect_name(&mut c, &b).await;

        assert_eq!(a.send_packet_all(BATTERY).await.unwrap(), 2);
        for session in [&mut b, &mut c] {
            assert_eq!(
                next_event(session).await,
                Event::Packet {
                    from: a.device_id(),
                    json: BATTERY.to_owned(),
                }
            );
        }
    })
    .await;
}

#[tokio::test]
async fn file_chunk_reaches_the_device_it_is_sent_to() {
    timed(async {
        let url = start_relay().await;
        let (a_dev, b_dev) = (TestDevice::new(&url), TestDevice::new(&url));
        let mut a = a_dev.connect().await;
        let mut b = b_dev.connect().await;
        pair(&mut a, &mut b).await;

        let transfer_id = [0x5a; 16];
        let full = vec![0xab; MAX_FILE_CHUNK_DATA_LEN];
        let offset = MAX_FILE_CHUNK_DATA_LEN as u64;
        a.send_file_chunk(b.device_id(), transfer_id, 0, &full)
            .await
            .unwrap();
        a.send_file_chunk(b.device_id(), transfer_id, offset, b"end")
            .await
            .unwrap();
        assert_eq!(
            next_event(&mut b).await,
            Event::FileChunk {
                from: a.device_id(),
                transfer_id,
                offset: 0,
                data: full,
            }
        );
        assert_eq!(
            next_event(&mut b).await,
            Event::FileChunk {
                from: a.device_id(),
                transfer_id,
                offset,
                data: b"end".to_vec(),
            }
        );
    })
    .await;
}

#[tokio::test]
async fn invalid_packets_and_chunks_are_refused_before_sending() {
    timed(async {
        let url = start_relay().await;
        let (a_dev, b_dev) = (TestDevice::new(&url), TestDevice::new(&url));
        let mut a = a_dev.connect().await;
        let mut b = b_dev.connect().await;
        pair(&mut a, &mut b).await;
        let to = b.device_id();
        let protocol = |result: weft_cli::Result<()>| match result {
            Err(Error::Protocol(err)) => err,
            other => panic!("expected a protocol error, got {other:?}"),
        };

        assert_eq!(
            protocol(a.send_packet(to, "").await),
            weft_protocol::Error::InvalidPlaintextLength(0)
        );
        let too_large = "a".repeat(MAX_PACKET_LEN + 1);
        assert_eq!(
            protocol(a.send_packet(to, &too_large).await),
            weft_protocol::Error::PlaintextTooLarge(MAX_PACKET_LEN + 1)
        );
        assert!(matches!(
            a.send_packet_all("").await,
            Err(Error::Protocol(_))
        ));
        assert_eq!(
            protocol(a.send_file_chunk(to, [1; 16], 0, b"").await),
            weft_protocol::Error::InvalidPlaintextLength(24)
        );
        let chunk = vec![0; MAX_FILE_CHUNK_DATA_LEN + 1];
        assert!(matches!(
            a.send_file_chunk(to, [1; 16], 0, &chunk).await,
            Err(Error::Protocol(_))
        ));

        let stranger = [0x77; 16];
        assert!(matches!(
            a.send_packet(stranger, BATTERY).await,
            Err(Error::UnknownDevice(_))
        ));
        assert!(matches!(
            a.send_file_chunk(stranger, [1; 16], 0, b"x").await,
            Err(Error::UnknownDevice(_))
        ));

        a.settle().await.unwrap();
        expect_nothing(&mut b).await;
    })
    .await;
}

#[tokio::test]
async fn unknown_kind_from_a_trusted_device_is_dropped() {
    timed(async {
        let url = start_relay().await;
        let (a_dev, b_dev) = (TestDevice::new(&url), TestDevice::new(&url));
        let mut a = a_dev.connect().await;
        let mut b = b_dev.connect().await;
        pair(&mut a, &mut b).await;

        // A packet relabelled with a kind this client does not know: the
        // relay forwards it, the receiver drops it.
        let keys = session_keys(a.identity(), &b.device_id(), &b.identity().kx_pk()).unwrap();
        let header = Header {
            version: VERSION,
            kind: Kind::Packet,
            sender: a.device_id(),
            recipient: b.device_id(),
            message_id: random_message_id(),
            counter: 1000,
        };
        let mut frame = seal(&header, BATTERY.as_bytes(), &keys.tx).unwrap();
        frame[1] = 0x7F;
        a.send_frame(frame).await.unwrap();
        assert_eq!(
            next_event(&mut b).await,
            Event::Dropped {
                from: None,
                reason: DropReason::Malformed,
            }
        );

        // The connection still works.
        a.send_packet(b.device_id(), BATTERY).await.unwrap();
        assert!(matches!(next_event(&mut b).await, Event::Packet { .. }));
    })
    .await;
}
