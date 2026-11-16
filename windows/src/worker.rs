//! The thread that talks to the relay.
//!
//! It owns the [`Session`], on a single-threaded tokio runtime: it connects,
//! reconnects when the connection is lost, hands received clipboard text to
//! a callback, and runs the [`Command`]s the other threads send it. What it
//! knows is published as an immutable [`Snapshot`] that any thread can read
//! and wait on.
//!
//! Clipboard text and pairing codes are never logged.

use std::future::Future;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use weft_cli::session::{
    JOIN_TIMEOUT, KEEPALIVE_INTERVAL, MAX_BACKOFF, MIN_BACKOFF, PAIRING_TIMEOUT, is_fatal,
};
use weft_cli::{Error, Event, Next, Result, Session, State, StateDir, hex, short_id};
use weft_protocol::{PairingCode, device_id_from_sign_pk};

use crate::sync::Pause;

/// The relay a new installation uses.
pub const DEFAULT_RELAY_URL: &str = "wss://weft.frohub.eu/v1";

/// How long one wait for a joining device lasts before the worker looks at
/// its commands and at the frames that arrived meanwhile.
const PAIRING_SLICE: Duration = Duration::from_millis(250);

/// A request to the worker.
pub enum Command {
    /// Send clipboard text to every trusted device.
    SendText(String),
    /// Open a pairing slot and show its code until a device joins.
    StartPairing,
    /// Stop showing the code, or dismiss the result of the last pairing.
    CancelPairing,
    /// Join the group of the device showing this `WEFT1:` code.
    Join(String),
    /// Remove a device from the group.
    RemoveDevice([u8; 16]),
    /// Change this device's name.
    Rename(String),
    /// Pause or resume the synchronisation in both directions.
    SetPaused(bool),
    /// Close the connection and end the thread.
    Shutdown,
}

/// The state of the connection to the relay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Connecting,
    Connected,
    /// The connection failed or was lost; another attempt follows.
    Offline {
        reason: String,
        retry_at: Instant,
    },
    /// An error a new attempt cannot fix; the worker stopped trying.
    Fatal(String),
}

/// A trusted device, with the name it announced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: [u8; 16],
    pub name: Option<String>,
}

/// Where the pairing started by [`Command::StartPairing`] or
/// [`Command::Join`] stands.
#[derive(Clone, PartialEq, Eq)]
pub enum PairingState {
    Idle,
    /// Waiting for a device to use this code. The code is a secret.
    ShowingCode {
        code: String,
        expires_at: Instant,
    },
    /// Waiting for the device that showed the code to answer.
    Joining,
    /// Paired with `device`: the one that joined, or the one that showed
    /// the code this device used.
    Success {
        device: [u8; 16],
    },
    /// The pairing ended without a new device, and why.
    Failed(String),
}

/// Never shows the code.
impl std::fmt::Debug for PairingState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PairingState::Idle => f.write_str("Idle"),
            PairingState::ShowingCode { .. } => f.write_str("ShowingCode"),
            PairingState::Joining => f.write_str("Joining"),
            PairingState::Success { device } => write!(f, "Success({})", short_id(device)),
            PairingState::Failed(reason) => write!(f, "Failed({reason:?})"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Sent,
    Received,
}

/// The last clipboard text that went through: when and which way, never
/// the text itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Activity {
    pub direction: Direction,
    pub at: SystemTime,
}

/// Everything an interface shows, as the worker last knew it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub status: Status,
    /// This device's own name and id.
    pub name: String,
    pub device_id: [u8; 16],
    pub relay_url: String,
    /// The trusted devices, this one excluded.
    pub devices: Vec<Device>,
    pub pause: Pause,
    pub pairing: PairingState,
    pub last_activity: Option<Activity>,
}

impl Snapshot {
    pub fn is_connected(&self) -> bool {
        self.status == Status::Connected
    }
}

/// A snapshot and its rank: each new snapshot gets the next generation.
#[derive(Clone)]
pub struct Published {
    pub generation: u64,
    pub snapshot: Arc<Snapshot>,
}

struct Board {
    published: Published,
    /// Set when the worker thread ends: no newer snapshot will come.
    stopped: bool,
}

struct Shared {
    board: Mutex<Board>,
    changed: Condvar,
}

/// The other threads' side of the worker.
#[derive(Clone)]
pub struct Handle {
    commands: UnboundedSender<Command>,
    shared: Arc<Shared>,
}

impl Handle {
    /// Queues a command. Commands are run in order; one sent after the
    /// worker stopped is dropped.
    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    pub fn published(&self) -> Published {
        self.shared.board.lock().unwrap().published.clone()
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.published().snapshot
    }

    /// Blocks until a snapshot newer than `generation` is published, and
    /// returns it. Returns `None` once the worker has stopped and there is
    /// nothing newer.
    pub fn wait_newer(&self, generation: u64) -> Option<Published> {
        let mut board = self.shared.board.lock().unwrap();
        loop {
            if board.published.generation > generation {
                return Some(board.published.clone());
            }
            if board.stopped {
                return None;
            }
            board = self.shared.changed.wait(board).unwrap();
        }
    }
}

/// Loads the state in `dir`, creating an identity on [`DEFAULT_RELAY_URL`]
/// if there is none, and starts the worker thread. `on_text` is called on
/// that thread with every clipboard text to put on the local clipboard.
///
/// Fails if the state cannot be read or created; nothing is started then.
pub fn spawn(
    dir: StateDir,
    on_text: impl FnMut(String) + Send + 'static,
) -> Result<(Handle, JoinHandle<()>)> {
    spawn_on(dir, DEFAULT_RELAY_URL, on_text)
}

/// [`spawn`], with the relay a new identity is created on. An existing
/// state keeps its relay.
pub fn spawn_on(
    dir: StateDir,
    relay_url: &str,
    on_text: impl FnMut(String) + Send + 'static,
) -> Result<(Handle, JoinHandle<()>)> {
    let state = match dir.load() {
        Err(Error::NotInitialised(_)) => dir.load_or_create(relay_url)?,
        loaded => loaded?,
    };
    let mut view = Snapshot {
        status: Status::Connecting,
        name: String::new(),
        device_id: state.identity().device_id(),
        relay_url: state.relay_url.clone(),
        devices: Vec::new(),
        pause: Pause::default(),
        pairing: PairingState::Idle,
        last_activity: None,
    };
    describe(&mut view, &state);
    let shared = Arc::new(Shared {
        board: Mutex::new(Board {
            published: Published {
                generation: 0,
                snapshot: Arc::new(view.clone()),
            },
            stopped: false,
        }),
        changed: Condvar::new(),
    });
    let (commands, inbox) = unbounded_channel();
    let worker = Worker {
        dir,
        commands: inbox,
        shared: shared.clone(),
        on_text,
        view,
        pairing: None,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|source| Error::Io {
            context: "starting the network runtime".into(),
            source,
        })?;
    let thread = std::thread::Builder::new()
        .name("weft-worker".into())
        .spawn(move || {
            let shared = worker.shared.clone();
            runtime.block_on(worker.run());
            shared.board.lock().unwrap().stopped = true;
            shared.changed.notify_all();
        })
        .map_err(|source| Error::Io {
            context: "starting the worker thread".into(),
            source,
        })?;
    Ok((Handle { commands, shared }, thread))
}

/// Copies what the snapshot shows of the state.
fn describe(view: &mut Snapshot, state: &State) {
    view.name = state.name().to_owned();
    view.devices = state
        .devices
        .iter()
        .map(|device| Device {
            id: device.device_id(),
            name: device.name.clone(),
        })
        .collect();
}

/// A pairing slot this device opened and is waiting on.
struct OpenPairing {
    code: PairingCode,
    expires_at: Instant,
    /// When to check next that the connection is still alive.
    keepalive_at: Instant,
}

struct Worker<F> {
    dir: StateDir,
    commands: UnboundedReceiver<Command>,
    shared: Arc<Shared>,
    on_text: F,
    /// The next snapshot, published by [`Worker::publish`].
    view: Snapshot,
    pairing: Option<OpenPairing>,
}

/// Errors that say a request failed, not that the connection is lost.
fn is_refusal(err: &Error) -> bool {
    matches!(
        err,
        Error::Relay(_)
            | Error::Protocol(_)
            | Error::GroupFull
            | Error::UnknownDevice(_)
            | Error::AmbiguousDevice(_)
            | Error::OwnCode
            | Error::RelayUrlMismatch { .. }
    ) && !is_fatal(err)
}

impl<F: FnMut(String)> Worker<F> {
    /// Publishes the view if it differs from the last snapshot.
    fn publish(&self) {
        let mut board = self.shared.board.lock().unwrap();
        if *board.published.snapshot != self.view {
            board.published = Published {
                generation: board.published.generation + 1,
                snapshot: Arc::new(self.view.clone()),
            };
            self.shared.changed.notify_all();
        }
    }

    fn set_status(&mut self, status: Status) {
        self.view.status = status;
        self.publish();
    }

    /// Connects, serves the connection, and starts again after a delay of
    /// 1 s doubling up to 30 s, until told to shut down.
    async fn run(mut self) {
        let mut backoff = MIN_BACKOFF;
        loop {
            self.set_status(Status::Connecting);
            let connect = Session::connect(self.dir.clone());
            let Some(connected) = self.offline_until(connect).await else {
                return;
            };
            let err = match connected {
                Ok(mut session) => {
                    backoff = MIN_BACKOFF;
                    tracing::info!(
                        device = %short_id(&session.device_id()),
                        devices = session.state().devices.len(),
                        "connected to the relay"
                    );
                    describe(&mut self.view, session.state());
                    self.set_status(Status::Connected);
                    match self.online(&mut session).await {
                        Ok(()) => {
                            session.close().await;
                            return;
                        }
                        Err(err) => err,
                    }
                }
                Err(err) => err,
            };

            // What was saved before the connection broke still counts.
            if let Ok(state) = self.dir.load() {
                describe(&mut self.view, &state);
            }
            if self.pairing.take().is_some() || self.view.pairing == PairingState::Joining {
                self.view.pairing = PairingState::Failed("the connection was lost".into());
            }
            if is_fatal(&err) {
                tracing::error!(error = %err, "stopped: reconnecting cannot fix this");
                self.set_status(Status::Fatal(err.to_string()));
                self.offline_until(std::future::pending::<()>()).await;
                return;
            }
            tracing::warn!(error = %err, "connection lost; retrying in {}s", backoff.as_secs());
            self.set_status(Status::Offline {
                reason: err.to_string(),
                retry_at: Instant::now() + backoff,
            });
            if self
                .offline_until(tokio::time::sleep(backoff))
                .await
                .is_none()
            {
                return;
            }
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }

    /// Waits for `wait` while there is no connection, running the commands
    /// that do not need one. Returns `None` on shutdown.
    async fn offline_until<T>(&mut self, wait: impl Future<Output = T>) -> Option<T> {
        tokio::pin!(wait);
        loop {
            tokio::select! {
                output = &mut wait => return Some(output),
                command = self.commands.recv() => match command {
                    None | Some(Command::Shutdown) => return None,
                    Some(command) => self.offline_command(command),
                },
            }
        }
    }

    fn offline_command(&mut self, command: Command) {
        const OFFLINE: &str = "not connected to the relay";
        match command {
            // Not kept for later: after a reconnection only new changes go.
            Command::SendText(_) => tracing::debug!("clipboard text not sent: {OFFLINE}"),
            Command::StartPairing | Command::Join(_) => {
                self.view.pairing = PairingState::Failed(OFFLINE.into());
            }
            Command::CancelPairing => self.view.pairing = PairingState::Idle,
            Command::RemoveDevice(device) => {
                tracing::warn!(device = %short_id(&device), "device not removed: {OFFLINE}");
            }
            // Saved now; the other devices learn the name at the next rename.
            Command::Rename(name) => match self.dir.update(|state| state.set_name(&name)) {
                Ok((Ok(()), state)) => describe(&mut self.view, &state),
                Ok((Err(err), _)) | Err(err) => tracing::warn!(error = %err, "rename failed"),
            },
            Command::SetPaused(paused) => self.view.pause.all = paused,
            Command::Shutdown => {}
        }
        self.publish();
    }

    /// Serves the connection until shutdown (`Ok`) or until it is lost.
    async fn online(&mut self, session: &mut Session) -> Result<()> {
        loop {
            if self.pairing.is_some() {
                self.pairing_slice(session).await?;
                // The slice cannot be interrupted without losing frames, so
                // commands wait for its end.
                loop {
                    match self.commands.try_recv() {
                        Ok(Command::Shutdown) | Err(TryRecvError::Disconnected) => return Ok(()),
                        Ok(command) => self.command(session, command).await?,
                        Err(TryRecvError::Empty) => break,
                    }
                }
                continue;
            }
            let next = session
                .next_event_or(Some(KEEPALIVE_INTERVAL), self.commands.recv())
                .await?;
            match next {
                Next::Event(event) => self.event(session, event),
                // Idle: make sure the connection is still alive.
                Next::Idle => session.settle().await?,
                Next::Interrupted(None | Some(Command::Shutdown)) => return Ok(()),
                Next::Interrupted(Some(command)) => self.command(session, command).await?,
            }
        }
    }

    /// Waits a moment for a device to use the code being shown, then
    /// handles the envelopes that arrived meanwhile.
    ///
    /// `wait_pair_request` keeps those envelopes and gives them back when
    /// it times out, and the relay keeps the slot, so waiting in slices
    /// loses nothing.
    async fn pairing_slice(&mut self, session: &mut Session) -> Result<()> {
        let Some(pairing) = self.pairing.as_mut() else {
            return Ok(());
        };
        let now = Instant::now();
        if now >= pairing.expires_at {
            self.pairing = None;
            self.view.pairing = PairingState::Failed("the pairing code expired".into());
            self.publish();
            return Ok(());
        }
        if now >= pairing.keepalive_at {
            pairing.keepalive_at = now + KEEPALIVE_INTERVAL;
            match session.settle().await {
                Ok(()) => {}
                Err(err) if is_refusal(&err) => {
                    tracing::warn!(error = %err, "while waiting for a device");
                }
                Err(err) => return Err(err),
            }
        }
        let slice = PAIRING_SLICE.min(pairing.expires_at - now);
        match session.wait_pair_request(&pairing.code, slice).await {
            Ok(device) => {
                tracing::info!(device = %short_id(&device), "device added");
                self.pairing = None;
                self.view.pairing = PairingState::Success { device };
            }
            // Nobody yet.
            Err(Error::Timeout(_)) => {}
            Err(err) if is_refusal(&err) => {
                tracing::warn!(error = %err, "pairing failed");
                self.pairing = None;
                self.view.pairing = PairingState::Failed(err.to_string());
            }
            Err(err) => return Err(err),
        }
        while let Some(event) = session.next_queued_event().await? {
            self.event(session, event);
        }
        describe(&mut self.view, session.state());
        self.publish();
        Ok(())
    }

    fn event(&mut self, session: &Session, event: Event) {
        match event {
            Event::Clipboard { from, text } => {
                if self.view.pause.blocks_receive() {
                    tracing::debug!(from = %short_id(&from), "clipboard text ignored: paused");
                } else {
                    tracing::debug!(from = %short_id(&from), "clipboard text received (not shown)");
                    (self.on_text)(text);
                    self.record(Direction::Received);
                }
            }
            Event::DeviceName { from, name } => {
                tracing::info!(device = %short_id(&from), %name, "device name received");
            }
            Event::Ping { from } => tracing::debug!(from = %short_id(&from), "ping"),
            Event::Welcome { from, added } => {
                tracing::info!(from = %short_id(&from), added = added.len(), "welcome received");
            }
            Event::DeviceAdded { by, device } => {
                tracing::info!(by = %short_id(&by), device = %short_id(&device), "device added");
            }
            Event::DeviceRemoved { by, device } => {
                tracing::info!(by = %short_id(&by), device = %short_id(&device), "device removed");
            }
            Event::RelayError(code) => {
                tracing::warn!(error = %Error::Relay(code), "from the relay");
            }
            Event::Dropped { from, reason } => {
                let from = from.as_ref().map(short_id).unwrap_or_else(|| "-".into());
                tracing::warn!(%from, "dropped a frame: {reason}");
            }
        }
        describe(&mut self.view, session.state());
        self.publish();
    }

    fn record(&mut self, direction: Direction) {
        self.view.last_activity = Some(Activity {
            direction,
            at: SystemTime::now(),
        });
    }

    /// Runs one command on the connection. An error means the connection
    /// is lost; a request the relay or the protocol refuses is reported in
    /// the snapshot or the log instead.
    async fn command(&mut self, session: &mut Session, command: Command) -> Result<()> {
        let done = match command {
            Command::SendText(_) if self.view.pause.blocks_send() => {
                tracing::debug!("clipboard text not sent: paused");
                Ok(())
            }
            Command::SendText(text) => session.send_text(&text).await.map(|devices| {
                tracing::debug!(devices, "clipboard text sent (not shown)");
                if devices > 0 {
                    self.record(Direction::Sent);
                }
            }),
            Command::StartPairing => self.start_pairing(session).await,
            Command::CancelPairing => {
                // The relay keeps the slot until it expires; a request for
                // it is dropped from now on.
                self.pairing = None;
                self.view.pairing = PairingState::Idle;
                Ok(())
            }
            Command::Join(code) => self.join(session, &code).await,
            Command::RemoveDevice(device) => session.remove(&hex(&device)).await.map(|_| {
                tracing::info!(device = %short_id(&device), "device removed");
            }),
            Command::Rename(name) => session.rename(&name).await.map(|_| {
                tracing::info!(%name, "device renamed");
            }),
            Command::SetPaused(paused) => {
                self.view.pause.all = paused;
                Ok(())
            }
            Command::Shutdown => Ok(()),
        };
        describe(&mut self.view, session.state());
        self.publish();
        match done {
            Err(err) if is_refusal(&err) => {
                tracing::warn!(error = %err, "request failed");
                Ok(())
            }
            done => done,
        }
    }

    async fn start_pairing(&mut self, session: &mut Session) -> Result<()> {
        self.pairing = None;
        let opened = session
            .open_pairing()
            .await
            .and_then(|code| Ok((code.encode()?, code)));
        match opened {
            Ok((encoded, code)) => {
                let now = Instant::now();
                let expires_at = now + PAIRING_TIMEOUT;
                self.pairing = Some(OpenPairing {
                    code,
                    expires_at,
                    keepalive_at: now + KEEPALIVE_INTERVAL,
                });
                self.view.pairing = PairingState::ShowingCode {
                    code: encoded,
                    expires_at,
                };
                tracing::info!("pairing code shown; waiting for a device");
                Ok(())
            }
            Err(err) if is_refusal(&err) => {
                self.view.pairing = PairingState::Failed(err.to_string());
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    async fn join(&mut self, session: &mut Session, code: &str) -> Result<()> {
        self.pairing = None;
        let code = match PairingCode::decode(code.trim()) {
            Ok(code) => code,
            Err(err) => {
                self.view.pairing = PairingState::Failed(err.to_string());
                return Ok(());
            }
        };
        self.view.pairing = PairingState::Joining;
        self.publish();
        match session.join(&code, JOIN_TIMEOUT).await {
            Ok(devices) => {
                tracing::info!("joined the group: {} trusted device(s)", devices.len());
                self.view.pairing = PairingState::Success {
                    device: device_id_from_sign_pk(&code.sign_pk),
                };
                Ok(())
            }
            // No answer in time: the code expired, or its device is gone.
            Err(err) if is_refusal(&err) || matches!(err, Error::Timeout(_)) => {
                tracing::warn!(error = %err, "joining failed");
                self.view.pairing = PairingState::Failed(err.to_string());
                Ok(())
            }
            Err(err) => Err(err),
        }
    }
}
