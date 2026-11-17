//! The feature switches of `docs/features.md` section 9 this app has, and
//! the capabilities they give (section 2); with the devices allowed to
//! control this computer and the commands they may run, they are the
//! [`Settings`] kept in `features.json` in the state directory.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::packets::{Capabilities, CommandItem, PLATFORM};

const FILE_NAME: &str = "features.json";

/// The packet types of a file transfer, both ways.
const FILE_PACKETS: &[&str] = &[
    "file.offer",
    "file.accept",
    "file.reject",
    "file.ack",
    "file.done",
    "file.cancel",
];

/// What a computer sends about the phone's notifications.
const NOTIFICATION_ANSWERS: &[&str] = &[
    "notification.dismiss",
    "notification.action",
    "notification.reply",
    "notification.request",
];

/// What the phone answers about its messages and contacts.
const SMS_ANSWERS: &[&str] = &[
    "sms.conversations",
    "sms.messages",
    "sms.sent",
    "sms.received",
    "contacts",
];

/// What a computer asks of the phone's messages and contacts.
const SMS_REQUESTS: &[&str] = &[
    "sms.conversations.request",
    "sms.messages.request",
    "sms.send",
    "contacts.request",
];

/// What another device may do to this computer's pointer and keyboard.
const INPUT_PACKETS: &[&str] = &["input.pointer", "input.click", "input.key", "presenter"];

/// Every switch is on by default. A switch missing from the file keeps its
/// default, so a later version can add one; one the file has that this
/// version no longer knows, like the old global `input`, is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Features {
    /// Send and receive files.
    pub files: bool,
    /// Open the links the other devices share.
    pub links: bool,
    /// Ring the other devices, and be rung.
    pub ring: bool,
    /// Show the battery and network of the other devices.
    pub battery: bool,
    /// Show the phone's notifications as toasts, and act on them.
    pub notifications: bool,
    /// Show the phone's incoming and missed calls.
    pub calls: bool,
    /// Share this computer's players, and control the other devices'.
    pub media: bool,
    /// Share this computer's volume, and control the other devices'.
    pub volume: bool,
    /// Read and send the phone's SMS, with its contacts to pick from. The
    /// phone has its own switch, off by default.
    pub sms: bool,
    /// Run the commands of [`Settings::commands`] for the other devices.
    pub commands: bool,
}

impl Default for Features {
    fn default() -> Features {
        Features {
            files: true,
            links: true,
            ring: true,
            battery: true,
            notifications: true,
            calls: true,
            media: true,
            volume: true,
            sms: true,
            commands: true,
        }
    }
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

impl Features {
    /// The switches saved in `dir`, or the defaults.
    pub fn load(dir: &Path) -> Features {
        Settings::load(dir).features
    }

    /// What this device announces to a device: a switched-off feature is
    /// in neither list. Pings and state requests always go both ways, and
    /// typing on the phone is always offered: the phone decides whether it
    /// types. Pointer, keyboard and slideshow input are taken only from a
    /// device allowed to `control` this computer, so only such a device is
    /// told about them.
    pub fn capabilities(&self, control: bool) -> Capabilities {
        let mut incoming = vec!["state.request", "ping"];
        let mut outgoing = vec!["state.request", "ping", "input.key"];
        if self.ring {
            incoming.extend(["ring", "ring.stop"]);
            outgoing.extend(["ring", "ring.stop"]);
        }
        if self.links {
            incoming.push("share.url");
            outgoing.push("share.url");
        }
        if self.files {
            incoming.extend(FILE_PACKETS);
            outgoing.extend(FILE_PACKETS);
        }
        if self.battery {
            incoming.extend(["battery", "connectivity"]);
        }
        if self.notifications {
            incoming.extend(["notification", "notification.removed"]);
            outgoing.extend(NOTIFICATION_ANSWERS);
        }
        if self.calls {
            incoming.push("call");
            outgoing.push("call.mute");
        }
        if self.media {
            incoming.extend(["media", "media.command"]);
            outgoing.extend(["media", "media.command"]);
        }
        if self.volume {
            incoming.extend(["volume", "volume.set"]);
            outgoing.extend(["volume", "volume.set"]);
        }
        if self.sms {
            incoming.extend(SMS_ANSWERS);
            outgoing.extend(SMS_REQUESTS);
        }
        if control {
            incoming.extend(INPUT_PACKETS);
        }
        if self.commands {
            incoming.push("command.run");
            outgoing.push("command.list");
        }
        Capabilities {
            platform: PLATFORM.into(),
            incoming: incoming.into_iter().map(String::from).collect(),
            outgoing: outgoing.into_iter().map(String::from).collect(),
        }
    }
}

/// A command the other devices may run on this computer: they only see
/// its id and name, and only ever ask for it by id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserCommand {
    pub id: String,
    pub name: String,
    /// Run as written here by [`crate::worker::Hooks::run_command`]: on
    /// Windows by `cmd /C`.
    pub line: String,
}

impl UserCommand {
    /// What `command.list` shows of it.
    pub fn item(&self) -> CommandItem {
        CommandItem {
            id: self.id.clone(),
            name: self.name.clone(),
        }
    }
}

/// Everything `features.json` holds. A member missing from the file keeps
/// its default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(flatten)]
    pub features: Features,
    /// The devices allowed to control this computer, by id in hex.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub control: BTreeSet<String>,
    /// Named apart from the `commands` switch, which shares the object.
    #[serde(rename = "commandList", skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<UserCommand>,
}

impl Settings {
    /// The settings saved in `dir`, or the defaults if there are none or
    /// the file cannot be read.
    pub fn load(dir: &Path) -> Settings {
        let Ok(text) = std::fs::read_to_string(path(dir)) else {
            return Settings::default();
        };
        serde_json::from_str(&text).unwrap_or_else(|err| {
            tracing::warn!(error = %err, "feature settings unreadable; using the defaults");
            Settings::default()
        })
    }

    /// Saves the settings in `dir`, through a temporary file so a crash
    /// leaves the old file whole.
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).expect("settings serialise");
        let temp = dir.join(format!("{FILE_NAME}.tmp"));
        std::fs::write(&temp, json)?;
        std::fs::rename(&temp, path(dir))
    }

    /// The devices allowed to control this computer.
    pub fn controllers(&self) -> BTreeSet<[u8; 16]> {
        self.control
            .iter()
            .filter_map(|id| weft_cli::unhex(id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_and_loaded() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Settings::load(dir.path()), Settings::default());
        let settings = Settings {
            features: Features {
                files: false,
                ..Features::default()
            },
            control: BTreeSet::from([weft_cli::hex(&[0xab; 16])]),
            commands: vec![UserCommand {
                id: "c1".into(),
                name: "Lock".into(),
                line: "rundll32.exe user32.dll,LockWorkStation".into(),
            }],
        };
        settings.save(dir.path()).unwrap();
        assert_eq!(Settings::load(dir.path()), settings);
        assert_eq!(Features::load(dir.path()), settings.features);
        assert_eq!(settings.controllers(), BTreeSet::from([[0xab; 16]]));

        // A missing switch keeps its default; a broken file gives them all.
        std::fs::write(path(dir.path()), r#"{"ring":false}"#).unwrap();
        let loaded = Settings::load(dir.path());
        assert!(!loaded.features.ring && loaded.features.files && loaded.features.sms);
        assert!(loaded.control.is_empty());
        // The old global input switch is read without harm.
        std::fs::write(path(dir.path()), r#"{"input":true,"files":false}"#).unwrap();
        assert!(!Settings::load(dir.path()).features.files);
        std::fs::write(path(dir.path()), "not json").unwrap();
        assert_eq!(Settings::load(dir.path()), Settings::default());
    }

    #[test]
    fn switched_off_features_leave_the_capabilities() {
        let all = Features::default().capabilities(false);
        assert_eq!(all.platform, PLATFORM);
        for kind in [
            "ring",
            "ring.stop",
            "share.url",
            "file.offer",
            "battery",
            "notification",
            "notification.removed",
            "call",
            "media",
            "media.command",
            "volume",
            "volume.set",
            "sms.conversations",
            "sms.received",
            "contacts",
            "command.run",
        ] {
            assert!(all.handles(kind), "{kind}");
        }
        assert!(all.sends("file.offer") && all.sends("ring"));
        assert!(all.sends("ping") && all.sends("share.url"));
        assert!(!all.sends("battery") && !all.sends("notification"));
        for kind in [
            "notification.reply",
            "call.mute",
            "media",
            "volume.set",
            "sms.send",
            "contacts.request",
            "command.list",
        ] {
            assert!(all.sends(kind), "{kind}");
        }
        // Input is offered only to a device allowed to control this
        // computer; typing on the phone does not wait for it.
        assert!(!all.handles("input.pointer") && !all.handles("input.key"));
        assert!(!all.handles("presenter") && all.sends("input.key"));
        let allowed = Features::default().capabilities(true);
        assert!(allowed.handles("input.pointer") && allowed.handles("input.click"));
        assert!(allowed.handles("input.key") && allowed.handles("presenter"));
        assert!(allowed.sends("input.key"));
        assert_eq!(allowed.outgoing, all.outgoing);

        let none = Features {
            files: false,
            links: false,
            ring: false,
            battery: false,
            notifications: false,
            calls: false,
            media: false,
            volume: false,
            sms: false,
            commands: false,
        };
        assert_eq!(
            none.capabilities(true).incoming[2..],
            ["input.pointer", "input.click", "input.key", "presenter"]
        );
        let none = none.capabilities(false);
        assert_eq!(none.incoming, ["state.request", "ping"]);
        assert_eq!(none.outgoing, ["state.request", "ping", "input.key"]);
    }
}
