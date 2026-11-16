//! The JSON packets of `docs/features.md` this app knows, sections 2 to 8.
//!
//! Parsing is tolerant, as the contract asks: members this app does not
//! know are ignored, a packet of a type it does not know is
//! [`Parsed::Unknown`], and only a packet that is not a JSON object with a
//! string `type`, or a known type with members of the wrong kind, is
//! [`Parsed::Invalid`].

use serde::{Deserialize, Serialize};

/// The `platform` this app announces.
pub const PLATFORM: &str = "windows";

/// The largest file a transfer may carry: 4 GiB.
pub const MAX_FILE_SIZE: u64 = 4 << 30;

/// The longest URL a `share.url` may carry, in bytes.
pub const MAX_URL_LEN: usize = 2048;

/// Not `Eq`: pointer moves and the presenter spot carry fractions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Packet {
    #[serde(rename = "capabilities")]
    Capabilities(Capabilities),
    #[serde(rename = "state.request")]
    StateRequest {},
    #[serde(rename = "battery")]
    Battery(Battery),
    #[serde(rename = "connectivity")]
    Connectivity(Connectivity),
    #[serde(rename = "ring")]
    Ring {},
    #[serde(rename = "ring.stop")]
    RingStop {},
    #[serde(rename = "ping")]
    Ping {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "share.url")]
    ShareUrl { url: String },
    #[serde(rename = "file.offer")]
    FileOffer(FileOffer),
    #[serde(rename = "file.accept")]
    FileAccept { id: String, offset: u64 },
    #[serde(rename = "file.reject")]
    FileReject { id: String, reason: String },
    #[serde(rename = "file.ack")]
    FileAck { id: String, offset: u64 },
    #[serde(rename = "file.done")]
    FileDone { id: String },
    #[serde(rename = "file.cancel")]
    FileCancel { id: String },
    #[serde(rename = "notification")]
    Notification(Notification),
    #[serde(rename = "notification.removed")]
    NotificationRemoved { id: String },
    #[serde(rename = "notification.dismiss")]
    NotificationDismiss { id: String },
    #[serde(rename = "notification.action")]
    NotificationAction { id: String, action: String },
    #[serde(rename = "notification.reply")]
    NotificationReply { id: String, text: String },
    #[serde(rename = "notification.request")]
    NotificationRequest {},
    #[serde(rename = "call")]
    Call(Call),
    #[serde(rename = "call.mute")]
    CallMute {},
    #[serde(rename = "media")]
    Media {
        #[serde(default)]
        players: Vec<Player>,
    },
    #[serde(rename = "media.command")]
    MediaCommand(MediaCommand),
    #[serde(rename = "volume")]
    Volume(Volume),
    #[serde(rename = "volume.set")]
    VolumeSet(VolumeSet),
    #[serde(rename = "sms.conversations.request")]
    SmsConversationsRequest {},
    #[serde(rename = "sms.conversations")]
    SmsConversations {
        #[serde(default)]
        conversations: Vec<Conversation>,
    },
    #[serde(rename = "sms.messages.request")]
    SmsMessagesRequest(SmsMessagesRequest),
    #[serde(rename = "sms.messages")]
    SmsMessages {
        thread: i64,
        #[serde(default)]
        messages: Vec<SmsMessage>,
    },
    #[serde(rename = "sms.send")]
    SmsSend(SmsSend),
    #[serde(rename = "sms.sent")]
    SmsSent(SmsSent),
    #[serde(rename = "sms.received")]
    SmsReceived(SmsReceived),
    #[serde(rename = "contacts.request")]
    ContactsRequest {},
    #[serde(rename = "contacts")]
    Contacts(Contacts),
    #[serde(rename = "input.pointer")]
    InputPointer(InputPointer),
    #[serde(rename = "input.click")]
    InputClick(InputClick),
    #[serde(rename = "input.key")]
    InputKey(InputKey),
    #[serde(rename = "presenter")]
    Presenter(Presenter),
    #[serde(rename = "command.list")]
    CommandList {
        #[serde(default)]
        commands: Vec<CommandItem>,
    },
    #[serde(rename = "command.run")]
    CommandRun { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub platform: String,
    #[serde(default)]
    pub incoming: Vec<String>,
    #[serde(default)]
    pub outgoing: Vec<String>,
}

impl Capabilities {
    /// Whether the device acts on packets of type `kind`.
    pub fn handles(&self, kind: &str) -> bool {
        self.incoming.iter().any(|known| known == kind)
    }

    /// Whether the device may send packets of type `kind`.
    pub fn sends(&self, kind: &str) -> bool {
        self.outgoing.iter().any(|known| known == kind)
    }
}

/// A missing member means "unknown".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Battery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charging: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub low: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connectivity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileOffer {
    pub id: String,
    pub name: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
}

/// A notification of the phone. Missing texts are empty, missing flags off.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: String,
    #[serde(default)]
    pub app: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<u64>,
    #[serde(default)]
    pub dismissable: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<String>,
    #[serde(default)]
    pub reply: bool,
    #[serde(default)]
    pub silent: bool,
}

/// What a `call` says happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallEvent {
    Ringing,
    Talking,
    Missed,
    Ended,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Call {
    /// `None` for an event this app does not know.
    pub fn event(&self) -> Option<CallEvent> {
        match self.event.as_str() {
            "ringing" => Some(CallEvent::Ringing),
            "talking" => Some(CallEvent::Talking),
            "missed" => Some(CallEvent::Missed),
            "ended" => Some(CallEvent::Ended),
            _ => None,
        }
    }

    /// Who calls: the contact's name, else the number.
    pub fn caller(&self) -> Option<&str> {
        fn present(text: &Option<String>) -> Option<&str> {
            text.as_deref().filter(|text| !text.trim().is_empty())
        }
        present(&self.name).or_else(|| present(&self.number))
    }
}

/// A media player, as `media` lists them. Times are in milliseconds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Player {
    pub id: String,
    pub app: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<u64>,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_seek: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<u8>,
}

/// What a `media.command` asks a player to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaAction {
    Play,
    Pause,
    PlayPause,
    Next,
    Previous,
    /// To this position, in milliseconds.
    Seek(u64),
    /// To this volume, 0 to 100.
    SetVolume(u8),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaCommand {
    pub player: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<u64>,
}

impl MediaCommand {
    pub fn new(player: &str, action: MediaAction) -> MediaCommand {
        let (action, value) = match action {
            MediaAction::Play => ("play", None),
            MediaAction::Pause => ("pause", None),
            MediaAction::PlayPause => ("playPause", None),
            MediaAction::Next => ("next", None),
            MediaAction::Previous => ("previous", None),
            MediaAction::Seek(position) => ("seek", Some(position)),
            MediaAction::SetVolume(volume) => ("setVolume", Some(u64::from(volume))),
        };
        MediaCommand {
            player: player.into(),
            action: action.into(),
            value,
        }
    }

    /// `None` for an action this app does not know, or a seek or volume
    /// without a value in range.
    pub fn action(&self) -> Option<MediaAction> {
        Some(match self.action.as_str() {
            "play" => MediaAction::Play,
            "pause" => MediaAction::Pause,
            "playPause" => MediaAction::PlayPause,
            "next" => MediaAction::Next,
            "previous" => MediaAction::Previous,
            "seek" => MediaAction::Seek(self.value?),
            "setVolume" => {
                let volume = u8::try_from(self.value?).ok().filter(|v| *v <= 100)?;
                MediaAction::SetVolume(volume)
            }
            _ => return None,
        })
    }
}

/// An output volume. A missing member is unknown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Volume {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
}

/// A change of volume; a missing member stays as it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeSet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
}

/// An SMS thread of the phone. `names` follows `addresses`, `None` where
/// no contact matches.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    pub thread: i64,
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(default)]
    pub names: Vec<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(default)]
    pub time: i64,
    #[serde(default)]
    pub unread: bool,
}

impl Conversation {
    /// Who the thread is with: the contacts' names, else the numbers.
    pub fn title(&self) -> String {
        let people: Vec<&str> = self
            .addresses
            .iter()
            .enumerate()
            .map(|(index, address)| {
                self.names
                    .get(index)
                    .and_then(|name| name.as_deref())
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or(address)
            })
            .collect();
        people.join(", ")
    }
}

/// One SMS. `status` is only set on an outgoing one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmsMessage {
    pub id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub time: i64,
    #[serde(default)]
    pub outgoing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmsMessagesRequest {
    pub thread: i64,
    /// Only messages older than this time, in ms since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmsSend {
    pub addresses: Vec<String>,
    pub text: String,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmsSent {
    pub request_id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A new SMS: its thread next to the members of the message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmsReceived {
    pub thread: i64,
    #[serde(flatten)]
    pub message: SmsMessage,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub name: String,
    #[serde(default)]
    pub numbers: Vec<String>,
}

/// One part of the address book: `part` of `parts`, from 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contacts {
    #[serde(default)]
    pub contacts: Vec<Contact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<u32>,
}

/// A pointer move in pixels, or a scroll in wheel notches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputPointer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dx: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dy: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_y: Option<f64>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputClick {
    pub button: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub double: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub down: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub up: bool,
}

/// Text to type, or a named key, with the modifiers held around it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputKey {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub ctrl: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub alt: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub shift: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub meta: bool,
}

impl InputKey {
    pub fn text(text: impl Into<String>) -> InputKey {
        InputKey {
            text: Some(text.into()),
            ..InputKey::default()
        }
    }

    pub fn key(key: &str) -> InputKey {
        InputKey {
            key: Some(key.into()),
            ..InputKey::default()
        }
    }
}

/// Where the presenter's spot is, from -1 to 1 on each axis, y downwards.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Spot {
    pub x: f64,
    pub y: f64,
}

/// A slide action, or the spot: `pointer` is `Some(None)` for
/// `"pointer": null`, which hides it, and `None` when absent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Presenter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub pointer: Option<Option<Spot>>,
}

/// A member that is there, even as `null`.
fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// A command of this computer, as `command.list` shows it to the others.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandItem {
    pub id: String,
    pub name: String,
}

/// Why a receiver refuses a file, as `file.reject` puts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    Disabled,
    TooLarge,
    NoSpace,
    Busy,
}

impl RejectReason {
    pub fn as_str(self) -> &'static str {
        match self {
            RejectReason::Disabled => "disabled",
            RejectReason::TooLarge => "tooLarge",
            RejectReason::NoSpace => "noSpace",
            RejectReason::Busy => "busy",
        }
    }

    /// `None` for a reason this app does not know.
    pub fn parse(reason: &str) -> Option<RejectReason> {
        [
            RejectReason::Disabled,
            RejectReason::TooLarge,
            RejectReason::NoSpace,
            RejectReason::Busy,
        ]
        .into_iter()
        .find(|known| known.as_str() == reason)
    }
}

/// What a received packet turned out to be.
#[derive(Debug, Clone, PartialEq)]
pub enum Parsed {
    Known(Packet),
    /// A well-formed packet of a type this app does not know, ignored.
    Unknown(String),
    /// Not a JSON object with a string `type`, or a known type with members
    /// of the wrong kind: dropped.
    Invalid,
}

/// The types [`Packet`] covers, for telling an unknown type from a known
/// type with bad members.
const KNOWN: &[&str] = &[
    "capabilities",
    "state.request",
    "battery",
    "connectivity",
    "ring",
    "ring.stop",
    "ping",
    "share.url",
    "file.offer",
    "file.accept",
    "file.reject",
    "file.ack",
    "file.done",
    "file.cancel",
    "notification",
    "notification.removed",
    "notification.dismiss",
    "notification.action",
    "notification.reply",
    "notification.request",
    "call",
    "call.mute",
    "media",
    "media.command",
    "volume",
    "volume.set",
    "sms.conversations.request",
    "sms.conversations",
    "sms.messages.request",
    "sms.messages",
    "sms.send",
    "sms.sent",
    "sms.received",
    "contacts.request",
    "contacts",
    "input.pointer",
    "input.click",
    "input.key",
    "presenter",
    "command.list",
    "command.run",
];

pub fn parse(json: &str) -> Parsed {
    let Ok(serde_json::Value::Object(object)) = serde_json::from_str(json) else {
        return Parsed::Invalid;
    };
    let Some(kind) = object.get("type").and_then(|kind| kind.as_str()) else {
        return Parsed::Invalid;
    };
    if !KNOWN.contains(&kind) {
        return Parsed::Unknown(kind.to_owned());
    }
    match serde_json::from_value(serde_json::Value::Object(object)) {
        Ok(packet) => Parsed::Known(packet),
        Err(_) => Parsed::Invalid,
    }
}

impl Packet {
    pub fn to_json(&self) -> String {
        // A packet of strings, numbers and lists always serialises.
        serde_json::to_string(self).expect("a packet serialises")
    }

    /// The packet's `type`, as capabilities list it.
    pub fn kind(&self) -> &'static str {
        let value = serde_json::to_value(self).expect("a packet serialises");
        let kind = value["type"].as_str().unwrap_or_default();
        KNOWN.iter().find(|known| **known == kind).unwrap_or(&"")
    }
}

/// A transfer id as packets write it: 32 lowercase hex characters.
pub fn transfer_id(id: &[u8; 16]) -> String {
    weft_cli::hex(id)
}

/// The inverse of [`transfer_id`]; uppercase is accepted.
pub fn parse_transfer_id(text: &str) -> Option<[u8; 16]> {
    weft_cli::unhex(text)
}

/// Whether `url` may be opened from a `share.url`: http or https, at most
/// [`MAX_URL_LEN`] bytes, no whitespace or control characters, and a host.
pub fn is_shareable_url(url: &str) -> bool {
    if url.is_empty() || url.len() > MAX_URL_LEN {
        return false;
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let lower = url.to_ascii_lowercase();
    let rest = match lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
    {
        Some(rest) => rest,
        None => return false,
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or("");
    !host.is_empty() && !host.starts_with(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_packets_parse_with_unknown_members_ignored() {
        assert_eq!(
            parse(r#"{"type":"battery","level":82,"charging":true,"low":false,"extra":1}"#),
            Parsed::Known(Packet::Battery(Battery {
                level: Some(82),
                charging: Some(true),
                low: Some(false),
            }))
        );
        // Missing members are unknown, not zero.
        assert_eq!(
            parse(r#"{"type":"battery"}"#),
            Parsed::Known(Packet::Battery(Battery::default()))
        );
        assert_eq!(
            parse(r#"{"type":"connectivity","network":"LTE","signal":3}"#),
            Parsed::Known(Packet::Connectivity(Connectivity {
                network: Some("LTE".into()),
                signal: Some(3),
            }))
        );
        assert_eq!(
            parse(r#"{"type":"ring","why":"lost"}"#),
            Parsed::Known(Packet::Ring {})
        );
        assert_eq!(
            parse(r#"{"type":"state.request"}"#),
            Parsed::Known(Packet::StateRequest {})
        );
        assert_eq!(
            parse(r#"{"type":"ping"}"#),
            Parsed::Known(Packet::Ping { message: None })
        );
        assert_eq!(
            parse(r#"{"type":"file.offer","id":"00","name":"a.txt","size":3}"#),
            Parsed::Known(Packet::FileOffer(FileOffer {
                id: "00".into(),
                name: "a.txt".into(),
                size: 3,
                mime: None,
            }))
        );
        assert_eq!(
            parse(
                r#"{"type":"capabilities","platform":"android","incoming":["ring"],"outgoing":["battery"],"version":2}"#
            ),
            Parsed::Known(Packet::Capabilities(Capabilities {
                platform: "android".into(),
                incoming: vec!["ring".into()],
                outgoing: vec!["battery".into()],
            }))
        );
    }

    #[test]
    fn unknown_and_invalid_packets() {
        assert_eq!(
            parse(r#"{"type":"sftp.list","path":"/"}"#),
            Parsed::Unknown("sftp.list".into())
        );
        for invalid in [
            "",
            "[]",
            "\"ring\"",
            "{}",
            r#"{"type":7}"#,
            r#"{"type":"battery","level":"full"}"#,
            r#"{"type":"battery","level":300}"#,
            r#"{"type":"file.accept","id":"00"}"#,
            r#"{"type":"share.url"}"#,
            "{not json",
        ] {
            assert_eq!(parse(invalid), Parsed::Invalid, "{invalid}");
        }
    }

    #[test]
    fn packets_serialise_as_the_contract_writes_them() {
        assert_eq!(
            Packet::StateRequest {}.to_json(),
            r#"{"type":"state.request"}"#
        );
        assert_eq!(Packet::Ring {}.to_json(), r#"{"type":"ring"}"#);
        assert_eq!(Packet::RingStop {}.to_json(), r#"{"type":"ring.stop"}"#);
        assert_eq!(
            Packet::FileAck {
                id: "ab".into(),
                offset: 131072
            }
            .to_json(),
            r#"{"type":"file.ack","id":"ab","offset":131072}"#
        );
        assert_eq!(
            Packet::FileOffer(FileOffer {
                id: "ab".into(),
                name: "a.txt".into(),
                size: 0,
                mime: None
            })
            .to_json(),
            r#"{"type":"file.offer","id":"ab","name":"a.txt","size":0}"#
        );
        let capabilities = Packet::Capabilities(Capabilities {
            platform: PLATFORM.into(),
            incoming: vec!["ring".into()],
            outgoing: vec![],
        });
        assert_eq!(parse(&capabilities.to_json()), Parsed::Known(capabilities));
    }

    #[test]
    fn notifications_and_calls_parse() {
        let parsed = parse(
            r#"{"type":"notification","id":"0|app|1","app":"Chat","appId":"org.chat","title":"Ann","text":"Hi","time":1700000000000,"dismissable":true,"actions":["Mark read"],"reply":true,"silent":false,"icon":"x"}"#,
        );
        assert_eq!(
            parsed,
            Parsed::Known(Packet::Notification(Notification {
                id: "0|app|1".into(),
                app: "Chat".into(),
                app_id: Some("org.chat".into()),
                title: "Ann".into(),
                text: "Hi".into(),
                time: Some(1_700_000_000_000),
                dismissable: true,
                actions: vec!["Mark read".into()],
                reply: true,
                silent: false,
            }))
        );
        // Only the id is needed.
        assert_eq!(
            parse(r#"{"type":"notification","id":"a"}"#),
            Parsed::Known(Packet::Notification(Notification {
                id: "a".into(),
                ..Notification::default()
            }))
        );
        assert_eq!(parse(r#"{"type":"notification"}"#), Parsed::Invalid);
        assert_eq!(
            Packet::NotificationReply {
                id: "a".into(),
                text: "ok".into()
            }
            .to_json(),
            r#"{"type":"notification.reply","id":"a","text":"ok"}"#
        );
        assert_eq!(
            Packet::NotificationRequest {}.to_json(),
            r#"{"type":"notification.request"}"#
        );
        assert_eq!(Packet::CallMute {}.to_json(), r#"{"type":"call.mute"}"#);

        let Parsed::Known(Packet::Call(call)) =
            parse(r#"{"type":"call","event":"ringing","number":"+33 6","name":" "}"#)
        else {
            panic!("not a call");
        };
        assert_eq!(call.event(), Some(CallEvent::Ringing));
        // A blank name gives way to the number.
        assert_eq!(call.caller(), Some("+33 6"));
        let Parsed::Known(Packet::Call(call)) = parse(r#"{"type":"call","event":"held"}"#) else {
            panic!("not a call");
        };
        assert_eq!((call.event(), call.caller()), (None, None));
    }

    #[test]
    fn media_and_volume_round_trip() {
        let player = Player {
            id: "spotify".into(),
            app: "Spotify".into(),
            title: "Song".into(),
            playing: true,
            position: Some(1000),
            length: Some(180_000),
            can_play: true,
            can_pause: true,
            can_seek: true,
            ..Player::default()
        };
        let media = Packet::Media {
            players: vec![player],
        };
        let json = media.to_json();
        assert!(json.contains(r#""canSeek":true"#) && json.contains(r#""canNext":false"#));
        assert_eq!(parse(&json), Parsed::Known(media));
        // Missing members: unknown or off.
        assert_eq!(
            parse(r#"{"type":"media","players":[{"id":"a"}]}"#),
            Parsed::Known(Packet::Media {
                players: vec![Player {
                    id: "a".into(),
                    ..Player::default()
                }]
            })
        );

        for (action, json) in [
            (
                MediaAction::PlayPause,
                r#"{"player":"a","action":"playPause"}"#,
            ),
            (
                MediaAction::Seek(5000),
                r#"{"player":"a","action":"seek","value":5000}"#,
            ),
            (
                MediaAction::SetVolume(40),
                r#"{"player":"a","action":"setVolume","value":40}"#,
            ),
        ] {
            let command = MediaCommand::new("a", action);
            assert_eq!(serde_json::to_string(&command).unwrap(), json);
            assert_eq!(command.action(), Some(action));
        }
        let command = |action: &str, value| MediaCommand {
            player: "a".into(),
            action: action.into(),
            value,
        };
        assert_eq!(command("seek", None).action(), None);
        assert_eq!(command("setVolume", Some(101)).action(), None);
        assert_eq!(command("shuffle", None).action(), None);

        assert_eq!(
            parse(r#"{"type":"volume","level":30,"muted":false}"#),
            Parsed::Known(Packet::Volume(Volume {
                level: Some(30),
                muted: Some(false)
            }))
        );
        assert_eq!(
            Packet::VolumeSet(VolumeSet {
                level: None,
                muted: Some(true)
            })
            .to_json(),
            r#"{"type":"volume.set","muted":true}"#
        );
    }

    #[test]
    fn transfer_ids_round_trip() {
        let id = [0xab; 16];
        let text = transfer_id(&id);
        assert_eq!(text, "ab".repeat(16));
        assert_eq!(parse_transfer_id(&text), Some(id));
        assert_eq!(parse_transfer_id(&text.to_uppercase()), Some(id));
        assert_eq!(parse_transfer_id("ab"), None);
        assert_eq!(parse_transfer_id(&"zz".repeat(16)), None);
        assert_eq!(parse_transfer_id(&"é".repeat(16)), None);
    }

    #[test]
    fn sms_and_contacts_parse() {
        let Parsed::Known(Packet::SmsConversations { conversations }) = parse(
            r#"{"type":"sms.conversations","conversations":[{"thread":7,"addresses":["+33 6 00","+33 7 00"],"names":["Ann",null],"snippet":"See you","time":1700000000000,"unread":true}]}"#,
        ) else {
            panic!("not conversations");
        };
        assert_eq!(conversations[0].title(), "Ann, +33 7 00");
        assert!(conversations[0].unread);

        // The thread sits next to the message's own members.
        let received = parse(
            r#"{"type":"sms.received","thread":7,"id":42,"address":"+33 6 00","body":"Hi","time":5,"outgoing":false}"#,
        );
        let expected = Packet::SmsReceived(SmsReceived {
            thread: 7,
            message: SmsMessage {
                id: 42,
                address: Some("+33 6 00".into()),
                body: "Hi".into(),
                time: 5,
                outgoing: false,
                status: None,
            },
        });
        assert_eq!(received, Parsed::Known(expected.clone()));
        assert_eq!(parse(&expected.to_json()), Parsed::Known(expected));

        let send = Packet::SmsSend(SmsSend {
            addresses: vec!["+33 6 00".into()],
            text: "On my way".into(),
            request_id: "r1".into(),
        });
        assert_eq!(
            send.to_json(),
            r#"{"type":"sms.send","addresses":["+33 6 00"],"text":"On my way","requestId":"r1"}"#
        );
        assert_eq!(
            parse(r#"{"type":"sms.sent","requestId":"r1","ok":false,"error":"radio off"}"#),
            Parsed::Known(Packet::SmsSent(SmsSent {
                request_id: "r1".into(),
                ok: false,
                error: Some("radio off".into()),
            }))
        );
        assert_eq!(
            Packet::SmsMessagesRequest(SmsMessagesRequest {
                thread: 7,
                before: Some(100),
                limit: Some(50)
            })
            .to_json(),
            r#"{"type":"sms.messages.request","thread":7,"before":100,"limit":50}"#
        );
        assert_eq!(
            parse(
                r#"{"type":"contacts","contacts":[{"name":"Ann","numbers":["1"]}],"part":1,"parts":2}"#
            ),
            Parsed::Known(Packet::Contacts(Contacts {
                contacts: vec![Contact {
                    name: "Ann".into(),
                    numbers: vec!["1".into()],
                }],
                part: Some(1),
                parts: Some(2),
            }))
        );
        assert_eq!(
            parse(r#"{"type":"sms.messages","messages":[]}"#),
            Parsed::Invalid
        );
    }

    #[test]
    fn input_presenter_and_commands_parse() {
        assert_eq!(
            parse(r#"{"type":"input.pointer","dx":1.5,"dy":-2}"#),
            Parsed::Known(Packet::InputPointer(InputPointer {
                dx: Some(1.5),
                dy: Some(-2.0),
                ..InputPointer::default()
            }))
        );
        assert_eq!(
            Packet::InputKey(InputKey::key("backspace")).to_json(),
            r#"{"type":"input.key","key":"backspace"}"#
        );
        assert_eq!(
            Packet::InputKey(InputKey::text("hé")).to_json(),
            r#"{"type":"input.key","text":"hé"}"#
        );
        // Absent, null and a spot are three different things.
        let presenter = |json| match parse(json) {
            Parsed::Known(Packet::Presenter(presenter)) => presenter,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            presenter(r#"{"type":"presenter","action":"next"}"#).pointer,
            None
        );
        assert_eq!(
            presenter(r#"{"type":"presenter","pointer":null}"#).pointer,
            Some(None)
        );
        assert_eq!(
            presenter(r#"{"type":"presenter","pointer":{"x":0.5,"y":-1}}"#).pointer,
            Some(Some(Spot { x: 0.5, y: -1.0 }))
        );
        assert_eq!(
            Packet::CommandList {
                commands: vec![CommandItem {
                    id: "a1".into(),
                    name: "Lock".into()
                }]
            }
            .to_json(),
            r#"{"type":"command.list","commands":[{"id":"a1","name":"Lock"}]}"#
        );
        assert_eq!(
            parse(r#"{"type":"command.run","id":"a1"}"#),
            Parsed::Known(Packet::CommandRun { id: "a1".into() })
        );
        assert_eq!(parse(r#"{"type":"input.click"}"#), Parsed::Invalid);
    }

    #[test]
    fn reject_reasons() {
        for reason in ["disabled", "tooLarge", "noSpace", "busy"] {
            assert_eq!(RejectReason::parse(reason).unwrap().as_str(), reason);
        }
        assert_eq!(RejectReason::parse("other"), None);
    }

    #[test]
    fn only_web_links_are_shareable() {
        assert!(is_shareable_url("https://example.com/a?b=c#d"));
        assert!(is_shareable_url("HTTP://example.com"));
        assert!(is_shareable_url("https://user@example.com:8080/"));
        for refused in [
            "",
            "file:///C:/Windows/system32/calc.exe",
            "javascript:alert(1)",
            "ms-settings:privacy",
            "https://",
            "https:///path",
            "https://exa mple.com",
            "https://example.com/\nline",
            "ftp://example.com",
        ] {
            assert!(!is_shareable_url(refused), "{refused}");
        }
        let long = format!("https://example.com/{}", "a".repeat(MAX_URL_LEN));
        assert!(!is_shareable_url(&long));
    }
}
