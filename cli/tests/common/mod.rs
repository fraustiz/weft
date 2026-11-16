//! Helpers shared by the client tests: a relay on an ephemeral port and
//! devices with their own state directory.

// Each test binary uses its own subset of these helpers.
#![allow(dead_code)]

use std::future::Future;
use std::time::Duration;

use tempfile::TempDir;
use tokio::net::TcpListener;
use weft_cli::{Event, Session, State, StateDir};
use weft_relay::Config;

/// Timeout for a single step.
pub const STEP: Duration = Duration::from_secs(10);
/// How long to wait before concluding that nothing arrives.
pub const QUIET: Duration = Duration::from_millis(500);
/// Timeout for a whole test.
pub const TEST: Duration = Duration::from_secs(60);

/// The name the test relay announces: the host the clients dial.
pub const RELAY_HOST: &str = "127.0.0.1";

/// Runs a test body with an overall timeout.
pub async fn timed<T>(body: impl Future<Output = T>) -> T {
    tokio::time::timeout(TEST, body)
        .await
        .expect("test timed out")
}

/// Starts a relay announcing `relay_name`, with groups in memory, that runs
/// until the test's runtime stops. Returns its url.
pub async fn start_relay_named(relay_name: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(weft_relay::serve(
        listener,
        Config::new(relay_name, ":memory:"),
    ));
    format!("ws://{addr}/v1")
}

pub async fn start_relay() -> String {
    start_relay_named(RELAY_HOST).await
}

/// A device with an initialised state directory, removed with the value.
pub struct TestDevice {
    pub temp: TempDir,
    pub dir: StateDir,
}

impl TestDevice {
    pub fn new(relay_url: &str) -> TestDevice {
        let temp = tempfile::Builder::new()
            .prefix("weft-test-")
            .tempdir()
            .unwrap();
        let dir = StateDir::new(temp.path().join("state"));
        dir.create(&State::generate(relay_url).unwrap(), false)
            .unwrap();
        TestDevice { temp, dir }
    }

    pub fn load(&self) -> State {
        self.dir.load().unwrap()
    }

    pub async fn connect(&self) -> Session {
        tokio::time::timeout(STEP, Session::connect(self.dir.clone()))
            .await
            .expect("connect timed out")
            .expect("connect failed")
    }
}

/// Pairs `joiner` into the group of `owner`, through the library API.
/// Returns the joiner's device id as seen by the owner.
pub async fn pair(owner: &mut Session, joiner: &mut Session) -> [u8; 16] {
    let code = owner.open_pairing().await.expect("open pairing");
    let (added, joined) = tokio::join!(
        owner.wait_pair_request(&code, STEP, None),
        joiner.join(&code, STEP, None),
    );
    let added = added.expect("owner side of the pairing");
    joined.expect("joiner side of the pairing");
    assert_eq!(added, joiner.device_id());
    added
}

/// The next event, which must arrive within [`STEP`].
pub async fn next_event(session: &mut Session) -> Event {
    session
        .next_event(Some(STEP))
        .await
        .expect("connection failed")
        .expect("no event before the timeout")
}

/// Asserts that the next event is clipboard `text` from `from`.
pub async fn expect_text(session: &mut Session, from: &Session, text: &str) {
    assert_eq!(
        next_event(session).await,
        Event::Clipboard {
            from: from.device_id(),
            text: text.to_owned(),
        }
    );
}

/// Asserts that nothing arrives for a while.
pub async fn expect_nothing(session: &mut Session) {
    let event = session
        .next_event(Some(QUIET))
        .await
        .expect("connection failed");
    assert_eq!(event, None);
}

/// The ids of the devices `session` trusts, as saved on disk.
pub fn trusted(session: &Session) -> Vec<[u8; 16]> {
    let mut ids: Vec<_> = session
        .state_dir()
        .load()
        .unwrap()
        .devices
        .iter()
        .map(|device| device.device_id())
        .collect();
    ids.sort();
    ids
}

pub fn sorted(mut ids: Vec<[u8; 16]>) -> Vec<[u8; 16]> {
    ids.sort();
    ids
}
