//! [`WeftCore`]: the worker, its snapshot watcher and the clipboard rules,
//! behind one object.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use weft_cli::{StateDir, short_id, unhex};
use weft_client::features::Features;
use weft_client::sync::{self, Clipboard, Content, Incoming, Outgoing, Sync};
use weft_client::worker::{self, Command, DEFAULT_RELAY_URL, Handle, Hooks, Published};
use weft_protocol::MAX_CLIPBOARD_TEXT_LEN;

use crate::map::{self, CoreNotice, CoreSnapshot, Now, PollOutcome, SendOutcome};
use crate::{CoreConfig, CoreError, check_device_name, check_relay_url, group};

/// How long the worker gets to close its connection politely.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(5);

/// The features this version of the Mac app handles. The others are left
/// out of the capabilities it announces, so no device sends it files,
/// rings or notifications it would drop. Later versions widen it.
const SUPPORTED: Features = Features {
    files: false,
    links: false,
    ring: false,
    battery: true,
    notifications: false,
    calls: false,
    media: false,
    volume: false,
    sms: false,
    commands: false,
};

/// How the core reaches the app. Called from Rust threads: the app hops to
/// its main thread before touching its interface.
#[uniffi::export(with_foreign)]
pub trait CoreDelegate: Send + std::marker::Sync {
    /// A newer snapshot is there; read it with [`WeftCore::snapshot`].
    fn snapshot_changed(&self, generation: u64);
    /// Something the user should see or hear.
    fn notice(&self, notice: CoreNotice);
    /// A received text waits: call [`WeftCore::poll_pasteboard`] on the
    /// main thread to write it.
    fn clipboard_waiting(&self);
}

/// What is on the pasteboard, as the app reads it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum PasteboardContent {
    Text {
        text: String,
    },
    /// Marked concealed, transient or auto-generated
    /// (`org.nspasteboard.*Type`): password managers and the like.
    Excluded,
    /// No plain text.
    NoText,
}

/// The system pasteboard, implemented by the app. Only ever called from
/// [`WeftCore::poll_pasteboard`] and [`WeftCore::send_clipboard_now`], so
/// on the thread the app calls those from.
#[uniffi::export(with_foreign)]
pub trait Pasteboard: Send + std::marker::Sync {
    /// `NSPasteboard.changeCount`.
    fn change_count(&self) -> i64;
    /// The plain text, unless excluded. Reading the types first costs no
    /// privacy prompt; reading the text may.
    fn read_text(&self) -> PasteboardContent;
    /// Replaces the content with `text`, marked concealed when `private`.
    /// Returns the change count right after, or `None` if it failed.
    fn write_text(&self, text: String, private: bool) -> Option<i64>;
}

/// The [`Clipboard`] the Sync rules need, over the app's pasteboard. The
/// change count wraps to 32 bits, as sequence numbers do.
struct AppClipboard(Arc<dyn Pasteboard>);

impl Clipboard for AppClipboard {
    fn sequence(&self) -> u32 {
        self.0.change_count() as u32
    }

    fn read(&mut self) -> Content {
        match self.0.read_text() {
            PasteboardContent::Text { text } if text.len() > MAX_CLIPBOARD_TEXT_LEN => {
                Content::TooLarge
            }
            PasteboardContent::Text { text } => Content::Text(text),
            PasteboardContent::Excluded => Content::Excluded,
            PasteboardContent::NoText => Content::NoText,
        }
    }

    fn write(&mut self, text: &str) -> Option<u32> {
        self.0
            .write_text(text.to_owned(), false)
            .map(|count| count as u32)
    }

    fn write_private(&mut self, text: &str) -> Option<u32> {
        self.0
            .write_text(text.to_owned(), true)
            .map(|count| count as u32)
    }
}

/// A running worker.
struct Running {
    handle: Handle,
    thread: JoinHandle<()>,
}

/// The Weft client core of the app: one per process.
#[derive(uniffi::Object)]
pub struct WeftCore {
    dir: StateDir,
    downloads: PathBuf,
    relay_url: String,
    delegate: Arc<dyn CoreDelegate>,
    pasteboard: Arc<dyn Pasteboard>,
    /// `None` while the worker restarts, or once stopped.
    running: Mutex<Option<Running>>,
    /// The last snapshot seen, shown while the worker restarts.
    last: Mutex<Published>,
    /// Held for the whole of a restart, so two never overlap.
    restarting: Mutex<()>,
    sync: Mutex<Sync<AppClipboard>>,
    /// The last received text, until the next poll writes it.
    received: Arc<Mutex<Option<String>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Logs go to stderr, at the level set by `WEFT_LOG` (`info` by default).
fn init_logging() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let filter = tracing_subscriber::EnvFilter::try_from_env("WEFT_LOG")
            .unwrap_or_else(|_| "info".into());
        let _ = tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_target(false)
            .with_env_filter(filter)
            .try_init();
    });
}

impl WeftCore {
    fn hooks(&self) -> Hooks {
        let received = self.received.clone();
        let text_delegate = self.delegate.clone();
        let notice_delegate = self.delegate.clone();
        Hooks {
            on_text: Box::new(move |text| {
                // Only the latest counts, as on the clipboard itself.
                *lock(&received) = Some(text);
                text_delegate.clipboard_waiting();
            }),
            on_notice: Arc::new(move |notice| notice_delegate.notice(map::notice(notice))),
            downloads: self.downloads.clone(),
            // Not announced (see SUPPORTED): nothing should come.
            on_input: Box::new(|_| tracing::debug!("remote input ignored: not handled yet")),
            run_command: Box::new(|_| tracing::debug!("command ignored: not handled yet")),
        }
    }

    /// Creates the identity on the relay of the config, named `name` when
    /// it is a valid device name (else after the host name). Done before
    /// the worker starts, so its first connection already has the name.
    fn create_identity(&self, name: Option<&str>) -> Result<(), CoreError> {
        let failed = |err: weft_cli::Error| CoreError::Start {
            message: err.to_string(),
        };
        self.dir.load_or_create(&self.relay_url).map_err(failed)?;
        match name.map(check_device_name) {
            Some(Ok(name)) => {
                let (renamed, _) = self
                    .dir
                    .update(|state| state.set_name(&name))
                    .map_err(failed)?;
                renamed.map_err(failed)
            }
            Some(Err(_)) => {
                tracing::info!("the Mac's name is not a valid device name; host name kept");
                Ok(())
            }
            None => Ok(()),
        }
    }

    /// Starts a worker on the state, and the thread that tells the app
    /// about its snapshots.
    fn spawn(&self) -> Result<Running, CoreError> {
        let (handle, thread) = worker::spawn_on(self.dir.clone(), &self.relay_url, self.hooks())
            .map_err(|err| CoreError::Start {
                message: err.to_string(),
            })?;
        if handle.snapshot().features != SUPPORTED {
            handle.send(Command::SetFeatures(SUPPORTED));
        }
        let watcher = handle.clone();
        let delegate = self.delegate.clone();
        let started = std::thread::Builder::new()
            .name("weft-snapshots".into())
            .spawn(move || {
                let mut generation = 0;
                // Ends with the worker.
                while let Some(published) = watcher.wait_newer(generation) {
                    generation = published.generation;
                    delegate.snapshot_changed(generation);
                }
            });
        if let Err(err) = started {
            handle.send(Command::Shutdown);
            return Err(CoreError::Start {
                message: err.to_string(),
            });
        }
        *lock(&self.last) = handle.published();
        Ok(Running { handle, thread })
    }

    fn handle(&self) -> Option<Handle> {
        lock(&self.running)
            .as_ref()
            .map(|running| running.handle.clone())
    }

    /// Queues a command, or drops it while there is no worker.
    fn send(&self, command: Command) {
        match self.handle() {
            Some(handle) => handle.send(command),
            None => tracing::info!("no worker running: command dropped"),
        }
    }

    fn published(&self) -> Published {
        match self.handle() {
            Some(handle) => {
                let published = handle.published();
                *lock(&self.last) = published.clone();
                published
            }
            None => lock(&self.last).clone(),
        }
    }

    /// Stops the worker and waits for its thread, at most [`SHUTDOWN_WAIT`].
    fn stop_worker(running: Running) {
        running.handle.send(Command::Shutdown);
        let deadline = Instant::now() + SHUTDOWN_WAIT;
        while !running.thread.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if running.thread.is_finished() {
            let _ = running.thread.join();
        } else {
            tracing::warn!("the worker did not stop in time");
        }
    }

    /// Stops the worker, runs `change` on the state, and starts a new
    /// worker whatever `change` returned. Blocks for a few seconds.
    fn restart<T>(
        &self,
        change: impl FnOnce(&StateDir) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let _restarting = lock(&self.restarting);
        let old = lock(&self.running).take();
        if let Some(old) = old {
            Self::stop_worker(old);
        }
        let result = change(&self.dir);
        let running = self.spawn()?;
        *lock(&self.running) = Some(running);
        self.delegate.snapshot_changed(0);
        result
    }
}

#[uniffi::export]
impl WeftCore {
    /// Loads the state (creating an identity if there is none) and starts
    /// the worker. Call it on the main thread: the pasteboard is first
    /// read here, and what it holds now is never sent.
    #[uniffi::constructor]
    pub fn start(
        config: CoreConfig,
        delegate: Arc<dyn CoreDelegate>,
        pasteboard: Arc<dyn Pasteboard>,
    ) -> Result<Arc<WeftCore>, CoreError> {
        init_logging();
        let dir = StateDir::new(&config.state_dir);
        let fresh = !dir.exists();
        let relay_url = match &config.relay_url {
            Some(url) => check_relay_url(url)?,
            None => DEFAULT_RELAY_URL.to_owned(),
        };
        let sync = Sync::new(AppClipboard(pasteboard.clone()));
        let core = WeftCore {
            dir,
            downloads: PathBuf::from(&config.downloads_dir),
            relay_url,
            delegate,
            pasteboard,
            running: Mutex::new(None),
            last: Mutex::new(Published {
                generation: 0,
                snapshot: Arc::new(placeholder()),
                paired: None,
            }),
            restarting: Mutex::new(()),
            sync: Mutex::new(sync),
            received: Arc::new(Mutex::new(None)),
        };
        if fresh {
            core.create_identity(config.default_name.as_deref())?;
        }
        let running = core.spawn()?;
        *lock(&core.running) = Some(running);
        tracing::info!(fresh, "core started");
        Ok(Arc::new(core))
    }

    /// The worker's current state.
    pub fn snapshot(&self) -> CoreSnapshot {
        map::snapshot(&self.published(), Now::read())
    }

    /// Opens a pairing slot and shows its code until a device joins.
    pub fn start_pairing(&self) {
        self.send(Command::StartPairing);
    }

    /// Stops showing the code, or dismisses the last pairing result.
    pub fn cancel_pairing(&self) {
        self.send(Command::CancelPairing);
    }

    /// Joins the group of the device showing this `WEFT1:` code.
    pub fn join(&self, code: String) {
        self.send(Command::Join(code.trim().to_owned()));
    }

    /// Removes a trusted device from the group, by id in hex.
    pub fn remove_device(&self, device_id: String) -> Result<(), CoreError> {
        let id = unhex::<16>(&device_id).ok_or(CoreError::UnknownDevice)?;
        let known = self
            .published()
            .snapshot
            .devices
            .iter()
            .any(|device| device.id == id);
        if !known {
            return Err(CoreError::UnknownDevice);
        }
        tracing::info!(device = %short_id(&id), "removing a device");
        self.send(Command::RemoveDevice(id));
        Ok(())
    }

    /// Renames this Mac, saved and announced to the group.
    pub fn rename(&self, name: String) -> Result<(), CoreError> {
        let name = check_device_name(&name)?;
        self.send(Command::Rename(name));
        Ok(())
    }

    /// Changes the relay. Only without other trusted devices, since the
    /// group lives on the relay. Restarts the worker: blocks for a few
    /// seconds, so call it off the main thread.
    pub fn set_relay_url(&self, url: String) -> Result<(), CoreError> {
        let url = check_relay_url(&url)?;
        if !self.published().snapshot.devices.is_empty() {
            return Err(CoreError::InGroup);
        }
        self.restart(|dir| {
            dir.update(|state| state.relay_url = url)
                .map(|_| ())
                .map_err(|err| CoreError::State {
                    message: err.to_string(),
                })
        })?;
        tracing::info!("relay changed");
        Ok(())
    }

    /// Leaves the group: tells the other devices and the relay, when it
    /// can, then forgets every trusted device. Restarts the worker: blocks
    /// for up to about fifteen seconds, so call it off the main thread.
    /// Returns whether the others were told; this Mac leaves either way.
    pub fn leave_group(&self) -> Result<bool, CoreError> {
        self.restart(group::leave)
    }

    /// Sends the pasteboard's text to every device now, pause or not.
    /// Call it on the main thread.
    pub fn send_clipboard_now(&self) -> SendOutcome {
        let published = self.published();
        if !published.snapshot.is_connected() {
            return SendOutcome::Offline;
        }
        if published.snapshot.devices.is_empty() {
            return SendOutcome::NoDevices;
        }
        let text = match self.pasteboard.read_text() {
            PasteboardContent::Text { text } => text,
            PasteboardContent::Excluded => return SendOutcome::Excluded,
            PasteboardContent::NoText => return SendOutcome::NoText,
        };
        if text.is_empty() {
            return SendOutcome::NoText;
        }
        if text.len() > MAX_CLIPBOARD_TEXT_LEN {
            return SendOutcome::TooLarge;
        }
        self.send(Command::SendText(text));
        SendOutcome::Sent
    }

    /// Writes the received text that waits, if any, then looks for a
    /// local change and sends it if the rules say so. Call it on the main
    /// thread, about every half second and after
    /// [`CoreDelegate::clipboard_waiting`].
    pub fn poll_pasteboard(&self) -> PollOutcome {
        let snapshot = self.published().snapshot;
        let mut sync = lock(&self.sync);
        let mut outcome = PollOutcome {
            written: false,
            sent: false,
        };
        let received = lock(&self.received).take();
        if let Some(text) = received {
            let incoming = sync.remote_text(&text, snapshot.pause);
            tracing::debug!(?incoming, "received text");
            outcome.written = incoming == Incoming::Written;
        }
        let online = snapshot.is_connected() && !snapshot.devices.is_empty();
        match sync.local_change(snapshot.pause, online, Instant::now()) {
            Outgoing::Send(text) => {
                self.send(Command::SendText(text));
                outcome.sent = true;
            }
            Outgoing::TooLarge => {
                tracing::info!("the copied text is over the size limit; not sent")
            }
            Outgoing::Unchanged => {}
            other => tracing::debug!(outcome = ?other, "pasteboard change not sent"),
        }
        outcome
    }

    /// Stops the worker. The core does nothing afterwards.
    pub fn stop(&self) {
        let running = lock(&self.running).take();
        if let Some(running) = running {
            Self::stop_worker(running);
        }
    }
}

/// What the snapshot says before the first worker publishes one.
fn placeholder() -> worker::Snapshot {
    worker::Snapshot {
        status: worker::Status::Connecting,
        name: String::new(),
        device_id: [0; 16],
        relay_url: String::new(),
        devices: Vec::new(),
        peers: Default::default(),
        pause: sync::Pause::default(),
        features: SUPPORTED,
        control: Default::default(),
        commands: Vec::new(),
        pairing: worker::PairingState::Idle,
        last_activity: None,
        ringing: None,
        transfers: Vec::new(),
    }
}
