//! The feature switches of `docs/features.md` section 9 this app has, kept
//! in `features.json` in the state directory, and the capabilities they
//! give (section 2).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::packets::{Capabilities, PLATFORM};

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

/// Every switch is on by default. A switch missing from the file keeps its
/// default, so a later version can add one.
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
        }
    }
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

impl Features {
    /// The switches saved in `dir`, or the defaults if there are none or
    /// the file cannot be read.
    pub fn load(dir: &Path) -> Features {
        let Ok(text) = std::fs::read_to_string(path(dir)) else {
            return Features::default();
        };
        serde_json::from_str(&text).unwrap_or_else(|err| {
            tracing::warn!(error = %err, "feature settings unreadable; using the defaults");
            Features::default()
        })
    }

    /// Saves the switches in `dir`, through a temporary file so a crash
    /// leaves the old file whole.
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).expect("settings serialise");
        let temp = dir.join(format!("{FILE_NAME}.tmp"));
        std::fs::write(&temp, json)?;
        std::fs::rename(&temp, path(dir))
    }

    /// What this device announces: a switched-off feature is in neither
    /// list. Pings and state requests are always handled.
    pub fn capabilities(&self) -> Capabilities {
        let mut incoming = vec!["state.request", "ping"];
        let mut outgoing = vec!["state.request"];
        if self.ring {
            incoming.extend(["ring", "ring.stop"]);
            outgoing.extend(["ring", "ring.stop"]);
        }
        if self.links {
            incoming.push("share.url");
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
        Capabilities {
            platform: PLATFORM.into(),
            incoming: incoming.into_iter().map(String::from).collect(),
            outgoing: outgoing.into_iter().map(String::from).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_and_loaded() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Features::load(dir.path()), Features::default());
        let features = Features {
            files: false,
            ..Features::default()
        };
        features.save(dir.path()).unwrap();
        assert_eq!(Features::load(dir.path()), features);

        // A missing switch keeps its default; a broken file gives them all.
        std::fs::write(path(dir.path()), r#"{"ring":false}"#).unwrap();
        let loaded = Features::load(dir.path());
        assert!(!loaded.ring && loaded.files && loaded.links && loaded.battery);
        std::fs::write(path(dir.path()), "not json").unwrap();
        assert_eq!(Features::load(dir.path()), Features::default());
    }

    #[test]
    fn switched_off_features_leave_the_capabilities() {
        let all = Features::default().capabilities();
        assert_eq!(all.platform, "windows");
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
        ] {
            assert!(all.handles(kind), "{kind}");
        }
        assert!(all.sends("file.offer") && all.sends("ring"));
        assert!(!all.sends("battery") && !all.sends("notification"));
        for kind in ["notification.reply", "call.mute", "media", "volume.set"] {
            assert!(all.sends(kind), "{kind}");
        }

        let none = Features {
            files: false,
            links: false,
            ring: false,
            battery: false,
            notifications: false,
            calls: false,
            media: false,
            volume: false,
        }
        .capabilities();
        assert_eq!(none.incoming, ["state.request", "ping"]);
        assert_eq!(none.outgoing, ["state.request"]);
    }
}
