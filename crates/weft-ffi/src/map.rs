//! The worker's snapshot and notices as UniFFI records.
//!
//! Instants become milliseconds since the Unix epoch, ids become lowercase
//! hex, and nothing borrows from the worker.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use weft_cli::{hex, short_id};
use weft_client::features::{Features, UserCommand};
use weft_client::input::{Button, Input, Key, Press};
use weft_client::media::Cover;
use weft_client::packets::{MediaAction, Player, RejectReason};
use weft_client::sms::{SendState, Sms};
use weft_client::transfer::{Failure, Stage};
use weft_client::worker::{
    Direction, Notice, PairingFailure, PairingKind, PairingState, PeerInfo, PeerMedia, Published,
    Snapshot, Status, TransferView,
};

/// The state of the connection to the relay.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    /// The connection failed or was lost; another attempt comes at
    /// `retry_at_ms`.
    Offline {
        reason: String,
        retry_at_ms: i64,
    },
    /// An error a new attempt cannot fix; the worker stopped trying.
    Stopped {
        reason: String,
    },
}

/// A device's battery, when it sent one.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BatteryInfo {
    /// 0 to 100.
    pub level: u8,
    pub charging: bool,
    pub low: bool,
}

/// A device's network, when it sent one.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ConnectivityInfo {
    /// `"5G"`, `"LTE"`, `"3G"`, `"2G"`, `"wifi"`, `"none"`, or unknown.
    pub network: Option<String>,
    /// 0 to 4 bars, when known.
    pub signal: Option<u8>,
}

/// A player of another device. Times are in milliseconds; `position_ms`
/// is where it was when the device sent it (see [`MediaInfo`]).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PlayerInfo {
    pub id: String,
    pub app: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    pub position_ms: Option<u64>,
    pub length_ms: Option<u64>,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_seek: bool,
    /// 0 to 100, when the player has its own volume.
    pub volume: Option<u8>,
    /// The id of its cover, only when the core has the image: get it with
    /// [`crate::WeftCore::media_art`]. The same id is the same image, so
    /// the app may keep it once read.
    pub art: Option<String>,
}

/// A cover image, as `media.art` carries it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CoverImage {
    /// `"image/jpeg"` or `"image/png"`.
    pub mime: String,
    /// At most 128 KiB.
    pub data: Vec<u8>,
}

/// One of this Mac's players, as the app reads it, for
/// [`crate::WeftCore::set_local_media`]. Times are in milliseconds.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LocalPlayer {
    /// Stable while the player exists: `media.command` names it.
    pub id: String,
    pub app: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    /// Where it is now, when known.
    pub position_ms: Option<u64>,
    pub length_ms: Option<u64>,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_seek: bool,
    /// Its cover, its longer side scaled to at most 512 px (300 px is
    /// enough), JPEG unless it needs transparency. One over 128 KiB or of
    /// another type is dropped, the player kept.
    pub cover: Option<CoverImage>,
}

/// What another device plays. A player that plays has moved on by the
/// time elapsed since `received_at_ms`, up to its length.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MediaInfo {
    /// Empty when nothing plays.
    pub players: Vec<PlayerInfo>,
    /// When the positions were received, in ms since the Unix epoch.
    pub received_at_ms: i64,
    /// The index in `players` of the one to show: the first one playing,
    /// else the first one; `None` when there is none.
    pub current: Option<u32>,
}

/// Another device's output volume. A missing member is unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct VolumeInfo {
    /// 0 to 100.
    pub level: Option<u8>,
    pub muted: Option<bool>,
}

/// The feature switches of `docs/features.md` section 10 this Mac has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct FeatureSwitches {
    pub files: bool,
    pub links: bool,
    pub notifications: bool,
    pub battery: bool,
    pub ring: bool,
    pub calls: bool,
    pub media: bool,
    pub volume: bool,
    pub sms: bool,
    pub commands: bool,
}

/// A shortcut this Mac offers its devices (`docs/features.md` section 8).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CommandItem {
    pub id: String,
    pub name: String,
    pub line: String,
}

/// An SMS conversation of a phone (`docs/features.md` section 7).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ConversationInfo {
    pub thread: i64,
    pub addresses: Vec<String>,
    pub names: Vec<Option<String>>,
    pub title: String,
    pub snippet: Option<String>,
    pub time_ms: i64,
    pub unread: bool,
}

/// One SMS message in a thread.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SmsMessageInfo {
    pub id: i64,
    pub address: Option<String>,
    pub sender: Option<String>,
    pub body: String,
    pub time_ms: i64,
    pub outgoing: bool,
    pub status: Option<String>,
}

/// The loaded messages of one thread, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ThreadMessages {
    pub thread: i64,
    pub messages: Vec<SmsMessageInfo>,
    /// True once the phone reported there are no older messages.
    pub complete: bool,
}

/// A contact of the phone.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ContactInfo {
    pub name: String,
    pub numbers: Vec<String>,
}

/// Where an SMS sent from this Mac stands.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum SmsSendState {
    Pending,
    Sent,
    Failed { error: Option<String> },
}

/// An SMS sent from this Mac, kept until its thread includes it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SendingSmsInfo {
    pub request_id: String,
    pub addresses: Vec<String>,
    pub text: String,
    pub thread: Option<i64>,
    pub state: SmsSendState,
    pub at_ms: i64,
}

/// What this Mac loaded of a phone's SMS and contacts.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct SmsInfo {
    /// Whether the phone has answered `sms.conversations.request`.
    pub loaded: bool,
    pub conversations: Vec<ConversationInfo>,
    pub threads: Vec<ThreadMessages>,
    pub contacts: Vec<ContactInfo>,
    pub sending: Vec<SendingSmsInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ClickPhase {
    Click,
    Double,
    Down,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SpecialKeyKind {
    Enter,
    Backspace,
    Tab,
    Escape,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
}

impl SpecialKeyKind {
    /// The wire name of the key in an `input.key` packet.
    pub(crate) fn wire_name(self) -> &'static str {
        match self {
            SpecialKeyKind::Enter => "enter",
            SpecialKeyKind::Backspace => "backspace",
            SpecialKeyKind::Tab => "tab",
            SpecialKeyKind::Escape => "escape",
            SpecialKeyKind::Left => "left",
            SpecialKeyKind::Right => "right",
            SpecialKeyKind::Up => "up",
            SpecialKeyKind::Down => "down",
            SpecialKeyKind::Home => "home",
            SpecialKeyKind::End => "end",
            SpecialKeyKind::PageUp => "pageUp",
            SpecialKeyKind::PageDown => "pageDown",
            SpecialKeyKind::Delete => "delete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PresenterCommand {
    Next,
    Previous,
    Start,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct SpotPoint {
    pub x: f64,
    pub y: f64,
}

/// What an allowed device asks of this Mac's pointer, keyboard or
/// presenter (`docs/features.md` section 8).
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum RemoteInput {
    Move {
        dx: f64,
        dy: f64,
    },
    Scroll {
        dx: f64,
        dy: f64,
    },
    Click {
        button: MouseButton,
        phase: ClickPhase,
    },
    KeyText {
        text: String,
    },
    SpecialKey {
        key: SpecialKeyKind,
        ctrl: bool,
        alt: bool,
        shift: bool,
        meta: bool,
    },
    Presenter {
        action: PresenterCommand,
    },
    PresenterSpot {
        point: Option<SpotPoint>,
    },
}

/// Why a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TransferFailureKind {
    /// The other device does not take files.
    Disabled,
    /// Too large for the other device.
    TooLargeForReceiver,
    /// Not enough space on the other device.
    NoSpace,
    /// The other device already runs as many transfers as it can.
    Busy,
    /// Refused, for a reason this version does not know.
    Refused,
    /// Nothing moved for 10 minutes.
    NoProgress,
    /// The file could not be read (sending) or written (receiving).
    Disk,
    /// Over 4 GiB, the most a transfer carries.
    TooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TransferStage {
    /// Waiting for a free slot, or for the other device to answer.
    Waiting,
    Running,
    Done,
    Cancelled,
    Failed {
        failure: TransferFailureKind,
    },
}

/// A file transfer, under way or among the last finished ones.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TransferInfo {
    /// In hex, as [`crate::WeftCore::cancel_transfer`] takes it.
    pub id: String,
    /// The device on the other end, in hex.
    pub device_id: String,
    /// Sent from this Mac, rather than received.
    pub outgoing: bool,
    pub name: String,
    pub size: u64,
    /// Bytes confirmed (sent) or written (received).
    pub done: u64,
    pub stage: TransferStage,
}

/// What [`crate::WeftCore::media_command`] asks a player to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MediaControl {
    Play,
    Pause,
    PlayPause,
    Next,
    Previous,
    /// To this position, in milliseconds.
    Seek {
        position_ms: u64,
    },
    /// To this volume, 0 to 100.
    SetVolume {
        level: u8,
    },
}

/// A trusted device, with what it told about itself.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DeviceInfo {
    /// The device id in hex, as commands take it.
    pub id: String,
    /// The short id shown next to the name.
    pub short_id: String,
    /// The name it announced, if any.
    pub name: Option<String>,
    /// `"android"`, `"windows"`, `"macos"`, `"cli"`…, once it sent its
    /// capabilities.
    pub platform: Option<String>,
    pub battery: Option<BatteryInfo>,
    pub connectivity: Option<ConnectivityInfo>,
    /// The packet types it acts on (`incoming` of its capabilities).
    pub handles: Vec<String>,
    /// The packet types it may send (`outgoing` of its capabilities).
    pub sends: Vec<String>,
    /// Its players, once it sent them and while the media switch is on.
    pub media: Option<MediaInfo>,
    /// Its volume, once it sent it and while the volume switch is on.
    pub volume: Option<VolumeInfo>,
    /// Whether this device is allowed to control this Mac.
    pub allowed_control: bool,
    /// Its SMS and contacts, while the SMS switch is on.
    pub sms: SmsInfo,
}

/// Why a pairing failed.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum PairingFailureKind {
    InvalidCode,
    Expired,
    OwnCode,
    OtherRelay,
    GroupFull,
    NotConnected,
    ConnectionLost,
    NoAnswer,
    Other { message: String },
}

/// Where a pairing stands.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Pairing {
    Idle,
    /// Waiting for a device to use `code`, a `WEFT1:` string to show as a
    /// QR code. The code is a secret: never log it.
    ShowingCode {
        code: String,
        expires_at_ms: i64,
    },
    Joining,
    /// Paired with this device id. `invited` is true when this Mac joined
    /// that device's group, rather than adding it to this Mac's own group.
    Paired {
        device_id: String,
        invited: bool,
    },
    Failed {
        failure: PairingFailureKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TextDirection {
    Sent,
    Received,
}

/// The last clipboard text that went through, without the text.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LastActivity {
    pub direction: TextDirection,
    pub at_ms: i64,
}

/// Everything the app shows, as the worker last knew it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CoreSnapshot {
    /// Grows with each snapshot of one worker; starts again when the
    /// worker restarts (relay change, leaving the group).
    pub generation: u64,
    /// This Mac's name and id.
    pub name: String,
    pub device_id: String,
    pub short_id: String,
    pub relay_url: String,
    pub status: ConnectionStatus,
    pub paused: bool,
    /// The other trusted devices.
    pub devices: Vec<DeviceInfo>,
    pub pairing: Pairing,
    pub last_activity: Option<LastActivity>,
    /// The feature switches, as saved.
    pub features: FeatureSwitches,
    /// The shortcuts this Mac offers its devices.
    pub commands: Vec<CommandItem>,
    /// The device ringing this Mac, in hex, while it rings.
    pub ringing: Option<String>,
    /// Transfers under way, then the last finished ones.
    pub transfers: Vec<TransferInfo>,
}

/// Something the user should see or hear, as the worker reported it.
/// `from` is a device id in hex, `device` its name when known.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum CoreNotice {
    ClipboardImage {
        from: String,
        mime: String,
        bytes: Vec<u8>,
    },
    Ring {
        from: String,
        device: Option<String>,
    },
    RingEnded,
    Ping {
        from: String,
        device: Option<String>,
        message: Option<String>,
    },
    Link {
        from: String,
        device: Option<String>,
        url: String,
    },
    FileReceived {
        from: String,
        device: Option<String>,
        path: String,
    },
    TransferFailed {
        device: Option<String>,
        name: String,
        outgoing: bool,
        failure: TransferFailureKind,
    },
    /// A phone notification, new or updated: `id` is stable for one
    /// notification. `silent`: an update, or one the phone already had,
    /// that should not pop up again.
    Notification {
        from: String,
        device: Option<String>,
        id: String,
        app: String,
        title: String,
        text: String,
        dismissable: bool,
        actions: Vec<String>,
        reply: bool,
        silent: bool,
    },
    NotificationRemoved {
        from: String,
        id: String,
    },
    Call {
        from: String,
        device: Option<String>,
        event: String,
        number: Option<String>,
        name: Option<String>,
    },
    /// A device asks one of this Mac's players (by its
    /// [`LocalPlayer::id`]) to act.
    MediaCommand {
        from: String,
        player: String,
        control: MediaControl,
    },
    SetVolume {
        from: String,
        level: Option<u8>,
        muted: Option<bool>,
    },
    SmsReceived {
        from: String,
        device: Option<String>,
        thread: i64,
        sender: String,
        text: String,
        addresses: Vec<String>,
    },
    CommandRan {
        from: String,
        device: Option<String>,
        name: String,
    },
}

/// What one look at the pasteboard did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct PollOutcome {
    /// A received text was written to the pasteboard.
    pub written: bool,
    /// A local change was sent to the group.
    pub sent: bool,
}

/// What "Send clipboard now" did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SendOutcome {
    Sent,
    Paused,
    /// Not connected to the relay.
    Offline,
    /// No other device to send to.
    NoDevices,
    /// No text on the pasteboard, or an empty one.
    NoText,
    /// Marked by the app that copied it as not to be shared.
    Excluded,
    /// Over 256 KiB, the most a text can be.
    TooLarge,
}

/// The two clocks a conversion needs, read once.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Now {
    pub instant: Instant,
    pub system: SystemTime,
}

impl Now {
    pub fn read() -> Now {
        Now {
            instant: Instant::now(),
            system: SystemTime::now(),
        }
    }

    /// `at` as milliseconds since the Unix epoch.
    pub fn unix_ms(&self, at: Instant) -> i64 {
        let at = if at >= self.instant {
            self.system + (at - self.instant)
        } else {
            self.system - (self.instant - at)
        };
        system_ms(at)
    }
}

pub(crate) fn system_ms(at: SystemTime) -> i64 {
    match at.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_millis()).unwrap_or(i64::MAX),
        Err(before) => -i64::try_from(before.duration().as_millis()).unwrap_or(i64::MAX),
    }
}

fn status(status: &Status, now: Now) -> ConnectionStatus {
    match status {
        Status::Connecting => ConnectionStatus::Connecting,
        Status::Connected => ConnectionStatus::Connected,
        Status::Offline { reason, retry_at } => ConnectionStatus::Offline {
            reason: reason.clone(),
            retry_at_ms: now.unix_ms(*retry_at),
        },
        Status::Fatal(reason) => ConnectionStatus::Stopped {
            reason: reason.clone(),
        },
    }
}

fn failure(failure: &PairingFailure) -> PairingFailureKind {
    match failure {
        PairingFailure::InvalidCode => PairingFailureKind::InvalidCode,
        PairingFailure::Expired => PairingFailureKind::Expired,
        PairingFailure::OwnCode => PairingFailureKind::OwnCode,
        PairingFailure::OtherRelay => PairingFailureKind::OtherRelay,
        PairingFailure::GroupFull => PairingFailureKind::GroupFull,
        PairingFailure::NotConnected => PairingFailureKind::NotConnected,
        PairingFailure::ConnectionLost => PairingFailureKind::ConnectionLost,
        PairingFailure::NoAnswer => PairingFailureKind::NoAnswer,
        PairingFailure::Other(message) => PairingFailureKind::Other {
            message: message.clone(),
        },
    }
}

fn pairing(pairing: &PairingState, paired: Option<PairingKind>, now: Now) -> Pairing {
    match pairing {
        PairingState::Idle => Pairing::Idle,
        PairingState::ShowingCode { code, expires_at } => Pairing::ShowingCode {
            code: code.clone(),
            expires_at_ms: now.unix_ms(*expires_at),
        },
        PairingState::Joining => Pairing::Joining,
        PairingState::Success { device } => Pairing::Paired {
            device_id: hex(device),
            invited: matches!(
                paired,
                Some(PairingKind::Invited | PairingKind::JoinedOther)
            ),
        },
        PairingState::Failed(reason) => Pairing::Failed {
            failure: failure(reason),
        },
    }
}

fn sms_info(sms: &Sms) -> SmsInfo {
    let conversations = sms
        .conversations
        .as_ref()
        .map(|list| {
            list.iter()
                .map(|conversation| {
                    let people: Vec<String> = conversation
                        .addresses
                        .iter()
                        .map(|address| {
                            sms.name_of(conversation.thread, address)
                                .unwrap_or_else(|| address.clone())
                        })
                        .collect();
                    let title = if people.is_empty() {
                        conversation.title()
                    } else {
                        people.join(", ")
                    };
                    ConversationInfo {
                        thread: conversation.thread,
                        addresses: conversation.addresses.clone(),
                        names: conversation.names.clone(),
                        title,
                        snippet: conversation.snippet.clone(),
                        time_ms: conversation.time,
                        unread: conversation.unread,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let threads = sms
        .threads
        .iter()
        .map(|(&thread, messages)| ThreadMessages {
            thread,
            messages: messages
                .iter()
                .map(|message| SmsMessageInfo {
                    id: message.id,
                    sender: message
                        .address
                        .as_deref()
                        .and_then(|address| sms.name_of(thread, address)),
                    address: message.address.clone(),
                    body: message.body.clone(),
                    time_ms: message.time,
                    outgoing: message.outgoing,
                    status: message.status.clone(),
                })
                .collect(),
            complete: sms.complete.contains(&thread),
        })
        .collect();
    let contacts = sms
        .contacts
        .iter()
        .map(|contact| ContactInfo {
            name: contact.name.clone(),
            numbers: contact.numbers.clone(),
        })
        .collect();
    let sending = sms
        .outbox
        .iter()
        .map(|sent| SendingSmsInfo {
            request_id: sent.request_id.clone(),
            addresses: sent.addresses.clone(),
            text: sent.text.clone(),
            thread: sent.thread,
            state: match &sent.state {
                SendState::Pending => SmsSendState::Pending,
                SendState::Sent => SmsSendState::Sent,
                SendState::Failed(error) => SmsSendState::Failed {
                    error: error.clone(),
                },
            },
            at_ms: sent.at,
        })
        .collect();
    SmsInfo {
        loaded: sms.conversations.is_some(),
        conversations,
        threads,
        contacts,
        sending,
    }
}

fn command_item(command: &UserCommand) -> CommandItem {
    CommandItem {
        id: command.id.clone(),
        name: command.name.clone(),
        line: command.line.clone(),
    }
}

pub(crate) fn remote_input(input: Input) -> RemoteInput {
    match input {
        Input::Move { dx, dy } => RemoteInput::Move { dx, dy },
        Input::Scroll { x, y } => RemoteInput::Scroll { dx: x, dy: y },
        Input::Click { button, press } => RemoteInput::Click {
            button: match button {
                Button::Left => MouseButton::Left,
                Button::Right => MouseButton::Right,
                Button::Middle => MouseButton::Middle,
            },
            phase: match press {
                Press::Click => ClickPhase::Click,
                Press::Double => ClickPhase::Double,
                Press::Down => ClickPhase::Down,
                Press::Up => ClickPhase::Up,
            },
        },
        Input::Text { text, .. } => RemoteInput::KeyText { text },
        Input::Key {
            key: Key::F5,
            modifiers: _,
        } => RemoteInput::Presenter {
            action: PresenterCommand::Start,
        },
        Input::Key { key, modifiers } => RemoteInput::SpecialKey {
            key: match key {
                Key::Enter => SpecialKeyKind::Enter,
                Key::Backspace => SpecialKeyKind::Backspace,
                Key::Tab => SpecialKeyKind::Tab,
                Key::Escape => SpecialKeyKind::Escape,
                Key::Left => SpecialKeyKind::Left,
                Key::Right => SpecialKeyKind::Right,
                Key::Up => SpecialKeyKind::Up,
                Key::Down => SpecialKeyKind::Down,
                Key::Home => SpecialKeyKind::Home,
                Key::End => SpecialKeyKind::End,
                Key::PageUp => SpecialKeyKind::PageUp,
                Key::PageDown => SpecialKeyKind::PageDown,
                Key::Delete => SpecialKeyKind::Delete,
                Key::F5 => unreachable!(),
            },
            ctrl: modifiers.ctrl,
            alt: modifiers.alt,
            shift: modifiers.shift,
            meta: modifiers.meta,
        },
        Input::Spot(point) => RemoteInput::PresenterSpot {
            point: point.map(|(x, y)| SpotPoint { x, y }),
        },
    }
}

/// The snapshot of a worker as the app gets it.
pub(crate) fn snapshot(published: &Published, now: Now) -> CoreSnapshot {
    let view: &Snapshot = &published.snapshot;
    let devices = view
        .devices
        .iter()
        .map(|device| {
            let peer = view.peer(&device.id);
            let battery = peer
                .battery
                .filter(|_| view.features.battery)
                .and_then(|battery| {
                    Some(BatteryInfo {
                        level: battery.level?.min(100),
                        charging: battery.charging == Some(true),
                        low: battery.low == Some(true),
                    })
                });
            let connectivity = peer
                .connectivity
                .clone()
                .filter(|_| view.features.battery)
                .map(|connectivity| ConnectivityInfo {
                    network: connectivity.network,
                    signal: connectivity.signal.filter(|signal| *signal <= 4),
                });
            let media = peer
                .media
                .as_ref()
                .filter(|_| view.features.media)
                .map(|media| media_info(media, &peer, now));
            let volume = peer
                .volume
                .filter(|_| view.features.volume)
                .map(|volume| VolumeInfo {
                    level: volume.level.map(|level| level.min(100)),
                    muted: volume.muted,
                });
            let sms = if view.features.sms {
                sms_info(&peer.sms)
            } else {
                SmsInfo::default()
            };
            let (handles, sends) = peer
                .capabilities
                .as_ref()
                .map(|capabilities| (capabilities.incoming.clone(), capabilities.outgoing.clone()))
                .unwrap_or_default();
            DeviceInfo {
                id: hex(&device.id),
                short_id: short_id(&device.id),
                name: device.name.clone(),
                platform: peer
                    .capabilities
                    .as_ref()
                    .map(|capabilities| capabilities.platform.clone()),
                battery,
                connectivity,
                handles,
                sends,
                media,
                volume,
                allowed_control: view.control.contains(&device.id),
                sms,
            }
        })
        .collect();
    CoreSnapshot {
        generation: published.generation,
        name: view.name.clone(),
        device_id: hex(&view.device_id),
        short_id: short_id(&view.device_id),
        relay_url: view.relay_url.clone(),
        status: status(&view.status, now),
        paused: view.pause.all,
        devices,
        pairing: pairing(&view.pairing, published.paired, now),
        last_activity: view.last_activity.map(|activity| LastActivity {
            direction: match activity.direction {
                Direction::Sent => TextDirection::Sent,
                Direction::Received => TextDirection::Received,
            },
            at_ms: system_ms(activity.at),
        }),
        features: switches(&view.features),
        commands: view.commands.iter().map(command_item).collect(),
        ringing: view.ringing.as_ref().map(|device| hex(device)),
        transfers: view.transfers.iter().map(transfer).collect(),
    }
}

fn player(player: &Player, peer: &PeerInfo) -> PlayerInfo {
    PlayerInfo {
        id: player.id.clone(),
        app: player.app.clone(),
        title: player.title.clone(),
        artist: player.artist.clone(),
        album: player.album.clone(),
        playing: player.playing,
        position_ms: player.position,
        length_ms: player.length,
        can_play: player.can_play,
        can_pause: player.can_pause,
        can_next: player.can_next,
        can_previous: player.can_previous,
        can_seek: player.can_seek,
        volume: player.volume.filter(|volume| *volume <= 100),
        art: peer.cover(player).and(player.art.clone()),
    }
}

/// A cover as the app gets it: its bytes are copied once, here.
pub(crate) fn cover_image(cover: &Cover) -> CoverImage {
    CoverImage {
        mime: cover.mime.clone(),
        data: cover.data.to_vec(),
    }
}

/// One of this Mac's players as the worker takes it.
pub(crate) fn local_player(local: LocalPlayer) -> weft_client::media::LocalPlayer {
    let length = local.length_ms.filter(|length| *length > 0);
    weft_client::media::LocalPlayer {
        player: Player {
            id: local.id,
            app: local.app,
            title: local.title,
            artist: local.artist,
            album: local.album,
            playing: local.playing,
            position: local
                .position_ms
                .map(|position| length.map_or(position, |length| position.min(length))),
            length,
            can_play: local.can_play,
            can_pause: local.can_pause,
            can_next: local.can_next,
            can_previous: local.can_previous,
            can_seek: local.can_seek && length.is_some(),
            volume: None,
            art: None,
        },
        cover: local.cover.map(|cover| Cover {
            mime: cover.mime,
            data: cover.data.into(),
        }),
    }
}

/// The inverse of [`MediaControl`]'s use in [`crate::WeftCore::media_command`].
fn media_control(action: MediaAction) -> MediaControl {
    match action {
        MediaAction::Play => MediaControl::Play,
        MediaAction::Pause => MediaControl::Pause,
        MediaAction::PlayPause => MediaControl::PlayPause,
        MediaAction::Next => MediaControl::Next,
        MediaAction::Previous => MediaControl::Previous,
        MediaAction::Seek(position_ms) => MediaControl::Seek { position_ms },
        MediaAction::SetVolume(level) => MediaControl::SetVolume { level },
    }
}

fn media_info(media: &PeerMedia, peer: &PeerInfo, now: Now) -> MediaInfo {
    let current = media.current().and_then(|shown| {
        let index = media
            .players
            .iter()
            .position(|player| std::ptr::eq(player, shown))?;
        u32::try_from(index).ok()
    });
    MediaInfo {
        players: media
            .players
            .iter()
            .map(|shown| player(shown, peer))
            .collect(),
        received_at_ms: now.unix_ms(media.at),
        current,
    }
}

/// The switches the app shows, out of all the core has.
pub(crate) fn switches(features: &Features) -> FeatureSwitches {
    FeatureSwitches {
        files: features.files,
        links: features.links,
        notifications: features.notifications,
        battery: features.battery,
        ring: features.ring,
        calls: features.calls,
        media: features.media,
        volume: features.volume,
        sms: features.sms,
        commands: features.commands,
    }
}

pub(crate) fn failure_kind(failure: Failure) -> TransferFailureKind {
    match failure {
        Failure::Rejected(Some(RejectReason::Disabled)) => TransferFailureKind::Disabled,
        Failure::Rejected(Some(RejectReason::TooLarge)) => TransferFailureKind::TooLargeForReceiver,
        Failure::Rejected(Some(RejectReason::NoSpace)) => TransferFailureKind::NoSpace,
        Failure::Rejected(Some(RejectReason::Busy)) => TransferFailureKind::Busy,
        Failure::Rejected(None) => TransferFailureKind::Refused,
        Failure::NoProgress => TransferFailureKind::NoProgress,
        Failure::Disk => TransferFailureKind::Disk,
        Failure::TooLarge => TransferFailureKind::TooLarge,
    }
}

fn transfer(view: &TransferView) -> TransferInfo {
    TransferInfo {
        id: hex(&view.id),
        device_id: hex(&view.device),
        outgoing: view.outgoing,
        name: view.name.clone(),
        size: view.size,
        done: view.done,
        stage: match view.stage {
            Stage::Waiting => TransferStage::Waiting,
            Stage::Running => TransferStage::Running,
            Stage::Done => TransferStage::Done,
            Stage::Cancelled => TransferStage::Cancelled,
            Stage::Failed(failure) => TransferStage::Failed {
                failure: failure_kind(failure),
            },
        },
    }
}

/// A notice of the worker as the app gets it.
pub(crate) fn notice(notice: Notice) -> CoreNotice {
    match notice {
        Notice::ClipboardImage { from, image } => CoreNotice::ClipboardImage {
            from: hex(&from),
            mime: image.mime.as_str().into(),
            bytes: image.bytes,
        },
        Notice::Ring { from, device } => CoreNotice::Ring {
            from: hex(&from),
            device,
        },
        Notice::RingEnded => CoreNotice::RingEnded,
        Notice::Ping {
            from,
            device,
            message,
        } => CoreNotice::Ping {
            from: hex(&from),
            device,
            message,
        },
        Notice::Link { from, device, url } => CoreNotice::Link {
            from: hex(&from),
            device,
            url,
        },
        Notice::FileReceived { from, device, path } => CoreNotice::FileReceived {
            from: hex(&from),
            device,
            path: path.to_string_lossy().into_owned(),
        },
        Notice::TransferFailed {
            device,
            name,
            outgoing,
            failure,
        } => CoreNotice::TransferFailed {
            device,
            name,
            outgoing,
            failure: failure_kind(failure),
        },
        Notice::Notification {
            from,
            device,
            notification,
        } => CoreNotice::Notification {
            from: hex(&from),
            device,
            id: notification.id,
            app: notification.app,
            title: notification.title,
            text: notification.text,
            dismissable: notification.dismissable,
            actions: notification.actions,
            reply: notification.reply,
            silent: notification.silent,
        },
        Notice::NotificationRemoved { from, id } => CoreNotice::NotificationRemoved {
            from: hex(&from),
            id,
        },
        Notice::Call { from, device, call } => CoreNotice::Call {
            from: hex(&from),
            device,
            event: call.event,
            number: call.number,
            name: call.name,
        },
        Notice::MediaCommand {
            from,
            player,
            action,
        } => CoreNotice::MediaCommand {
            from: hex(&from),
            player,
            control: media_control(action),
        },
        Notice::SetVolume { from, set } => CoreNotice::SetVolume {
            from: hex(&from),
            level: set.level,
            muted: set.muted,
        },
        Notice::SmsReceived {
            from,
            device,
            thread,
            sender,
            text,
            addresses,
        } => CoreNotice::SmsReceived {
            from: hex(&from),
            device,
            thread,
            sender,
            text,
            addresses,
        },
        Notice::CommandRan { from, device, name } => CoreNotice::CommandRan {
            from: hex(&from),
            device,
            name,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;
    use std::time::Duration;

    use weft_client::features::Features;
    use weft_client::packets::{Battery, Capabilities};
    use weft_client::sync::Pause;
    use weft_client::worker::{Activity, Device, PeerInfo};

    use super::*;

    const PHONE: [u8; 16] = [0xab; 16];
    const LAPTOP: [u8; 16] = [0x01; 16];

    fn view() -> Snapshot {
        let mut peers = BTreeMap::new();
        peers.insert(
            PHONE,
            PeerInfo {
                capabilities: Some(Capabilities {
                    platform: "android".into(),
                    incoming: vec!["ring".into(), "ping".into()],
                    outgoing: vec!["battery".into()],
                }),
                battery: Some(Battery {
                    level: Some(87),
                    charging: Some(true),
                    low: None,
                }),
                ..PeerInfo::default()
            },
        );
        Snapshot {
            status: Status::Connected,
            name: "Studio".into(),
            device_id: [0x42; 16],
            relay_url: "wss://weft.frohub.eu/v1".into(),
            devices: vec![
                Device {
                    id: PHONE,
                    name: Some("Pixel 8".into()),
                },
                Device {
                    id: LAPTOP,
                    name: None,
                },
            ],
            peers,
            pause: Pause::default(),
            features: Features::default(),
            control: BTreeSet::new(),
            commands: Vec::new(),
            pairing: PairingState::Idle,
            last_activity: None,
            ringing: None,
            transfers: Vec::new(),
        }
    }

    fn published(view: Snapshot) -> Published {
        Published {
            generation: 7,
            snapshot: Arc::new(view),
            paired: None,
        }
    }

    fn fixed_now() -> Now {
        Now {
            instant: Instant::now(),
            system: UNIX_EPOCH + Duration::from_secs(1_800_000_000),
        }
    }

    #[test]
    fn devices_carry_their_platform_and_battery() {
        let mapped = snapshot(&published(view()), fixed_now());
        assert_eq!(mapped.generation, 7);
        assert_eq!(mapped.name, "Studio");
        assert_eq!(mapped.device_id, "42".repeat(16));
        assert_eq!(mapped.status, ConnectionStatus::Connected);
        let phone = &mapped.devices[0];
        assert_eq!(phone.id, "ab".repeat(16));
        assert_eq!(phone.short_id, short_id(&PHONE));
        assert_eq!(phone.name.as_deref(), Some("Pixel 8"));
        assert_eq!(phone.platform.as_deref(), Some("android"));
        assert_eq!(
            phone.battery,
            Some(BatteryInfo {
                level: 87,
                charging: true,
                low: false
            })
        );
        assert_eq!(phone.handles, ["ring", "ping"]);
        // Nothing told yet: no platform, no battery.
        let laptop = &mapped.devices[1];
        assert_eq!(
            (laptop.name.as_ref(), laptop.platform.as_ref()),
            (None, None)
        );
        assert_eq!(laptop.battery, None);
    }

    #[test]
    fn the_battery_is_hidden_when_its_feature_is_off() {
        let mut view = view();
        view.features.battery = false;
        let mapped = snapshot(&published(view), fixed_now());
        assert_eq!(mapped.devices[0].battery, None);
    }

    #[test]
    fn instants_become_wall_clock_milliseconds() {
        let now = fixed_now();
        let mut view = view();
        view.pairing = PairingState::ShowingCode {
            code: "WEFT1:abc".into(),
            expires_at: now.instant + Duration::from_secs(300),
        };
        view.status = Status::Offline {
            reason: "the relay closed the connection".into(),
            retry_at: now.instant + Duration::from_millis(1500),
        };
        view.last_activity = Some(Activity {
            direction: Direction::Received,
            at: now.system - Duration::from_secs(60),
        });
        let mapped = snapshot(&published(view), now);
        let base = 1_800_000_000_000;
        assert_eq!(
            mapped.pairing,
            Pairing::ShowingCode {
                code: "WEFT1:abc".into(),
                expires_at_ms: base + 300_000
            }
        );
        assert_eq!(
            mapped.status,
            ConnectionStatus::Offline {
                reason: "the relay closed the connection".into(),
                retry_at_ms: base + 1500
            }
        );
        assert_eq!(
            mapped.last_activity,
            Some(LastActivity {
                direction: TextDirection::Received,
                at_ms: base - 60_000
            })
        );
    }

    #[test]
    fn pairing_results_name_the_device_and_the_failure() {
        let mut view = view();
        view.pairing = PairingState::Success { device: PHONE };
        assert_eq!(
            snapshot(&published(view.clone()), fixed_now()).pairing,
            Pairing::Paired {
                device_id: "ab".repeat(16),
                invited: false,
            }
        );
        let invited = Published {
            generation: 8,
            snapshot: Arc::new(view.clone()),
            paired: Some(PairingKind::Invited),
        };
        assert_eq!(
            snapshot(&invited, fixed_now()).pairing,
            Pairing::Paired {
                device_id: "ab".repeat(16),
                invited: true,
            }
        );
        let joined_other = Published {
            generation: 9,
            snapshot: Arc::new(view.clone()),
            paired: Some(PairingKind::JoinedOther),
        };
        assert_eq!(
            snapshot(&joined_other, fixed_now()).pairing,
            Pairing::Paired {
                device_id: "ab".repeat(16),
                invited: true,
            }
        );
        view.pairing = PairingState::Failed(PairingFailure::Other("boom".into()));
        assert_eq!(
            snapshot(&published(view), fixed_now()).pairing,
            Pairing::Failed {
                failure: PairingFailureKind::Other {
                    message: "boom".into()
                }
            }
        );
    }

    #[test]
    fn notices_keep_their_sender() {
        assert_eq!(
            notice(Notice::Ping {
                from: PHONE,
                device: Some("Pixel 8".into()),
                message: None,
            }),
            CoreNotice::Ping {
                from: "ab".repeat(16),
                device: Some("Pixel 8".into()),
                message: None
            }
        );
        assert_eq!(notice(Notice::RingEnded), CoreNotice::RingEnded);
    }
}
