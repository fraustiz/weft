//! The worker against a real relay, with a plain `Session` as the other
//! device and a list in memory in place of the clipboard.
//!
//! The relay runs on the test's runtime and the worker on its own thread,
//! so the tests never block: they poll the snapshot between awaits.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use tempfile::TempDir;
use tokio::net::TcpListener;
use weft_cli::{Error, Event, Session, State, StateDir};
use weft_protocol::PairingCode;
use weft_relay::Config;
use weft_windows::worker::{
    Command, Device, Direction, Handle, PairingFailure, PairingState, Snapshot, Status, spawn_on,
};

/// Timeout for a single step.
const STEP: Duration = Duration::from_secs(10);
/// How long to wait before concluding that nothing arrives.
const QUIET: Duration = Duration::from_millis(500);
/// Timeout for a whole test.
const TEST: Duration = Duration::from_secs(60);

async fn timed<T>(body: impl Future<Output = T>) -> T {
    tokio::time::timeout(TEST, body)
        .await
        .expect("test timed out")
}

/// Starts a relay announcing `relay_name`, with groups in memory, that runs
/// until the test's runtime stops. Returns its url.
async fn start_relay_named(relay_name: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(weft_relay::serve(
        listener,
        Config::new(relay_name, ":memory:"),
    ));
    format!("ws://{addr}/v1")
}

async fn start_relay() -> String {
    start_relay_named("127.0.0.1").await
}

fn temp_dir() -> TempDir {
    tempfile::Builder::new()
        .prefix("weft-test-")
        .tempdir()
        .unwrap()
}

/// A plain device: a state directory and the session on it.
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
            .expect("connect timed out")
            .expect("connect failed");
        Peer {
            _temp: temp,
            session,
        }
    }

    fn id(&self) -> [u8; 16] {
        self.session.device_id()
    }

    async fn next_event(&mut self) -> Event {
        self.session
            .next_event(Some(STEP))
            .await
            .expect("connection failed")
            .expect("no event before the timeout")
    }

    async fn expect_text(&mut self, from: [u8; 16], text: &str) {
        let text = text.to_owned();
        assert_eq!(self.next_event().await, Event::Clipboard { from, text });
    }

    async fn expect_name(&mut self, from: [u8; 16], name: &str) {
        let name = name.to_owned();
        assert_eq!(self.next_event().await, Event::DeviceName { from, name });
    }

    /// Waits for `wanted`, skipping what comes from `other` before it.
    ///
    /// A device that just joined sends its name to the members while the
    /// device that let it in tells them about it: the two arrive in either
    /// order, and a name that comes first is dropped as from a stranger.
    async fn expect_skipping(&mut self, other: [u8; 16], wanted: Event) {
        loop {
            let event = self.next_event().await;
            if event == wanted {
                return;
            }
            let from_other = match &event {
                Event::DeviceName { from, .. } => *from == other,
                Event::Dropped { from, .. } => *from == Some(other),
                _ => false,
            };
            assert!(from_other, "unexpected {event:?}");
        }
    }
}

/// The worker under test: its state directory, its handle, its thread and
/// the texts it asked to put on the clipboard.
struct Tested {
    _temp: TempDir,
    dir: StateDir,
    worker: Handle,
    thread: JoinHandle<()>,
    clipboard: Arc<Mutex<Vec<String>>>,
}

impl Tested {
    /// Starts a worker on a fresh state directory, which it creates.
    fn start(relay_url: &str) -> Tested {
        let temp = temp_dir();
        let dir = StateDir::new(temp.path().join("state"));
        let clipboard = Arc::new(Mutex::new(Vec::new()));
        let sink = clipboard.clone();
        let (worker, thread) = spawn_on(dir.clone(), relay_url, move |text| {
            sink.lock().unwrap().push(text);
        })
        .unwrap();
        Tested {
            _temp: temp,
            dir,
            worker,
            thread,
            clipboard,
        }
    }

    /// A connected worker named "Test PC".
    async fn connected(relay_url: &str) -> Tested {
        let tested = Tested::start(relay_url);
        tested.wait(Snapshot::is_connected).await;
        tested.worker.send(Command::Rename("Test PC".into()));
        tested.wait(|snapshot| snapshot.name == "Test PC").await;
        tested
    }

    fn id(&self) -> [u8; 16] {
        self.worker.snapshot().device_id
    }

    /// Waits until the snapshot satisfies `wanted`.
    async fn wait(&self, wanted: impl Fn(&Snapshot) -> bool) -> Arc<Snapshot> {
        let found = async {
            loop {
                let snapshot = self.worker.snapshot();
                if wanted(&snapshot) {
                    return snapshot;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        match tokio::time::timeout(STEP, found).await {
            Ok(snapshot) => snapshot,
            Err(_) => panic!("still waiting, with {:?}", self.worker.snapshot()),
        }
    }

    /// Waits until the texts given to the clipboard are exactly `wanted`.
    async fn expect_clipboard(&self, wanted: &[&str]) {
        let found = async {
            while *self.clipboard.lock().unwrap() != wanted {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        if tokio::time::timeout(STEP, found).await.is_err() {
            panic!("the clipboard got {:?}", self.clipboard.lock().unwrap());
        }
    }

    /// Asks for a pairing code and returns it once the snapshot shows it.
    async fn show_code(&self) -> PairingCode {
        self.worker.send(Command::StartPairing);
        let snapshot = self
            .wait(|snapshot| matches!(snapshot.pairing, PairingState::ShowingCode { .. }))
            .await;
        let PairingState::ShowingCode { code, expires_at } = &snapshot.pairing else {
            unreachable!();
        };
        assert!(*expires_at > std::time::Instant::now());
        PairingCode::decode(code).unwrap()
    }

    /// Pairs `peer` into the worker's group through a code from the
    /// snapshot, and lets each side receive the other's name.
    async fn pair(&self, peer: &mut Peer, peer_name: &str) -> PairingCode {
        let code = self.show_code().await;
        peer.session.join(&code, STEP).await.unwrap();
        let joined = peer.id();
        self.wait(|snapshot| {
            snapshot.pairing == PairingState::Success { device: joined }
                && snapshot
                    .devices
                    .iter()
                    .any(|device| device.id == joined && device.name.as_deref() == Some(peer_name))
        })
        .await;
        peer.expect_name(self.id(), "Test PC").await;
        code
    }

    /// Shuts the worker down and waits for its thread.
    async fn stop(self) {
        self.worker.send(Command::Shutdown);
        tokio::task::spawn_blocking(move || self.thread.join().unwrap())
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn pairs_and_exchanges_text_both_ways() {
    timed(async {
        let url = start_relay().await;
        let tested = Tested::connected(&url).await;
        let snapshot = tested.worker.snapshot();
        assert_eq!(snapshot.relay_url, url);
        assert_eq!(
            snapshot.device_id,
            tested.dir.load().unwrap().identity().device_id()
        );
        assert!(snapshot.devices.is_empty());
        assert_eq!(snapshot.pairing, PairingState::Idle);
        assert_eq!(snapshot.last_activity, None);

        let mut phone = Peer::connect(&url, "Test phone").await;
        tested.pair(&mut phone, "Test phone").await;
        let snapshot = tested.worker.snapshot();
        assert_eq!(
            snapshot.devices,
            [Device {
                id: phone.id(),
                name: Some("Test phone".into()),
            }]
        );

        // From the other device to the clipboard.
        phone.session.send_text("from the phone").await.unwrap();
        tested.expect_clipboard(&["from the phone"]).await;
        let received = tested
            .wait(|snapshot| snapshot.last_activity.is_some())
            .await
            .last_activity
            .unwrap();
        assert_eq!(received.direction, Direction::Received);

        // From this device to the other one.
        tested
            .worker
            .send(Command::SendText("from the PC ✓".into()));
        phone.expect_text(tested.id(), "from the PC ✓").await;
        let sent = tested
            .wait(|snapshot| snapshot.last_activity != Some(received))
            .await
            .last_activity
            .unwrap();
        assert_eq!(sent.direction, Direction::Sent);
        assert!(sent.at >= received.at);

        // Paused: nothing in, nothing out, and nothing kept for later.
        tested.worker.send(Command::SetPaused(true));
        tested.wait(|snapshot| snapshot.pause.all).await;
        phone.session.send_text("while paused").await.unwrap();
        tested.worker.send(Command::SendText("while paused".into()));
        tested.worker.send(Command::SetPaused(false));
        tested.wait(|snapshot| !snapshot.pause.all).await;
        phone.session.send_text("after the pause").await.unwrap();
        tested
            .expect_clipboard(&["from the phone", "after the pause"])
            .await;
        tested.worker.send(Command::SendText("resumed".into()));
        phone.expect_text(tested.id(), "resumed").await;

        tested.stop().await;
    })
    .await;
}

#[tokio::test]
async fn pairing_can_be_cancelled_and_devices_removed() {
    timed(async {
        let url = start_relay().await;
        let tested = Tested::connected(&url).await;
        let mut phone = Peer::connect(&url, "Test phone").await;
        tested.pair(&mut phone, "Test phone").await;

        // Text that arrives while a code is shown is not held back.
        let cancelled = tested.show_code().await;
        phone.session.send_text("during pairing").await.unwrap();
        tested.expect_clipboard(&["during pairing"]).await;
        assert!(matches!(
            tested.worker.snapshot().pairing,
            PairingState::ShowingCode { .. }
        ));
        // And text is still sent.
        tested
            .worker
            .send(Command::SendText("still sending".into()));
        phone.expect_text(tested.id(), "still sending").await;

        // Once cancelled, the code no longer lets a device in.
        tested.worker.send(Command::CancelPairing);
        tested
            .wait(|snapshot| snapshot.pairing == PairingState::Idle)
            .await;
        let mut tablet = Peer::connect(&url, "Test tablet").await;
        let joined = tablet.session.join(&cancelled, QUIET).await;
        assert!(matches!(joined, Err(Error::Timeout(_))), "{joined:?}");
        assert_eq!(tested.worker.snapshot().devices.len(), 1);
        assert_eq!(tested.dir.load().unwrap().devices.len(), 1);

        // A new code works, and is a different one.
        let code = tested.pair(&mut tablet, "Test tablet").await;
        assert_ne!(code.pairing_id, cancelled.pairing_id);
        let snapshot = tested.wait(|snapshot| snapshot.devices.len() == 2).await;
        assert!(
            snapshot
                .devices
                .iter()
                .any(|device| device.id == tablet.id())
        );
        let added = Event::DeviceAdded {
            by: tested.id(),
            device: tablet.id(),
        };
        phone.expect_skipping(tablet.id(), added).await;

        // Dismissing the result brings the pairing back to idle.
        tested.worker.send(Command::CancelPairing);
        tested
            .wait(|snapshot| snapshot.pairing == PairingState::Idle)
            .await;

        // A removed device disappears from the snapshot and from the state.
        tested.worker.send(Command::RemoveDevice(tablet.id()));
        let snapshot = tested.wait(|snapshot| snapshot.devices.len() == 1).await;
        assert_eq!(snapshot.devices[0].id, phone.id());
        assert_eq!(tested.dir.load().unwrap().devices.len(), 1);
        let removed = Event::DeviceRemoved {
            by: tested.id(),
            device: tablet.id(),
        };
        phone.expect_skipping(tablet.id(), removed).await;

        tested.stop().await;
    })
    .await;
}

#[tokio::test]
async fn joins_a_group_from_a_pasted_code() {
    timed(async {
        let url = start_relay().await;
        let tested = Tested::connected(&url).await;
        let mut phone = Peer::connect(&url, "Test phone").await;

        let fails = |code: String, failure: PairingFailure| {
            tested.worker.send(Command::Join(code));
            tested.wait(move |snapshot| snapshot.pairing == PairingState::Failed(failure.clone()))
        };
        fails("WEFT1:not-a-code".into(), PairingFailure::InvalidCode).await;
        let own = tested.show_code().await;
        fails(own.encode().unwrap(), PairingFailure::OwnCode).await;

        let code = phone.session.open_pairing().await.unwrap();
        let elsewhere = PairingCode {
            relay_url: "wss://relay.example/v1".into(),
            ..code.clone()
        };
        fails(elsewhere.encode().unwrap(), PairingFailure::OtherRelay).await;
        // A slot the relay does not know, as after it expired.
        let unknown = PairingCode {
            pairing_id: [7; 16],
            ..code.clone()
        };
        fails(unknown.encode().unwrap(), PairingFailure::Expired).await;

        // Pasted codes often come with whitespace around them.
        let pasted = format!("  {}\n", code.encode().unwrap());
        tested.worker.send(Command::Join(pasted));
        let added = phone.session.wait_pair_request(&code, STEP).await.unwrap();
        assert_eq!(added, tested.id());
        let owner = phone.id();
        tested
            .wait(|snapshot| {
                snapshot.pairing == PairingState::Success { device: owner }
                    && snapshot.devices
                        == [Device {
                            id: owner,
                            name: Some("Test phone".into()),
                        }]
            })
            .await;
        phone.expect_name(tested.id(), "Test PC").await;

        phone.session.send_text("to the new member").await.unwrap();
        tested.expect_clipboard(&["to the new member"]).await;
        tested.stop().await;
    })
    .await;
}

#[tokio::test]
async fn reconnects_after_the_relay_drops_the_connection() {
    timed(async {
        let url = start_relay().await;
        let tested = Tested::connected(&url).await;
        let mut phone = Peer::connect(&url, "Test phone").await;
        tested.pair(&mut phone, "Test phone").await;

        // Blocks until a newer snapshot is published.
        let before = tested.worker.published().generation;
        let waiter = {
            let worker = tested.worker.clone();
            tokio::task::spawn_blocking(move || worker.wait_newer(before))
        };

        // The relay keeps one connection per device: a second one with the
        // same identity makes it close the worker's.
        let intruder = Session::connect(tested.dir.clone()).await.unwrap();
        let offline = tested
            .wait(|snapshot| matches!(snapshot.status, Status::Offline { .. }))
            .await;
        let Status::Offline { retry_at, .. } = &offline.status else {
            unreachable!();
        };
        assert!(*retry_at > std::time::Instant::now());
        let newer = waiter.await.unwrap().expect("the worker is running");
        assert!(newer.generation > before);

        // It comes back by itself, with its devices, and works again.
        tested.wait(Snapshot::is_connected).await;
        drop(intruder);
        assert_eq!(tested.worker.snapshot().devices.len(), 1);
        phone
            .session
            .send_text("after the reconnection")
            .await
            .unwrap();
        tested.expect_clipboard(&["after the reconnection"]).await;
        tested.worker.send(Command::SendText("back".into()));
        phone.expect_text(tested.id(), "back").await;

        // Once stopped, there is nothing newer to wait for.
        let worker = tested.worker.clone();
        tested.stop().await;
        let last = worker.published().generation;
        assert!(worker.wait_newer(last).is_none());
    })
    .await;
}

#[tokio::test]
async fn stays_usable_while_the_relay_is_unreachable() {
    timed(async {
        // A port nothing listens on.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/v1", listener.local_addr().unwrap());
        drop(listener);

        let tested = Tested::start(&url);
        tested
            .wait(|snapshot| matches!(snapshot.status, Status::Offline { .. }))
            .await;
        tested.worker.send(Command::StartPairing);
        tested
            .wait(|snapshot| snapshot.pairing == PairingState::Failed(PairingFailure::NotConnected))
            .await;
        tested.worker.send(Command::CancelPairing);
        tested.worker.send(Command::Rename("Test PC".into()));
        tested.worker.send(Command::SetPaused(true));
        tested.worker.send(Command::SendText("never sent".into()));
        let snapshot = tested
            .wait(|snapshot| snapshot.pause.all && snapshot.name == "Test PC")
            .await;
        assert_eq!(snapshot.pairing, PairingState::Idle);
        assert_eq!(snapshot.last_activity, None);
        assert_eq!(tested.dir.load().unwrap().name(), "Test PC");
        tested.stop().await;
    })
    .await;
}

#[tokio::test]
async fn a_relay_with_another_name_is_a_fatal_error() {
    timed(async {
        let url = start_relay_named("relay.example").await;
        let tested = Tested::start(&url);
        let snapshot = tested
            .wait(|snapshot| matches!(snapshot.status, Status::Fatal(_)))
            .await;
        assert!(!snapshot.is_connected());
        // No new attempt follows.
        tokio::time::sleep(QUIET).await;
        assert!(matches!(tested.worker.snapshot().status, Status::Fatal(_)));
        tested.stop().await;
    })
    .await;
}

#[test]
fn the_pairing_code_never_shows_in_debug_output() {
    let state = PairingState::ShowingCode {
        code: "WEFT1:secret".into(),
        expires_at: std::time::Instant::now(),
    };
    assert_eq!(format!("{state:?}"), "ShowingCode");
}
