//! The thread that talks to the relay.
//!
//! It owns the [`Session`], on a single-threaded tokio runtime: it connects,
//! reconnects when the connection is lost, hands received clipboard text to
//! a callback, and runs the [`Command`]s the other threads send it. What it
//! knows is published as an immutable [`Snapshot`] that any thread can read
//! and wait on.
//!
//! It also handles the packets of `docs/features.md` sections 2 to 6:
//! capabilities and status of the other devices, ring, ping, links, files,
//! the phone's notifications and calls, media and volume. What the user
//! should see or hear of them, and what this computer's players and volume
//! are asked to do, goes to a [`Notice`] callback; the worker itself never
//! opens, plays, shows or changes anything. This computer's own players and
//! volume come in as [`Command::LocalMedia`] and [`Command::LocalVolume`].
//!
//! Section 7 and 8 too: the phone's SMS and contacts, kept in the snapshot
//! (in memory only); remote input and the presenter, handed to the
//! [`Hooks::on_input`] callback only from the devices the user allowed;
//! the user's commands, run through [`Hooks::run_command`] by id only.
//!
//! Clipboard text, pairing codes, links, file contents, notification
//! texts, messages, contacts, typed text and command lines are never
//! logged.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::future::Future;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender, unbounded_channel};
use weft_cli::session::{
    JOIN_TIMEOUT, KEEPALIVE_INTERVAL, MAX_BACKOFF, MIN_BACKOFF, PAIRING_TIMEOUT, is_fatal,
};
use weft_cli::{Error, Event, Next, PairOutcome, Result, Session, State, StateDir, hex, short_id};
use weft_protocol::{ControlError, PairingCode, device_id_from_sign_pk, random_message_id};

use crate::features::{Features, Implemented, Settings, UserCommand};
use crate::input::Input;
use crate::media::{Cover, Covers, LocalPlayer};
use crate::packets::{
    self, Battery, Call, Capabilities, Connectivity, FileOffer, MediaAction, Notification, Packet,
    Parsed, Player, RejectReason, SmsMessagesRequest, SmsSend, Volume, VolumeSet, is_shareable_url,
    parse_transfer_id, transfer_id,
};
use crate::sms::{self, OutgoingSms, SendState, Sms};
use crate::sync::Pause;
use crate::sync::{ClipboardImage, MAX_CLIPBOARD_IMAGE_BYTES};
use crate::transfer::{self, Failure, Incoming, Outgoing, Stage, Tick};
use crate::{files, input, media};

/// The relay a new installation uses.
pub const DEFAULT_RELAY_URL: &str = "wss://weft.frohub.eu/v1";

/// How long one wait for a joining device lasts before the worker looks at
/// its commands and at the frames that arrived meanwhile.
const PAIRING_SLICE: Duration = Duration::from_millis(250);

/// How long a ring lasts unless someone stops it.
pub const RING_DURATION: Duration = Duration::from_secs(60);

/// How often the clocks of rings and transfers are looked at while one runs.
const TICK: Duration = Duration::from_secs(1);

/// The shortest time between two snapshots that only move a progress bar.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// Finished transfers kept in the snapshot, per direction.
const FINISHED_SHOWN: usize = 8;

/// Finished incoming transfers remembered, to answer a repeated offer.
const COMPLETED_KEPT: usize = 64;

/// Notifications that come this soon after a `notification.request` are
/// the phone's current ones, not news: they go to the notification centre
/// without popping up.
const REQUEST_QUIET: Duration = Duration::from_secs(10);

/// The longest notification text shown, in characters, as the contract
/// caps it.
const MAX_NOTIFICATION_TEXT: usize = 2000;

/// The most players of another device kept.
const MAX_PLAYERS: usize = 16;

/// A request to the worker.
pub enum Command {
    /// Send clipboard text to every trusted device.
    SendText(String),
    /// Send a fresh clipboard image; never queued across reconnection.
    SendImage(ClipboardImage),
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
    /// Change the feature switches, saved and announced.
    SetFeatures(Features),
    /// Make a device ring.
    Ring([u8; 16]),
    /// Stop the ring this device started on another one.
    StopRinging([u8; 16]),
    /// The user found this computer: stop its ring.
    FoundIt,
    /// Send these files to a device.
    SendFiles([u8; 16], Vec<PathBuf>),
    /// Cancel a transfer, either way, by its id.
    CancelTransfer([u8; 16]),
    /// Send a packet the user asked for to a device: an answer to one of
    /// its notifications, a call mute, a media or volume control. Dropped
    /// if the feature it belongs to is off.
    SendPacket([u8; 16], Packet),
    /// This computer's players changed; sent on to the devices that show
    /// them, unless only the positions moved with time. Their covers go
    /// first, once per connection, to the devices that show covers.
    LocalMedia(Vec<LocalPlayer>),
    /// This computer's output volume changed.
    LocalVolume(Volume),
    /// The messages of a phone are shown: ask for its conversations, and
    /// for its contacts while none came.
    OpenMessages([u8; 16]),
    /// Ask a phone for the newest messages of a thread, or for those
    /// older than `before`.
    LoadMessages {
        device: [u8; 16],
        thread: i64,
        before: Option<i64>,
    },
    /// Send an SMS from a phone, in `thread` or to start one.
    SendSms {
        device: [u8; 16],
        addresses: Vec<String>,
        text: String,
        thread: Option<i64>,
    },
    /// Allow or forbid a device to control this computer, saved.
    SetControl([u8; 16], bool),
    /// Replace the commands the devices may run, saved and announced.
    SetCommands(Vec<UserCommand>),
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

/// The players of another device, and when they were received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerMedia {
    pub players: Vec<Player>,
    pub at: Instant,
}

impl PeerMedia {
    /// The player to show: the first one playing, else the first one.
    pub fn current(&self) -> Option<&Player> {
        self.players
            .iter()
            .find(|player| player.playing)
            .or(self.players.first())
    }
}

/// What a trusted device told about itself since this worker started.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerInfo {
    pub capabilities: Option<Capabilities>,
    pub battery: Option<Battery>,
    pub connectivity: Option<Connectivity>,
    pub media: Option<PeerMedia>,
    /// The last covers it sent, by id.
    pub covers: Covers,
    pub volume: Option<Volume>,
    /// Its SMS and contacts, shared between snapshots until they change.
    pub sms: Arc<Sms>,
}

impl PeerInfo {
    /// Whether the device acts on `kind`, as far as this device knows.
    pub fn handles(&self, kind: &str) -> bool {
        self.capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.handles(kind))
    }

    /// Whether the device may send `kind`, as far as this device knows.
    pub fn sends(&self, kind: &str) -> bool {
        self.capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.sends(kind))
    }

    /// The cover of one of its players, when it sent the image `art`
    /// names.
    pub fn cover(&self, player: &Player) -> Option<&Cover> {
        self.covers.get(player.art.as_deref()?)
    }
}

/// A transfer as an interface shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferView {
    pub id: [u8; 16],
    /// The device on the other end.
    pub device: [u8; 16],
    /// Sent by this device, rather than received.
    pub outgoing: bool,
    pub name: String,
    pub size: u64,
    /// Bytes confirmed (sent) or written (received).
    pub done: u64,
    pub stage: Stage,
}

/// Something the user should see or hear. `device` is the other device's
/// name, when it announced one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// An integrity-checked image to put on the clipboard, without echoing it.
    ClipboardImage {
        from: [u8; 16],
        image: ClipboardImage,
    },
    /// A device rings this computer, until [`Notice::RingEnded`].
    Ring {
        from: [u8; 16],
        device: Option<String>,
    },
    RingEnded,
    Ping {
        from: [u8; 16],
        device: Option<String>,
        message: Option<String>,
    },
    /// A web link to open, already checked with [`is_shareable_url`].
    Link {
        from: [u8; 16],
        device: Option<String>,
        url: String,
    },
    /// A file was received and saved at `path`.
    FileReceived {
        from: [u8; 16],
        device: Option<String>,
        path: PathBuf,
    },
    TransferFailed {
        device: Option<String>,
        name: String,
        outgoing: bool,
        failure: Failure,
    },
    /// A notification of the phone, new or updated. Its text is cut to
    /// the contract's length, and `silent` is set for an update or a
    /// notification the phone already had.
    Notification {
        from: [u8; 16],
        device: Option<String>,
        notification: Notification,
    },
    /// The phone's notification `id` is gone.
    NotificationRemoved {
        from: [u8; 16],
        id: String,
    },
    /// A call on the phone: ringing, answered, missed or ended.
    Call {
        from: [u8; 16],
        device: Option<String>,
        call: Call,
    },
    /// A device asks a player of this computer to act.
    MediaCommand {
        from: [u8; 16],
        player: String,
        action: MediaAction,
    },
    /// A device asks for another output volume, level in 0 to 100.
    SetVolume {
        from: [u8; 16],
        set: VolumeSet,
    },
    /// An SMS came to the phone. `sender` is the contact's name or the
    /// number; a reply goes to `addresses`, the people of the thread.
    SmsReceived {
        from: [u8; 16],
        device: Option<String>,
        thread: i64,
        sender: String,
        text: String,
        addresses: Vec<String>,
    },
    /// A command ran for a device.
    CommandRan {
        from: [u8; 16],
        device: Option<String>,
        name: String,
    },
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
    Failed(PairingFailure),
}

/// How the pairing shown as [`PairingState::Success`] went, so an interface
/// can say who joined whose group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingKind {
    /// A device used the code shown here and joined this device's group.
    Joined,
    /// A device used the code shown here to invite this device into its
    /// group, which this device is now in.
    Invited,
    /// This device used the code of the other device
    /// ([`Command::Join`]) and joined its group.
    JoinedOther,
}

/// Why a pairing failed, in terms an interface can put into words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingFailure {
    /// The text given to [`Command::Join`] is not a pairing code.
    InvalidCode,
    /// The code shown here expired unused, or the relay no longer knows
    /// the code given to [`Command::Join`].
    Expired,
    /// The code given to [`Command::Join`] was made by this device.
    OwnCode,
    /// The code belongs to a group on another relay.
    OtherRelay,
    /// The group already has as many devices as it can hold.
    GroupFull,
    /// There was no connection to the relay.
    NotConnected,
    /// The connection to the relay was lost on the way.
    ConnectionLost,
    /// The device that showed the code did not answer in time.
    NoAnswer,
    /// Anything else, as the error put it.
    Other(String),
}

impl PairingFailure {
    /// The failure a refused or timed-out request stands for.
    fn of(err: &Error) -> PairingFailure {
        match err {
            Error::Relay(ControlError::UnknownPairing) => PairingFailure::Expired,
            Error::Relay(ControlError::GroupFull) | Error::GroupFull => PairingFailure::GroupFull,
            Error::OwnCode => PairingFailure::OwnCode,
            Error::RelayUrlMismatch { .. } => PairingFailure::OtherRelay,
            Error::Timeout(_) | Error::NoAnswer | Error::NoWelcome => PairingFailure::NoAnswer,
            err => PairingFailure::Other(err.to_string()),
        }
    }
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
    /// What the trusted devices told about themselves, by id.
    pub peers: BTreeMap<[u8; 16], PeerInfo>,
    pub pause: Pause,
    pub features: Features,
    /// The devices allowed to control this computer.
    pub control: BTreeSet<[u8; 16]>,
    /// The commands the devices may run.
    pub commands: Vec<UserCommand>,
    pub pairing: PairingState,
    pub last_activity: Option<Activity>,
    /// The device ringing this computer, while it rings.
    pub ringing: Option<[u8; 16]>,
    /// Transfers under way, then the last finished ones.
    pub transfers: Vec<TransferView>,
}

impl Snapshot {
    pub fn is_connected(&self) -> bool {
        self.status == Status::Connected
    }

    /// What `device` told about itself; nothing yet for an unknown one.
    pub fn peer(&self, device: &[u8; 16]) -> PeerInfo {
        self.peers.get(device).cloned().unwrap_or_default()
    }
}

/// A snapshot and its rank: each new snapshot gets the next generation.
#[derive(Clone)]
pub struct Published {
    pub generation: u64,
    pub snapshot: Arc<Snapshot>,
    /// How the pairing went, while `snapshot.pairing` is
    /// [`PairingState::Success`]; `None` otherwise.
    pub paired: Option<PairingKind>,
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

/// How the worker reaches the rest of the app.
pub struct Hooks {
    /// Called with every clipboard text to put on the local clipboard.
    pub on_text: Box<dyn FnMut(String) + Send>,
    /// Called with what the user should see or hear, from the worker
    /// thread or from a pool thread.
    pub on_notice: Arc<dyn Fn(Notice) + Send + Sync>,
    /// Where received files go once complete.
    pub downloads: PathBuf,
    /// Called on the worker thread with what an allowed device asks of
    /// this computer's pointer and keyboard, and the presenter's spot.
    pub on_input: Box<dyn FnMut(Input) + Send>,
    /// Called on the worker thread with the line of a command to run.
    pub run_command: Box<dyn FnMut(&str) + Send>,
    /// The packet types the app acts on and sends, if fewer than this core
    /// knows: its capabilities list only these, and packets the user asks
    /// for ([`Command::SendPacket`]) go only if they are among them.
    /// `None` for an app that implements every feature.
    pub implemented: Option<Implemented>,
}

/// Loads the state in `dir`, creating an identity on [`DEFAULT_RELAY_URL`]
/// if there is none, and starts the worker thread.
///
/// Fails if the state cannot be read or created; nothing is started then.
pub fn spawn(dir: StateDir, hooks: Hooks) -> Result<(Handle, JoinHandle<()>)> {
    spawn_on(dir, DEFAULT_RELAY_URL, hooks)
}

/// [`spawn`], with the relay a new identity is created on. An existing
/// state keeps its relay.
pub fn spawn_on(dir: StateDir, relay_url: &str, hooks: Hooks) -> Result<(Handle, JoinHandle<()>)> {
    let state = match dir.load() {
        Err(Error::NotInitialised(_)) => dir.load_or_create(relay_url)?,
        loaded => loaded?,
    };
    let settings = Settings::load(dir.path());
    let partial_dir = files::partial_dir(dir.path());
    files::remove_stale(&partial_dir);
    let mut view = Snapshot {
        status: Status::Connecting,
        name: String::new(),
        device_id: state.identity().device_id(),
        relay_url: state.relay_url.clone(),
        devices: Vec::new(),
        peers: BTreeMap::new(),
        pause: Pause::default(),
        features: settings.features,
        control: settings.controllers(),
        commands: settings.commands,
        pairing: PairingState::Idle,
        last_activity: None,
        ringing: None,
        transfers: Vec::new(),
    };
    describe(&mut view, &state);
    let shared = Arc::new(Shared {
        board: Mutex::new(Board {
            published: Published {
                generation: 0,
                snapshot: Arc::new(view.clone()),
                paired: None,
            },
            stopped: false,
        }),
        changed: Condvar::new(),
    });
    let (commands, inbox) = unbounded_channel();
    let now = Instant::now();
    let worker = Worker {
        dir,
        commands: inbox,
        shared: shared.clone(),
        hooks,
        partial_dir,
        view,
        pairing: None,
        pairing_kind: PairingKind::Joined,
        ring_until: None,
        outgoing: Vec::new(),
        incoming: Vec::new(),
        completed: Vec::new(),
        local_media: None,
        local_covers: BTreeMap::new(),
        art_sent: BTreeMap::new(),
        local_volume: None,
        requested: BTreeMap::new(),
        shown: BTreeMap::new(),
        older_asked: BTreeSet::new(),
        traffic_at: now,
        progress_at: now,
        image_out: BTreeMap::new(),
        image_in: BTreeMap::new(),
        lan: None,
        lan_frames: mpsc::channel(16),
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

/// Copies what the snapshot shows of the state, and forgets what devices
/// no longer trusted told.
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
    let devices = &view.devices;
    view.peers
        .retain(|id, _| devices.iter().any(|device| device.id == *id));
}

/// A pairing slot this device opened and is waiting on.
struct OpenPairing {
    code: PairingCode,
    expires_at: Instant,
    /// When to check next that the connection is still alive.
    keepalive_at: Instant,
}

/// A file being sent, and the open file once offered.
struct Sending {
    transfer: Outgoing,
    path: PathBuf,
    file: Option<File>,
}

/// A file being received into its partial file.
struct Receiving {
    transfer: Incoming,
    partial: PathBuf,
    file: Option<File>,
}

struct Worker {
    lan: Option<crate::lan::Transport>,
    lan_frames: (mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>),
    image_out: BTreeMap<ImageKey, ImageSending>,
    image_in: BTreeMap<([u8; 16], [u8; 16]), ImageReceiving>,
    dir: StateDir,
    commands: UnboundedReceiver<Command>,
    shared: Arc<Shared>,
    hooks: Hooks,
    partial_dir: PathBuf,
    /// The next snapshot, published by [`Worker::publish`].
    view: Snapshot,
    pairing: Option<OpenPairing>,
    /// How the last successful pairing went.
    pairing_kind: PairingKind,
    /// When the ring of this computer stops by itself.
    ring_until: Option<Instant>,
    outgoing: Vec<Sending>,
    incoming: Vec<Receiving>,
    /// Incoming transfers already complete, as (sender, id).
    completed: Vec<([u8; 16], [u8; 16])>,
    /// This computer's players as last sent, and when they were read.
    local_media: Option<(Vec<Player>, Instant)>,
    /// The covers of these players, by id.
    local_covers: BTreeMap<String, Cover>,
    /// The covers each device got on this connection, by id: sent again
    /// only after new capabilities.
    art_sent: BTreeMap<[u8; 16], BTreeSet<String>>,
    /// This computer's volume as last sent.
    local_volume: Option<Volume>,
    /// The last `notification.request` to each device.
    requested: BTreeMap<[u8; 16], NotificationRequest>,
    /// The notifications of each device shown here and not removed since,
    /// by id.
    shown: BTreeMap<[u8; 16], BTreeSet<String>>,
    /// The threads asked for older messages, by phone.
    older_asked: BTreeSet<([u8; 16], i64)>,
    /// The last frame from the relay, for the keepalive.
    traffic_at: Instant,
    /// The last snapshot that moved a progress bar.
    progress_at: Instant,
}

struct ImageReceiving {
    offer: packets::ClipboardImageOffer,
    bytes: Vec<u8>,
    at: Instant,
}

type ImageKey = ([u8; 16], [u8; 16]);
type ImageSending = (Arc<ClipboardImage>, Instant);

enum WorkerInput {
    Command(Option<Command>),
    Lan(Vec<u8>),
}

/// A `notification.request` sent to a device.
struct NotificationRequest {
    at: Instant,
    /// The notifications shown when it was sent and not sent again since:
    /// those still here after [`REQUEST_QUIET`] are gone from the phone.
    stale: BTreeSet<String>,
}

/// Whether a device shows this computer's commands: it said it takes
/// their list or runs them, or it said nothing yet.
fn wants_commands(peer: &PeerInfo) -> bool {
    peer.capabilities.is_none() || peer.handles("command.list") || peer.sends("command.run")
}

/// Now, in ms since the epoch, as SMS times are.
fn epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
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
            | Error::NoGroup
            | Error::NoAnswer
            | Error::NoWelcome
    ) && !is_fatal(err)
}

/// A connection error is passed on; a refusal is logged and swallowed.
fn only_connection_errors(done: Result<()>, what: &str) -> Result<()> {
    match done {
        Err(err) if is_refusal(&err) => {
            tracing::warn!(error = %err, "{what} not sent");
            Ok(())
        }
        done => done,
    }
}

/// Reads the `len` bytes at `offset` of `file`.
fn read_chunk(file: &mut File, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
    let mut data = vec![0; len];
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut data)?;
    Ok(data)
}

/// Opens the partial file at `offset` bytes, cutting what is past it.
fn open_partial(path: &std::path::Path, offset: u64) -> std::io::Result<File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    file.set_len(offset)?;
    file.seek(SeekFrom::Start(offset))?;
    Ok(file)
}

impl Worker {
    async fn send_image(&mut self, session: &mut Session, image: ClipboardImage) -> Result<()> {
        if self.view.pause.blocks_send() || !image.is_valid() {
            tracing::debug!(
                paused = self.view.pause.blocks_send(),
                valid = image.is_valid(),
                "clipboard image not sent"
            );
            return Ok(());
        }
        self.image_out.clear();
        let image = Arc::new(image);
        let devices: Vec<_> = self.view.devices.iter().map(|d| d.id).collect();
        let mut offered = 0;
        for device in devices {
            let peer = self.view.peer(&device);
            let peer_takes = peer.handles("clipboard.image.offer");
            let we_send = self
                .capabilities_for(&device)
                .sends("clipboard.image.offer");
            if !peer_takes || !we_send {
                tracing::debug!(
                    device = %short_id(&device),
                    capabilities_known = peer.capabilities.is_some(),
                    peer_takes,
                    we_send,
                    "clipboard image not offered to this device"
                );
                continue;
            }
            let id = random_message_id();
            let offer = packets::ClipboardImageOffer {
                id: transfer_id(&id),
                mime: image.mime,
                size: image.bytes.len() as u64,
                hash: hex(&image.hash()),
            };
            self.image_out
                .insert((device, id), (image.clone(), Instant::now()));
            let lan = self.lan.as_ref().is_some_and(|lan| lan.has_route(&device));
            self.send(session, device, &Packet::ClipboardImageOffer(offer))
                .await?;
            tracing::info!(
                device = %short_id(&device),
                mime = image.mime.as_str(),
                size = image.bytes.len(),
                route = if lan { "lan" } else { "relay" },
                "clipboard image offered"
            );
            offered += 1;
        }
        if offered == 0 {
            tracing::debug!("clipboard image not sent: no device takes images");
        }
        Ok(())
    }

    async fn image_offer(
        &mut self,
        session: &mut Session,
        from: [u8; 16],
        offer: packets::ClipboardImageOffer,
    ) -> Result<()> {
        if self.view.pause.blocks_receive()
            || !self
                .capabilities_for(&from)
                .handles("clipboard.image.offer")
            || offer.size == 0
            || offer.size > MAX_CLIPBOARD_IMAGE_BYTES as u64
            || weft_cli::unhex::<16>(&offer.hash).is_none()
        {
            return Ok(());
        }
        let Some(id) = parse_transfer_id(&offer.id) else {
            return Ok(());
        };
        // One incoming image per peer and at most two full images reserved
        // across the group. Size accounting uses offers, not bytes received.
        self.image_in.retain(|(peer, _), _| *peer != from);
        let reserved: u64 = self.image_in.values().map(|i| i.offer.size).sum();
        if reserved + offer.size > (2 * MAX_CLIPBOARD_IMAGE_BYTES) as u64
            || self.incoming.iter().any(|r| r.transfer.id == id)
        {
            return Ok(());
        }
        let accept = Packet::ClipboardImageAccept {
            id: offer.id.clone(),
        };
        self.image_in.insert(
            (from, id),
            ImageReceiving {
                offer,
                bytes: Vec::new(),
                at: Instant::now(),
            },
        );
        self.send(session, from, &accept).await
    }

    async fn image_chunk(
        &mut self,
        session: &mut Session,
        from: [u8; 16],
        id: [u8; 16],
        offset: u64,
        data: &[u8],
    ) -> Result<()> {
        let key = (from, id);
        let Some(image) = self.image_in.get_mut(&key) else {
            return Ok(());
        };
        if self.view.pause.blocks_receive()
            || offset != image.bytes.len() as u64
            || data.is_empty()
            || image.bytes.len() as u64 + data.len() as u64 > image.offer.size
            || image.at.elapsed() >= Duration::from_secs(30)
        {
            self.image_in.remove(&key);
            return self
                .send(
                    session,
                    from,
                    &Packet::FileCancel {
                        id: transfer_id(&id),
                    },
                )
                .await;
        }
        image.bytes.extend_from_slice(data);
        let ack = Packet::FileAck {
            id: transfer_id(&id),
            offset: image.bytes.len() as u64,
        };
        if image.bytes.len() as u64 == image.offer.size {
            let image = self.image_in.remove(&key).expect("present above");
            if let Some(content) = ClipboardImage::new(image.offer.mime, image.bytes)
                && hex(&content.hash()) == image.offer.hash
            {
                self.notify(Notice::ClipboardImage {
                    from,
                    image: content,
                });
                self.record(Direction::Received);
                self.send(session, from, &ack).await?;
                return self
                    .send(
                        session,
                        from,
                        &Packet::FileDone {
                            id: transfer_id(&id),
                        },
                    )
                    .await;
            }
            return self
                .send(
                    session,
                    from,
                    &Packet::FileCancel {
                        id: transfer_id(&id),
                    },
                )
                .await;
        }
        self.send(session, from, &ack).await
    }

    /// Publishes the view if it differs from the last snapshot.
    fn publish(&self) {
        let mut board = self.shared.board.lock().unwrap();
        let paired =
            matches!(self.view.pairing, PairingState::Success { .. }).then_some(self.pairing_kind);
        if *board.published.snapshot != self.view || board.published.paired != paired {
            board.published = Published {
                generation: board.published.generation + 1,
                snapshot: Arc::new(self.view.clone()),
                paired,
            };
            self.shared.changed.notify_all();
        }
    }

    fn set_status(&mut self, status: Status) {
        self.view.status = status;
        self.publish();
    }

    fn notify(&self, notice: Notice) {
        (self.hooks.on_notice)(notice);
    }

    fn device_name(&self, device: &[u8; 16]) -> Option<String> {
        self.view
            .devices
            .iter()
            .find(|known| known.id == *device)
            .and_then(|known| known.name.clone())
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

            self.lan = None;
            self.image_in.clear();
            self.image_out.clear();
            // What was saved before the connection broke still counts.
            if let Ok(state) = self.dir.load() {
                describe(&mut self.view, &state);
            }
            if self.pairing.take().is_some() || self.view.pairing == PairingState::Joining {
                self.view.pairing = PairingState::Failed(PairingFailure::ConnectionLost);
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
    /// that do not need one and the clocks. Returns `None` on shutdown.
    async fn offline_until<T>(&mut self, wait: impl Future<Output = T>) -> Option<T> {
        tokio::pin!(wait);
        loop {
            tokio::select! {
                output = &mut wait => return Some(output),
                command = self.commands.recv() => match command {
                    None | Some(Command::Shutdown) => return None,
                    Some(command) => self.offline_command(command),
                },
                () = tokio::time::sleep(TICK), if self.is_busy() => {
                    self.clocks(Instant::now());
                    self.publish();
                }
            }
        }
    }

    fn offline_command(&mut self, command: Command) {
        const OFFLINE: &str = "not connected to the relay";
        match command {
            // Not kept for later: after a reconnection only new changes go.
            Command::SendText(_) | Command::SendImage(_) => {
                tracing::debug!("clipboard not sent: {OFFLINE}")
            }
            Command::StartPairing | Command::Join(_) => {
                self.view.pairing = PairingState::Failed(PairingFailure::NotConnected);
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
            // Announced at the next connection.
            Command::SetFeatures(features) => self.set_features(features),
            Command::Ring(device) | Command::StopRinging(device) => {
                tracing::info!(device = %short_id(&device), "ring not sent: {OFFLINE}");
            }
            Command::FoundIt => self.stop_ring(),
            // Offered at the next connection.
            Command::SendFiles(device, paths) => self.queue_files(device, paths),
            Command::CancelTransfer(id) => {
                self.cancel_locally(&id);
            }
            Command::SendPacket(device, _) => {
                tracing::info!(device = %short_id(&device), "packet not sent: {OFFLINE}");
            }
            // Kept, and sent with the state at the next connection.
            Command::LocalMedia(players) => {
                let players = self.keep_covers(players);
                self.local_media = Some((players, Instant::now()));
            }
            Command::LocalVolume(volume) => self.local_volume = Some(volume),
            Command::OpenMessages(device) | Command::LoadMessages { device, .. } => {
                tracing::info!(device = %short_id(&device), "messages not asked for: {OFFLINE}");
            }
            // Shown as not sent: it is not kept for later.
            Command::SendSms {
                device,
                addresses,
                text,
                thread,
            } => {
                let sent = self.outgoing_sms(addresses, text, thread);
                let sms = self.sms_mut(device);
                sms.outbox.push(OutgoingSms {
                    state: SendState::Failed(None),
                    ..sent
                });
                sms.trim_outbox();
            }
            // Saved now; the devices learn the commands at the next
            // connection.
            Command::SetControl(device, allowed) => self.set_control(device, allowed),
            Command::SetCommands(commands) => {
                self.view.commands = commands;
                self.save_settings();
            }
            Command::Shutdown => {}
        }
        self.show_transfers();
        self.publish();
    }

    /// Whether a clock runs: a ring, or a transfer under way.
    fn is_busy(&self) -> bool {
        self.ring_until.is_some()
            || self.outgoing.iter().any(|s| s.transfer.stage.is_active())
            || self.incoming.iter().any(|r| r.transfer.stage.is_active())
    }

    /// Whether notifications asked for again may still be swept.
    fn sweeping(&self) -> bool {
        self.requested
            .values()
            .any(|request| !request.stale.is_empty())
    }

    fn forget_notification(&mut self, from: &[u8; 16], id: &str) {
        if let Some(shown) = self.shown.get_mut(from) {
            shown.remove(id);
        }
        if let Some(request) = self.requested.get_mut(from) {
            request.stale.remove(id);
        }
    }

    /// Removes the notifications a device did not send again within
    /// [`REQUEST_QUIET`] of being asked for them all: the phone no longer
    /// has them. Only online, where the answer could come.
    fn sweep_notifications(&mut self, now: Instant) {
        let mut gone = Vec::new();
        for (from, request) in &mut self.requested {
            if now.duration_since(request.at) >= REQUEST_QUIET {
                gone.extend(
                    std::mem::take(&mut request.stale)
                        .into_iter()
                        .map(|id| (*from, id)),
                );
            }
        }
        for (from, id) in gone {
            if let Some(shown) = self.shown.get_mut(&from) {
                shown.remove(&id);
            }
            tracing::debug!(from = %short_id(&from), "stale notification removed");
            self.notify(Notice::NotificationRemoved { from, id });
        }
    }

    /// Serves the connection until shutdown (`Ok`) or until it is lost.
    async fn online(&mut self, session: &mut Session) -> Result<()> {
        self.image_out.clear();
        self.image_in.clear();
        self.refresh_lan(session).await;
        // Tell every device what this one does, ask for their state, and
        // pick up the transfers the connection loss interrupted.
        let devices: Vec<[u8; 16]> = self.view.devices.iter().map(|device| device.id).collect();
        for device in devices {
            self.send_capabilities(session, device).await?;
            self.send(session, device, &Packet::StateRequest {}).await?;
            // A device not heard from yet is asked once its capabilities
            // say it has notifications.
            if self.view.peer(&device).sends("notification") {
                self.request_notifications(session, device).await?;
            }
            self.offer_again(session, device).await?;
        }
        self.start_queued(session).await?;
        self.traffic_at = Instant::now();
        self.show_transfers();
        self.publish();
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
                self.online_clocks(session).await?;
                continue;
            }
            let keepalive =
                (self.traffic_at + KEEPALIVE_INTERVAL).saturating_duration_since(Instant::now());
            let wait = if self.is_busy() || self.sweeping() {
                keepalive.min(TICK)
            } else {
                keepalive
            };
            let input = async {
                let maintain_lan = async {
                    if let Some(lan) = &mut self.lan {
                        lan.maintain().await;
                    }
                    std::future::pending::<WorkerInput>().await
                };
                tokio::select! {
                    command = self.commands.recv() => WorkerInput::Command(command),
                    never = maintain_lan => never,
                }
            };
            let lan = async {
                WorkerInput::Lan(self.lan_frames.1.recv().await.expect("LAN frame sender"))
            };
            let next = session.next_event_with_lan(Some(wait), input, lan).await?;
            match next {
                Next::Event(event) => {
                    self.traffic_at = Instant::now();
                    self.event(session, event).await?;
                }
                Next::Idle => {}
                Next::Interrupted(WorkerInput::Command(None | Some(Command::Shutdown))) => {
                    return Ok(());
                }
                Next::Interrupted(WorkerInput::Command(Some(command))) => {
                    self.command(session, command).await?
                }
                Next::Interrupted(WorkerInput::Lan(frame)) => {
                    let event = session.receive_frame(&frame).await?;
                    self.event(session, event).await?;
                }
            }
            self.online_clocks(session).await?;
        }
    }

    /// Runs the clocks, sends what they ask for, and checks the connection
    /// once it has been quiet for a while.
    async fn online_clocks(&mut self, session: &mut Session) -> Result<()> {
        let now = Instant::now();
        self.image_out
            .retain(|_, (_, at)| now.duration_since(*at) < Duration::from_secs(30));
        self.image_in
            .retain(|_, image| now.duration_since(image.at) < Duration::from_secs(30));
        for (to, packet) in self.clocks(now) {
            self.send(session, to, &packet).await?;
        }
        self.sweep_notifications(now);
        self.start_queued(session).await?;
        if now.duration_since(self.traffic_at) >= KEEPALIVE_INTERVAL {
            session.settle().await?;
            self.traffic_at = Instant::now();
        }
        self.show_transfers();
        self.publish();
        Ok(())
    }

    /// Ends a ring that lasted long enough, offers again what stalled and
    /// gives up on what stalled too long. Returns the packets to send.
    fn clocks(&mut self, now: Instant) -> Vec<([u8; 16], Packet)> {
        if self.ring_until.is_some_and(|until| now >= until) {
            self.stop_ring();
        }
        let mut packets = Vec::new();
        let mut failed = Vec::new();
        for sending in &mut self.outgoing {
            let transfer = &mut sending.transfer;
            match transfer.tick(now) {
                Tick::Nothing => {}
                Tick::Offer => {
                    tracing::debug!(id = %short_id(&transfer.id), "no progress; offering again");
                    packets.push((transfer.to, transfer.start(now)));
                }
                Tick::GiveUp => {
                    tracing::warn!(id = %short_id(&transfer.id), "no progress; giving up");
                    sending.file = None;
                    let id = transfer_id(&transfer.id);
                    packets.push((transfer.to, Packet::FileCancel { id }));
                    failed.push((transfer.to, transfer.name.clone()));
                }
            }
        }
        for (to, name) in failed {
            self.notify(Notice::TransferFailed {
                device: self.device_name(&to),
                name,
                outgoing: true,
                failure: Failure::NoProgress,
            });
        }
        for receiving in &mut self.incoming {
            if receiving.transfer.tick(now) {
                // The partial file stays for a resume.
                receiving.file = None;
            }
        }
        packets
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
            self.view.pairing = PairingState::Failed(PairingFailure::Expired);
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
            Ok(outcome) => {
                match &outcome {
                    PairOutcome::Joined { device } => {
                        tracing::info!(device = %short_id(device), "device added");
                        self.pairing_kind = PairingKind::Joined;
                    }
                    PairOutcome::Invited { inviter, devices } => {
                        tracing::info!(
                            inviter = %short_id(inviter),
                            "invited into a group: {} trusted device(s)",
                            devices.len()
                        );
                        self.pairing_kind = PairingKind::Invited;
                    }
                }
                self.pairing = None;
                self.view.pairing = PairingState::Success {
                    device: outcome.device(),
                };
                describe(&mut self.view, session.state());
                // Capabilities go with the names the session sent.
                for device in outcome.new_devices() {
                    self.send_capabilities(session, device).await?;
                }
            }
            // Nobody yet.
            Err(Error::Timeout(_)) => {}
            Err(err) if is_refusal(&err) => {
                tracing::warn!(error = %err, "pairing failed");
                self.pairing = None;
                self.view.pairing = PairingState::Failed(PairingFailure::of(&err));
            }
            Err(err) => return Err(err),
        }
        loop {
            let event = if let Ok(frame) = self.lan_frames.1.try_recv() {
                session.receive_frame(&frame).await?
            } else if let Some(event) = session.next_queued_event().await? {
                event
            } else {
                break;
            };
            self.event(session, event).await?;
        }
        describe(&mut self.view, session.state());
        self.publish();
        Ok(())
    }

    async fn event(&mut self, session: &mut Session, event: Event) -> Result<()> {
        if matches!(&event, Event::Welcome { .. } | Event::DeviceAdded { .. }) {
            // Refresh listener trust after membership changes; old streams
            // cannot keep a snapshot of a superseded trusted-device list.
            self.refresh_lan(session).await;
            if let Some(lan) = &self.lan {
                let (port, addrs) = lan.endpoints();
                session
                    .send_packet_all(&Packet::LanEndpoints { port, addrs }.to_json())
                    .await?;
            }
        }
        match event {
            Event::Clipboard { from, text } => {
                if self.view.pause.blocks_receive() {
                    tracing::debug!(from = %short_id(&from), "clipboard text ignored: paused");
                } else {
                    tracing::debug!(from = %short_id(&from), "clipboard text received (not shown)");
                    (self.hooks.on_text)(text);
                    self.record(Direction::Received);
                }
            }
            Event::DeviceName { from, name } => {
                tracing::info!(device = %short_id(&from), %name, "device name received");
                describe(&mut self.view, session.state());
            }
            Event::Ping { from } => tracing::debug!(from = %short_id(&from), "ping"),
            Event::Packet { from, json } => {
                describe(&mut self.view, session.state());
                self.packet(session, from, &json).await?;
            }
            Event::FileChunk {
                from,
                transfer_id,
                offset,
                data,
            } => {
                if self.image_in.contains_key(&(from, transfer_id)) {
                    self.image_chunk(session, from, transfer_id, offset, &data)
                        .await?;
                } else {
                    self.chunk(session, from, transfer_id, offset, &data)
                        .await?
                }
            }
            Event::Welcome { from, added } => {
                tracing::info!(from = %short_id(&from), added = added.len(), "welcome received");
                describe(&mut self.view, session.state());
                for device in std::iter::once(from).chain(added) {
                    self.send_capabilities(session, device).await?;
                }
            }
            Event::DeviceAdded { by, device } => {
                tracing::info!(by = %short_id(&by), device = %short_id(&device), "device added");
                describe(&mut self.view, session.state());
                self.send_capabilities(session, device).await?;
            }
            Event::DeviceRemoved { by, device } => {
                tracing::info!(by = %short_id(&by), device = %short_id(&device), "device removed");
                self.drop_transfers_with(&device);
                self.lan = None;
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
        self.show_transfers();
        self.publish();
        Ok(())
    }

    fn record(&mut self, direction: Direction) {
        self.view.last_activity = Some(Activity {
            direction,
            at: SystemTime::now(),
        });
    }

    /// Sends `packet` to `to`. Only a lost connection is an error.
    async fn send(&mut self, session: &mut Session, to: [u8; 16], packet: &Packet) -> Result<()> {
        let frame =
            session.seal_for(to, weft_protocol::Kind::Packet, packet.to_json().as_bytes())?;
        let done = self.send_wire(session, to, frame).await;
        only_connection_errors(done, "packet")
    }

    async fn send_wire(
        &mut self,
        session: &mut Session,
        to: [u8; 16],
        frame: Vec<u8>,
    ) -> Result<()> {
        if let Some(lan) = &mut self.lan
            && lan.send(to, &frame).await
        {
            return Ok(());
        }
        session.send_frame(frame).await
    }

    async fn refresh_lan(&mut self, session: &Session) {
        // Accepted frames stay queued when routes are replaced.
        self.lan = crate::lan::Transport::start(
            session.identity_arc(),
            session.state().devices.clone(),
            self.lan_frames.0.clone(),
        )
        .await
        .ok();
    }

    async fn send_chunk(
        &mut self,
        session: &mut Session,
        to: [u8; 16],
        id: [u8; 16],
        offset: u64,
        data: &[u8],
    ) -> Result<()> {
        let plaintext = weft_protocol::encode_file_chunk(&id, offset, data)?;
        let frame = session.seal_for(to, weft_protocol::Kind::FileChunk, &plaintext)?;
        self.send_wire(session, to, frame).await
    }

    /// What this device announces to `to`: input only if `to` may control
    /// it.
    fn capabilities_for(&self, to: &[u8; 16]) -> Capabilities {
        self.announced(self.view.control.contains(to))
    }

    /// What the switches give, less what the app does not implement.
    fn announced(&self, control: bool) -> Capabilities {
        let capabilities = self.view.features.capabilities(control);
        match &self.hooks.implemented {
            Some(implemented) => implemented.restrict(capabilities),
            None => capabilities,
        }
    }

    /// Sends this device's capabilities, then its current state: what its
    /// players and volume are, when it knows them and the feature is on.
    /// The device may have restarted: it gets the covers again.
    async fn send_capabilities(&mut self, session: &mut Session, to: [u8; 16]) -> Result<()> {
        // The name first: a device offline when it changed still learns it.
        session.send_name_to(to).await?;
        let packet = Packet::Capabilities(self.capabilities_for(&to));
        self.send(session, to, &packet).await?;
        if let Some(lan) = &self.lan {
            let (port, addrs) = lan.endpoints();
            // Endpoints and capabilities use the relay so a stale LAN route
            // can always be replaced after a peer reconnects.
            session
                .send_packet(to, &Packet::LanEndpoints { port, addrs }.to_json())
                .await?;
        }
        self.art_sent.remove(&to);
        self.send_state(session, to).await
    }

    /// Keeps the covers of this computer's players, and names each in its
    /// player's `art`. A cover the contract does not allow is dropped, its
    /// player kept.
    fn keep_covers(&mut self, players: Vec<LocalPlayer>) -> Vec<Player> {
        self.local_covers.clear();
        players
            .into_iter()
            .map(|LocalPlayer { mut player, cover }| {
                player.art = None;
                match cover {
                    Some(cover) if cover.is_valid() => {
                        let id = cover.id();
                        player.art = Some(id.clone());
                        self.local_covers.insert(id, cover);
                    }
                    Some(cover) => {
                        tracing::debug!(
                            ?cover,
                            "cover not sent: over the limit or of another type"
                        );
                    }
                    None => {}
                }
                player
            })
            .collect()
    }

    /// The `media` packet of `players` for `to`, after the `media.art`
    /// packets of the covers it names that `to` did not get yet on this
    /// connection, now marked as sent. A device that does not show covers,
    /// or an app that does not send them, gets the players without `art`.
    fn media_packets(&mut self, to: [u8; 16], mut players: Vec<Player>) -> Vec<Packet> {
        let mut packets = Vec::new();
        if self.view.peer(&to).handles("media.art") && self.announced(false).sends("media.art") {
            let sent = self.art_sent.entry(to).or_default();
            for player in &players {
                if let Some(id) = &player.art
                    && let Some(cover) = self.local_covers.get(id)
                    && sent.insert(id.clone())
                {
                    packets.push(Packet::MediaArt(packets::MediaArt::new(id, cover)));
                }
            }
        } else {
            for player in &mut players {
                player.art = None;
            }
        }
        packets.push(Packet::Media { players });
        packets
    }

    /// This computer's players, positions brought up to now.
    fn local_players_now(&self) -> Option<Vec<Player>> {
        let (players, at) = self.local_media.as_ref()?;
        let elapsed = at.elapsed();
        Some(
            players
                .iter()
                .map(|player| Player {
                    position: media::position(player, elapsed),
                    ..player.clone()
                })
                .collect(),
        )
    }

    /// The current `media` and `volume` packets, positions brought up to
    /// now, the covers `to` needs first. A device that told its
    /// capabilities gets only what it handles.
    fn state_packets(&mut self, to: &[u8; 16]) -> Vec<Packet> {
        let peer = self.view.peer(to);
        let wants = |kind| peer.capabilities.is_none() || peer.handles(kind);
        let features = self.view.features;
        let mut packets = Vec::new();
        if features.media
            && wants("media")
            && let Some(players) = self.local_players_now()
        {
            packets.extend(self.media_packets(*to, players));
        }
        if features.volume
            && wants("volume")
            && let Some(volume) = self.local_volume
        {
            packets.push(Packet::Volume(volume));
        }
        if features.commands && wants_commands(&peer) {
            packets.push(self.command_list());
        }
        packets
    }

    fn command_list(&self) -> Packet {
        Packet::CommandList {
            commands: self.view.commands.iter().map(UserCommand::item).collect(),
        }
    }

    async fn send_state(&mut self, session: &mut Session, to: [u8; 16]) -> Result<()> {
        for packet in self.state_packets(&to) {
            self.send(session, to, &packet).await?;
        }
        Ok(())
    }

    /// Asks `to` for its current notifications, if they are shown here.
    async fn request_notifications(&mut self, session: &mut Session, to: [u8; 16]) -> Result<()> {
        if !self.view.features.notifications {
            return Ok(());
        }
        let stale = self.shown.get(&to).cloned().unwrap_or_default();
        let request = NotificationRequest {
            at: Instant::now(),
            stale,
        };
        self.requested.insert(to, request);
        self.send(session, to, &Packet::NotificationRequest {})
            .await
    }

    /// Sends this computer's players to every device that shows them, each
    /// after the covers it needs.
    async fn send_media(&mut self, session: &mut Session, players: Vec<Player>) -> Result<()> {
        let devices: Vec<[u8; 16]> = self
            .view
            .devices
            .iter()
            .map(|device| device.id)
            .filter(|device| self.view.peer(device).handles("media"))
            .collect();
        for device in devices {
            for packet in self.media_packets(device, players.clone()) {
                self.send(session, device, &packet).await?;
            }
        }
        Ok(())
    }

    /// Sends `packet` to every device that handles it.
    async fn send_to_handlers(&mut self, session: &mut Session, packet: &Packet) -> Result<()> {
        let kind = packet.kind();
        let devices: Vec<[u8; 16]> = self
            .view
            .devices
            .iter()
            .map(|device| device.id)
            .filter(|device| self.view.peer(device).handles(kind))
            .collect();
        for device in devices {
            self.send(session, device, packet).await?;
        }
        Ok(())
    }

    /// Acts on a packet from a trusted device. Packets of a switched-off
    /// feature are ignored, except file offers, which get a reason.
    async fn packet(&mut self, session: &mut Session, from: [u8; 16], json: &str) -> Result<()> {
        let packet = match packets::parse(json) {
            Parsed::Known(packet) => packet,
            Parsed::Unknown(kind) => {
                tracing::debug!(from = %short_id(&from), %kind, "packet of an unknown type ignored");
                return Ok(());
            }
            Parsed::Invalid => {
                tracing::warn!(from = %short_id(&from), "dropped an invalid packet");
                return Ok(());
            }
        };
        let features = self.view.features;
        let now = Instant::now();
        match packet {
            Packet::LanEndpoints { port, addrs } => {
                if let Some(lan) = &mut self.lan {
                    lan.connect(from, port, &addrs);
                }
            }
            Packet::ClipboardImageOffer(offer) => {
                self.image_offer(session, from, offer).await?;
            }
            Packet::ClipboardImageAccept { id } => {
                if let Some(id) = parse_transfer_id(&id)
                    && let Some((image, at)) = self.image_out.remove(&(from, id))
                    && !self.view.pause.blocks_send()
                    && now.duration_since(at) < Duration::from_secs(30)
                {
                    for (index, data) in image
                        .bytes
                        .chunks(weft_protocol::MAX_FILE_CHUNK_DATA_LEN)
                        .enumerate()
                    {
                        self.send_chunk(
                            session,
                            from,
                            id,
                            (index * weft_protocol::MAX_FILE_CHUNK_DATA_LEN) as u64,
                            data,
                        )
                        .await?;
                    }
                    self.record(Direction::Sent);
                }
            }
            Packet::Capabilities(capabilities) => {
                tracing::info!(
                    from = %short_id(&from),
                    platform = %capabilities.platform,
                    "capabilities received"
                );
                let peer = self.view.peers.entry(from).or_default();
                let fresh = peer.capabilities.is_none();
                let notifies = capabilities.sends("notification");
                let shows_covers =
                    |peer: &PeerInfo| peer.handles("media") && peer.handles("media.art");
                let covers_before = shows_covers(peer);
                peer.capabilities = Some(capabilities);
                let covers_now = shows_covers(peer);
                // It may have restarted: it needs this device's too.
                if fresh {
                    self.send_capabilities(session, from).await?;
                    if notifies {
                        self.request_notifications(session, from).await?;
                    }
                    self.offer_again(session, from).await?;
                } else if covers_now && !covers_before {
                    // Its media switch just came on: it forgot the covers.
                    self.art_sent.remove(&from);
                    if features.media
                        && let Some(players) = self.local_players_now()
                    {
                        for packet in self.media_packets(from, players) {
                            self.send(session, from, &packet).await?;
                        }
                    }
                }
            }
            Packet::StateRequest {} => {
                self.send_capabilities(session, from).await?;
                // It just came back: what it was receiving is waiting.
                self.offer_again(session, from).await?;
            }
            Packet::Battery(battery) if features.battery => {
                let battery = Battery {
                    level: battery.level.filter(|level| *level <= 100),
                    ..battery
                };
                self.view.peers.entry(from).or_default().battery = Some(battery);
            }
            Packet::Connectivity(connectivity) if features.battery => {
                let connectivity = Connectivity {
                    signal: connectivity.signal.filter(|signal| *signal <= 4),
                    ..connectivity
                };
                self.view.peers.entry(from).or_default().connectivity = Some(connectivity);
            }
            Packet::Ring {} if features.ring => {
                tracing::info!(from = %short_id(&from), "ringing");
                self.ring_until = Some(now + RING_DURATION);
                self.view.ringing = Some(from);
                self.notify(Notice::Ring {
                    from,
                    device: self.device_name(&from),
                });
            }
            Packet::RingStop {} if features.ring => {
                if self.view.ringing == Some(from) {
                    self.stop_ring();
                }
            }
            Packet::Ping { message } => self.notify(Notice::Ping {
                from,
                device: self.device_name(&from),
                message,
            }),
            Packet::ShareUrl { url } if features.links => {
                if is_shareable_url(&url) {
                    tracing::info!(from = %short_id(&from), "link received (not shown)");
                    self.notify(Notice::Link {
                        from,
                        device: self.device_name(&from),
                        url,
                    });
                } else {
                    tracing::warn!(from = %short_id(&from), "dropped a link that is not a web link");
                }
            }
            Packet::FileOffer(offer) => self.offered(session, from, offer).await?,
            Packet::FileAccept { id, offset } => {
                if let Some(index) = self.sending(from, &id) {
                    if !self.outgoing[index].transfer.accept(offset, now) {
                        tracing::warn!(from = %short_id(&from), "ignored an acceptance that does not fit");
                    }
                    self.pump(session, index).await?;
                }
            }
            Packet::FileAck { id, offset } => {
                if let Some(index) = self.sending(from, &id) {
                    self.outgoing[index].transfer.ack(offset, now);
                    self.pump(session, index).await?;
                }
            }
            Packet::FileDone { id } => {
                if let Some(index) = self.sending(from, &id) {
                    let sending = &mut self.outgoing[index];
                    sending.transfer.done();
                    sending.file = None;
                    tracing::info!(to = %short_id(&from), "file sent");
                }
            }
            Packet::FileReject { id, reason } => {
                if let Some(index) = self.sending(from, &id) {
                    let reason = reason.as_deref().and_then(RejectReason::parse);
                    let failure = Failure::Rejected(reason);
                    let sending = &mut self.outgoing[index];
                    sending.transfer.end(Stage::Failed(failure));
                    sending.file = None;
                    let name = sending.transfer.name.clone();
                    tracing::info!(to = %short_id(&from), ?reason, "file refused");
                    self.notify(Notice::TransferFailed {
                        device: self.device_name(&from),
                        name,
                        outgoing: true,
                        failure,
                    });
                }
            }
            Packet::FileCancel { id } => {
                if let Some(id) = parse_transfer_id(&id) {
                    tracing::info!(from = %short_id(&from), "transfer cancelled by the other device");
                    self.cancel_from(&from, &id);
                }
            }
            Packet::Notification(mut notification) if features.notifications => {
                tracing::debug!(from = %short_id(&from), "notification received (not shown)");
                if let Some((cut, _)) = notification.text.char_indices().nth(MAX_NOTIFICATION_TEXT)
                {
                    notification.text.truncate(cut);
                }
                if let Some(request) = self.requested.get_mut(&from)
                    && now.duration_since(request.at) < REQUEST_QUIET
                {
                    notification.silent = true;
                    request.stale.remove(&notification.id);
                }
                self.shown
                    .entry(from)
                    .or_default()
                    .insert(notification.id.clone());
                self.notify(Notice::Notification {
                    from,
                    device: self.device_name(&from),
                    notification,
                });
            }
            Packet::NotificationRemoved { id } if features.notifications => {
                self.forget_notification(&from, &id);
                self.notify(Notice::NotificationRemoved { from, id });
            }
            Packet::Call(call) if features.calls => {
                if call.event().is_some() {
                    tracing::info!(from = %short_id(&from), event = %call.event, "call");
                    self.notify(Notice::Call {
                        from,
                        device: self.device_name(&from),
                        call,
                    });
                } else {
                    tracing::debug!(from = %short_id(&from), "call event of an unknown kind ignored");
                }
            }
            Packet::Media { mut players } if features.media => {
                players.truncate(MAX_PLAYERS);
                for player in &mut players {
                    player.volume = player.volume.filter(|volume| *volume <= 100);
                }
                let peer = self.view.peers.entry(from).or_default();
                peer.media = Some(PeerMedia { players, at: now });
            }
            Packet::MediaArt(art)
                if features.media && self.announced(false).handles("media.art") =>
            {
                tracing::debug!(from = %short_id(&from), ?art, "cover received");
                let peer = self.view.peers.entry(from).or_default();
                peer.covers.insert(art.art.clone(), art.cover());
            }
            Packet::Volume(volume) if features.volume => {
                let volume = Volume {
                    level: volume.level.filter(|level| *level <= 100),
                    ..volume
                };
                self.view.peers.entry(from).or_default().volume = Some(volume);
            }
            Packet::MediaCommand(command) if features.media => match command.action() {
                Some(action) => self.notify(Notice::MediaCommand {
                    from,
                    player: command.player,
                    action,
                }),
                None => {
                    tracing::debug!(from = %short_id(&from), "media command of an unknown kind ignored");
                }
            },
            Packet::VolumeSet(set) if features.volume => {
                let set = VolumeSet {
                    level: set.level.filter(|level| *level <= 100),
                    ..set
                };
                if set.level.is_some() || set.muted.is_some() {
                    self.notify(Notice::SetVolume { from, set });
                }
            }
            Packet::SmsConversations { mut conversations } if features.sms => {
                tracing::debug!(from = %short_id(&from), "conversations received (not shown)");
                conversations.truncate(sms::MAX_CONVERSATIONS);
                self.sms_mut(from).conversations = Some(conversations);
            }
            Packet::SmsMessages { thread, messages } if features.sms => {
                let empty = messages.is_empty();
                let sms = self.sms_mut(from);
                let added = sms.add_messages(thread, messages, true);
                let asked_older = self.older_asked.remove(&(from, thread));
                // The phone may shorten a page to stay under 256 KiB, so a
                // short page says nothing: the start of the thread is an
                // older page with nothing new in it, or an empty thread.
                if empty || (asked_older && added == 0) {
                    self.sms_mut(from).complete.insert(thread);
                }
            }
            Packet::SmsReceived(received) if features.sms => {
                self.sms_received(from, received.thread, received.message);
            }
            Packet::SmsSent(sent) if features.sms => {
                let sms = self.sms_mut(from);
                let Some(outgoing) = sms
                    .outbox
                    .iter_mut()
                    .find(|outgoing| outgoing.request_id == sent.request_id)
                else {
                    return Ok(());
                };
                tracing::info!(from = %short_id(&from), ok = sent.ok, "SMS sent");
                outgoing.state = if sent.ok {
                    SendState::Sent
                } else {
                    SendState::Failed(sent.error)
                };
                let thread = outgoing.thread;
                if sent.ok {
                    // The thread and its snippet changed on the phone.
                    if let Some(thread) = thread {
                        self.request_messages(session, from, thread, None).await?;
                    }
                    self.send(session, from, &Packet::SmsConversationsRequest {})
                        .await?;
                }
            }
            Packet::Contacts(part) if features.sms => {
                let sms = self.sms_mut(from);
                if part.part.unwrap_or(1) <= 1 {
                    sms.contacts.clear();
                }
                sms.contacts.extend(part.contacts);
                sms.contacts.truncate(sms::MAX_CONTACTS);
            }
            ref asked @ (Packet::InputPointer(_)
            | Packet::InputClick(_)
            | Packet::InputKey(_)
            | Packet::Presenter(_)) => {
                if self.view.control.contains(&from) {
                    for input in input::of_packet(asked) {
                        (self.hooks.on_input)(input);
                    }
                } else {
                    tracing::debug!(from = %short_id(&from), "input ignored: the device may not control this computer");
                }
            }
            Packet::CommandRun { id } if features.commands => {
                let command = self.view.commands.iter().find(|command| command.id == id);
                match command.cloned() {
                    Some(command) => {
                        tracing::info!(from = %short_id(&from), %id, "running a command");
                        (self.hooks.run_command)(&command.line);
                        self.notify(Notice::CommandRan {
                            from,
                            device: self.device_name(&from),
                            name: command.name,
                        });
                    }
                    None => {
                        tracing::warn!(from = %short_id(&from), "asked to run a command that is not listed")
                    }
                }
            }
            // A switched-off feature.
            _ => tracing::debug!(from = %short_id(&from), "packet of a disabled feature ignored"),
        }
        Ok(())
    }

    /// The SMS of `device`, to change: copied first if a snapshot shares
    /// them.
    fn sms_mut(&mut self, device: [u8; 16]) -> &mut Sms {
        Arc::make_mut(&mut self.view.peers.entry(device).or_default().sms)
    }

    /// A new SMS on the phone: into its thread if loaded, to the top of
    /// the conversations, and a notice when it came in.
    fn sms_received(&mut self, from: [u8; 16], thread: i64, message: packets::SmsMessage) {
        tracing::debug!(from = %short_id(&from), "SMS received (not shown)");
        let device = self.device_name(&from);
        let sms = self.sms_mut(from);
        sms.add_messages(thread, vec![message.clone()], false);
        let address = message.address.clone().unwrap_or_default();
        let conversations = sms.conversations.get_or_insert_with(Vec::new);
        let mut conversation = match conversations.iter().position(|c| c.thread == thread) {
            Some(index) => conversations.remove(index),
            None => packets::Conversation {
                thread,
                addresses: message.address.iter().cloned().collect(),
                names: vec![sms::contact_name(&sms.contacts, &address).map(str::to_owned)],
                ..packets::Conversation::default()
            },
        };
        conversation.snippet = Some(message.body.clone());
        conversation.time = conversation.time.max(message.time);
        conversation.unread = !message.outgoing;
        let addresses = conversation.addresses.clone();
        conversations.insert(0, conversation);
        conversations.truncate(sms::MAX_CONVERSATIONS);
        if message.outgoing {
            return;
        }
        let sender = sms
            .name_of(thread, &address)
            .or(message.address)
            .unwrap_or_else(|| "?".into());
        self.notify(Notice::SmsReceived {
            from,
            device,
            thread,
            sender,
            text: message.body,
            addresses,
        });
    }

    /// A message about to be sent, pending, with a fresh request id.
    fn outgoing_sms(
        &self,
        addresses: Vec<String>,
        text: String,
        thread: Option<i64>,
    ) -> OutgoingSms {
        OutgoingSms {
            request_id: hex(&random_message_id()),
            addresses,
            text,
            thread,
            state: SendState::Pending,
            at: epoch_ms(),
        }
    }

    async fn request_messages(
        &mut self,
        session: &mut Session,
        to: [u8; 16],
        thread: i64,
        before: Option<i64>,
    ) -> Result<()> {
        if before.is_some() {
            self.older_asked.insert((to, thread));
        }
        let request = SmsMessagesRequest {
            thread,
            before,
            limit: Some(sms::PAGE),
        };
        self.send(session, to, &Packet::SmsMessagesRequest(request))
            .await
    }

    fn set_control(&mut self, device: [u8; 16], allowed: bool) {
        if allowed {
            self.view.control.insert(device);
        } else if self.view.control.remove(&device) {
            // Its spot goes with it.
            (self.hooks.on_input)(Input::Spot(None));
        }
        self.save_settings();
    }

    fn save_settings(&self) {
        let settings = Settings {
            features: self.view.features,
            control: self.view.control.iter().map(|id| hex(id)).collect(),
            commands: self.view.commands.clone(),
        };
        if let Err(err) = settings.save(self.dir.path()) {
            tracing::warn!(error = %err, "could not save the feature settings");
        }
    }

    /// Sends the commands to every device that shows them.
    async fn send_command_list(&mut self, session: &mut Session) -> Result<()> {
        if !self.view.features.commands {
            return Ok(());
        }
        let packet = self.command_list();
        let devices: Vec<[u8; 16]> = self
            .view
            .devices
            .iter()
            .map(|device| device.id)
            .filter(|device| wants_commands(&self.view.peer(device)))
            .collect();
        for device in devices {
            self.send(session, device, &packet).await?;
        }
        Ok(())
    }

    fn stop_ring(&mut self) {
        if self.ring_until.take().is_some() || self.view.ringing.is_some() {
            self.view.ringing = None;
            self.notify(Notice::RingEnded);
        }
    }

    /// The outgoing transfer `id` (as hex) to `to`.
    fn sending(&self, to: [u8; 16], id: &str) -> Option<usize> {
        let id = parse_transfer_id(id)?;
        self.outgoing
            .iter()
            .position(|sending| sending.transfer.id == id && sending.transfer.to == to)
    }

    fn queue_files(&mut self, to: [u8; 16], paths: Vec<PathBuf>) {
        let now = Instant::now();
        for path in paths {
            let size = match std::fs::metadata(&path) {
                Ok(metadata) if metadata.is_file() => metadata.len(),
                _ => {
                    tracing::warn!("not a file; not sent");
                    continue;
                }
            };
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".into());
            let transfer = Outgoing::new(random_message_id(), to, name, size, now);
            if let Stage::Failed(failure) = transfer.stage {
                self.notify(Notice::TransferFailed {
                    device: self.device_name(&to),
                    name: transfer.name.clone(),
                    outgoing: true,
                    failure,
                });
            }
            self.outgoing.push(Sending {
                transfer,
                path,
                file: None,
            });
        }
    }

    /// Offers the queued files while each device has a free slot.
    async fn start_queued(&mut self, session: &mut Session) -> Result<()> {
        if !self.view.features.files {
            return Ok(());
        }
        let now = Instant::now();
        for index in 0..self.outgoing.len() {
            let transfer = &self.outgoing[index].transfer;
            if !transfer.queued || !transfer.stage.is_active() {
                continue;
            }
            let to = transfer.to;
            let running = self
                .outgoing
                .iter()
                .filter(|s| {
                    s.transfer.to == to && !s.transfer.queued && s.transfer.stage.is_active()
                })
                .count();
            if running >= transfer::MAX_ACTIVE {
                continue;
            }
            let sending = &mut self.outgoing[index];
            match File::open(&sending.path) {
                Ok(file) => {
                    sending.file = Some(file);
                    let offer = sending.transfer.start(now);
                    self.send(session, to, &offer).await?;
                }
                Err(err) => {
                    tracing::warn!(error = %err, "could not open a file to send");
                    sending.transfer.queued = false;
                    sending.transfer.end(Stage::Failed(Failure::Disk));
                    let name = sending.transfer.name.clone();
                    self.notify(Notice::TransferFailed {
                        device: self.device_name(&to),
                        name,
                        outgoing: true,
                        failure: Failure::Disk,
                    });
                }
            }
        }
        Ok(())
    }

    /// Offers again what was under way to `to`, so it resumes.
    async fn offer_again(&mut self, session: &mut Session, to: [u8; 16]) -> Result<()> {
        let now = Instant::now();
        for index in 0..self.outgoing.len() {
            let transfer = &mut self.outgoing[index].transfer;
            if transfer.to == to && !transfer.queued && transfer.stage.is_active() {
                let offer = transfer.start(now);
                self.send(session, to, &offer).await?;
            }
        }
        Ok(())
    }

    /// Sends the chunks the window allows for the transfer at `index`.
    async fn pump(&mut self, session: &mut Session, index: usize) -> Result<()> {
        loop {
            let sending = &mut self.outgoing[index];
            let Some((offset, len)) = sending.transfer.next_chunk() else {
                return Ok(());
            };
            let read = match sending.file.as_mut() {
                Some(file) => read_chunk(file, offset, len),
                None => Err(std::io::ErrorKind::NotFound.into()),
            };
            let (to, id) = (sending.transfer.to, sending.transfer.id);
            let data = match read {
                Ok(data) => data,
                Err(err) => {
                    tracing::warn!(error = %err, "could not read a file being sent");
                    sending.transfer.end(Stage::Failed(Failure::Disk));
                    sending.file = None;
                    let name = sending.transfer.name.clone();
                    self.notify(Notice::TransferFailed {
                        device: self.device_name(&to),
                        name,
                        outgoing: true,
                        failure: Failure::Disk,
                    });
                    let cancel = Packet::FileCancel {
                        id: transfer_id(&id),
                    };
                    return self.send(session, to, &cancel).await;
                }
            };
            let sent = self.send_chunk(session, to, id, offset, &data).await;
            only_connection_errors(sent, "file chunk")?;
            self.outgoing[index].transfer.chunk_sent(len);
        }
    }

    /// Answers an offer: resumes, accepts or rejects it.
    async fn offered(
        &mut self,
        session: &mut Session,
        from: [u8; 16],
        offer: FileOffer,
    ) -> Result<()> {
        let Some(id) = parse_transfer_id(&offer.id) else {
            tracing::warn!(from = %short_id(&from), "dropped an offer with an invalid id");
            return Ok(());
        };
        let reply_id = offer.id.clone();
        if self.completed.contains(&(from, id)) {
            // The done got lost on the way.
            return self
                .send(session, from, &Packet::FileDone { id: reply_id })
                .await;
        }
        let now = Instant::now();
        let existing = self
            .incoming
            .iter()
            .position(|r| r.transfer.id == id && r.transfer.from == from);
        if let Some(index) = existing {
            let receiving = &mut self.incoming[index];
            let resumable = matches!(
                receiving.transfer.stage,
                Stage::Running | Stage::Failed(Failure::NoProgress)
            ) && receiving.transfer.size == offer.size;
            if resumable && self.view.features.files {
                if receiving.file.is_none() {
                    match open_partial(&receiving.partial, receiving.transfer.written) {
                        Ok(file) => receiving.file = Some(file),
                        Err(err) => {
                            tracing::warn!(error = %err, "could not reopen a partial file");
                            let reason = Some(RejectReason::NoSpace.as_str().into());
                            receiving.transfer.stage = Stage::Failed(Failure::Disk);
                            return self
                                .send(
                                    session,
                                    from,
                                    &Packet::FileReject {
                                        id: reply_id,
                                        reason,
                                    },
                                )
                                .await;
                        }
                    }
                }
                receiving.transfer.resume(now);
                let offset = receiving.transfer.written;
                tracing::info!(from = %short_id(&from), offset, "resuming a file");
                return self
                    .send(
                        session,
                        from,
                        &Packet::FileAccept {
                            id: reply_id,
                            offset,
                        },
                    )
                    .await;
            }
            // Ended here: a new attempt starts afresh.
            self.incoming.remove(index);
        }
        let partial = files::partial_path(&self.partial_dir, &from, &id);
        let active = self
            .incoming
            .iter()
            .filter(|r| r.transfer.from == from && r.transfer.stage.is_active())
            .count();
        let decision = transfer::decide_offer(
            self.view.features.files,
            offer.size,
            active,
            files::kept_len(&partial),
            files::free_space(&self.partial_dir).or_else(|| files::free_space(self.dir.path())),
        );
        let opened = decision.and_then(|offset| {
            open_partial(&partial, offset)
                .map(|file| (offset, file))
                .map_err(|err| {
                    tracing::warn!(error = %err, "could not create a partial file");
                    RejectReason::NoSpace
                })
        });
        let (offset, file) = match opened {
            Ok(opened) => opened,
            Err(reason) => {
                tracing::info!(from = %short_id(&from), ?reason, "file refused");
                let reason = Some(reason.as_str().into());
                return self
                    .send(
                        session,
                        from,
                        &Packet::FileReject {
                            id: reply_id,
                            reason,
                        },
                    )
                    .await;
            }
        };
        let name = transfer::sanitize_name(&offer.name);
        tracing::info!(from = %short_id(&from), size = offer.size, offset, "receiving a file");
        self.incoming.push(Receiving {
            transfer: Incoming::new(id, from, name, offer.size, offset, now),
            partial,
            file: Some(file),
        });
        self.send(
            session,
            from,
            &Packet::FileAccept {
                id: reply_id,
                offset,
            },
        )
        .await?;
        let index = self.incoming.len() - 1;
        if self.incoming[index].transfer.is_complete() {
            self.complete(session, index).await?;
        }
        Ok(())
    }

    /// Writes a chunk that comes in order, acknowledges it, and finishes
    /// the file with its last one. Any other chunk is ignored.
    async fn chunk(
        &mut self,
        session: &mut Session,
        from: [u8; 16],
        id: [u8; 16],
        offset: u64,
        data: &[u8],
    ) -> Result<()> {
        let Some(index) = self
            .incoming
            .iter()
            .position(|r| r.transfer.id == id && r.transfer.from == from)
        else {
            tracing::debug!(from = %short_id(&from), "chunk of no transfer ignored");
            return Ok(());
        };
        let receiving = &mut self.incoming[index];
        if !receiving.transfer.wants(offset, data.len()) {
            tracing::debug!(from = %short_id(&from), offset, "chunk out of order ignored");
            return Ok(());
        }
        let written = match receiving.file.as_mut() {
            Some(file) => file.write_all(data),
            None => Err(std::io::ErrorKind::NotFound.into()),
        };
        if let Err(err) = written {
            tracing::warn!(error = %err, "could not write a file being received");
            receiving.transfer.stage = Stage::Failed(Failure::Disk);
            receiving.file = None;
            let _ = std::fs::remove_file(&receiving.partial);
            let name = receiving.transfer.name.clone();
            self.notify(Notice::TransferFailed {
                device: self.device_name(&from),
                name,
                outgoing: false,
                failure: Failure::Disk,
            });
            let cancel = Packet::FileCancel {
                id: transfer_id(&id),
            };
            return self.send(session, from, &cancel).await;
        }
        let complete = receiving.transfer.wrote(data.len(), Instant::now());
        let ack = Packet::FileAck {
            id: transfer_id(&id),
            offset: receiving.transfer.written,
        };
        self.send(session, from, &ack).await?;
        if complete {
            self.complete(session, index).await?;
        }
        Ok(())
    }

    /// Closes a complete file, says so, and moves it to Downloads on a
    /// pool thread: across volumes the move is a copy.
    async fn complete(&mut self, session: &mut Session, index: usize) -> Result<()> {
        let receiving = &mut self.incoming[index];
        let flushed = receiving.file.take().map(|file| file.sync_all());
        if let Some(Err(err)) = flushed {
            tracing::warn!(error = %err, "could not flush a received file");
        }
        receiving.transfer.stage = Stage::Done;
        let (from, id) = (receiving.transfer.from, receiving.transfer.id);
        let (partial, name) = (receiving.partial.clone(), receiving.transfer.name.clone());
        self.completed.push((from, id));
        if self.completed.len() > COMPLETED_KEPT {
            self.completed.remove(0);
        }
        tracing::info!(from = %short_id(&from), "file received");
        self.send(
            session,
            from,
            &Packet::FileDone {
                id: transfer_id(&id),
            },
        )
        .await?;

        let (notify, downloads) = (self.hooks.on_notice.clone(), self.hooks.downloads.clone());
        let device = self.device_name(&from);
        tokio::task::spawn_blocking(move || {
            match files::move_unique(&partial, &downloads, &name) {
                Ok(path) => notify(Notice::FileReceived { from, device, path }),
                Err(err) => {
                    tracing::warn!(error = %err, "could not move a received file to Downloads");
                    notify(Notice::TransferFailed {
                        device,
                        name,
                        outgoing: false,
                        failure: Failure::Disk,
                    });
                }
            }
        });
        Ok(())
    }

    /// The other device cancelled the transfer `id`.
    fn cancel_from(&mut self, device: &[u8; 16], id: &[u8; 16]) {
        self.image_in.remove(&(*device, *id));
        self.image_out.remove(&(*device, *id));
        if let Some(sending) = self
            .outgoing
            .iter_mut()
            .find(|s| s.transfer.id == *id && s.transfer.to == *device)
        {
            sending.transfer.end(Stage::Cancelled);
            sending.file = None;
        }
        if let Some(receiving) = self
            .incoming
            .iter_mut()
            .find(|r| r.transfer.id == *id && r.transfer.from == *device)
            && receiving.transfer.stage != Stage::Done
        {
            receiving.transfer.stage = Stage::Cancelled;
            receiving.file = None;
            let _ = std::fs::remove_file(&receiving.partial);
        }
    }

    /// Cancels the transfer `id` here. Returns the device on the other
    /// end, to tell it, if the transfer was under way.
    fn cancel_locally(&mut self, id: &[u8; 16]) -> Option<[u8; 16]> {
        if let Some(sending) = self
            .outgoing
            .iter_mut()
            .find(|s| s.transfer.id == *id && s.transfer.stage.is_active())
        {
            let offered = !sending.transfer.queued;
            sending.transfer.end(Stage::Cancelled);
            sending.file = None;
            return offered.then_some(sending.transfer.to);
        }
        let receiving = self
            .incoming
            .iter_mut()
            .find(|r| r.transfer.id == *id && r.transfer.stage.is_active())?;
        receiving.transfer.stage = Stage::Cancelled;
        receiving.file = None;
        let _ = std::fs::remove_file(&receiving.partial);
        Some(receiving.transfer.from)
    }

    /// A device left: its transfers end, and their partial files go.
    fn drop_transfers_with(&mut self, device: &[u8; 16]) {
        self.image_out.retain(|(peer, _), _| peer != device);
        self.image_in.retain(|(peer, _), _| peer != device);
        let ids: Vec<[u8; 16]> = self
            .outgoing
            .iter()
            .filter(|s| s.transfer.to == *device)
            .map(|s| s.transfer.id)
            .chain(
                self.incoming
                    .iter()
                    .filter(|r| r.transfer.from == *device)
                    .map(|r| r.transfer.id),
            )
            .collect();
        for id in ids {
            self.cancel_locally(&id);
        }
    }

    fn set_features(&mut self, features: Features) {
        self.view.features = features;
        self.save_settings();
        if !features.sms {
            for peer in self.view.peers.values_mut() {
                peer.sms = Arc::default();
            }
            self.older_asked.clear();
        }
        if !features.battery {
            for peer in self.view.peers.values_mut() {
                peer.battery = None;
                peer.connectivity = None;
            }
        }
        if !features.ring {
            self.stop_ring();
        }
        for peer in self.view.peers.values_mut() {
            if !features.media {
                peer.media = None;
                peer.covers = Covers::default();
            }
            if !features.volume {
                peer.volume = None;
            }
        }
    }

    /// Rebuilds the transfers of the view: at once when one starts or
    /// ends, at most every [`PROGRESS_INTERVAL`] when only bytes move.
    /// Finished ones beyond the last few are forgotten.
    fn show_transfers(&mut self) {
        prune(&mut self.outgoing, |s| s.transfer.stage);
        prune(&mut self.incoming, |r| r.transfer.stage);
        let views: Vec<TransferView> = self
            .outgoing
            .iter()
            .map(|s| TransferView {
                id: s.transfer.id,
                device: s.transfer.to,
                outgoing: true,
                name: s.transfer.name.clone(),
                size: s.transfer.size,
                done: s.transfer.acked,
                stage: s.transfer.stage,
            })
            .chain(self.incoming.iter().map(|r| TransferView {
                id: r.transfer.id,
                device: r.transfer.from,
                outgoing: false,
                name: r.transfer.name.clone(),
                size: r.transfer.size,
                done: r.transfer.written,
                stage: r.transfer.stage,
            }))
            .collect();
        let outline = |views: &[TransferView]| -> Vec<([u8; 16], Stage)> {
            views.iter().map(|view| (view.id, view.stage)).collect()
        };
        let now = Instant::now();
        let only_progress = outline(&views) == outline(&self.view.transfers);
        if only_progress && now.duration_since(self.progress_at) < PROGRESS_INTERVAL {
            return;
        }
        if views != self.view.transfers {
            self.progress_at = now;
            self.view.transfers = views;
        }
    }

    /// Runs one command on the connection. An error means the connection
    /// is lost; a request the relay or the protocol refuses is reported in
    /// the snapshot or the log instead.
    async fn command(&mut self, session: &mut Session, command: Command) -> Result<()> {
        let done = match command {
            Command::SendImage(image) => self.send_image(session, image).await,
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
            Command::RemoveDevice(device) => {
                self.drop_transfers_with(&device);
                session.remove(&hex(&device)).await.map(|_| {
                    tracing::info!(device = %short_id(&device), "device removed");
                })
            }
            Command::Rename(name) => session.rename(&name).await.map(|_| {
                tracing::info!(%name, "device renamed");
            }),
            Command::SetPaused(paused) => {
                self.view.pause.all = paused;
                Ok(())
            }
            Command::SetFeatures(features) => {
                let before = self.view.features;
                self.set_features(features);
                // Each device gets its own: input goes only to those
                // allowed to control this computer.
                let devices: Vec<[u8; 16]> =
                    self.view.devices.iter().map(|device| device.id).collect();
                for device in devices {
                    let packet = Packet::Capabilities(self.capabilities_for(&device));
                    self.send(session, device, &packet).await?;
                }
                self.features_turned_on(session, before).await
            }
            Command::Ring(device) if self.view.features.ring => {
                tracing::info!(device = %short_id(&device), "ringing a device");
                self.send(session, device, &Packet::Ring {}).await
            }
            Command::StopRinging(device) => self.send(session, device, &Packet::RingStop {}).await,
            Command::Ring(_) => Ok(()),
            Command::FoundIt => {
                self.stop_ring();
                Ok(())
            }
            Command::SendFiles(device, paths) => {
                self.queue_files(device, paths);
                self.start_queued(session).await
            }
            Command::CancelTransfer(id) => match self.cancel_locally(&id) {
                Some(device) => {
                    let cancel = Packet::FileCancel {
                        id: transfer_id(&id),
                    };
                    self.send(session, device, &cancel).await
                }
                None => Ok(()),
            },
            Command::SendPacket(device, Packet::ShareUrl { url }) if !is_shareable_url(&url) => {
                tracing::debug!(device = %short_id(&device), "link not sent: not a web link");
                Ok(())
            }
            Command::SendPacket(device, packet) => {
                if self.announced(false).sends(packet.kind()) {
                    self.send(session, device, &packet).await
                } else {
                    tracing::debug!(
                        kind = packet.kind(),
                        "packet of a disabled feature not sent"
                    );
                    Ok(())
                }
            }
            Command::LocalMedia(players) => {
                let now = Instant::now();
                let players = self.keep_covers(players);
                let news = match &self.local_media {
                    Some((old, at)) => media::differs(old, now.duration_since(*at), &players),
                    None => true,
                };
                self.local_media = Some((players.clone(), now));
                if news && self.view.features.media {
                    self.send_media(session, players).await
                } else {
                    Ok(())
                }
            }
            Command::LocalVolume(volume) => {
                let news = self.local_volume != Some(volume);
                self.local_volume = Some(volume);
                if news && self.view.features.volume {
                    self.send_to_handlers(session, &Packet::Volume(volume))
                        .await
                } else {
                    Ok(())
                }
            }
            Command::OpenMessages(device) => self.open_messages(session, device).await,
            Command::LoadMessages {
                device,
                thread,
                before,
            } if self.view.features.sms => {
                self.request_messages(session, device, thread, before).await
            }
            Command::LoadMessages { .. } => Ok(()),
            Command::SendSms {
                device,
                addresses,
                text,
                thread,
            } if self.view.features.sms => {
                let text: String = text.chars().take(sms::MAX_TEXT).collect();
                let outgoing = self.outgoing_sms(addresses, text, thread);
                let packet = Packet::SmsSend(SmsSend {
                    addresses: outgoing.addresses.clone(),
                    text: outgoing.text.clone(),
                    request_id: outgoing.request_id.clone(),
                });
                let sms = self.sms_mut(device);
                sms.outbox.push(outgoing);
                sms.trim_outbox();
                tracing::info!(device = %short_id(&device), "sending an SMS");
                self.send(session, device, &packet).await
            }
            Command::SendSms { .. } => Ok(()),
            // At once, so the device shows or hides its remote control.
            Command::SetControl(device, allowed) => {
                self.set_control(device, allowed);
                let packet = Packet::Capabilities(self.capabilities_for(&device));
                self.send(session, device, &packet).await
            }
            Command::SetCommands(commands) => {
                self.view.commands = commands;
                self.save_settings();
                self.send_command_list(session).await
            }
            Command::Shutdown => Ok(()),
        };
        describe(&mut self.view, session.state());
        self.show_transfers();
        self.publish();
        match done {
            Err(err) if is_refusal(&err) => {
                tracing::warn!(error = %err, "request failed");
                Ok(())
            }
            done => done,
        }
    }

    /// What a feature just turned on needs: the state it shares, and the
    /// notifications the devices have now.
    async fn features_turned_on(&mut self, session: &mut Session, before: Features) -> Result<()> {
        let now = self.view.features;
        let devices: Vec<[u8; 16]> = self.view.devices.iter().map(|device| device.id).collect();
        for device in devices {
            if (now.media && !before.media) || (now.volume && !before.volume) {
                self.send_state(session, device).await?;
            }
            if now.notifications
                && !before.notifications
                && self.view.peer(&device).sends("notification")
            {
                self.request_notifications(session, device).await?;
            }
        }
        if now.commands && !before.commands {
            self.send_command_list(session).await?;
        }
        Ok(())
    }

    /// Asks a phone for its conversations, and for its contacts until it
    /// has sent some.
    async fn open_messages(&mut self, session: &mut Session, device: [u8; 16]) -> Result<()> {
        let peer = self.view.peer(&device);
        if !self.view.features.sms || !peer.handles("sms.conversations.request") {
            return Ok(());
        }
        self.send(session, device, &Packet::SmsConversationsRequest {})
            .await?;
        // Asked again while none came: the phone may have been without
        // the permission, or switched off, the last time.
        if peer.handles("contacts.request") && peer.sms.contacts.is_empty() {
            self.send(session, device, &Packet::ContactsRequest {})
                .await?;
        }
        Ok(())
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
                self.view.pairing = PairingState::Failed(PairingFailure::of(&err));
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    async fn join(&mut self, session: &mut Session, code: &str) -> Result<()> {
        self.pairing = None;
        let code = match PairingCode::decode(code.trim()) {
            Ok(code) => code,
            Err(_) => {
                self.view.pairing = PairingState::Failed(PairingFailure::InvalidCode);
                return Ok(());
            }
        };
        self.view.pairing = PairingState::Joining;
        self.publish();
        match session.join(&code, JOIN_TIMEOUT).await {
            Ok(devices) => {
                tracing::info!("joined the group: {} trusted device(s)", devices.len());
                self.pairing_kind = PairingKind::JoinedOther;
                self.view.pairing = PairingState::Success {
                    device: device_id_from_sign_pk(&code.sign_pk),
                };
                describe(&mut self.view, session.state());
                for device in devices {
                    self.send_capabilities(session, device).await?;
                }
                Ok(())
            }
            // No answer in time: the code expired, or its device is gone.
            Err(err) if is_refusal(&err) || matches!(err, Error::Timeout(_)) => {
                tracing::warn!(error = %err, "joining failed");
                self.view.pairing = PairingState::Failed(PairingFailure::of(&err));
                Ok(())
            }
            Err(err) => Err(err),
        }
    }
}

/// Forgets the oldest finished transfers past [`FINISHED_SHOWN`].
fn prune<T>(list: &mut Vec<T>, stage: impl Fn(&T) -> Stage) {
    let finished = list.iter().filter(|item| !stage(item).is_active()).count();
    let mut extra = finished.saturating_sub(FINISHED_SHOWN);
    list.retain(|item| {
        if extra > 0 && !stage(item).is_active() {
            extra -= 1;
            return false;
        }
        true
    });
}
