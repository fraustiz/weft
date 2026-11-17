//! [`WeftCore`]: the worker, its snapshot watcher and the clipboard rules,
//! behind one object.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use weft_cli::{StateDir, short_id, unhex};
use weft_client::features::{Features, Implemented};
use weft_client::packets::{MediaAction, MediaCommand, Packet, Volume, VolumeSet};
use weft_client::sync::{self, Clipboard, Content, Incoming, Outgoing, Sync};
use weft_client::worker::{self, Command, DEFAULT_RELAY_URL, Handle, Hooks, Published};
use weft_protocol::MAX_CLIPBOARD_TEXT_LEN;

use crate::map::{
    self, CoreNotice, CoreSnapshot, FeatureSwitches, MediaControl, Now, PollOutcome, SendOutcome,
};
use crate::{CoreConfig, CoreError, check_device_name, check_relay_url, group};

/// How long the worker gets to close its connection politely.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(5);

/// The feature switches the Mac app has. The others (SMS and contacts,
/// commands) are always off, whatever `features.json` says.
const SUPPORTED: Features = Features {
    files: true,
    links: true,
    ring: true,
    battery: true,
    notifications: true,
    calls: true,
    media: true,
    volume: true,
    sms: false,
    commands: false,
};

/// The packet types the Mac app acts on: its capabilities list only
/// these, less the switched-off features (`docs/features.md` sections 2
/// and 10). Its own players are not here: macOS has no public way to read
/// what other apps play, so it never sends `media` nor takes
/// `media.command`. No remote input, presenter, SMS or commands either.
///
/// A later version that implements a packet adds it here (and to
/// [`MAC_OUTGOING`] if it sends it), with its switch in [`SUPPORTED`].
pub const MAC_INCOMING: &[&str] = &[
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
    // The players of the other devices, shown and controlled from here.
    "media",
    "volume",
    "volume.set",
];

/// The packet types the Mac app sends; see [`MAC_INCOMING`].
pub const MAC_OUTGOING: &[&str] = &[
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
    // Control of the other devices' players.
    "media.command",
    "volume",
    "volume.set",
];

/// What the Mac app implements, for the worker.
pub const MAC_IMPLEMENTED: Implemented = Implemented {
    incoming: MAC_INCOMING,
    outgoing: MAC_OUTGOING,
};

/// The switches the first Mac version forced, as it had no settings for
/// them; `features.json` kept them.
const FIRST_VERSION: Features = Features {
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

/// Marks a state directory whose switches were checked by [`upgraded`].
const UPGRADED_MARK: &str = "features-upgraded";

/// `features`, or the defaults if they are the ones the first Mac version
/// forced: its user never chose them. Checked once per state directory, so
/// the same choice made later in Settings stays.
fn upgraded(dir: &std::path::Path, features: Features) -> Features {
    let mark = dir.join(UPGRADED_MARK);
    if mark.exists() {
        return features;
    }
    if let Err(err) = std::fs::write(&mark, b"") {
        tracing::warn!(error = %err, "could not mark the feature switches as checked");
    }
    if features == FIRST_VERSION && dir.join("features.json").exists() {
        Features::default()
    } else {
        features
    }
}

/// `features` with the switches the Mac does not have turned off.
fn supported(features: Features) -> Features {
    Features {
        files: features.files && SUPPORTED.files,
        links: features.links && SUPPORTED.links,
        ring: features.ring && SUPPORTED.ring,
        battery: features.battery && SUPPORTED.battery,
        notifications: features.notifications && SUPPORTED.notifications,
        calls: features.calls && SUPPORTED.calls,
        media: features.media && SUPPORTED.media,
        volume: features.volume && SUPPORTED.volume,
        sms: features.sms && SUPPORTED.sms,
        commands: features.commands && SUPPORTED.commands,
    }
}

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
            // Not announced (see MAC_INCOMING): nothing should come.
            on_input: Box::new(|_| tracing::debug!("remote input ignored: not handled")),
            run_command: Box::new(|_| tracing::debug!("command ignored: not handled")),
            implemented: Some(MAC_IMPLEMENTED),
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
        let features = upgraded(self.dir.path(), handle.snapshot().features);
        if supported(features) != features || features != handle.snapshot().features {
            handle.send(Command::SetFeatures(supported(features)));
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

    /// A trusted device by its id in hex.
    fn device(&self, device_id: &str) -> Result<[u8; 16], CoreError> {
        let id = unhex::<16>(device_id).ok_or(CoreError::UnknownDevice)?;
        let known = self
            .published()
            .snapshot
            .devices
            .iter()
            .any(|device| device.id == id);
        if known {
            Ok(id)
        } else {
            Err(CoreError::UnknownDevice)
        }
    }

    /// Sends a packet the user asked for to a trusted device. The worker
    /// drops it if its feature is off.
    fn send_packet(&self, device_id: &str, packet: Packet) -> Result<(), CoreError> {
        let device = self.device(device_id)?;
        tracing::debug!(device = %short_id(&device), kind = packet.kind(), "packet asked for");
        self.send(Command::SendPacket(device, packet));
        Ok(())
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
        let id = self.device(&device_id)?;
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

    /// Turns features on or off: saved, and new capabilities go to every
    /// device. Switches this Mac does not have stay off.
    pub fn set_features(&self, switches: FeatureSwitches) {
        let features = supported(Features {
            files: switches.files,
            links: switches.links,
            ring: switches.ring,
            battery: switches.battery,
            notifications: switches.notifications,
            calls: switches.calls,
            media: switches.media,
            volume: switches.volume,
            sms: false,
            commands: false,
        });
        tracing::info!(?switches, "feature switches changed");
        self.send(Command::SetFeatures(features));
    }

    /// Makes a device ring, until [`WeftCore::stop_ringing`] or a minute.
    pub fn ring(&self, device_id: String) -> Result<(), CoreError> {
        let device = self.device(&device_id)?;
        self.send(Command::Ring(device));
        Ok(())
    }

    /// Stops the ring this Mac started on a device.
    pub fn stop_ringing(&self, device_id: String) -> Result<(), CoreError> {
        let device = self.device(&device_id)?;
        self.send(Command::StopRinging(device));
        Ok(())
    }

    /// The user found this Mac: stops its ring.
    pub fn found_it(&self) {
        self.send(Command::FoundIt);
    }

    pub fn ping(&self, device_id: String) -> Result<(), CoreError> {
        self.send_packet(&device_id, Packet::Ping { message: None })
    }

    /// Sends a web link, opened on the device. Only http and https links
    /// of at most 2048 bytes ([`is_shareable_url`]).
    pub fn share_url(&self, device_id: String, url: String) -> Result<(), CoreError> {
        let url = url.trim().to_owned();
        if !weft_client::packets::is_shareable_url(&url) {
            return Err(CoreError::InvalidLink);
        }
        self.send_packet(&device_id, Packet::ShareUrl { url })
    }

    /// Offers these files (absolute paths) to a device; progress shows in
    /// the snapshot's transfers.
    pub fn send_files(&self, device_id: String, paths: Vec<String>) -> Result<(), CoreError> {
        let device = self.device(&device_id)?;
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        tracing::info!(device = %short_id(&device), files = paths.len(), "sending files");
        self.send(Command::SendFiles(device, paths));
        Ok(())
    }

    /// Cancels a transfer, either way, by its id in hex.
    pub fn cancel_transfer(&self, transfer_id: String) {
        match unhex::<16>(&transfer_id) {
            Some(id) => self.send(Command::CancelTransfer(id)),
            None => tracing::debug!("cancel of an invalid transfer id ignored"),
        }
    }

    /// Asks a player of a device to act.
    pub fn media_command(
        &self,
        device_id: String,
        player: String,
        control: MediaControl,
    ) -> Result<(), CoreError> {
        let action = match control {
            MediaControl::Play => MediaAction::Play,
            MediaControl::Pause => MediaAction::Pause,
            MediaControl::PlayPause => MediaAction::PlayPause,
            MediaControl::Next => MediaAction::Next,
            MediaControl::Previous => MediaAction::Previous,
            MediaControl::Seek { position_ms } => MediaAction::Seek(position_ms),
            MediaControl::SetVolume { level } => MediaAction::SetVolume(level.min(100)),
        };
        let command = MediaCommand::new(&player, action);
        self.send_packet(&device_id, Packet::MediaCommand(command))
    }

    /// Changes a device's volume (0 to 100) or mute; `None` keeps it.
    pub fn set_device_volume(
        &self,
        device_id: String,
        level: Option<u8>,
        muted: Option<bool>,
    ) -> Result<(), CoreError> {
        let set = VolumeSet {
            level: level.map(|level| level.min(100)),
            muted,
        };
        self.send_packet(&device_id, Packet::VolumeSet(set))
    }

    /// Dismisses a notification on the phone.
    pub fn dismiss_notification(&self, device_id: String, id: String) -> Result<(), CoreError> {
        self.send_packet(&device_id, Packet::NotificationDismiss { id })
    }

    /// Presses one of a notification's actions, by its label.
    pub fn notification_action(
        &self,
        device_id: String,
        id: String,
        action: String,
    ) -> Result<(), CoreError> {
        self.send_packet(&device_id, Packet::NotificationAction { id, action })
    }

    /// Answers a notification with its inline reply.
    pub fn reply_to_notification(
        &self,
        device_id: String,
        id: String,
        text: String,
    ) -> Result<(), CoreError> {
        let text = text.trim().to_owned();
        if text.is_empty() {
            return Ok(());
        }
        self.send_packet(&device_id, Packet::NotificationReply { id, text })
    }

    /// Asks the phone for its current notifications again.
    pub fn request_notifications(&self, device_id: String) -> Result<(), CoreError> {
        self.send_packet(&device_id, Packet::NotificationRequest {})
    }

    /// Silences the ringer of the phone's incoming call.
    pub fn mute_call(&self, device_id: String) -> Result<(), CoreError> {
        self.send_packet(&device_id, Packet::CallMute {})
    }

    /// This Mac's output volume, 0 to 100, as it is now: sent to the
    /// devices when it changed.
    pub fn local_volume(&self, level: u8, muted: bool) {
        self.send(Command::LocalVolume(Volume {
            level: Some(level.min(100)),
            muted: Some(muted),
        }));
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
        features: supported(Features::default()),
        control: Default::default(),
        commands: Vec::new(),
        pairing: worker::PairingState::Idle,
        last_activity: None,
        ringing: None,
        transfers: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_versions_switches_become_the_defaults_once() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("features.json"), b"{}").unwrap();
        assert_eq!(upgraded(dir.path(), FIRST_VERSION), Features::default());
        // Chosen again later in Settings, they stay.
        assert_eq!(upgraded(dir.path(), FIRST_VERSION), FIRST_VERSION);
    }

    #[test]
    fn other_switches_stay() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("features.json"), b"{}").unwrap();
        let chosen = Features {
            ring: false,
            ..Features::default()
        };
        assert_eq!(upgraded(dir.path(), chosen), chosen);
    }

    #[test]
    fn a_new_install_keeps_its_switches() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(upgraded(dir.path(), FIRST_VERSION), FIRST_VERSION);
    }
}
