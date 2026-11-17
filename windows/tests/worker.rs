//! The worker against a real relay, with a plain `Session` as the other
//! device and a list in memory in place of the clipboard.
//!
//! The relay runs on the test's runtime and the worker on its own thread,
//! so the tests never block: they poll the snapshot between awaits.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use tempfile::TempDir;
use tokio::net::TcpListener;
use weft_cli::{Error, Event, Session, State, StateDir};
use weft_protocol::PairingCode;
use weft_relay::Config;
use weft_windows::features::{Features, Settings, UserCommand};
use weft_windows::input::{Button, Input, Key, Modifiers, Press};
use weft_windows::packets::{
    self, Battery, Call, Capabilities, Connectivity, FileOffer, MediaAction, MediaCommand,
    Notification, Packet, Parsed, Player, Volume, VolumeSet, transfer_id,
};
use weft_windows::sms::SendState;
use weft_windows::transfer::{CHUNK, Stage, WINDOW};
use weft_windows::worker::{
    Command, Device, Direction, Handle, Hooks, Notice, PairingFailure, PairingState, Snapshot,
    Status, spawn_on,
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
    dir: StateDir,
    session: Session,
}

impl Peer {
    async fn connect(relay_url: &str, name: &str) -> Peer {
        let temp = temp_dir();
        let dir = StateDir::new(temp.path().join("state"));
        let mut state = State::generate(relay_url).unwrap();
        state.set_name(name).unwrap();
        dir.create(&state, false).unwrap();
        let session = tokio::time::timeout(STEP, Session::connect(dir.clone()))
            .await
            .expect("connect timed out")
            .expect("connect failed");
        Peer {
            _temp: temp,
            dir,
            session,
        }
    }

    /// Drops the connection and opens a new one, as after a network loss.
    async fn reconnect(&mut self) {
        let session = tokio::time::timeout(STEP, Session::connect(self.dir.clone()))
            .await
            .expect("connect timed out")
            .expect("connect failed");
        let old = std::mem::replace(&mut self.session, session);
        drop(old);
    }

    /// The next event of any kind, packets included.
    async fn next_any(&mut self) -> Event {
        self.session
            .next_event(Some(STEP))
            .await
            .expect("connection failed")
            .expect("no event before the timeout")
    }

    async fn send(&mut self, to: [u8; 16], packet: &Packet) {
        self.session
            .send_packet(to, &packet.to_json())
            .await
            .unwrap();
    }

    /// Waits for a packet from `from` that `wanted` accepts, skipping
    /// everything else.
    async fn expect_packet(&mut self, from: [u8; 16], wanted: impl Fn(&Packet) -> bool) -> Packet {
        loop {
            if let Event::Packet { from: sender, json } = self.next_any().await
                && sender == from
                && let Parsed::Known(packet) = packets::parse(&json)
                && wanted(&packet)
            {
                return packet;
            }
        }
    }

    fn id(&self) -> [u8; 16] {
        self.session.device_id()
    }

    /// The next event that is not a packet: the worker sends its
    /// capabilities and state requests on its own.
    async fn next_event(&mut self) -> Event {
        loop {
            match self.next_any().await {
                Event::Packet { .. } => continue,
                event => return event,
            }
        }
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
    notices: Arc<Mutex<Vec<Notice>>>,
    downloads: PathBuf,
    /// What reached this computer's pointer and keyboard, in place of
    /// `SendInput`.
    inputs: Arc<Mutex<Vec<Input>>>,
    /// The command lines run, in place of `cmd`.
    ran: Arc<Mutex<Vec<String>>>,
}

impl Tested {
    /// Starts a worker on a fresh state directory, which it creates.
    fn start(relay_url: &str) -> Tested {
        let temp = temp_dir();
        let dir = StateDir::new(temp.path().join("state"));
        let clipboard = Arc::new(Mutex::new(Vec::new()));
        let notices = Arc::new(Mutex::new(Vec::new()));
        let inputs = Arc::new(Mutex::new(Vec::new()));
        let ran = Arc::new(Mutex::new(Vec::new()));
        let downloads = temp.path().join("Downloads");
        let (sink, notice_sink) = (clipboard.clone(), notices.clone());
        let (input_sink, runner) = (inputs.clone(), ran.clone());
        let hooks = Hooks {
            on_text: Box::new(move |text| sink.lock().unwrap().push(text)),
            on_notice: Arc::new(move |notice| notice_sink.lock().unwrap().push(notice)),
            downloads: downloads.clone(),
            on_input: Box::new(move |input| input_sink.lock().unwrap().push(input)),
            run_command: Box::new(move |line| runner.lock().unwrap().push(line.to_owned())),
        };
        let (worker, thread) = spawn_on(dir.clone(), relay_url, hooks).unwrap();
        Tested {
            _temp: temp,
            dir,
            worker,
            thread,
            clipboard,
            notices,
            downloads,
            inputs,
            ran,
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

    /// Waits for a notice `wanted` accepts, and takes it out.
    async fn expect_notice(&self, wanted: impl Fn(&Notice) -> bool) -> Notice {
        let found = async {
            loop {
                {
                    let mut notices = self.notices.lock().unwrap();
                    if let Some(index) = notices.iter().position(&wanted) {
                        return notices.remove(index);
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        match tokio::time::timeout(STEP, found).await {
            Ok(notice) => notice,
            Err(_) => panic!("notices: {:?}", self.notices.lock().unwrap()),
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

// Features: capabilities, status, ring, links and files
// (`docs/features.md` sections 2 to 4), with the plain session playing
// the phone.

/// What a phone running Weft would announce.
fn phone_capabilities() -> Packet {
    let list = |kinds: &[&str]| kinds.iter().map(|kind| kind.to_string()).collect();
    Packet::Capabilities(Capabilities {
        platform: "android".into(),
        incoming: list(&["ring", "ring.stop", "ping", "share.url", "file.offer"]),
        outgoing: list(&["battery", "connectivity", "ring", "file.offer"]),
    })
}

fn is_capabilities(packet: &Packet) -> bool {
    matches!(packet, Packet::Capabilities(_))
}

/// Bytes that differ from one position to the next, `len` of them.
fn content(len: usize, seed: u32) -> Vec<u8> {
    let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect()
}

impl Peer {
    /// The next event, or `None` if nothing comes for a while.
    async fn try_next(&mut self, wait: Duration) -> Option<Event> {
        self.session.next_event(Some(wait)).await.unwrap()
    }

    async fn expect_offer(&mut self, from: [u8; 16]) -> FileOffer {
        match self
            .expect_packet(from, |packet| matches!(packet, Packet::FileOffer(_)))
            .await
        {
            Packet::FileOffer(offer) => offer,
            _ => unreachable!(),
        }
    }

    /// Receives the chunks of `offer` that come in order into `have`,
    /// acknowledging each, until it holds `until` bytes.
    async fn receive_chunks(
        &mut self,
        from: [u8; 16],
        offer: &FileOffer,
        have: &mut Vec<u8>,
        until: u64,
    ) {
        while (have.len() as u64) < until {
            if let Event::FileChunk {
                from: sender,
                transfer_id: id,
                offset,
                data,
            } = self.next_any().await
            {
                assert_eq!(sender, from);
                assert_eq!(transfer_id(&id), offer.id);
                if offset == have.len() as u64 {
                    have.extend_from_slice(&data);
                    let ack = Packet::FileAck {
                        id: offer.id.clone(),
                        offset: have.len() as u64,
                    };
                    self.send(from, &ack).await;
                }
            }
        }
    }

    /// Sends `data` from `offset` as the transfer `id`, keeping at most
    /// [`WINDOW`] chunks unacknowledged, until `until` bytes are confirmed
    /// or the receiver says done. Returns the bytes confirmed.
    async fn send_chunks(
        &mut self,
        to: [u8; 16],
        id: [u8; 16],
        data: &[u8],
        offset: u64,
        until: u64,
    ) -> u64 {
        let (mut acked, mut sent) = (offset, offset);
        let len = data.len() as u64;
        while acked < until {
            while sent < len && sent - acked < WINDOW * CHUNK {
                let end = (sent + CHUNK).min(len);
                self.session
                    .send_file_chunk(to, id, sent, &data[sent as usize..end as usize])
                    .await
                    .unwrap();
                sent = end;
            }
            let wanted = |packet: &Packet| {
                matches!(packet, Packet::FileAck { .. } | Packet::FileDone { .. })
            };
            match self.expect_packet(to, wanted).await {
                Packet::FileAck { offset, .. } => acked = acked.max(offset),
                _ => return len,
            }
        }
        acked
    }

    /// Sends a whole file to `to` and returns once it says done.
    async fn send_file(&mut self, to: [u8; 16], name: &str, data: &[u8]) {
        let id = rand_id();
        let offer = Packet::FileOffer(FileOffer {
            id: transfer_id(&id),
            name: name.into(),
            size: data.len() as u64,
            mime: None,
        });
        self.send(to, &offer).await;
        let accepted = self
            .expect_packet(to, |packet| matches!(packet, Packet::FileAccept { .. }))
            .await;
        assert_eq!(
            accepted,
            Packet::FileAccept {
                id: transfer_id(&id),
                offset: 0
            }
        );
        if !data.is_empty() {
            self.send_chunks(to, id, data, 0, data.len() as u64).await;
        }
        self.expect_packet(to, |packet| matches!(packet, Packet::FileDone { .. }))
            .await;
    }
}

fn rand_id() -> [u8; 16] {
    weft_protocol::random_message_id()
}

impl Tested {
    /// Waits for the notice of a received file, and returns its path.
    async fn received(&self) -> PathBuf {
        let notice = self
            .expect_notice(|notice| matches!(notice, Notice::FileReceived { .. }))
            .await;
        let Notice::FileReceived { path, .. } = notice else {
            unreachable!();
        };
        path
    }

    fn partial_files(&self) -> Vec<PathBuf> {
        let dir = self.dir.path().join("partial");
        std::fs::read_dir(dir)
            .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
            .unwrap_or_default()
    }
}

/// A connected worker and a phone paired with it, past the first
/// capabilities.
async fn paired(url: &str) -> (Tested, Peer) {
    let tested = Tested::connected(url).await;
    let mut phone = Peer::connect(url, "Test phone").await;
    tested.pair(&mut phone, "Test phone").await;
    let packet = phone.expect_packet(tested.id(), is_capabilities).await;
    let Packet::Capabilities(capabilities) = packet else {
        unreachable!();
    };
    assert_eq!(capabilities, Features::default().capabilities(false));
    (tested, phone)
}

#[tokio::test]
async fn pings_and_links_go_to_the_phone() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut phone) = paired(&url).await;
        let (pc, phone_id) = (tested.id(), phone.id());
        phone.send(pc, &phone_capabilities()).await;
        phone.expect_packet(pc, is_capabilities).await;

        let ping = Packet::Ping { message: None };
        tested
            .worker
            .send(Command::SendPacket(phone_id, ping.clone()));
        phone.expect_packet(pc, |packet| *packet == ping).await;
        let link = Packet::ShareUrl {
            url: "https://example.org/a?b=c".into(),
        };
        tested
            .worker
            .send(Command::SendPacket(phone_id, link.clone()));
        phone.expect_packet(pc, |packet| *packet == link).await;

        // Not a web link, or links switched off: nothing goes.
        let bad = Packet::ShareUrl {
            url: "file:///C:/secret.txt".into(),
        };
        tested.worker.send(Command::SendPacket(phone_id, bad));
        let off = Features {
            links: false,
            ..Features::default()
        };
        tested.worker.send(Command::SetFeatures(off));
        tested
            .worker
            .send(Command::SendPacket(phone_id, link.clone()));
        tested.worker.send(Command::Ring(phone_id));
        let seen = packets_until(&mut phone, pc, |packet| *packet == Packet::Ring {}).await;
        assert!(
            !seen
                .iter()
                .any(|packet| matches!(packet, Packet::ShareUrl { .. })),
            "{seen:?}"
        );
        tested.stop().await;
    })
    .await;
}

#[tokio::test]
async fn exchanges_capabilities_and_status() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut phone) = paired(&url).await;
        let pc = tested.id();
        let phone_id = phone.id();

        // Its capabilities are kept, and answered once.
        phone.send(pc, &phone_capabilities()).await;
        let snapshot = tested
            .wait(|snapshot| snapshot.peer(&phone_id).capabilities.is_some())
            .await;
        assert!(snapshot.peer(&phone_id).handles("ring"));
        assert!(!snapshot.peer(&phone_id).handles("media"));
        phone.expect_packet(pc, is_capabilities).await;

        // A state request gets them again.
        phone.send(pc, &Packet::StateRequest {}).await;
        phone.expect_packet(pc, is_capabilities).await;

        // Battery and network reach the snapshot; values out of range are
        // unknown; unknown types and broken packets change nothing.
        phone
            .session
            .send_packet(pc, r#"{"type":"media","players":[]}"#)
            .await
            .unwrap();
        phone.session.send_packet(pc, "not json").await.unwrap();
        let battery = Battery {
            level: Some(82),
            charging: Some(true),
            low: Some(false),
        };
        phone.send(pc, &Packet::Battery(battery)).await;
        let network = Connectivity {
            network: Some("LTE".into()),
            signal: Some(9),
        };
        phone.send(pc, &Packet::Connectivity(network)).await;
        let snapshot = tested
            .wait(|snapshot| snapshot.peer(&phone_id).connectivity.is_some())
            .await;
        let peer = snapshot.peer(&phone_id);
        assert_eq!(peer.battery, Some(battery));
        assert_eq!(
            peer.connectivity,
            Some(Connectivity {
                network: Some("LTE".into()),
                signal: None,
            })
        );
        phone
            .session
            .send_packet(pc, r#"{"type":"battery","level":120,"charging":false}"#)
            .await
            .unwrap();
        tested
            .wait(|snapshot| {
                snapshot.peer(&phone_id).battery
                    == Some(Battery {
                        level: None,
                        charging: Some(false),
                        low: None,
                    })
            })
            .await;

        // Switched off: left out of the capabilities sent at once, and the
        // status is forgotten and no longer taken.
        let off = Features {
            battery: false,
            ..Features::default()
        };
        tested.worker.send(Command::SetFeatures(off));
        let packet = phone.expect_packet(pc, is_capabilities).await;
        assert_eq!(packet, Packet::Capabilities(off.capabilities(false)));
        tested
            .wait(|snapshot| {
                !snapshot.features.battery && snapshot.peer(&phone_id).battery.is_none()
            })
            .await;
        assert_eq!(Features::load(tested.dir.path()), off);
        phone.send(pc, &Packet::Battery(battery)).await;
        phone.send(pc, &Packet::StateRequest {}).await;
        phone.expect_packet(pc, is_capabilities).await;
        assert_eq!(tested.worker.snapshot().peer(&phone_id).battery, None);

        // After reconnecting, it asks every device for its state.
        let intruder = Session::connect(tested.dir.clone()).await.unwrap();
        tested
            .wait(|snapshot| matches!(snapshot.status, Status::Offline { .. }))
            .await;
        drop(intruder);
        tested.wait(Snapshot::is_connected).await;
        phone
            .expect_packet(pc, |packet| *packet == Packet::StateRequest {})
            .await;
        tested.stop().await;
    })
    .await;
}

#[tokio::test]
async fn rings_pings_and_opens_links() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut phone) = paired(&url).await;
        let (pc, phone_id) = (tested.id(), phone.id());
        let rang =
            |notice: &Notice| matches!(notice, Notice::Ring { from, .. } if *from == phone_id);
        let ended = |notice: &Notice| *notice == Notice::RingEnded;

        // Rung, then stopped by the device that rang.
        phone.send(pc, &Packet::Ring {}).await;
        tested.expect_notice(rang).await;
        tested
            .wait(|snapshot| snapshot.ringing == Some(phone_id))
            .await;
        phone.send(pc, &Packet::RingStop {}).await;
        tested.expect_notice(ended).await;
        tested.wait(|snapshot| snapshot.ringing.is_none()).await;

        // Rung, then found.
        phone.send(pc, &Packet::Ring {}).await;
        tested.expect_notice(rang).await;
        tested.worker.send(Command::FoundIt);
        tested.expect_notice(ended).await;
        tested.wait(|snapshot| snapshot.ringing.is_none()).await;

        // Ringing the phone, and stopping it.
        tested.worker.send(Command::Ring(phone_id));
        phone
            .expect_packet(pc, |packet| *packet == Packet::Ring {})
            .await;
        tested.worker.send(Command::StopRinging(phone_id));
        phone
            .expect_packet(pc, |packet| *packet == Packet::RingStop {})
            .await;

        // Pings and web links are shown; other links are not opened.
        let message = Some("hello".to_owned());
        phone
            .send(
                pc,
                &Packet::Ping {
                    message: message.clone(),
                },
            )
            .await;
        let ping = tested
            .expect_notice(|notice| matches!(notice, Notice::Ping { .. }))
            .await;
        assert_eq!(
            ping,
            Notice::Ping {
                from: phone_id,
                device: Some("Test phone".into()),
                message,
            }
        );
        for url in ["javascript:alert(1)", "file:///C:/Windows/notepad.exe"] {
            phone.send(pc, &Packet::ShareUrl { url: url.into() }).await;
        }
        let link = "https://example.com/page?q=1";
        phone.send(pc, &Packet::ShareUrl { url: link.into() }).await;
        let shared = tested
            .expect_notice(|notice| matches!(notice, Notice::Link { .. }))
            .await;
        assert_eq!(
            shared,
            Notice::Link {
                from: phone_id,
                device: Some("Test phone".into()),
                url: link.into(),
            }
        );

        // Switched off, rings and links are ignored.
        let off = Features {
            ring: false,
            links: false,
            ..Features::default()
        };
        tested.worker.send(Command::SetFeatures(off));
        phone.expect_packet(pc, is_capabilities).await;
        phone.send(pc, &Packet::Ring {}).await;
        phone.send(pc, &Packet::ShareUrl { url: link.into() }).await;
        // A packet answered after both, so both were handled.
        phone.send(pc, &Packet::StateRequest {}).await;
        phone.expect_packet(pc, is_capabilities).await;
        assert_eq!(tested.worker.snapshot().ringing, None);
        let notices = tested.notices.lock().unwrap().clone();
        assert!(
            !notices
                .iter()
                .any(|notice| matches!(notice, Notice::Ring { .. } | Notice::Link { .. })),
            "{notices:?}"
        );
        tested.stop().await;
    })
    .await;
}

#[tokio::test]
async fn files_go_both_ways_byte_for_byte() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut phone) = paired(&url).await;
        let (pc, phone_id) = (tested.id(), phone.id());

        // From the computer: at most 8 chunks before an acknowledgement.
        let data = content(3 * 1024 * 1024 + 12_345, 1);
        let path = tested._temp.path().join("report.bin");
        std::fs::write(&path, &data).unwrap();
        tested.worker.send(Command::SendFiles(phone_id, vec![path]));
        let offer = phone.expect_offer(pc).await;
        assert_eq!(offer.name, "report.bin");
        assert_eq!(offer.size, data.len() as u64);
        let accept = Packet::FileAccept {
            id: offer.id.clone(),
            offset: 0,
        };
        phone.send(pc, &accept).await;
        let mut have = Vec::new();
        let mut unacked = 0;
        while let Some(event) = phone.try_next(QUIET).await {
            if let Event::FileChunk { offset, data, .. } = event {
                assert_eq!(offset, have.len() as u64);
                assert_eq!(data.len() as u64, CHUNK);
                have.extend_from_slice(&data);
                unacked += 1;
            }
        }
        assert_eq!(unacked, WINDOW);
        let ack = Packet::FileAck {
            id: offer.id.clone(),
            offset: have.len() as u64,
        };
        phone.send(pc, &ack).await;
        phone
            .receive_chunks(pc, &offer, &mut have, data.len() as u64)
            .await;
        assert!(have == data, "the received file differs");
        phone
            .send(
                pc,
                &Packet::FileDone {
                    id: offer.id.clone(),
                },
            )
            .await;
        let snapshot = tested
            .wait(|snapshot| {
                snapshot
                    .transfers
                    .iter()
                    .any(|transfer| transfer.outgoing && transfer.stage == Stage::Done)
            })
            .await;
        assert_eq!(snapshot.transfers[0].device, phone_id);
        assert_eq!(snapshot.transfers[0].done, data.len() as u64);

        // To the computer: into Downloads once complete, nothing left
        // behind.
        let data = content(2 * 1024 * 1024 + 777, 2);
        phone.send_file(pc, "photo.jpg", &data).await;
        let path = tested.received().await;
        assert_eq!(path, tested.downloads.join("photo.jpg"));
        assert!(
            std::fs::read(&path).unwrap() == data,
            "the received file differs"
        );
        assert!(tested.partial_files().is_empty());
        let snapshot = tested
            .wait(|snapshot| {
                snapshot
                    .transfers
                    .iter()
                    .any(|transfer| !transfer.outgoing && transfer.stage == Stage::Done)
            })
            .await;
        assert!(
            snapshot
                .transfers
                .iter()
                .all(|transfer| transfer.device == phone_id)
        );

        // The same name again is not overwritten; an empty file is done at
        // once.
        phone.send_file(pc, "photo.jpg", b"second").await;
        let second = tested.received().await;
        assert_eq!(second, tested.downloads.join("photo (1).jpg"));
        assert_eq!(std::fs::read(&path).unwrap(), data);
        phone.send_file(pc, "empty.txt", b"").await;
        let empty = tested.received().await;
        assert_eq!(std::fs::read(empty).unwrap(), b"");
        tested.stop().await;
    })
    .await;
}

#[tokio::test]
async fn received_names_are_sanitised() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut phone) = paired(&url).await;
        let pc = tested.id();
        let name_of = |path: &Path| path.file_name().unwrap().to_string_lossy().into_owned();
        for (offered, saved) in [
            (r"..\..\Windows\CON.txt", "_CON.txt"),
            ("../../etc/passwd", "passwd"),
            ("..", "file"),
            ("tab\there?.txt. . ", "tabhere_.txt"),
            ("con.txt", "_con (1).txt"),
        ] {
            phone.send_file(pc, offered, b"data").await;
            let path = tested.received().await;
            assert_eq!(path.parent(), Some(tested.downloads.as_path()), "{offered}");
            assert_eq!(name_of(&path), saved, "{offered}");
        }
        tested.stop().await;
    })
    .await;
}

#[tokio::test]
async fn offers_are_rejected_with_a_reason_and_cancels_clean_up() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut phone) = paired(&url).await;
        let pc = tested.id();
        let offer = |id: [u8; 16], size: u64| {
            Packet::FileOffer(FileOffer {
                id: transfer_id(&id),
                name: "a.bin".into(),
                size,
                mime: None,
            })
        };
        let reply = |id: [u8; 16]| {
            let id = transfer_id(&id);
            move |packet: &Packet| match packet {
                Packet::FileAccept { id: of, .. } | Packet::FileReject { id: of, .. } => *of == id,
                _ => false,
            }
        };
        let rejected = |reason: &str, id: [u8; 16]| Packet::FileReject {
            id: transfer_id(&id),
            reason: Some(reason.into()),
        };

        // Over 4 GiB.
        let id = rand_id();
        phone.send(pc, &offer(id, 5 << 30)).await;
        assert_eq!(
            phone.expect_packet(pc, reply(id)).await,
            rejected("tooLarge", id)
        );

        // At most 4 at once from one device.
        let ids: Vec<[u8; 16]> = (0..5).map(|_| rand_id()).collect();
        for id in &ids {
            phone.send(pc, &offer(*id, 4 * 1024 * 1024)).await;
        }
        for id in &ids[..4] {
            let answer = phone.expect_packet(pc, reply(*id)).await;
            assert!(
                matches!(answer, Packet::FileAccept { offset: 0, .. }),
                "{answer:?}"
            );
        }
        assert_eq!(
            phone.expect_packet(pc, reply(ids[4])).await,
            rejected("busy", ids[4])
        );

        // Cancelled by the sender: the partial file goes.
        phone
            .send_chunks(pc, ids[0], &content(4 * 1024 * 1024, 3), 0, 2 * CHUNK)
            .await;
        assert_eq!(tested.partial_files().len(), 4);
        for id in &ids[..4] {
            phone
                .send(
                    pc,
                    &Packet::FileCancel {
                        id: transfer_id(id),
                    },
                )
                .await;
        }
        tested
            .wait(|snapshot| {
                snapshot
                    .transfers
                    .iter()
                    .filter(|transfer| transfer.stage == Stage::Cancelled)
                    .count()
                    == 4
            })
            .await;
        assert!(tested.partial_files().is_empty());

        // Switched off.
        let off = Features {
            files: false,
            ..Features::default()
        };
        tested.worker.send(Command::SetFeatures(off));
        phone.expect_packet(pc, is_capabilities).await;
        let id = rand_id();
        phone.send(pc, &offer(id, 10)).await;
        assert_eq!(
            phone.expect_packet(pc, reply(id)).await,
            rejected("disabled", id)
        );
        tested.stop().await;
    })
    .await;
}

#[tokio::test]
async fn a_file_resumes_after_the_computer_reconnects() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut phone) = paired(&url).await;
        let pc = tested.id();
        let data = content(3 * 1024 * 1024, 4);
        let id = rand_id();
        let offer = Packet::FileOffer(FileOffer {
            id: transfer_id(&id),
            name: "video.mp4".into(),
            size: data.len() as u64,
            mime: Some("video/mp4".into()),
        });
        phone.send(pc, &offer).await;
        phone
            .expect_packet(pc, |packet| {
                matches!(packet, Packet::FileAccept { offset: 0, .. })
            })
            .await;
        let acked = phone.send_chunks(pc, id, &data, 0, 10 * CHUNK).await;

        // The computer loses its connection; what is sent meanwhile is lost.
        let intruder = Session::connect(tested.dir.clone()).await.unwrap();
        tested
            .wait(|snapshot| matches!(snapshot.status, Status::Offline { .. }))
            .await;
        for offset in (acked..acked + 3 * CHUNK).step_by(CHUNK as usize) {
            let chunk = &data[offset as usize..(offset + CHUNK) as usize];
            phone
                .session
                .send_file_chunk(pc, id, offset, chunk)
                .await
                .unwrap();
        }
        assert_eq!(tested.partial_files().len(), 1, "the partial file is kept");
        drop(intruder);
        tested.wait(Snapshot::is_connected).await;

        // Back, it asks for the state; the same offer resumes where the
        // computer stopped writing.
        phone
            .expect_packet(pc, |packet| *packet == Packet::StateRequest {})
            .await;
        phone.send(pc, &offer).await;
        let accepted = phone
            .expect_packet(pc, |packet| matches!(packet, Packet::FileAccept { .. }))
            .await;
        let Packet::FileAccept { offset, .. } = accepted else {
            unreachable!();
        };
        assert!(
            offset >= acked && offset % CHUNK == 0 && offset < data.len() as u64,
            "{offset}"
        );
        phone
            .send_chunks(pc, id, &data, offset, data.len() as u64)
            .await;
        phone
            .expect_packet(pc, |packet| matches!(packet, Packet::FileDone { .. }))
            .await;
        let path = tested.received().await;
        assert!(
            std::fs::read(path).unwrap() == data,
            "the resumed file differs"
        );
        assert!(tested.partial_files().is_empty());
        tested.stop().await;
    })
    .await;
}

#[tokio::test]
async fn a_file_resumes_after_the_phone_reconnects() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut phone) = paired(&url).await;
        let (pc, phone_id) = (tested.id(), phone.id());
        let data = content(2 * 1024 * 1024 + 5, 5);
        let path = tested._temp.path().join("notes.pdf");
        std::fs::write(&path, &data).unwrap();
        tested.worker.send(Command::SendFiles(phone_id, vec![path]));
        let offer = phone.expect_offer(pc).await;
        let accept = Packet::FileAccept {
            id: offer.id.clone(),
            offset: 0,
        };
        phone.send(pc, &accept).await;
        let mut have = Vec::new();
        phone.receive_chunks(pc, &offer, &mut have, 5 * CHUNK).await;

        // The phone drops off and comes back: it asks for the state, and
        // the computer offers the same transfer again.
        phone.reconnect().await;
        phone.send(pc, &Packet::StateRequest {}).await;
        let again = phone.expect_offer(pc).await;
        assert_eq!(again, offer);
        let resume = Packet::FileAccept {
            id: offer.id.clone(),
            offset: have.len() as u64,
        };
        phone.send(pc, &resume).await;
        phone
            .receive_chunks(pc, &offer, &mut have, data.len() as u64)
            .await;
        assert!(have == data, "the resumed file differs");
        phone
            .send(
                pc,
                &Packet::FileDone {
                    id: offer.id.clone(),
                },
            )
            .await;
        tested
            .wait(|snapshot| {
                snapshot
                    .transfers
                    .iter()
                    .any(|transfer| transfer.outgoing && transfer.stage == Stage::Done)
            })
            .await;
        tested.stop().await;
    })
    .await;
}

// Notifications, calls, media and volume (`docs/features.md` sections 5
// and 6). The notice callback stands for the toasts and for this
// computer's players and volume; commands stand for the clicks on toasts
// and for the system thread's readings.

/// A phone that shares its notifications, calls, players and volume.
fn phone_capabilities_with_media() -> Packet {
    let list = |kinds: &[&str]| kinds.iter().map(|kind| kind.to_string()).collect();
    Packet::Capabilities(Capabilities {
        platform: "android".into(),
        incoming: list(&[
            "ring",
            "notification.dismiss",
            "notification.action",
            "notification.reply",
            "notification.request",
            "call.mute",
            "media",
            "media.command",
            "volume",
            "volume.set",
        ]),
        outgoing: list(&[
            "notification",
            "notification.removed",
            "call",
            "media",
            "media.command",
            "volume",
            "volume.set",
        ]),
    })
}

/// A paired phone that told its capabilities, past the computer's answer
/// and its request for the notifications.
async fn paired_with_media(url: &str) -> (Tested, Peer) {
    let (tested, mut phone) = paired(url).await;
    let (pc, phone_id) = (tested.id(), phone.id());
    phone.send(pc, &phone_capabilities_with_media()).await;
    tested
        .wait(|snapshot| snapshot.peer(&phone_id).handles("media"))
        .await;
    phone
        .expect_packet(pc, |packet| *packet == Packet::NotificationRequest {})
        .await;
    (tested, phone)
}

fn song(title: &str, position: u64) -> Player {
    Player {
        id: "Spotify.exe".into(),
        app: "Spotify".into(),
        title: title.into(),
        artist: "Artist".into(),
        playing: true,
        position: Some(position),
        length: Some(200_000),
        can_play: true,
        can_pause: true,
        can_seek: true,
        ..Player::default()
    }
}

/// The packets from `from` up to the first one `wanted` accepts, that one
/// included.
async fn packets_until(
    peer: &mut Peer,
    from: [u8; 16],
    wanted: impl Fn(&Packet) -> bool,
) -> Vec<Packet> {
    let mut seen = Vec::new();
    loop {
        if let Event::Packet { from: sender, json } = peer.next_any().await
            && sender == from
            && let Parsed::Known(packet) = packets::parse(&json)
        {
            let done = wanted(&packet);
            seen.push(packet);
            if done {
                return seen;
            }
        }
    }
}

#[tokio::test]
async fn notifications_not_sent_again_after_a_request_are_removed() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut phone) = paired_with_media(&url).await;
        let (pc, phone_id) = (tested.id(), phone.id());
        let notification = |id: &str| Notification {
            id: id.into(),
            app: "Chat".into(),
            ..Notification::default()
        };
        for id in ["kept", "gone"] {
            phone
                .send(pc, &Packet::Notification(notification(id)))
                .await;
            tested
                .expect_notice(|notice| matches!(notice, Notice::Notification { .. }))
                .await;
        }

        // Asked again, as after a reconnection: only one comes back.
        let off = Features {
            notifications: false,
            ..Features::default()
        };
        tested.worker.send(Command::SetFeatures(off));
        phone.expect_packet(pc, is_capabilities).await;
        tested
            .worker
            .send(Command::SetFeatures(Features::default()));
        phone
            .expect_packet(pc, |packet| *packet == Packet::NotificationRequest {})
            .await;
        phone
            .send(pc, &Packet::Notification(notification("kept")))
            .await;
        tested
            .expect_notice(|notice| matches!(notice, Notice::Notification { .. }))
            .await;

        // Past the quiet window, the other one's toast goes.
        let removed = |notice: &Notice| matches!(notice, Notice::NotificationRemoved { .. });
        let swept = async {
            loop {
                if tested.notices.lock().unwrap().iter().any(removed) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(15), swept)
            .await
            .expect("no stale notification removed");
        let notices = tested.notices.lock().unwrap().clone();
        let removed: Vec<_> = notices.iter().filter(|notice| removed(notice)).collect();
        assert_eq!(
            removed,
            [&Notice::NotificationRemoved {
                from: phone_id,
                id: "gone".into()
            }]
        );
        tested.stop().await;
    })
    .await;
}

#[tokio::test]
async fn phone_notifications_and_calls_reach_the_notices_and_answers_go_back() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut phone) = paired_with_media(&url).await;
        let (pc, phone_id) = (tested.id(), phone.id());

        // Right after the request, the phone's current notifications come
        // without popping up; the text is cut to 2000 characters.
        let notification = Notification {
            id: "0|org.chat|7".into(),
            app: "Chat".into(),
            title: "Ann".into(),
            text: "é".repeat(2500),
            dismissable: true,
            actions: vec!["Mark read".into()],
            reply: true,
            ..Notification::default()
        };
        phone
            .send(pc, &Packet::Notification(notification.clone()))
            .await;
        let shown = tested
            .expect_notice(|notice| matches!(notice, Notice::Notification { .. }))
            .await;
        let Notice::Notification {
            from,
            device,
            notification: shown,
        } = shown
        else {
            unreachable!();
        };
        assert_eq!((from, device.as_deref()), (phone_id, Some("Test phone")));
        assert!(shown.silent);
        assert_eq!(shown.text.chars().count(), 2000);
        assert_eq!(
            Notification {
                text: notification.text.clone(),
                silent: false,
                ..shown
            },
            notification
        );

        phone
            .send(
                pc,
                &Packet::NotificationRemoved {
                    id: notification.id.clone(),
                },
            )
            .await;
        let removed = tested
            .expect_notice(|notice| matches!(notice, Notice::NotificationRemoved { .. }))
            .await;
        assert_eq!(
            removed,
            Notice::NotificationRemoved {
                from: phone_id,
                id: notification.id.clone()
            }
        );

        // The toast's buttons: an action, a reply, a dismiss.
        let id = notification.id.clone();
        for answer in [
            Packet::NotificationAction {
                id: id.clone(),
                action: "Mark read".into(),
            },
            Packet::NotificationReply {
                id: id.clone(),
                text: "On my way".into(),
            },
            Packet::NotificationDismiss { id: id.clone() },
        ] {
            tested
                .worker
                .send(Command::SendPacket(phone_id, answer.clone()));
            phone.expect_packet(pc, |packet| *packet == answer).await;
        }

        // A ringing call, an unknown event ignored, and the mute.
        let call = Call {
            event: "ringing".into(),
            number: Some("+33 600 000 000".into()),
            name: Some("Ann".into()),
        };
        let held = Call {
            event: "held".into(),
            ..call.clone()
        };
        phone.send(pc, &Packet::Call(held)).await;
        phone.send(pc, &Packet::Call(call.clone())).await;
        let rang = tested
            .expect_notice(|notice| matches!(notice, Notice::Call { .. }))
            .await;
        assert_eq!(
            rang,
            Notice::Call {
                from: phone_id,
                device: Some("Test phone".into()),
                call,
            }
        );
        tested
            .worker
            .send(Command::SendPacket(phone_id, Packet::CallMute {}));
        phone
            .expect_packet(pc, |packet| *packet == Packet::CallMute {})
            .await;
        let calls = tested
            .notices
            .lock()
            .unwrap()
            .iter()
            .filter(|notice| matches!(notice, Notice::Call { .. }))
            .count();
        assert_eq!(calls, 0, "the unknown event made a notice");

        // Switched off: notifications and calls are ignored, and answers
        // to them are not sent.
        let off = Features {
            notifications: false,
            calls: false,
            ..Features::default()
        };
        tested.worker.send(Command::SetFeatures(off));
        phone.expect_packet(pc, is_capabilities).await;
        phone
            .send(pc, &Packet::Notification(notification.clone()))
            .await;
        let missed = Call {
            event: "missed".into(),
            ..Call::default()
        };
        phone.send(pc, &Packet::Call(missed)).await;
        let dismiss = Packet::NotificationDismiss { id: id.clone() };
        tested.worker.send(Command::SendPacket(phone_id, dismiss));
        tested
            .worker
            .send(Command::SendPacket(phone_id, Packet::CallMute {}));
        tested.worker.send(Command::Ring(phone_id));
        let seen = packets_until(&mut phone, pc, |packet| *packet == Packet::Ring {}).await;
        let answered = seen.iter().any(|packet| {
            matches!(
                packet,
                Packet::NotificationDismiss { .. } | Packet::CallMute {}
            )
        });
        assert!(!answered, "{seen:?}");
        phone.send(pc, &Packet::StateRequest {}).await;
        phone.expect_packet(pc, is_capabilities).await;
        let notices = tested.notices.lock().unwrap().clone();
        let shown = notices
            .iter()
            .any(|notice| matches!(notice, Notice::Notification { .. } | Notice::Call { .. }));
        assert!(!shown, "{notices:?}");
        tested.stop().await;
    })
    .await;
}

#[tokio::test]
async fn media_and_volume_go_both_ways() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut phone) = paired_with_media(&url).await;
        let (pc, phone_id) = (tested.id(), phone.id());
        let is_state = |packet: &Packet| matches!(packet, Packet::Media { .. } | Packet::Volume(_));

        // This computer's players and volume go to the phone on change; a
        // reading where only time moved, or the same volume, does not.
        let players = vec![song("First", 10_000)];
        tested.worker.send(Command::LocalMedia(players.clone()));
        let sent = phone.expect_packet(pc, is_state).await;
        assert_eq!(sent, Packet::Media { players });
        tested
            .worker
            .send(Command::LocalMedia(vec![song("First", 10_100)]));
        let volume = Volume {
            level: Some(35),
            muted: Some(false),
        };
        tested.worker.send(Command::LocalVolume(volume));
        tested.worker.send(Command::LocalVolume(volume));
        let sent = phone.expect_packet(pc, is_state).await;
        assert_eq!(sent, Packet::Volume(volume));
        let next = vec![song("Second", 0)];
        tested.worker.send(Command::LocalMedia(next.clone()));
        let sent = phone.expect_packet(pc, is_state).await;
        assert_eq!(sent, Packet::Media { players: next });

        // A state request gets them again, after the capabilities.
        phone.send(pc, &Packet::StateRequest {}).await;
        let seen =
            packets_until(&mut phone, pc, |packet| matches!(packet, Packet::Volume(_))).await;
        let kinds: Vec<&str> = seen.iter().map(Packet::kind).collect();
        assert_eq!(kinds, ["capabilities", "media", "volume"]);
        let Packet::Media { players } = &seen[1] else {
            unreachable!();
        };
        assert_eq!(players[0].title, "Second");

        // The phone's commands reach this computer's players and volume;
        // ones that make no sense do not.
        let without_value = MediaCommand {
            player: "Spotify.exe".into(),
            action: "seek".into(),
            value: None,
        };
        let seek = MediaCommand::new("Spotify.exe", MediaAction::Seek(42_000));
        for command in [without_value, seek] {
            phone.send(pc, &Packet::MediaCommand(command)).await;
        }
        let asked = tested
            .expect_notice(|notice| matches!(notice, Notice::MediaCommand { .. }))
            .await;
        assert_eq!(
            asked,
            Notice::MediaCommand {
                from: phone_id,
                player: "Spotify.exe".into(),
                action: MediaAction::Seek(42_000),
            }
        );
        let too_loud = VolumeSet {
            level: Some(150),
            muted: None,
        };
        let quiet = VolumeSet {
            level: Some(20),
            muted: Some(true),
        };
        for set in [too_loud, quiet] {
            phone.send(pc, &Packet::VolumeSet(set)).await;
        }
        let set = tested
            .expect_notice(|notice| matches!(notice, Notice::SetVolume { .. }))
            .await;
        assert_eq!(
            set,
            Notice::SetVolume {
                from: phone_id,
                set: quiet,
            }
        );
        let extra = tested
            .notices
            .lock()
            .unwrap()
            .iter()
            .filter(|notice| {
                matches!(
                    notice,
                    Notice::MediaCommand { .. } | Notice::SetVolume { .. }
                )
            })
            .count();
        assert_eq!(extra, 0, "a command that makes no sense made a notice");

        // The phone's players and volume reach the snapshot, and the
        // computer controls them.
        let theirs = vec![Player {
            id: "org.podcasts".into(),
            ..song("Episode", 5000)
        }];
        let media = Packet::Media {
            players: theirs.clone(),
        };
        phone.send(pc, &media).await;
        let level = Volume {
            level: Some(60),
            muted: Some(false),
        };
        phone.send(pc, &Packet::Volume(level)).await;
        let snapshot = tested
            .wait(|snapshot| snapshot.peer(&phone_id).volume.is_some())
            .await;
        let peer = snapshot.peer(&phone_id);
        assert_eq!(peer.media.unwrap().players, theirs);
        assert_eq!(peer.volume, Some(level));
        let pause = Packet::MediaCommand(MediaCommand::new("org.podcasts", MediaAction::Pause));
        let louder = Packet::VolumeSet(VolumeSet {
            level: Some(80),
            muted: None,
        });
        for packet in [pause, louder] {
            tested
                .worker
                .send(Command::SendPacket(phone_id, packet.clone()));
            phone.expect_packet(pc, |sent| *sent == packet).await;
        }

        // Switched off, the phone's state is forgotten and this
        // computer's is no longer sent.
        let off = Features {
            media: false,
            volume: false,
            ..Features::default()
        };
        tested.worker.send(Command::SetFeatures(off));
        phone.expect_packet(pc, is_capabilities).await;
        tested
            .wait(|snapshot| {
                let peer = snapshot.peer(&phone_id);
                peer.media.is_none() && peer.volume.is_none()
            })
            .await;
        tested
            .worker
            .send(Command::LocalMedia(vec![song("Third", 0)]));
        tested.worker.send(Command::Ring(phone_id));
        let seen = packets_until(&mut phone, pc, |packet| *packet == Packet::Ring {}).await;
        assert!(!seen.iter().any(is_state), "{seen:?}");
        tested.stop().await;
    })
    .await;
}

/// A phone with SMS, contacts, remote input and commands.
fn phone_capabilities_full() -> Packet {
    let list = |kinds: &[&str]| kinds.iter().map(|kind| kind.to_string()).collect();
    Packet::Capabilities(Capabilities {
        platform: "android".into(),
        incoming: list(&[
            "ring",
            "sms.conversations.request",
            "sms.messages.request",
            "sms.send",
            "contacts.request",
            "input.key",
            "command.list",
        ]),
        outgoing: list(&[
            "sms.conversations",
            "sms.messages",
            "sms.sent",
            "sms.received",
            "contacts",
            "input.pointer",
            "input.click",
            "input.key",
            "presenter",
            "command.run",
        ]),
    })
}

/// A paired phone that told its full capabilities.
async fn paired_full(url: &str) -> (Tested, Peer) {
    let (tested, mut phone) = paired(url).await;
    let (pc, phone_id) = (tested.id(), phone.id());
    phone.send(pc, &phone_capabilities_full()).await;
    tested
        .wait(|snapshot| snapshot.peer(&phone_id).handles("sms.send"))
        .await;
    (tested, phone)
}

fn sms(id: i64, time: i64, body: &str, outgoing: bool) -> packets::SmsMessage {
    packets::SmsMessage {
        id,
        address: Some("+33 6 00 00 00 01".into()),
        body: body.into(),
        time,
        outgoing,
        status: None,
    }
}

/// Every byte under `dir`, to look for what must not be on disk.
fn all_bytes(dir: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            bytes.extend(all_bytes(&path));
        } else {
            bytes.extend(std::fs::read(&path).unwrap());
        }
    }
    bytes
}

/// Lets the worker handle everything the phone sent before.
async fn settle(phone: &mut Peer, pc: [u8; 16]) {
    phone.send(pc, &Packet::StateRequest {}).await;
    phone.expect_packet(pc, is_capabilities).await;
}

#[tokio::test]
async fn contacts_are_asked_again_while_none_came() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut phone) = paired_full(&url).await;
        let (pc, phone_id) = (tested.id(), phone.id());
        async fn opened(tested: &Tested, phone: &mut Peer) -> Vec<Packet> {
            let (pc, phone_id) = (tested.id(), phone.id());
            tested.worker.send(Command::OpenMessages(phone_id));
            tested.worker.send(Command::Ring(phone_id));
            packets_until(phone, pc, |packet| *packet == Packet::Ring {}).await
        }

        // No answer yet (the phone may have lacked the permission): the
        // next opening asks again.
        assert!(
            opened(&tested, &mut phone)
                .await
                .contains(&Packet::ContactsRequest {})
        );
        assert!(
            opened(&tested, &mut phone)
                .await
                .contains(&Packet::ContactsRequest {})
        );
        let contacts = packets::Contacts {
            contacts: vec![packets::Contact {
                name: "Ann".into(),
                numbers: vec!["01".into()],
            }],
            part: None,
            parts: None,
        };
        phone.send(pc, &Packet::Contacts(contacts)).await;
        tested
            .wait(|snapshot| !snapshot.peer(&phone_id).sms.contacts.is_empty())
            .await;
        assert!(
            !opened(&tested, &mut phone)
                .await
                .contains(&Packet::ContactsRequest {})
        );
        tested.stop().await;
    })
    .await;
}

#[tokio::test]
async fn sms_conversations_messages_and_sends_reach_the_snapshot() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut phone) = paired_full(&url).await;
        let (pc, phone_id) = (tested.id(), phone.id());

        // Opening the messages asks for the conversations and, once, the
        // contacts.
        tested.worker.send(Command::OpenMessages(phone_id));
        phone
            .expect_packet(pc, |packet| *packet == Packet::SmsConversationsRequest {})
            .await;
        phone
            .expect_packet(pc, |packet| *packet == Packet::ContactsRequest {})
            .await;
        let conversations = vec![
            packets::Conversation {
                thread: 7,
                addresses: vec!["+33 6 00 00 00 01".into()],
                names: vec![Some("Ann".into())],
                snippet: Some("See you".into()),
                time: 3000,
                unread: false,
            },
            packets::Conversation {
                thread: 9,
                addresses: vec!["36179".into()],
                names: vec![None],
                snippet: Some("Your code".into()),
                time: 1000,
                unread: true,
            },
        ];
        phone
            .send(
                pc,
                &Packet::SmsConversations {
                    conversations: conversations.clone(),
                },
            )
            .await;
        for (part, name) in [(1, "Ann"), (2, "Bob")] {
            let contacts = packets::Contacts {
                contacts: vec![packets::Contact {
                    name: name.into(),
                    numbers: vec![format!("0{part}")],
                }],
                part: Some(part),
                parts: Some(2),
            };
            phone.send(pc, &Packet::Contacts(contacts)).await;
        }
        let snapshot = tested
            .wait(|snapshot| snapshot.peer(&phone_id).sms.contacts.len() == 2)
            .await;
        let sms_state = snapshot.peer(&phone_id).sms;
        assert_eq!(sms_state.conversations.as_ref(), Some(&conversations));

        tested.worker.send(Command::OpenMessages(phone_id));
        tested.worker.send(Command::Ring(phone_id));
        let seen = packets_until(&mut phone, pc, |packet| *packet == Packet::Ring {}).await;
        assert!(!seen.contains(&Packet::ContactsRequest {}), "{seen:?}");

        // A thread loads, oldest first. A short page may only be short to
        // fit the packet: the thread is all there only once an older page
        // brings nothing new.
        tested.worker.send(Command::LoadMessages {
            device: phone_id,
            thread: 7,
            before: None,
        });
        let asked = phone
            .expect_packet(pc, |packet| matches!(packet, Packet::SmsMessagesRequest(_)))
            .await;
        assert_eq!(
            asked,
            Packet::SmsMessagesRequest(packets::SmsMessagesRequest {
                thread: 7,
                before: None,
                limit: Some(50),
            })
        );
        let messages = vec![
            sms(1, 1000, "Hi", false),
            sms(2, 2000, "Hello", true),
            sms(3, 3000, "See you", false),
        ];
        phone
            .send(
                pc,
                &Packet::SmsMessages {
                    thread: 7,
                    messages: messages.clone(),
                },
            )
            .await;
        let snapshot = tested
            .wait(|snapshot| snapshot.peer(&phone_id).sms.threads.contains_key(&7))
            .await;
        let sms_state = snapshot.peer(&phone_id).sms;
        assert_eq!(sms_state.threads[&7], messages);
        assert!(!sms_state.complete.contains(&7));
        tested.worker.send(Command::LoadMessages {
            device: phone_id,
            thread: 7,
            before: Some(1000),
        });
        phone
            .expect_packet(pc, |packet| {
                matches!(packet, Packet::SmsMessagesRequest(request) if request.before == Some(1000))
            })
            .await;
        phone
            .send(
                pc,
                &Packet::SmsMessages {
                    thread: 7,
                    messages: vec![messages[0].clone()],
                },
            )
            .await;
        let snapshot = tested
            .wait(|snapshot| snapshot.peer(&phone_id).sms.complete.contains(&7))
            .await;
        assert_eq!(snapshot.peer(&phone_id).sms.threads[&7], messages);

        // A send is pending, then sent; the thread is asked for again,
        // and once it has the message the outbox lets it go.
        let addresses = vec!["+33 6 00 00 00 01".to_string()];
        tested.worker.send(Command::SendSms {
            device: phone_id,
            addresses: addresses.clone(),
            text: "On my way".into(),
            thread: Some(7),
        });
        let Packet::SmsSend(send) = phone
            .expect_packet(pc, |packet| matches!(packet, Packet::SmsSend(_)))
            .await
        else {
            unreachable!();
        };
        assert_eq!(
            (&send.addresses, send.text.as_str()),
            (&addresses, "On my way")
        );
        let snapshot = tested
            .wait(|snapshot| !snapshot.peer(&phone_id).sms.outbox.is_empty())
            .await;
        assert_eq!(
            snapshot.peer(&phone_id).sms.outbox[0].state,
            SendState::Pending
        );
        let sent = packets::SmsSent {
            request_id: send.request_id.clone(),
            ok: true,
            error: None,
        };
        phone.send(pc, &Packet::SmsSent(sent)).await;
        tested
            .wait(|snapshot| snapshot.peer(&phone_id).sms.outbox[0].state == SendState::Sent)
            .await;
        phone
            .expect_packet(pc, |packet| matches!(packet, Packet::SmsMessagesRequest(_)))
            .await;
        phone
            .expect_packet(pc, |packet| *packet == Packet::SmsConversationsRequest {})
            .await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        phone
            .send(
                pc,
                &Packet::SmsMessages {
                    thread: 7,
                    messages: vec![sms(4, now, "On my way", true)],
                },
            )
            .await;
        tested
            .wait(|snapshot| {
                let sms = snapshot.peer(&phone_id).sms;
                sms.outbox.is_empty() && sms.threads[&7].len() == 4
            })
            .await;

        // A refused send says why.
        tested.worker.send(Command::SendSms {
            device: phone_id,
            addresses: addresses.clone(),
            text: "Again".into(),
            thread: Some(7),
        });
        let Packet::SmsSend(send) = phone
            .expect_packet(pc, |packet| matches!(packet, Packet::SmsSend(_)))
            .await
        else {
            unreachable!();
        };
        let refused = packets::SmsSent {
            request_id: send.request_id,
            ok: false,
            error: Some("radio off".into()),
        };
        phone.send(pc, &Packet::SmsSent(refused)).await;
        tested
            .wait(|snapshot| {
                snapshot
                    .peer(&phone_id)
                    .sms
                    .outbox
                    .first()
                    .map(|outgoing| outgoing.state.clone())
                    == Some(SendState::Failed(Some("radio off".into())))
            })
            .await;

        // A received message goes to the top and to a notice with the
        // thread's people.
        let received = packets::SmsReceived {
            thread: 9,
            message: packets::SmsMessage {
                address: Some("36179".into()),
                ..sms(5, 4000, "Your code is 1234", false)
            },
        };
        phone.send(pc, &Packet::SmsReceived(received)).await;
        let notice = tested
            .expect_notice(|notice| matches!(notice, Notice::SmsReceived { .. }))
            .await;
        assert_eq!(
            notice,
            Notice::SmsReceived {
                from: phone_id,
                device: Some("Test phone".into()),
                thread: 9,
                sender: "36179".into(),
                text: "Your code is 1234".into(),
                addresses: vec!["36179".into()],
            }
        );
        let snapshot = tested.worker.snapshot();
        let sms_state = snapshot.peer(&phone_id).sms;
        let top = &sms_state.conversations.as_ref().unwrap()[0];
        assert_eq!(
            (top.thread, top.snippet.as_deref(), top.unread),
            (9, Some("Your code is 1234"), true)
        );
        // Thread 9 was never loaded: nothing is kept for it.
        assert!(!sms_state.threads.contains_key(&9));

        // Nothing of it is on disk.
        let disk = all_bytes(tested.dir.path());
        for text in ["On my way", "See you", "Your code", "Ann", "Bob"] {
            assert!(
                !disk
                    .windows(text.len())
                    .any(|bytes| bytes == text.as_bytes()),
                "{text} is on disk"
            );
        }

        // Switched off: forgotten, and ignored.
        let off = Features {
            sms: false,
            ..Features::default()
        };
        tested.worker.send(Command::SetFeatures(off));
        phone.expect_packet(pc, is_capabilities).await;
        tested
            .wait(|snapshot| snapshot.peer(&phone_id).sms.conversations.is_none())
            .await;
        let again = packets::SmsReceived {
            thread: 9,
            message: sms(6, 5000, "Ignored", false),
        };
        phone.send(pc, &Packet::SmsReceived(again)).await;
        settle(&mut phone, pc).await;
        let snapshot = tested.worker.snapshot();
        assert!(snapshot.peer(&phone_id).sms.conversations.is_none());
        let notices = tested.notices.lock().unwrap().clone();
        assert!(
            !notices
                .iter()
                .any(|notice| matches!(notice, Notice::SmsReceived { .. })),
            "{notices:?}"
        );
        tested.stop().await;
    })
    .await;
}

#[tokio::test]
async fn input_reaches_the_sink_only_from_an_allowed_device() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut phone) = paired_full(&url).await;
        let (pc, phone_id) = (tested.id(), phone.id());
        let pointer = Packet::InputPointer(packets::InputPointer {
            dx: Some(1.5),
            dy: Some(-2.0),
            ..packets::InputPointer::default()
        });

        // Not allowed by default, and not offered: typing on the phone is
        // the user's own doing and works anyway.
        phone.send(pc, &pointer).await;
        let typed = Packet::InputKey(packets::InputKey::text("hi"));
        tested
            .worker
            .send(Command::SendPacket(phone_id, typed.clone()));
        phone.expect_packet(pc, |packet| *packet == typed).await;
        settle(&mut phone, pc).await;
        assert!(tested.inputs.lock().unwrap().is_empty());
        phone.send(pc, &Packet::StateRequest {}).await;
        let Packet::Capabilities(offered) = phone.expect_packet(pc, is_capabilities).await else {
            unreachable!();
        };
        assert!(!offered.handles("input.pointer") && !offered.handles("presenter"));

        // Allowed: saved, offered to this phone at once, and everything
        // goes through, mapped.
        tested.worker.send(Command::SetControl(phone_id, true));
        let Packet::Capabilities(offered) = phone.expect_packet(pc, is_capabilities).await else {
            unreachable!();
        };
        for kind in ["input.pointer", "input.click", "input.key", "presenter"] {
            assert!(offered.handles(kind), "{kind}");
        }
        assert_eq!(offered, Features::default().capabilities(true));
        tested
            .wait(|snapshot| snapshot.control.contains(&phone_id))
            .await;
        // Changing a feature keeps it offered to this phone.
        let quiet = Features {
            ring: false,
            ..Features::default()
        };
        tested.worker.send(Command::SetFeatures(quiet));
        let packet = phone.expect_packet(pc, is_capabilities).await;
        assert_eq!(packet, Packet::Capabilities(quiet.capabilities(true)));
        assert_eq!(
            Settings::load(tested.dir.path()).controllers(),
            [phone_id].into()
        );
        let asked = [
            pointer.clone(),
            Packet::InputPointer(packets::InputPointer {
                scroll_y: Some(0.5),
                ..packets::InputPointer::default()
            }),
            Packet::InputClick(packets::InputClick {
                button: "left".into(),
                double: true,
                ..packets::InputClick::default()
            }),
            Packet::InputKey(packets::InputKey::text("é😀")),
            Packet::InputKey(packets::InputKey {
                key: Some("enter".into()),
                ctrl: true,
                ..packets::InputKey::default()
            }),
            Packet::Presenter(packets::Presenter {
                action: Some("next".into()),
                pointer: None,
            }),
            Packet::Presenter(packets::Presenter {
                action: Some("start".into()),
                pointer: None,
            }),
            Packet::Presenter(packets::Presenter {
                action: None,
                pointer: Some(Some(packets::Spot { x: 0.5, y: -3.0 })),
            }),
            Packet::Presenter(packets::Presenter {
                action: None,
                pointer: Some(None),
            }),
        ];
        for packet in &asked {
            phone.send(pc, packet).await;
        }
        settle(&mut phone, pc).await;
        let plain = |key| Input::Key {
            key,
            modifiers: Modifiers::default(),
        };
        let expected = vec![
            Input::Move { dx: 1.5, dy: -2.0 },
            Input::Scroll { x: 0.0, y: 0.5 },
            Input::Click {
                button: Button::Left,
                press: Press::Double,
            },
            Input::Text {
                text: "é😀".into(),
                modifiers: Modifiers::default(),
            },
            Input::Key {
                key: Key::Enter,
                modifiers: Modifiers {
                    ctrl: true,
                    ..Modifiers::default()
                },
            },
            plain(Key::Right),
            plain(Key::F5),
            Input::Spot(Some((0.5, -1.0))),
            Input::Spot(None),
        ];
        assert_eq!(*tested.inputs.lock().unwrap(), expected);

        // Forbidden again: no longer offered, the spot is hidden, and
        // nothing more goes.
        tested.inputs.lock().unwrap().clear();
        tested.worker.send(Command::SetControl(phone_id, false));
        let packet = phone.expect_packet(pc, is_capabilities).await;
        assert_eq!(packet, Packet::Capabilities(quiet.capabilities(false)));
        tested.wait(|snapshot| snapshot.control.is_empty()).await;
        phone.send(pc, &pointer).await;
        settle(&mut phone, pc).await;
        assert_eq!(*tested.inputs.lock().unwrap(), [Input::Spot(None)]);
        tested.stop().await;
    })
    .await;
}

#[tokio::test]
async fn commands_are_listed_and_run_by_id_only() {
    timed(async {
        let url = start_relay().await;
        let (tested, mut phone) = paired_full(&url).await;
        let (pc, phone_id) = (tested.id(), phone.id());
        let is_list = |packet: &Packet| matches!(packet, Packet::CommandList { .. });

        let commands = vec![
            UserCommand {
                id: "c1".into(),
                name: "Lock".into(),
                line: "rundll32.exe user32.dll,LockWorkStation".into(),
            },
            UserCommand {
                id: "c2".into(),
                name: "Sleep".into(),
                line: "echo sleep".into(),
            },
        ];
        tested.worker.send(Command::SetCommands(commands.clone()));
        let listed = phone
            .expect_packet(
                pc,
                |packet| matches!(packet, Packet::CommandList { commands } if !commands.is_empty()),
            )
            .await;
        assert_eq!(
            listed,
            Packet::CommandList {
                commands: commands.iter().map(UserCommand::item).collect(),
            }
        );
        assert_eq!(Settings::load(tested.dir.path()).commands, commands);

        // Only a listed id runs, and only its own line.
        for id in ["nope", "c1; del *", "c2"] {
            phone.send(pc, &Packet::CommandRun { id: id.into() }).await;
        }
        let ran = tested
            .expect_notice(|notice| matches!(notice, Notice::CommandRan { .. }))
            .await;
        assert_eq!(
            ran,
            Notice::CommandRan {
                from: phone_id,
                device: Some("Test phone".into()),
                name: "Sleep".into(),
            }
        );
        assert_eq!(*tested.ran.lock().unwrap(), ["echo sleep"]);

        // A state request gets the list again.
        phone.send(pc, &Packet::StateRequest {}).await;
        let again = phone.expect_packet(pc, is_list).await;
        assert_eq!(again, listed);

        // Switched off, nothing runs and no list goes.
        let off = Features {
            commands: false,
            ..Features::default()
        };
        tested.worker.send(Command::SetFeatures(off));
        phone.expect_packet(pc, is_capabilities).await;
        phone
            .send(pc, &Packet::CommandRun { id: "c1".into() })
            .await;
        phone.send(pc, &Packet::StateRequest {}).await;
        let seen = packets_until(&mut phone, pc, is_capabilities).await;
        assert!(!seen.iter().any(is_list), "{seen:?}");
        assert_eq!(*tested.ran.lock().unwrap(), ["echo sleep"]);
        tested.stop().await;
    })
    .await;
}
