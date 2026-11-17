//! The core as the Swift app drives it, against a real relay, with a plain
//! `Session` as the other device and a pasteboard in memory.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tempfile::TempDir;
use tokio::net::TcpListener;
use weft_cli::{Event, Session, State, StateDir, unhex};
use weft_client::packets::{self, Packet, Parsed};
use weft_ffi::{
    ConnectionStatus, ConnectivityInfo, CoreConfig, CoreDelegate, CoreError, CoreNotice,
    CoreSnapshot, FeatureSwitches, MediaControl, Pairing, Pasteboard, PasteboardContent,
    SendOutcome, TransferStage, VolumeInfo, WeftCore,
};
use weft_protocol::PairingCode;
use weft_relay::Config;

const STEP: Duration = Duration::from_secs(10);
const TEST: Duration = Duration::from_secs(90);

async fn timed<T>(body: impl Future<Output = T>) -> T {
    tokio::time::timeout(TEST, body)
        .await
        .expect("test timed out")
}

async fn start_relay() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(weft_relay::serve(
        listener,
        Config::new("127.0.0.1", ":memory:"),
    ));
    format!("ws://{addr}/v1")
}

fn temp_dir() -> TempDir {
    tempfile::Builder::new()
        .prefix("weft-ffi-test-")
        .tempdir()
        .unwrap()
}

/// `NSPasteboard` in memory: a change count and a content.
struct FakePasteboard {
    board: Mutex<(i64, PasteboardContent)>,
}

impl FakePasteboard {
    fn new() -> Arc<FakePasteboard> {
        Arc::new(FakePasteboard {
            board: Mutex::new((
                41,
                PasteboardContent::Text {
                    text: "there before Weft".into(),
                },
            )),
        })
    }

    /// Another app copies `content`.
    fn copy(&self, content: PasteboardContent) {
        let mut board = self.board.lock().unwrap();
        board.0 += 1;
        board.1 = content;
    }

    fn copy_text(&self, text: &str) {
        self.copy(PasteboardContent::Text { text: text.into() });
    }

    fn content(&self) -> PasteboardContent {
        self.board.lock().unwrap().1.clone()
    }
}

impl Pasteboard for FakePasteboard {
    fn change_count(&self) -> i64 {
        self.board.lock().unwrap().0
    }

    fn read_text(&self) -> PasteboardContent {
        self.content()
    }

    fn write_text(&self, text: String, private: bool) -> Option<i64> {
        self.copy(if private {
            PasteboardContent::Excluded
        } else {
            PasteboardContent::Text { text }
        });
        Some(self.change_count())
    }
}

/// What the app would hear from the core.
#[derive(Default)]
struct Recorder {
    generations: Mutex<Vec<u64>>,
    notices: Mutex<Vec<CoreNotice>>,
    waiting: Mutex<u32>,
}

impl CoreDelegate for Recorder {
    fn snapshot_changed(&self, generation: u64) {
        self.generations.lock().unwrap().push(generation);
    }

    fn notice(&self, notice: CoreNotice) {
        self.notices.lock().unwrap().push(notice);
    }

    fn clipboard_waiting(&self) {
        *self.waiting.lock().unwrap() += 1;
    }
}

/// The core under test, with what it was given.
struct Tested {
    temp: TempDir,
    core: Arc<WeftCore>,
    pasteboard: Arc<FakePasteboard>,
    recorder: Arc<Recorder>,
}

impl Tested {
    fn start(relay_url: &str) -> Tested {
        let temp = temp_dir();
        let pasteboard = FakePasteboard::new();
        let recorder = Arc::new(Recorder::default());
        let config = CoreConfig {
            state_dir: temp.path().join("state").to_string_lossy().into_owned(),
            downloads_dir: temp.path().join("Downloads").to_string_lossy().into_owned(),
            relay_url: Some(relay_url.to_owned()),
            default_name: Some("Studio Mac".into()),
        };
        let core = WeftCore::start(config, recorder.clone(), pasteboard.clone()).unwrap();
        Tested {
            temp,
            core,
            pasteboard,
            recorder,
        }
    }

    async fn wait(&self, wanted: impl Fn(&CoreSnapshot) -> bool) -> CoreSnapshot {
        let found = async {
            loop {
                let snapshot = self.core.snapshot();
                if wanted(&snapshot) {
                    return snapshot;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        match tokio::time::timeout(STEP, found).await {
            Ok(snapshot) => snapshot,
            Err(_) => panic!("still waiting, with {:?}", self.core.snapshot()),
        }
    }

    fn id(&self) -> [u8; 16] {
        unhex(&self.core.snapshot().device_id).unwrap()
    }

    /// Polls the pasteboard, as the app's timer does, until a received
    /// text was written.
    async fn poll_until_written(&self) {
        let found = async {
            loop {
                if self.core.poll_pasteboard().written {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        tokio::time::timeout(STEP, found)
            .await
            .expect("no text was written");
    }

    async fn expect_notice(&self, wanted: impl Fn(&CoreNotice) -> bool) -> CoreNotice {
        let found = async {
            loop {
                {
                    let mut notices = self.recorder.notices.lock().unwrap();
                    if let Some(index) = notices.iter().position(&wanted) {
                        return notices.remove(index);
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        tokio::time::timeout(STEP, found)
            .await
            .expect("no such notice")
    }

    /// Runs a blocking call of the core off the test's runtime, as the
    /// app runs it off its main thread.
    async fn blocking<T: Send + 'static>(
        &self,
        call: impl FnOnce(&WeftCore) -> T + Send + 'static,
    ) -> T {
        let core = self.core.clone();
        tokio::task::spawn_blocking(move || call(&core))
            .await
            .unwrap()
    }
}

/// A plain device.
struct Peer {
    _temp: TempDir,
    session: Session,
}

impl Peer {
    async fn connect(relay_url: &str, name: &str) -> Peer {
        let temp = temp_dir();
        let dir = StateDir::new(temp.path().join("state"));
        let mut state = State::generate(relay_url).unwrap();
        state.set_name(name).unwrap();
        dir.create(&state, false).unwrap();
        let session = tokio::time::timeout(STEP, Session::connect(dir))
            .await
            .unwrap()
            .unwrap();
        Peer {
            _temp: temp,
            session,
        }
    }

    fn id(&self) -> [u8; 16] {
        self.session.device_id()
    }

    async fn next_any(&mut self) -> Event {
        self.session
            .next_event(Some(STEP))
            .await
            .expect("connection failed")
            .expect("no event before the timeout")
    }

    /// The next event that `wanted` accepts, skipping the others.
    async fn expect(&mut self, wanted: impl Fn(&Event) -> bool) -> Event {
        loop {
            let event = self.next_any().await;
            if wanted(&event) {
                return event;
            }
        }
    }

    async fn expect_text(&mut self, from: [u8; 16], text: &str) {
        let wanted = Event::Clipboard {
            from,
            text: text.into(),
        };
        self.expect(|event| *event == wanted).await;
    }

    async fn expect_packet(&mut self, wanted: impl Fn(&Packet) -> bool) -> Packet {
        loop {
            if let Event::Packet { json, .. } = self.next_any().await
                && let Parsed::Known(packet) = packets::parse(&json)
                && wanted(&packet)
            {
                return packet;
            }
        }
    }
}

fn sorted(kinds: &[String]) -> Vec<String> {
    let mut kinds = kinds.to_vec();
    kinds.sort();
    kinds
}

fn sorted_str(kinds: &[&str]) -> Vec<String> {
    sorted(
        &kinds
            .iter()
            .map(|kind| kind.to_string())
            .collect::<Vec<_>>(),
    )
}

/// What the Mac acts on, spelled out: a change to `MAC_INCOMING` or
/// `MAC_OUTGOING` must change these too.
const MAC_INCOMING: &[&str] = &[
    "state.request",
    "ping",
    "ring",
    "ring.stop",
    "share.url",
    "file.offer",
    "file.accept",
    "file.reject",
    "file.ack",
    "file.done",
    "file.cancel",
    "battery",
    "connectivity",
    "notification",
    "notification.removed",
    "call",
    "media",
    "volume",
    "volume.set",
];

const MAC_OUTGOING: &[&str] = &[
    "state.request",
    "ping",
    "ring",
    "ring.stop",
    "share.url",
    "file.offer",
    "file.accept",
    "file.reject",
    "file.ack",
    "file.done",
    "file.cancel",
    "notification.dismiss",
    "notification.action",
    "notification.reply",
    "notification.request",
    "call.mute",
    "media.command",
    "volume",
    "volume.set",
];

/// A connected core named "Studio Mac", paired with a peer named "Pixel".
async fn paired(relay_url: &str) -> (Tested, Peer) {
    let tested = Tested::start(relay_url);
    tested
        .wait(|snapshot| {
            snapshot.status == ConnectionStatus::Connected && snapshot.name == "Studio Mac"
        })
        .await;
    tested.core.start_pairing();
    let snapshot = tested
        .wait(|snapshot| matches!(snapshot.pairing, Pairing::ShowingCode { .. }))
        .await;
    let Pairing::ShowingCode {
        code,
        expires_at_ms,
    } = snapshot.pairing
    else {
        unreachable!();
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let left = expires_at_ms - now_ms;
    assert!((290_000..=300_500).contains(&left), "expires in {left} ms");
    assert!(code.starts_with("WEFT1:"));

    let mut peer = Peer::connect(relay_url, "Pixel").await;
    let code = PairingCode::decode(&code).unwrap();
    peer.session.join(&code, STEP).await.unwrap();
    let peer_hex = weft_cli::hex(&peer.id());
    tested
        .wait(|snapshot| {
            snapshot.pairing
                == Pairing::Paired {
                    device_id: peer_hex.clone(),
                }
                && snapshot
                    .devices
                    .iter()
                    .any(|device| device.id == peer_hex && device.name.as_deref() == Some("Pixel"))
        })
        .await;
    let name = Event::DeviceName {
        from: tested.id(),
        name: "Studio Mac".into(),
    };
    peer.expect(|event| *event == name).await;
    (tested, peer)
}

#[tokio::test]
async fn starts_with_a_snapshot_of_this_mac() {
    timed(async {
        let url = start_relay().await;
        let tested = Tested::start(&url);
        let snapshot = tested
            .wait(|snapshot| {
                snapshot.status == ConnectionStatus::Connected && snapshot.name == "Studio Mac"
            })
            .await;
        assert_eq!(snapshot.relay_url, url);
        assert_eq!(snapshot.device_id.len(), 32);
        assert!(
            snapshot
                .device_id
                .starts_with(&snapshot.short_id.to_lowercase())
        );
        assert!(snapshot.devices.is_empty());
        assert_eq!(snapshot.pairing, Pairing::Idle);
        assert!(!tested.recorder.generations.lock().unwrap().is_empty());

        // Names follow the wire rule.
        assert_eq!(tested.core.rename("".into()), Err(CoreError::InvalidName));
        tested.core.rename("Atelier".into()).unwrap();
        tested.wait(|snapshot| snapshot.name == "Atelier").await;

        // Without other devices, nothing is sent.
        tested.pasteboard.copy_text("alone");
        assert!(!tested.core.poll_pasteboard().sent);
        assert_eq!(tested.core.send_clipboard_now(), SendOutcome::NoDevices);

        // Cancelling a pairing brings it back to idle.
        tested.core.start_pairing();
        tested
            .wait(|snapshot| matches!(snapshot.pairing, Pairing::ShowingCode { .. }))
            .await;
        tested.core.cancel_pairing();
        tested
            .wait(|snapshot| snapshot.pairing == Pairing::Idle)
            .await;
        tested.blocking(|core| core.stop()).await;
    })
    .await;
}

#[tokio::test]
async fn pairs_and_syncs_the_pasteboard_both_ways() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut peer) = paired(&url).await;
        let mac = tested.id();

        // The Mac announces exactly what it implements.
        let capabilities = peer
            .expect_packet(|packet| matches!(packet, Packet::Capabilities(_)))
            .await;
        let Packet::Capabilities(capabilities) = capabilities else {
            unreachable!();
        };
        assert_eq!(capabilities.platform, "macos");
        assert_eq!(sorted(&capabilities.incoming), sorted_str(MAC_INCOMING));
        assert_eq!(sorted(&capabilities.outgoing), sorted_str(MAC_OUTGOING));

        // What the pasteboard held at start is never sent.
        assert!(!tested.core.poll_pasteboard().sent);

        // Received: written once, and not sent back.
        peer.session.send_text("hello mac").await.unwrap();
        tested.poll_until_written().await;
        assert!(*tested.recorder.waiting.lock().unwrap() >= 1);
        assert_eq!(
            tested.pasteboard.content(),
            PasteboardContent::Text {
                text: "hello mac".into()
            }
        );
        let again = tested.core.poll_pasteboard();
        assert!(!again.sent && !again.written);

        // A local copy goes to the peer.
        tested.pasteboard.copy_text("hello peer");
        assert!(tested.core.poll_pasteboard().sent);
        peer.expect_text(mac, "hello peer").await;

        // Concealed content (a password manager) stays here.
        tested.pasteboard.copy(PasteboardContent::Excluded);
        assert!(!tested.core.poll_pasteboard().sent);
        assert_eq!(tested.core.send_clipboard_now(), SendOutcome::Excluded);

        // Too large is never cut.
        tested.pasteboard.copy_text(&"x".repeat(256 * 1024 + 1));
        assert!(!tested.core.poll_pasteboard().sent);
        assert_eq!(tested.core.send_clipboard_now(), SendOutcome::TooLarge);

        // "Send clipboard now" sends what is there, even unchanged.
        tested.pasteboard.copy_text("on demand");
        assert!(tested.core.poll_pasteboard().sent);
        peer.expect_text(mac, "on demand").await;
        assert_eq!(tested.core.send_clipboard_now(), SendOutcome::Sent);
        peer.expect_text(mac, "on demand").await;

        // The battery shows once the peer tells it.
        let battery = r#"{"type":"battery","level":64,"charging":false}"#;
        peer.session.send_packet(mac, battery).await.unwrap();
        let peer_hex = weft_cli::hex(&peer.id());
        let snapshot = tested
            .wait(|snapshot| {
                snapshot
                    .devices
                    .iter()
                    .any(|device| device.id == peer_hex && device.battery.is_some())
            })
            .await;
        let battery = snapshot.devices[0].battery.clone().unwrap();
        assert_eq!(
            (battery.level, battery.charging, battery.low),
            (64, false, false)
        );

        // Notices reach the delegate, even those the app does not show.
        let ping = r#"{"type":"ping","message":"hi"}"#;
        peer.session.send_packet(mac, ping).await.unwrap();
        let notice = tested
            .expect_notice(|notice| matches!(notice, CoreNotice::Ping { .. }))
            .await;
        assert_eq!(
            notice,
            CoreNotice::Ping {
                from: peer_hex.clone(),
                device: Some("Pixel".into()),
                message: Some("hi".into())
            }
        );

        // A rename reaches the peer.
        tested.core.rename("Atelier".into()).unwrap();
        let name = Event::DeviceName {
            from: mac,
            name: "Atelier".into(),
        };
        peer.expect(|event| *event == name).await;

        // The relay stays while in a group.
        assert_eq!(
            tested.core.set_relay_url("wss://relay.example/v1".into()),
            Err(CoreError::InGroup)
        );
        tested.blocking(|core| core.stop()).await;
    })
    .await;
}

#[tokio::test]
async fn removes_a_device() {
    timed(async {
        let url = start_relay().await;
        // The peer stays connected: the removed device is not told.
        let (tested, peer) = paired(&url).await;
        let phone = peer.id();
        assert_eq!(
            tested.core.remove_device("00".repeat(16)),
            Err(CoreError::UnknownDevice)
        );
        assert_eq!(
            tested.core.remove_device("not hex".into()),
            Err(CoreError::UnknownDevice)
        );
        tested.core.remove_device(weft_cli::hex(&phone)).unwrap();
        tested.wait(|snapshot| snapshot.devices.is_empty()).await;
        tested.blocking(|core| core.stop()).await;
    })
    .await;
}

#[tokio::test]
async fn leaves_the_group_and_changes_the_relay() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut peer) = paired(&url).await;
        let mac = tested.id();

        let told = tested.blocking(|core| core.leave_group()).await.unwrap();
        assert!(told);
        let removed = Event::DeviceRemoved {
            by: mac,
            device: mac,
        };
        peer.expect(|event| *event == removed).await;
        // Same identity, no other device, connected again.
        tested
            .wait(|snapshot| {
                snapshot.devices.is_empty() && snapshot.status == ConnectionStatus::Connected
            })
            .await;
        assert_eq!(tested.id(), mac);

        // The peer can no longer reach the Mac.
        peer.session.send_text("still there?").await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!tested.core.poll_pasteboard().written);

        // Alone, the relay can change.
        assert!(matches!(
            tested.core.set_relay_url("ws://relay.example/v1".into()),
            Err(CoreError::InvalidRelayUrl { .. })
        ));
        let other = start_relay().await;
        let target = other.clone();
        tested
            .blocking(move |core| core.set_relay_url(target))
            .await
            .unwrap();
        tested
            .wait(|snapshot| {
                snapshot.relay_url == other && snapshot.status == ConnectionStatus::Connected
            })
            .await;
        tested.blocking(|core| core.stop()).await;
    })
    .await;
}

#[tokio::test]
async fn announces_only_what_the_mac_implements() {
    timed(async {
        assert_eq!(sorted_str(weft_ffi::MAC_INCOMING), sorted_str(MAC_INCOMING));
        assert_eq!(sorted_str(weft_ffi::MAC_OUTGOING), sorted_str(MAC_OUTGOING));
        for kind in [
            "media",
            "media.command",
            "input.key",
            "input.pointer",
            "presenter",
            "sms.send",
            "contacts",
            "command.run",
            "command.list",
        ] {
            let incoming = weft_ffi::MAC_INCOMING.contains(&kind);
            let outgoing = weft_ffi::MAC_OUTGOING.contains(&kind);
            match kind {
                // The phone's players are shown and controlled; the Mac's
                // own are not shared.
                "media" => assert!(incoming && !outgoing),
                "media.command" => assert!(!incoming && outgoing),
                _ => assert!(!incoming && !outgoing, "{kind}"),
            }
        }

        let url = start_relay().await;
        let (tested, mut peer) = paired(&url).await;
        peer.expect_packet(|packet| matches!(packet, Packet::Capabilities(_)))
            .await;
        // SMS and commands stay off whatever is asked; the others follow
        // the switches, and new capabilities go out at once.
        let snapshot = tested.core.snapshot();
        assert_eq!(
            snapshot.features,
            FeatureSwitches {
                files: true,
                links: true,
                notifications: true,
                battery: true,
                ring: true,
                calls: true,
                media: true,
                volume: true,
            }
        );
        tested.core.set_features(FeatureSwitches {
            media: false,
            ring: false,
            ..snapshot.features
        });
        let Packet::Capabilities(capabilities) = peer
            .expect_packet(|packet| matches!(packet, Packet::Capabilities(_)))
            .await
        else {
            unreachable!();
        };
        for kind in ["media", "media.command", "ring", "ring.stop"] {
            assert!(
                !capabilities.handles(kind) && !capabilities.sends(kind),
                "{kind}"
            );
        }
        assert!(capabilities.handles("file.offer") && capabilities.sends("volume"));
        tested
            .wait(|snapshot| !snapshot.features.media && !snapshot.features.ring)
            .await;
        tested.blocking(|core| core.stop()).await;
    })
    .await;
}

#[tokio::test]
async fn shows_the_phone_and_tells_what_it_does() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut peer) = paired(&url).await;
        let mac = tested.id();
        let phone = weft_cli::hex(&peer.id());
        peer.expect_packet(|packet| matches!(packet, Packet::Capabilities(_)))
            .await;

        // The phone's state, as it sends it after its capabilities.
        for packet in [
            r#"{"type":"capabilities","platform":"android","incoming":["ring","ping","share.url","file.offer","media.command","volume.set","notification.dismiss","notification.action","notification.reply","notification.request","call.mute"],"outgoing":["battery","connectivity","media","volume","notification","call"]}"#,
            r#"{"type":"battery","level":52,"charging":true}"#,
            r#"{"type":"connectivity","network":"LTE","signal":3}"#,
            r#"{"type":"media","players":[{"id":"p1","app":"Music","title":"Song","artist":"Band","playing":false,"position":1000,"length":200000,"canPlay":true},{"id":"p2","app":"Radio","title":"News","playing":true,"position":5000,"length":60000,"canPause":true,"canSeek":true,"volume":40}]}"#,
            r#"{"type":"volume","level":70,"muted":false}"#,
        ] {
            peer.session.send_packet(mac, packet).await.unwrap();
        }
        let snapshot = tested
            .wait(|snapshot| {
                snapshot.devices[0].volume.is_some() && snapshot.devices[0].media.is_some()
            })
            .await;
        let device = &snapshot.devices[0];
        assert_eq!(device.platform.as_deref(), Some("android"));
        assert!(device.handles.contains(&"ring".to_string()));
        assert!(device.sends.contains(&"media".to_string()));
        assert_eq!(device.battery.as_ref().unwrap().level, 52);
        assert_eq!(
            device.connectivity,
            Some(ConnectivityInfo {
                network: Some("LTE".into()),
                signal: Some(3)
            })
        );
        assert_eq!(
            device.volume,
            Some(VolumeInfo {
                level: Some(70),
                muted: Some(false)
            })
        );
        let media = device.media.as_ref().unwrap();
        assert_eq!(media.players.len(), 2);
        // The one playing is the one shown.
        assert_eq!(media.current, Some(1));
        let radio = &media.players[1];
        assert_eq!(
            (radio.playing, radio.position_ms, radio.length_ms, radio.volume),
            (true, Some(5000), Some(60000), Some(40))
        );
        assert!(radio.can_seek && radio.can_pause && !radio.can_next);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        assert!((now_ms - media.received_at_ms).abs() < 5_000);

        // What the Mac sends the phone.
        assert_eq!(
            tested.core.ping("00".repeat(16)),
            Err(CoreError::UnknownDevice)
        );
        tested.core.ping(phone.clone()).unwrap();
        peer.expect_packet(|packet| matches!(packet, Packet::Ping { message: None }))
            .await;
        tested.core.ring(phone.clone()).unwrap();
        peer.expect_packet(|packet| matches!(packet, Packet::Ring {}))
            .await;
        tested.core.stop_ringing(phone.clone()).unwrap();
        peer.expect_packet(|packet| matches!(packet, Packet::RingStop {}))
            .await;
        assert_eq!(
            tested
                .core
                .share_url(phone.clone(), "file:///etc/passwd".into()),
            Err(CoreError::InvalidLink)
        );
        assert!(!weft_ffi::is_shareable_url("javascript:alert(1)"));
        assert!(weft_ffi::is_shareable_url(" https://example.org/a "));
        tested
            .core
            .share_url(phone.clone(), " https://example.org/a ".into())
            .unwrap();
        peer.expect_packet(|packet| {
            *packet
                == Packet::ShareUrl {
                    url: "https://example.org/a".into(),
                }
        })
        .await;
        tested
            .core
            .media_command(phone.clone(), "p2".into(), MediaControl::Seek { position_ms: 30_000 })
            .unwrap();
        peer.expect_packet(|packet| {
            matches!(packet, Packet::MediaCommand(command)
                if command.player == "p2" && command.action == "seek" && command.value == Some(30_000))
        })
        .await;
        tested
            .core
            .media_command(phone.clone(), "p2".into(), MediaControl::SetVolume { level: 120 })
            .unwrap();
        peer.expect_packet(|packet| {
            matches!(packet, Packet::MediaCommand(command)
                if command.action == "setVolume" && command.value == Some(100))
        })
        .await;
        tested
            .core
            .set_device_volume(phone.clone(), Some(30), None)
            .unwrap();
        peer.expect_packet(|packet| {
            matches!(packet, Packet::VolumeSet(set) if set.level == Some(30) && set.muted.is_none())
        })
        .await;
        tested
            .core
            .dismiss_notification(phone.clone(), "n1".into())
            .unwrap();
        peer.expect_packet(|packet| *packet == Packet::NotificationDismiss { id: "n1".into() })
            .await;
        tested
            .core
            .notification_action(phone.clone(), "n1".into(), "Archive".into())
            .unwrap();
        peer.expect_packet(|packet| {
            *packet
                == Packet::NotificationAction {
                    id: "n1".into(),
                    action: "Archive".into(),
                }
        })
        .await;
        tested
            .core
            .reply_to_notification(phone.clone(), "n1".into(), "  on my way ".into())
            .unwrap();
        peer.expect_packet(|packet| {
            *packet
                == Packet::NotificationReply {
                    id: "n1".into(),
                    text: "on my way".into(),
                }
        })
        .await;
        tested.core.request_notifications(phone.clone()).unwrap();
        peer.expect_packet(|packet| *packet == Packet::NotificationRequest {})
            .await;
        tested.core.mute_call(phone.clone()).unwrap();
        peer.expect_packet(|packet| *packet == Packet::CallMute {})
            .await;
        // This Mac's volume goes to the devices that show it.
        tested.core.local_volume(35, true);

        // What the phone asks of the Mac.
        peer.session
            .send_packet(mac, r#"{"type":"ring"}"#)
            .await
            .unwrap();
        tested
            .expect_notice(|notice| matches!(notice, CoreNotice::Ring { .. }))
            .await;
        tested
            .wait(|snapshot| snapshot.ringing.as_deref() == Some(phone.as_str()))
            .await;
        tested.core.found_it();
        tested
            .expect_notice(|notice| *notice == CoreNotice::RingEnded)
            .await;
        tested.wait(|snapshot| snapshot.ringing.is_none()).await;

        peer.session
            .send_packet(mac, r#"{"type":"share.url","url":"https://example.org/b"}"#)
            .await
            .unwrap();
        let link = tested
            .expect_notice(|notice| matches!(notice, CoreNotice::Link { .. }))
            .await;
        assert_eq!(
            link,
            CoreNotice::Link {
                from: phone.clone(),
                device: Some("Pixel".into()),
                url: "https://example.org/b".into()
            }
        );
        peer.session
            .send_packet(mac, r#"{"type":"notification","id":"n2","app":"Chat","title":"Ann","text":"Hi","dismissable":true,"actions":["Like"],"reply":true}"#)
            .await
            .unwrap();
        let notification = tested
            .expect_notice(|notice| matches!(notice, CoreNotice::Notification { id, .. } if id == "n2"))
            .await;
        assert_eq!(
            notification,
            CoreNotice::Notification {
                from: phone.clone(),
                device: Some("Pixel".into()),
                id: "n2".into(),
                app: "Chat".into(),
                title: "Ann".into(),
                text: "Hi".into(),
                dismissable: true,
                actions: vec!["Like".into()],
                reply: true,
                // Within seconds of a notification.request: one the phone
                // already had, which does not pop up again.
                silent: true,
            }
        );
        peer.session
            .send_packet(mac, r#"{"type":"notification.removed","id":"n2"}"#)
            .await
            .unwrap();
        tested
            .expect_notice(|notice| matches!(notice, CoreNotice::NotificationRemoved { id, .. } if id == "n2"))
            .await;
        peer.session
            .send_packet(mac, r#"{"type":"call","event":"ringing","name":"Ann"}"#)
            .await
            .unwrap();
        tested
            .expect_notice(|notice| matches!(notice, CoreNotice::Call { event, .. } if event == "ringing"))
            .await;
        peer.session
            .send_packet(mac, r#"{"type":"volume.set","level":20}"#)
            .await
            .unwrap();
        let set = tested
            .expect_notice(|notice| matches!(notice, CoreNotice::SetVolume { .. }))
            .await;
        assert_eq!(
            set,
            CoreNotice::SetVolume {
                from: phone.clone(),
                level: Some(20),
                muted: None
            }
        );
        // Not implemented on the Mac: a media command for its own players
        // is never announced, so it never comes; one that does is ignored
        // by the app (the core still reports it).
        tested.blocking(|core| core.stop()).await;
    })
    .await;
}

#[tokio::test]
async fn sends_and_receives_files() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut peer) = paired(&url).await;
        let mac = tested.id();
        let phone = weft_cli::hex(&peer.id());
        peer.expect_packet(|packet| matches!(packet, Packet::Capabilities(_)))
            .await;

        // Phone -> Mac: lands in the Downloads folder of the config.
        let id = [0x5a; 16];
        let offer = format!(
            r#"{{"type":"file.offer","id":"{}","name":"photo.jpg","size":5}}"#,
            weft_cli::hex(&id)
        );
        peer.session.send_packet(mac, &offer).await.unwrap();
        peer.expect_packet(|packet| matches!(packet, Packet::FileAccept { offset: 0, .. }))
            .await;
        peer.session
            .send_file_chunk(mac, id, 0, b"hello")
            .await
            .unwrap();
        peer.expect_packet(|packet| matches!(packet, Packet::FileDone { .. }))
            .await;
        let received = tested
            .expect_notice(|notice| matches!(notice, CoreNotice::FileReceived { .. }))
            .await;
        let CoreNotice::FileReceived { from, path, .. } = received else {
            unreachable!();
        };
        assert_eq!(from, phone);
        let downloads = tested.temp.path().join("Downloads");
        assert_eq!(std::path::Path::new(&path), downloads.join("photo.jpg"));
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
        tested
            .wait(|snapshot| {
                snapshot
                    .transfers
                    .iter()
                    .any(|transfer| !transfer.outgoing && transfer.stage == TransferStage::Done)
            })
            .await;

        // Mac -> phone: offered, sent in chunks, done.
        let source = tested.temp.path().join("notes.txt");
        std::fs::write(&source, b"weft notes").unwrap();
        tested
            .core
            .send_files(phone.clone(), vec![source.to_string_lossy().into_owned()])
            .unwrap();
        let Packet::FileOffer(offer) = peer
            .expect_packet(|packet| matches!(packet, Packet::FileOffer(_)))
            .await
        else {
            unreachable!();
        };
        assert_eq!((offer.name.as_str(), offer.size), ("notes.txt", 10));
        let accept = format!(r#"{{"type":"file.accept","id":"{}","offset":0}}"#, offer.id);
        peer.session.send_packet(mac, &accept).await.unwrap();
        let chunk = peer
            .expect(|event| matches!(event, Event::FileChunk { .. }))
            .await;
        let Event::FileChunk { data, .. } = chunk else {
            unreachable!();
        };
        assert_eq!(data, b"weft notes");
        let ack = format!(r#"{{"type":"file.ack","id":"{}","offset":10}}"#, offer.id);
        peer.session.send_packet(mac, &ack).await.unwrap();
        let done = format!(r#"{{"type":"file.done","id":"{}"}}"#, offer.id);
        peer.session.send_packet(mac, &done).await.unwrap();
        let snapshot = tested
            .wait(|snapshot| {
                snapshot
                    .transfers
                    .iter()
                    .any(|transfer| transfer.outgoing && transfer.stage == TransferStage::Done)
            })
            .await;
        let sent = snapshot
            .transfers
            .iter()
            .find(|transfer| transfer.outgoing)
            .unwrap();
        assert_eq!(
            (sent.size, sent.done, sent.device_id.as_str()),
            (10, 10, phone.as_str())
        );

        // A cancelled offer: the peer hears of it.
        std::fs::write(&source, vec![7u8; 300_000]).unwrap();
        tested
            .core
            .send_files(phone.clone(), vec![source.to_string_lossy().into_owned()])
            .unwrap();
        let Packet::FileOffer(offer) = peer
            .expect_packet(|packet| matches!(packet, Packet::FileOffer(_)))
            .await
        else {
            unreachable!();
        };
        tested.core.cancel_transfer(offer.id.clone());
        peer.expect_packet(|packet| {
            *packet
                == Packet::FileCancel {
                    id: offer.id.clone(),
                }
        })
        .await;
        tested
            .wait(|snapshot| {
                snapshot
                    .transfers
                    .iter()
                    .any(|transfer| transfer.stage == TransferStage::Cancelled)
            })
            .await;
        tested.blocking(|core| core.stop()).await;
    })
    .await;
}
