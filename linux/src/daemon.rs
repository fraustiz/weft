//! `weft-desktop` background daemon: single-instance lock, `weft-client`
//! worker lifecycle, 250 ms clipboard sync loop, 2 s MPRIS2 media and
//! PipeWire/PulseAudio volume watcher, daily auto-update check, and loopback
//! TCP IPC server.

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, watch};
use weft_cli::session::relay_endpoint;
use weft_cli::{Session, StateDir, hex, short_id};
use weft_client::features::{Features, UserCommand};
use weft_client::input::Input;
use weft_client::packets::{Packet, is_shareable_url};
use weft_client::sms::{self, SendState};
use weft_client::sync::{Content, Outgoing, Sync as ClipboardSync};
use weft_client::transfer::Stage;
use weft_client::worker::{
    self, Command, DEFAULT_RELAY_URL, Device, Handle, Hooks, PairingFailure, PairingKind,
    PairingState, Published, Snapshot, Status,
};
use weft_protocol::{
    ControlFrame, Header, Kind, MAX_CLIPBOARD_TEXT_LEN, VERSION, encode_device_name,
    encode_device_removed, random_message_id, seal, session_keys,
};

use crate::clipboard::{DisplayEnv, LinuxClipboard};
use crate::input::{InputInjector, run_shortcut_command};
use crate::ipc::{
    AuthenticatedRequest, DeviceInfo, IpcRequest, IpcResponse, MessagesPayload, OutgoingSmsView,
    PairPollOutcome, StatusPayload, TransferInfo, create_private_dir, generate_token,
    read_json_line, remove_daemon_endpoint, verify_token, write_daemon_endpoint, write_json_line,
};
use crate::media::MediaAdapter;
use crate::notify::Notifier;
use crate::runner::{Runner, SystemRunner};
use crate::sound::Ringer;
use crate::update::{self, INSTALLED_VERSION_NAME, Updater};
use crate::volume::VolumeAdapter;

/// Clipboard polling interval (250 ms).
pub const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// MPRIS2 media & system volume polling interval (2 s).
pub const MEDIA_VOLUME_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Periodic wake interval for the daily update schedule check (1 hour).
const UPDATE_CHECK_TICK: Duration = Duration::from_secs(3600);
const LOCK_FILE: &str = "daemon.lock";
const WORKER_STOP_TIMEOUT: Duration = Duration::from_secs(4);
const LEAVE_ANNOUNCE_TIMEOUT: Duration = Duration::from_secs(10);
const IPC_CLIENT_READ_TIMEOUT: Duration = Duration::from_secs(35);

/// Default Linux state directory: `$WEFT_STATE`, `$XDG_STATE_HOME/weft`, or
/// `~/.local/state/weft`.
pub fn default_state_dir() -> Option<PathBuf> {
    let non_empty_path = |key: &str| {
        std::env::var_os(key)
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
    };
    if let Some(explicit) = non_empty_path("WEFT_STATE") {
        return Some(explicit);
    }
    if let Some(xdg_state) = non_empty_path("XDG_STATE_HOME").filter(|p| p.is_absolute()) {
        return Some(xdg_state.join("weft"));
    }
    let home = non_empty_path("HOME")
        .or_else(|| non_empty_path("USERPROFILE"))
        .filter(|p| p.is_absolute())?;
    Some(home.join(".local").join("state").join("weft"))
}

/// Default Downloads directory for received files.
pub fn default_downloads_dir(state_dir: &Path) -> PathBuf {
    let non_empty_abs = |key: &str| {
        std::env::var_os(key)
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty() && p.is_absolute())
    };
    if let Some(dir) = non_empty_abs("XDG_DOWNLOAD_DIR") {
        return dir;
    }
    if let Some(home) = non_empty_abs("HOME").or_else(|| non_empty_abs("USERPROFILE")) {
        let dl = home.join("Downloads");
        if dl.is_dir() {
            return dl;
        }
        return dl;
    }
    state_dir.join("Received")
}

/// Errors encountered starting the daemon.
#[derive(Debug)]
pub enum DaemonError {
    AlreadyRunning(PathBuf),
    Io(io::Error),
    Client(weft_cli::Error),
}

impl std::fmt::Display for DaemonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DaemonError::AlreadyRunning(dir) => write!(
                f,
                "weft-desktop daemon is already running on state directory {}",
                dir.display()
            ),
            DaemonError::Io(err) => write!(f, "{err}"),
            DaemonError::Client(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for DaemonError {}

impl From<io::Error> for DaemonError {
    fn from(err: io::Error) -> Self {
        DaemonError::Io(err)
    }
}

impl From<weft_cli::Error> for DaemonError {
    fn from(err: weft_cli::Error) -> Self {
        DaemonError::Client(err)
    }
}

/// Acquires an exclusive non-blocking single-instance lock in `state_dir`.
pub fn acquire_single_instance_lock(state_dir: &Path) -> Result<File, DaemonError> {
    create_private_dir(state_dir)?;
    let lock_path = state_dir.join(LOCK_FILE);
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&lock_path)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(DaemonError::AlreadyRunning(state_dir.to_path_buf())),
        Err(TryLockError::Error(err)) => Err(DaemonError::Io(err)),
    }
}

/// Configuration for starting a [`DaemonHandle`].
pub struct DaemonConfig {
    pub state_dir: PathBuf,
    pub relay_url: String,
    pub downloads_dir: PathBuf,
    pub display_env: DisplayEnv,
    pub runner: Arc<dyn Runner>,
    pub use_os_audio_spawn: bool,
}

impl DaemonConfig {
    pub fn system(state_dir: PathBuf, relay_url: Option<String>) -> Self {
        let downloads_dir = default_downloads_dir(&state_dir);
        Self {
            state_dir,
            relay_url: relay_url.unwrap_or_else(|| DEFAULT_RELAY_URL.to_owned()),
            downloads_dir,
            display_env: DisplayEnv::from_env(),
            runner: Arc::new(SystemRunner),
            use_os_audio_spawn: true,
        }
    }
}

struct RunningWorker {
    handle: Handle,
    thread: JoinHandle<()>,
}

struct DaemonCore {
    dir: StateDir,
    downloads: PathBuf,
    default_relay: String,
    running: Mutex<Option<RunningWorker>>,
    restarting: Mutex<()>,
    clipboard: LinuxClipboard,
    sync: Arc<Mutex<ClipboardSync<LinuxClipboard>>>,
    remote_clip_tx: mpsc::UnboundedSender<Content>,
    input_tx: std::sync::mpsc::Sender<Input>,
    notifier: Arc<Notifier>,
    updater: Arc<Updater>,
    stop_tx: watch::Sender<bool>,
}

impl DaemonCore {
    fn hooks(&self) -> Hooks {
        let clip_tx = self.remote_clip_tx.clone();
        let image_tx = self.remote_clip_tx.clone();
        let notifier = self.notifier.clone();
        let input_tx = self.input_tx.clone();
        Hooks {
            on_text: Box::new(move |text| {
                let _ = clip_tx.send(Content::Text(text));
            }),
            on_notice: Arc::new(move |notice| match notice {
                worker::Notice::ClipboardImage { image, .. } => {
                    let _ = image_tx.send(Content::Image(image));
                }
                notice => notifier.handle(notice),
            }),
            downloads: self.downloads.clone(),
            on_input: Box::new(move |input| {
                let _ = input_tx.send(input);
            }),
            run_command: Box::new(run_shortcut_command),
            implemented: None,
        }
    }

    fn spawn_worker(&self) -> weft_cli::Result<RunningWorker> {
        let (handle, thread) =
            worker::spawn_on(self.dir.clone(), &self.default_relay, self.hooks())?;
        Ok(RunningWorker { handle, thread })
    }

    fn handle(&self) -> Option<Handle> {
        self.running
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| r.handle.clone())
    }

    fn published(&self) -> Option<Published> {
        self.handle().map(|h| h.published())
    }

    fn stop_worker_slot(slot: RunningWorker) {
        slot.handle.send(Command::Shutdown);
        let deadline = Instant::now() + WORKER_STOP_TIMEOUT;
        while !slot.thread.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if slot.thread.is_finished() {
            let _ = slot.thread.join();
        }
    }

    fn restart_worker<T>(
        &self,
        mutate: impl FnOnce(&StateDir) -> Result<T, String>,
    ) -> Result<T, String> {
        let _guard = self.restarting.lock().unwrap();
        if let Some(old) = self.running.lock().unwrap().take() {
            Self::stop_worker_slot(old);
        }
        let outcome = mutate(&self.dir);
        let started = self.spawn_worker().map_err(|e| e.to_string())?;
        *self.running.lock().unwrap() = Some(started);
        outcome
    }
}

/// Resolves a trusted device by hex ID, unique hex prefix, or unambiguous
/// case-insensitive name.
pub fn resolve_device(devices: &[Device], query: &str) -> Result<Device, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("device identifier cannot be empty".into());
    }
    let lower = q.to_ascii_lowercase();
    if lower.bytes().all(|b| b.is_ascii_hexdigit()) {
        let hex_matches: Vec<&Device> = devices
            .iter()
            .filter(|d| hex(&d.id).starts_with(&lower))
            .collect();
        match hex_matches.as_slice() {
            [only] => return Ok((*only).clone()),
            [_, _, ..] => return Err(format!("ambiguous device prefix '{q}'")),
            [] => {}
        }
    }
    let name_matches: Vec<&Device> = devices
        .iter()
        .filter(|d| {
            d.name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(q))
        })
        .collect();
    match name_matches.as_slice() {
        [only] => Ok((*only).clone()),
        [_, _, ..] => Err(format!("more than one trusted device is named '{q}'")),
        [] => Err(format!("no trusted device matches '{q}'")),
    }
}

/// Updates a single named feature switch (`files`, `links`, `ring`, `battery`,
/// `notifications`, `calls`, `media`, `volume`, `sms`, `commands`).
pub fn apply_feature_toggle(
    features: &mut Features,
    name: &str,
    enabled: bool,
) -> Result<(), String> {
    match name.trim().to_ascii_lowercase().as_str() {
        "files" | "file" => features.files = enabled,
        "links" | "link" | "urls" | "url" => features.links = enabled,
        "ring" => features.ring = enabled,
        "battery" | "connectivity" => features.battery = enabled,
        "notifications" | "notification" => features.notifications = enabled,
        "calls" | "call" => features.calls = enabled,
        "media" | "mpris" => features.media = enabled,
        "volume" => features.volume = enabled,
        "sms" | "messages" => features.sms = enabled,
        "commands" | "shortcuts" => features.commands = enabled,
        other => {
            return Err(format!(
                "unknown feature '{other}' (expected files, links, ring, battery, notifications, calls, media, volume, sms, or commands)"
            ));
        }
    }
    Ok(())
}

fn pairing_failure_text(failure: &PairingFailure) -> String {
    match failure {
        PairingFailure::InvalidCode => "not a valid WEFT1: pairing code".into(),
        PairingFailure::Expired => "pairing code expired or unknown".into(),
        PairingFailure::OwnCode => "cannot join using this device's own pairing code".into(),
        PairingFailure::OtherRelay => {
            "pairing code belongs to a different relay (run `weft-desktop relay <url>` first)"
                .into()
        }
        PairingFailure::GroupFull => "group is full (maximum 10 devices)".into(),
        PairingFailure::NotConnected => "not connected to the relay yet".into(),
        PairingFailure::ConnectionLost => "connection to the relay was lost during pairing".into(),
        PairingFailure::NoAnswer => "the other device did not answer in time".into(),
        PairingFailure::Other(msg) => msg.clone(),
    }
}

fn pairing_kind_str(kind: PairingKind) -> &'static str {
    match kind {
        PairingKind::Joined => "joined",
        PairingKind::Invited => "invited",
        PairingKind::JoinedOther => "joined_other",
    }
}

fn stage_label(stage: Stage) -> String {
    match stage {
        Stage::Waiting => "waiting".into(),
        Stage::Running => "running".into(),
        Stage::Done => "done".into(),
        Stage::Cancelled => "cancelled".into(),
        Stage::Failed(failure) => format!("failed ({failure:?})"),
    }
}

fn build_device_infos(snapshot: &Snapshot) -> Vec<DeviceInfo> {
    snapshot
        .devices
        .iter()
        .map(|dev| {
            let peer = snapshot.peer(&dev.id);
            let now_playing = peer
                .media
                .as_ref()
                .and_then(|m| m.current())
                .filter(|p| !p.title.is_empty())
                .map(|p| {
                    if p.artist.is_empty() {
                        p.title.clone()
                    } else {
                        format!("{} — {}", p.artist, p.title)
                    }
                });
            DeviceInfo {
                id: hex(&dev.id),
                short_id: short_id(&dev.id),
                name: dev.name.clone(),
                platform: peer.capabilities.map(|c| c.platform),
                battery: peer.battery,
                connectivity: peer.connectivity,
                volume: peer.volume,
                now_playing,
                control_allowed: snapshot.control.contains(&dev.id),
            }
        })
        .collect()
}

fn build_status_payload(snapshot: &Snapshot) -> StatusPayload {
    let devices = build_device_infos(snapshot);
    let paused = snapshot.pause.all;
    let (status_str, class_str, alt_str) = match &snapshot.status {
        Status::Connected if paused => ("connected".to_owned(), "paused", "paused"),
        Status::Connected => ("connected".to_owned(), "connected", "connected"),
        Status::Connecting => ("connecting".to_owned(), "connecting", "connecting"),
        Status::Offline { .. } => ("offline".to_owned(), "offline", "offline"),
        Status::Fatal(_) => ("fatal".to_owned(), "error", "error"),
    };

    let count = devices.len();
    let text = match (&snapshot.status, paused) {
        (Status::Connected, true) => "Weft (paused)".to_owned(),
        (Status::Connected, false) if count > 0 => format!("Weft ({count})"),
        (Status::Connected, false) => "Weft".to_owned(),
        (Status::Connecting, _) => "Weft …".to_owned(),
        (Status::Offline { .. }, _) => "Weft (offline)".to_owned(),
        (Status::Fatal(_), _) => "Weft (error)".to_owned(),
    };

    let mut tooltip_lines = vec![
        format!("Weft {INSTALLED_VERSION_NAME} — {status_str}"),
        format!(
            "This device: {} ({})",
            snapshot.name,
            short_id(&snapshot.device_id)
        ),
        format!("Relay: {}", snapshot.relay_url),
    ];
    if paused {
        tooltip_lines.push("Sync: paused".into());
    }
    if devices.is_empty() {
        tooltip_lines.push("Devices: none paired".into());
    } else {
        for dev in &devices {
            let label = dev.name.as_deref().unwrap_or(&dev.short_id);
            let bat = dev
                .battery
                .and_then(|b| b.level)
                .map(|lvl| format!(" ({lvl}%)"))
                .unwrap_or_default();
            tooltip_lines.push(format!("• {label}{bat}"));
        }
    }

    let transfers = snapshot
        .transfers
        .iter()
        .map(|t| TransferInfo {
            id: hex(&t.id),
            device_id: hex(&t.device),
            outgoing: t.outgoing,
            name: t.name.clone(),
            size: t.size,
            done: t.done,
            stage: stage_label(t.stage),
        })
        .collect();

    StatusPayload {
        text,
        alt: alt_str.to_owned(),
        class: class_str.to_owned(),
        tooltip: tooltip_lines.join("\n"),
        status: status_str,
        connected: snapshot.is_connected(),
        paused,
        name: snapshot.name.clone(),
        device_id: hex(&snapshot.device_id),
        short_id: short_id(&snapshot.device_id),
        relay_url: snapshot.relay_url.clone(),
        devices,
        features: snapshot.features,
        ringing: snapshot.ringing.is_some(),
        transfers,
        version: INSTALLED_VERSION_NAME.to_owned(),
    }
}

/// Announces self-removal to group members and the relay, then clears trusted
/// devices on disk. Called only while the worker is stopped.
fn leave_group_on_disk(dir: &StateDir) -> Result<bool, String> {
    let members = dir.load().map_err(|e| e.to_string())?.devices.len();
    let told = if members == 0 {
        true
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let res = rt.block_on(tokio::time::timeout(LEAVE_ANNOUNCE_TIMEOUT, async {
            let mut session = Session::connect(dir.clone()).await?;
            let own = session.device_id();
            let peers = session.state().devices.clone();
            let plaintext = encode_device_removed(&own);
            let mut frames = Vec::with_capacity(peers.len());
            for peer in &peers {
                let peer_id = peer.device_id();
                let keys = session_keys(session.identity(), &peer_id, &peer.kx_pk)?;
                let (counter, _) = dir.update(|state| state.next_counter(&peer_id))?;
                let header = Header {
                    version: VERSION,
                    kind: Kind::DeviceRemoved,
                    sender: own,
                    recipient: peer_id,
                    message_id: random_message_id(),
                    counter,
                };
                frames.push(seal(&header, &plaintext, &keys.tx)?);
            }
            for frame in frames {
                session.send_frame(frame).await?;
            }
            session
                .send_frame(ControlFrame::GroupRemove { device_id: own }.encode())
                .await?;
            let settled = session.settle().await;
            session.close().await;
            settled
        }));
        matches!(res, Ok(Ok(())))
    };
    dir.update(|state| state.devices.clear())
        .map_err(|e| e.to_string())?;
    Ok(told)
}

/// Running daemon handle. Calling [`DaemonHandle::shutdown`] stops all loops,
/// cleans up `daemon.json`, and releases the single-instance lock.
pub struct DaemonHandle {
    state_dir: PathBuf,
    addr: SocketAddr,
    stop_tx: watch::Sender<bool>,
    done_rx: oneshot::Receiver<Option<PathBuf>>,
    lock_file: Option<File>,
}

impl DaemonHandle {
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    /// Waits until the daemon stops (e.g. via IPC `Shutdown` or self-update
    /// restart) and returns an executable path to spawn if an update installed.
    pub async fn wait(mut self) -> Option<PathBuf> {
        let restart = (&mut self.done_rx).await.ok().flatten();
        drop(self.lock_file.take());
        restart
    }

    /// Requests a clean shutdown and waits for completion.
    pub async fn shutdown(self) -> Option<PathBuf> {
        let _ = self.stop_tx.send(true);
        self.wait().await
    }
}

/// Starts the `weft-desktop` daemon in the current tokio runtime.
pub async fn start(config: DaemonConfig) -> Result<DaemonHandle, DaemonError> {
    let lock_file = acquire_single_instance_lock(&config.state_dir)?;
    update::cleanup_old_executable(&config.state_dir);

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let token = generate_token()?;

    let ringer = if config.use_os_audio_spawn {
        Ringer::new(config.state_dir.clone(), config.runner.clone())
    } else {
        Ringer::with_fake_runner(config.state_dir.clone(), config.runner.clone())
    };
    let media = Arc::new(MediaAdapter::new(config.runner.clone()));
    let volume = Arc::new(VolumeAdapter::new(config.runner.clone()));
    let notifier = Notifier::new(
        config.runner.clone(),
        ringer.clone(),
        media.clone(),
        volume.clone(),
    );

    let (stop_tx, stop_rx) = watch::channel(false);
    let updater = {
        let notify_update = notifier.clone();
        let restart_tx = stop_tx.clone();
        Updater::new(
            config.state_dir.clone(),
            move |ver| notify_update.update_available(ver),
            move || {
                let _ = restart_tx.send(true);
            },
        )
    };

    let clipboard = LinuxClipboard::with_runner(&config.display_env, config.runner.clone());
    let sync = Arc::new(Mutex::new(ClipboardSync::new(clipboard.clone())));
    let (remote_clip_tx, mut remote_clip_rx) = mpsc::unbounded_channel::<Content>();

    let (input_tx, input_rx) = std::sync::mpsc::channel::<Input>();
    let input_env = config.display_env.clone();
    let input_runner = config.runner.clone();
    let input_thread = std::thread::Builder::new()
        .name("weft-input".into())
        .spawn(move || {
            let mut injector = InputInjector::with_runner(&input_env, input_runner);
            while let Ok(item) = input_rx.recv() {
                injector.apply(item);
            }
        })?;

    let core = Arc::new(DaemonCore {
        dir: StateDir::new(&config.state_dir),
        downloads: config.downloads_dir.clone(),
        default_relay: config.relay_url.clone(),
        running: Mutex::new(None),
        restarting: Mutex::new(()),
        clipboard,
        sync: sync.clone(),
        remote_clip_tx,
        input_tx,
        notifier: notifier.clone(),
        updater: updater.clone(),
        stop_tx: stop_tx.clone(),
    });

    let initial_worker = core.spawn_worker()?;
    *core.running.lock().unwrap() = Some(initial_worker);

    // Publish daemon.json only once the worker and IPC socket are ready.
    write_daemon_endpoint(&config.state_dir, addr, &token)?;
    tracing::info!(addr = %addr, "weft-desktop daemon started");

    // 1. Clipboard sync loop (250 ms poll + immediate incoming writes).
    let clip_task = {
        let core = core.clone();
        let mut stop = stop_rx.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(CLIPBOARD_POLL_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    changed = stop.changed() => {
                        if changed.is_err() || *stop.borrow() {
                            break;
                        }
                    }
                    Some(remote_text) = remote_clip_rx.recv() => {
                        if let Some(published) = core.published() {
                            let pause = published.snapshot.pause;
                            let sync = core.sync.clone();
                            let outcome = tokio::task::spawn_blocking(move || {
                                let mut sync = sync.lock().unwrap();
                                match remote_text {
                                    Content::Text(text) => sync.remote_text(&text, pause),
                                    Content::Image(image) => sync.remote_image(&image, pause),
                                    _ => unreachable!("only text and images are queued"),
                                }
                            }).await;
                            if let Ok(incoming) = outcome {
                                tracing::debug!(?incoming, "applied remote clipboard text");
                            }
                        }
                    }
                    _ = ticker.tick() => {
                        let Some(handle) = core.handle() else {
                            continue;
                        };
                        let snapshot = handle.snapshot();
                        let pause = snapshot.pause;
                        let online = snapshot.is_connected() && !snapshot.devices.is_empty();
                        let sync = core.sync.clone();
                        let outcome = tokio::task::spawn_blocking(move || {
                            sync.lock().unwrap().local_change(pause, online, Instant::now())
                        }).await;
                        match outcome {
                            Ok(Outgoing::Send(text)) => handle.send(Command::SendText(text)),
                            Ok(Outgoing::SendImage(image)) => handle.send(Command::SendImage(image)),
                            Ok(Outgoing::TooLarge) => {
                                tracing::info!("copied text exceeds 256 KiB limit; not sent");
                            }
                            Ok(Outgoing::Unchanged) | Err(_) => {}
                            Ok(other) => tracing::debug!(outcome = ?other, "clipboard change skipped"),
                        }
                    }
                }
            }
        })
    };

    // 2. MPRIS2 media & PipeWire/PulseAudio volume watcher (2 s).
    let media_volume_task = {
        let core = core.clone();
        let media = media.clone();
        let volume = volume.clone();
        let mut stop = stop_rx.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(MEDIA_VOLUME_POLL_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    changed = stop.changed() => {
                        if changed.is_err() || *stop.borrow() {
                            break;
                        }
                    }
                    _ = ticker.tick() => {
                        let Some(handle) = core.handle() else {
                            continue;
                        };
                        let features = handle.snapshot().features;
                        if !features.media && !features.volume {
                            continue;
                        }
                        let m = media.clone();
                        let v = volume.clone();
                        let polled = tokio::task::spawn_blocking(move || {
                            let players = features.media.then(|| m.list_players());
                            let vol = features.volume.then(|| v.get_volume()).flatten();
                            (players, vol)
                        }).await;
                        if let Ok((players, vol)) = polled {
                            if let Some(players) = players {
                                handle.send(Command::LocalMedia(players));
                            }
                            if let Some(vol) = vol {
                                handle.send(Command::LocalVolume(vol));
                            }
                        }
                    }
                }
            }
        })
    };

    // 3. Daily auto-update check loop.
    let update_task = {
        let updater = updater.clone();
        let mut stop = stop_rx.clone();
        tokio::spawn(async move {
            updater.check_if_due();
            let mut ticker = tokio::time::interval(UPDATE_CHECK_TICK);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    changed = stop.changed() => {
                        if changed.is_err() || *stop.borrow() {
                            break;
                        }
                    }
                    _ = ticker.tick() => {
                        updater.check_if_due();
                    }
                }
            }
        })
    };

    // 4. Loopback TCP IPC server + clean teardown coordinator.
    let (done_tx, done_rx) = oneshot::channel();
    let state_dir_cleanup = config.state_dir.clone();
    let mut server_stop = stop_rx.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                changed = server_stop.changed() => {
                    if changed.is_err() || *server_stop.borrow() {
                        break;
                    }
                }
                accepted = listener.accept() => {
                    match accepted {
                        Ok((stream, peer)) => {
                            if !peer.ip().is_loopback() {
                                continue;
                            }
                            let core = core.clone();
                            let token = token.clone();
                            tokio::spawn(async move {
                                serve_ipc_client(stream, core, &token).await;
                            });
                        }
                        Err(err) => {
                            tracing::warn!(error = %err, "IPC accept error");
                        }
                    }
                }
            }
        }

        remove_daemon_endpoint(&state_dir_cleanup);
        let _ = clip_task.await;
        let _ = media_volume_task.await;
        let _ = update_task.await;

        ringer.stop();
        updater.stop();
        let restart_exe = updater.take_restart_exe();

        let worker_slot = core.running.lock().unwrap().take();
        drop(core);
        let _ = tokio::task::spawn_blocking(move || {
            if let Some(slot) = worker_slot {
                DaemonCore::stop_worker_slot(slot);
            }
            let _ = input_thread.join();
        })
        .await;

        let _ = done_tx.send(restart_exe);
    });

    Ok(DaemonHandle {
        state_dir: config.state_dir,
        addr,
        stop_tx,
        done_rx,
        lock_file: Some(lock_file),
    })
}

async fn serve_ipc_client(stream: TcpStream, core: Arc<DaemonCore>, expected_token: &str) {
    let (read_half, write_half) = stream.into_split();
    let read_res = tokio::time::timeout(
        IPC_CLIENT_READ_TIMEOUT,
        read_json_line::<_, AuthenticatedRequest>(read_half),
    )
    .await;
    let Ok(Ok(envelope)) = read_res else {
        return;
    };
    if !verify_token(expected_token, &envelope.token) {
        let _ = write_json_line(
            write_half,
            &IpcResponse::Error {
                message: "unauthorized IPC token".into(),
            },
        )
        .await;
        return;
    }
    let response = handle_ipc_request(&core, envelope.request).await;
    let _ = write_json_line(write_half, &response).await;
}

async fn wait_worker_condition<T>(
    handle: Handle,
    timeout: Duration,
    mut check: impl FnMut(&Published) -> Option<T> + Send + 'static,
) -> Option<T>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let deadline = Instant::now() + timeout;
        let mut published = handle.published();
        if let Some(found) = check(&published) {
            return Some(found);
        }
        while Instant::now() < deadline {
            let generation = published.generation;
            let h = handle.clone();
            // Poll wait_newer in short slices or spawn wait; since worker ticks
            // during pairing every 250ms, wait_newer wakes promptly.
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(h.wait_newer(generation));
            });
            let remaining = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(remaining) {
                Ok(Some(next)) => {
                    published = next;
                    if let Some(found) = check(&published) {
                        return Some(found);
                    }
                }
                Ok(None) | Err(_) => return check(&handle.published()),
            }
        }
        check(&handle.published())
    })
    .await
    .ok()
    .flatten()
}

async fn handle_ipc_request(core: &Arc<DaemonCore>, req: IpcRequest) -> IpcResponse {
    fn err(msg: impl Into<String>) -> IpcResponse {
        IpcResponse::Error {
            message: msg.into(),
        }
    }
    fn ack(msg: impl Into<String>) -> IpcResponse {
        IpcResponse::Ack {
            message: msg.into(),
        }
    }

    match req {
        IpcRequest::Status => match core.published() {
            Some(pub_snap) => IpcResponse::Status {
                status: build_status_payload(&pub_snap.snapshot),
            },
            None => err("worker is restarting"),
        },
        IpcRequest::Devices => match core.published() {
            Some(pub_snap) => IpcResponse::Devices {
                devices: build_device_infos(&pub_snap.snapshot),
            },
            None => err("worker is restarting"),
        },
        IpcRequest::PairStart => {
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            // Wait up to 8s for relay connection if still connecting on fresh start.
            if !handle.snapshot().is_connected() {
                let _ = wait_worker_condition(handle.clone(), Duration::from_secs(8), |p| {
                    p.snapshot.is_connected().then_some(())
                })
                .await;
            }
            let before_gen = handle.published().generation;
            handle.send(Command::StartPairing);
            let res = wait_worker_condition(handle, Duration::from_secs(12), move |p| {
                if p.generation <= before_gen
                    && !matches!(p.snapshot.pairing, PairingState::ShowingCode { .. })
                {
                    return None;
                }
                match &p.snapshot.pairing {
                    PairingState::ShowingCode { code, expires_at } => {
                        let secs = expires_at
                            .saturating_duration_since(Instant::now())
                            .as_secs();
                        Some(Ok((code.clone(), secs)))
                    }
                    PairingState::Failed(reason) => Some(Err(pairing_failure_text(reason))),
                    _ => None,
                }
            })
            .await;
            match res {
                Some(Ok((code, expires_in_secs))) => IpcResponse::PairStarted {
                    code,
                    expires_in_secs,
                },
                Some(Err(reason)) => err(reason),
                None => err("timed out opening pairing slot on relay"),
            }
        }
        IpcRequest::PairWait { timeout_ms } => {
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            let wait = Duration::from_millis(timeout_ms.clamp(50, 10_000));
            let outcome =
                wait_worker_condition(handle.clone(), wait, |p| match &p.snapshot.pairing {
                    PairingState::Success { device } => {
                        let name = p
                            .snapshot
                            .devices
                            .iter()
                            .find(|d| d.id == *device)
                            .and_then(|d| d.name.clone());
                        let kind =
                            pairing_kind_str(p.paired.unwrap_or(PairingKind::Joined)).to_owned();
                        Some(PairPollOutcome::Success {
                            device_id: hex(device),
                            short_id: short_id(device),
                            name,
                            kind,
                        })
                    }
                    PairingState::Failed(reason) => Some(PairPollOutcome::Failed {
                        reason: pairing_failure_text(reason),
                    }),
                    PairingState::Idle => Some(PairPollOutcome::Failed {
                        reason: "pairing cancelled".into(),
                    }),
                    PairingState::ShowingCode { .. } | PairingState::Joining => None,
                })
                .await;

            match outcome {
                Some(done @ PairPollOutcome::Success { .. }) => {
                    handle.send(Command::CancelPairing);
                    IpcResponse::PairOutcome { outcome: done }
                }
                Some(done @ PairPollOutcome::Failed { .. }) => {
                    handle.send(Command::CancelPairing);
                    IpcResponse::PairOutcome { outcome: done }
                }
                Some(waiting) => IpcResponse::PairOutcome { outcome: waiting },
                None => {
                    let secs = match &handle.snapshot().pairing {
                        PairingState::ShowingCode { expires_at, .. } => expires_at
                            .saturating_duration_since(Instant::now())
                            .as_secs(),
                        _ => 0,
                    };
                    IpcResponse::PairOutcome {
                        outcome: PairPollOutcome::Waiting {
                            expires_in_secs: secs,
                        },
                    }
                }
            }
        }
        IpcRequest::PairCancel => {
            if let Some(handle) = core.handle() {
                handle.send(Command::CancelPairing);
            }
            ack("pairing cancelled")
        }
        IpcRequest::Join { code } => {
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            if !handle.snapshot().is_connected() {
                let _ = wait_worker_condition(handle.clone(), Duration::from_secs(8), |p| {
                    p.snapshot.is_connected().then_some(())
                })
                .await;
            }
            let before_gen = handle.published().generation;
            handle.send(Command::Join(code.trim().to_owned()));
            let outcome =
                wait_worker_condition(handle.clone(), Duration::from_secs(32), move |p| {
                    if p.generation <= before_gen {
                        return None;
                    }
                    match &p.snapshot.pairing {
                        PairingState::Success { device } => {
                            let name = p
                                .snapshot
                                .devices
                                .iter()
                                .find(|d| d.id == *device)
                                .and_then(|d| d.name.clone());
                            let kind =
                                pairing_kind_str(p.paired.unwrap_or(PairingKind::JoinedOther))
                                    .to_owned();
                            Some(PairPollOutcome::Success {
                                device_id: hex(device),
                                short_id: short_id(device),
                                name,
                                kind,
                            })
                        }
                        PairingState::Failed(reason) => Some(PairPollOutcome::Failed {
                            reason: pairing_failure_text(reason),
                        }),
                        PairingState::Joining
                        | PairingState::ShowingCode { .. }
                        | PairingState::Idle => None,
                    }
                })
                .await;

            match outcome {
                Some(done) => {
                    handle.send(Command::CancelPairing);
                    IpcResponse::PairOutcome { outcome: done }
                }
                None => {
                    handle.send(Command::CancelPairing);
                    err("timed out waiting to join group")
                }
            }
        }
        IpcRequest::Remove { device } => {
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            let snap = handle.snapshot();
            let target = match resolve_device(&snap.devices, &device) {
                Ok(dev) => dev,
                Err(msg) => return err(msg),
            };
            if !snap.is_connected() {
                return err("not connected to the relay");
            }
            let removed_id = target.id;
            handle.send(Command::RemoveDevice(removed_id));
            let _ = wait_worker_condition(handle, Duration::from_secs(5), move |p| {
                (!p.snapshot.devices.iter().any(|d| d.id == removed_id)).then_some(())
            })
            .await;
            ack(format!("removed device {}", short_id(&removed_id)))
        }
        IpcRequest::Rename { name } => {
            let trimmed = name.trim().to_owned();
            if let Err(e) = encode_device_name(&trimmed) {
                return err(format!("invalid device name: {e}"));
            }
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            let expected = trimmed.clone();
            handle.send(Command::Rename(trimmed.clone()));
            let _ = wait_worker_condition(handle, Duration::from_secs(3), move |p| {
                (p.snapshot.name == expected).then_some(())
            })
            .await;
            ack(format!("renamed device to '{trimmed}'"))
        }
        IpcRequest::SetPaused { paused } => {
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            handle.send(Command::SetPaused(paused));
            let _ = wait_worker_condition(handle, Duration::from_secs(2), move |p| {
                (p.snapshot.pause.all == paused).then_some(())
            })
            .await;
            ack(if paused {
                "sync paused"
            } else {
                "sync resumed"
            })
        }
        IpcRequest::SendClipboardNow => {
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            let snap = handle.snapshot();
            if !snap.is_connected() {
                return err("not connected to the relay");
            }
            if snap.devices.is_empty() {
                return err("no trusted devices paired");
            }
            let clip = core.clipboard.clone();
            let read = tokio::task::spawn_blocking(move || clip.read_direct())
                .await
                .unwrap_or(Content::NoText);
            match read {
                Content::Text(text) if text.is_empty() => err("clipboard is empty"),
                Content::Text(text) if text.len() > MAX_CLIPBOARD_TEXT_LEN => {
                    err("clipboard text exceeds 256 KiB limit")
                }
                Content::Text(text) => {
                    handle.send(Command::SendText(text));
                    ack(format!(
                        "clipboard sent to {} device(s)",
                        snap.devices.len()
                    ))
                }
                Content::Excluded => err("clipboard contains excluded password-manager content"),
                Content::Image(image) => {
                    handle.send(Command::SendImage(image));
                    ack("clipboard image sent")
                }
                Content::TooLarge => err("clipboard text exceeds 256 KiB limit"),
                Content::NoText => err("no plain text on clipboard"),
            }
        }
        IpcRequest::SendFiles { device, files } => {
            if files.is_empty() {
                return err("no files specified");
            }
            for path in &files {
                match std::fs::metadata(path) {
                    Ok(meta) if meta.is_file() => {}
                    Ok(_) => return err(format!("{} is not a regular file", path.display())),
                    Err(e) => return err(format!("{}: {e}", path.display())),
                }
            }
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            let snap = handle.snapshot();
            let target = match resolve_device(&snap.devices, &device) {
                Ok(dev) => dev,
                Err(msg) => return err(msg),
            };
            let count = files.len();
            handle.send(Command::SendFiles(target.id, files));
            ack(format!(
                "queued {count} file(s) for {}",
                target.name.as_deref().unwrap_or(&short_id(&target.id))
            ))
        }
        IpcRequest::ShareUrl { device, url } => {
            let clean = url.trim().to_owned();
            if !is_shareable_url(&clean) {
                return err(
                    "invalid URL (must start with http:// or https:// and be <= 2048 bytes)",
                );
            }
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            let snap = handle.snapshot();
            let target = match resolve_device(&snap.devices, &device) {
                Ok(dev) => dev,
                Err(msg) => return err(msg),
            };
            handle.send(Command::SendPacket(
                target.id,
                Packet::ShareUrl { url: clean },
            ));
            ack(format!(
                "sent URL to {}",
                target.name.as_deref().unwrap_or(&short_id(&target.id))
            ))
        }
        IpcRequest::Ring { device } => {
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            let target = match resolve_device(&handle.snapshot().devices, &device) {
                Ok(dev) => dev,
                Err(msg) => return err(msg),
            };
            handle.send(Command::Ring(target.id));
            ack(format!(
                "ringing {}",
                target.name.as_deref().unwrap_or(&short_id(&target.id))
            ))
        }
        IpcRequest::StopRing { device } => {
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            let target = match resolve_device(&handle.snapshot().devices, &device) {
                Ok(dev) => dev,
                Err(msg) => return err(msg),
            };
            handle.send(Command::StopRinging(target.id));
            ack(format!(
                "stopped ringing {}",
                target.name.as_deref().unwrap_or(&short_id(&target.id))
            ))
        }
        IpcRequest::FoundIt => {
            core.notifier.stop_ring();
            if let Some(handle) = core.handle() {
                handle.send(Command::FoundIt);
            }
            ack("stopped ringing this computer")
        }
        IpcRequest::Ping { device, message } => {
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            let target = match resolve_device(&handle.snapshot().devices, &device) {
                Ok(dev) => dev,
                Err(msg) => return err(msg),
            };
            let message = message
                .map(|m| m.trim().to_owned())
                .filter(|m| !m.is_empty());
            handle.send(Command::SendPacket(target.id, Packet::Ping { message }));
            ack(format!(
                "pinged {}",
                target.name.as_deref().unwrap_or(&short_id(&target.id))
            ))
        }
        IpcRequest::Messages { device, thread } => {
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            let snap = handle.snapshot();
            let target = match resolve_device(&snap.devices, &device) {
                Ok(dev) => dev,
                Err(msg) => return err(msg),
            };
            let dev_id = target.id;
            handle.send(Command::OpenMessages(dev_id));
            if let Some(t_id) = thread {
                handle.send(Command::LoadMessages {
                    device: dev_id,
                    thread: t_id,
                    before: None,
                });
            }
            // Wait briefly if SMS data is not cached yet and we are online.
            let need_wait = {
                let peer = snap.peer(&dev_id);
                snap.is_connected()
                    && (peer.sms.conversations.is_none()
                        || thread.is_some_and(|t| !peer.sms.threads.contains_key(&t)))
            };
            if need_wait {
                let _ =
                    wait_worker_condition(handle.clone(), Duration::from_millis(1500), move |p| {
                        let peer = p.snapshot.peer(&dev_id);
                        match thread {
                            Some(t) => peer.sms.threads.contains_key(&t).then_some(()),
                            None => peer.sms.conversations.is_some().then_some(()),
                        }
                    })
                    .await;
            }
            let latest = handle.snapshot();
            let sms_state = latest.peer(&dev_id).sms;
            let conversations = sms_state.conversations.clone().unwrap_or_default();
            let messages = thread
                .and_then(|t| sms_state.threads.get(&t).cloned())
                .unwrap_or_default();
            let outbox = sms_state
                .outbox
                .iter()
                .map(|o| OutgoingSmsView {
                    request_id: o.request_id.clone(),
                    addresses: o.addresses.clone(),
                    text: o.text.clone(),
                    thread: o.thread,
                    state: match &o.state {
                        SendState::Pending => "pending".into(),
                        SendState::Sent => "sent".into(),
                        SendState::Failed(Some(why)) => format!("failed: {why}"),
                        SendState::Failed(None) => "failed".into(),
                    },
                })
                .collect();
            IpcResponse::Messages {
                payload: MessagesPayload {
                    device_id: hex(&dev_id),
                    device_name: target.name,
                    conversations,
                    thread,
                    messages,
                    outbox,
                },
            }
        }
        IpcRequest::SendSms { device, to, text } => {
            let body = text.trim().to_owned();
            if body.is_empty() {
                return err("SMS text cannot be empty");
            }
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            let snap = handle.snapshot();
            let target = match resolve_device(&snap.devices, &device) {
                Ok(dev) => dev,
                Err(msg) => return err(msg),
            };
            let peer = snap.peer(&target.id);
            let number =
                sms::recipient(&peer.sms.contacts, &to).unwrap_or_else(|| to.trim().to_owned());
            if number.is_empty() {
                return err("recipient phone number cannot be empty");
            }
            let addresses = vec![number];
            let thread = peer.sms.conversation_with(&addresses).map(|c| c.thread);
            handle.send(Command::SendSms {
                device: target.id,
                addresses,
                text: body,
                thread,
            });
            ack("SMS queued to phone")
        }
        IpcRequest::CommandsList => match core.published() {
            Some(p) => IpcResponse::Commands {
                commands: p.snapshot.commands.clone(),
            },
            None => err("worker is restarting"),
        },
        IpcRequest::CommandsAdd { name, line } => {
            let (name, line) = (name.trim().to_owned(), line.trim().to_owned());
            if name.is_empty() || line.is_empty() {
                return err("shortcut name and command line must be non-empty");
            }
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            let mut commands = handle.snapshot().commands.clone();
            let id = hex(&random_message_id()[..8]);
            commands.push(UserCommand { id, name, line });
            let expected_len = commands.len();
            handle.send(Command::SetCommands(commands));
            let _ = wait_worker_condition(handle.clone(), Duration::from_secs(2), move |p| {
                (p.snapshot.commands.len() == expected_len).then_some(())
            })
            .await;
            IpcResponse::Commands {
                commands: handle.snapshot().commands.clone(),
            }
        }
        IpcRequest::CommandsRemove { query } => {
            let q = query.trim();
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            let mut commands = handle.snapshot().commands.clone();
            let before = commands.len();
            commands.retain(|c| !c.id.eq_ignore_ascii_case(q) && !c.name.eq_ignore_ascii_case(q));
            if commands.len() == before {
                return err(format!("no shortcut matches '{q}'"));
            }
            let expected_len = commands.len();
            handle.send(Command::SetCommands(commands));
            let _ = wait_worker_condition(handle.clone(), Duration::from_secs(2), move |p| {
                (p.snapshot.commands.len() == expected_len).then_some(())
            })
            .await;
            IpcResponse::Commands {
                commands: handle.snapshot().commands.clone(),
            }
        }
        IpcRequest::SetControl { device, allowed } => {
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            let target = match resolve_device(&handle.snapshot().devices, &device) {
                Ok(dev) => dev,
                Err(msg) => return err(msg),
            };
            let id = target.id;
            handle.send(Command::SetControl(id, allowed));
            let _ = wait_worker_condition(handle, Duration::from_secs(2), move |p| {
                (p.snapshot.control.contains(&id) == allowed).then_some(())
            })
            .await;
            ack(format!(
                "remote control {} for {}",
                if allowed { "enabled" } else { "disabled" },
                target.name.as_deref().unwrap_or(&short_id(&id))
            ))
        }
        IpcRequest::GetFeatures => match core.published() {
            Some(p) => IpcResponse::Features {
                features: p.snapshot.features,
            },
            None => err("worker is restarting"),
        },
        IpcRequest::SetFeature { feature, enabled } => {
            let Some(handle) = core.handle() else {
                return err("worker is restarting");
            };
            let mut features = handle.snapshot().features;
            if let Err(msg) = apply_feature_toggle(&mut features, &feature, enabled) {
                return err(msg);
            }
            handle.send(Command::SetFeatures(features));
            let _ = wait_worker_condition(handle.clone(), Duration::from_secs(2), move |p| {
                (p.snapshot.features == features).then_some(())
            })
            .await;
            IpcResponse::Features {
                features: handle.snapshot().features,
            }
        }
        IpcRequest::GetRelay => match core.published() {
            Some(p) => IpcResponse::Relay {
                relay_url: p.snapshot.relay_url.clone(),
            },
            None => err("worker is restarting"),
        },
        IpcRequest::SetRelay { url } => {
            let clean = url.trim().to_owned();
            if let Err(e) = relay_endpoint(&clean) {
                return err(e.to_string());
            }
            if let Some(p) = core.published()
                && !p.snapshot.devices.is_empty()
            {
                return err(
                    "cannot change relay while paired with devices; run `weft-desktop leave` first",
                );
            }
            let core_bg = core.clone();
            let clean_clone = clean.clone();
            let restarted = tokio::task::spawn_blocking(move || {
                core_bg.restart_worker(|dir| {
                    dir.update(|state| state.relay_url = clean_clone)
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                })
            })
            .await;
            match restarted {
                Ok(Ok(())) => IpcResponse::Relay { relay_url: clean },
                Ok(Err(msg)) => err(msg),
                Err(join_err) => err(join_err.to_string()),
            }
        }
        IpcRequest::Leave => {
            let core_bg = core.clone();
            let res =
                tokio::task::spawn_blocking(move || core_bg.restart_worker(leave_group_on_disk))
                    .await;
            match res {
                Ok(Ok(told_peers)) => IpcResponse::Left { told_peers },
                Ok(Err(msg)) => err(msg),
                Err(join_err) => err(join_err.to_string()),
            }
        }
        IpcRequest::UpdateCheck => {
            let updater = core.updater.clone();
            let snapshot = tokio::task::spawn_blocking(move || updater.check_now())
                .await
                .unwrap_or_else(|_| core.updater.snapshot());
            IpcResponse::Update { snapshot }
        }
        IpcRequest::UpdateInstall => {
            let updater = core.updater.clone();
            let snapshot = tokio::task::spawn_blocking(move || updater.install_now())
                .await
                .unwrap_or_else(|_| core.updater.snapshot());
            IpcResponse::Update { snapshot }
        }
        IpcRequest::Shutdown => {
            let _ = core.stop_tx.send(true);
            ack("daemon shutting down")
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn single_instance_lock_blocks_second_daemon_in_same_state_dir() {
        let dir = TempDir::new().unwrap();
        let state_dir = dir.path().join("weft-state");
        let first = acquire_single_instance_lock(&state_dir).unwrap();
        assert!(matches!(
            acquire_single_instance_lock(&state_dir),
            Err(DaemonError::AlreadyRunning(_))
        ));
        drop(first);
        assert!(acquire_single_instance_lock(&state_dir).is_ok());
    }

    #[test]
    fn resolves_devices_by_prefix_or_name_and_toggles_features() {
        let devices = vec![
            Device {
                id: [0x1a; 16],
                name: Some("Pixel 9".into()),
            },
            Device {
                id: [0x2b; 16],
                name: Some("ThinkPad".into()),
            },
        ];
        assert_eq!(resolve_device(&devices, "1a1a").unwrap().id, [0x1a; 16]);
        assert_eq!(resolve_device(&devices, "pixel 9").unwrap().id, [0x1a; 16]);
        assert!(resolve_device(&devices, "unknown").is_err());

        let mut f = Features::default();
        apply_feature_toggle(&mut f, "media", false).unwrap();
        assert!(!f.media && f.volume);
        assert!(apply_feature_toggle(&mut f, "nonexistent", false).is_err());
    }
}
