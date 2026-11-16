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
use weft_windows::features::Features;
use weft_windows::packets::{
    self, Battery, Capabilities, Connectivity, FileOffer, Packet, Parsed, transfer_id,
};
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
}

impl Tested {
    /// Starts a worker on a fresh state directory, which it creates.
    fn start(relay_url: &str) -> Tested {
        let temp = temp_dir();
        let dir = StateDir::new(temp.path().join("state"));
        let clipboard = Arc::new(Mutex::new(Vec::new()));
        let notices = Arc::new(Mutex::new(Vec::new()));
        let downloads = temp.path().join("Downloads");
        let (sink, notice_sink) = (clipboard.clone(), notices.clone());
        let hooks = Hooks {
            on_text: Box::new(move |text| sink.lock().unwrap().push(text)),
            on_notice: Arc::new(move |notice| notice_sink.lock().unwrap().push(notice)),
            downloads: downloads.clone(),
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
    assert_eq!(capabilities, Features::default().capabilities());
    (tested, phone)
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
        assert_eq!(packet, Packet::Capabilities(off.capabilities()));
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
            reason: reason.into(),
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
