//! The worker's snapshot and notices as UniFFI records.
//!
//! Instants become milliseconds since the Unix epoch, ids become lowercase
//! hex, and nothing borrows from the worker.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use weft_cli::{hex, short_id};
use weft_client::worker::{
    Direction, Notice, PairingFailure, PairingState, Published, Snapshot, Status,
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
    /// The packet types it acts on (`incoming` of its capabilities).
    pub handles: Vec<String>,
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
    /// Paired with this device id.
    Paired {
        device_id: String,
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
}

/// Something the user should see or hear, as the worker reported it.
/// `from` is a device id in hex, `device` its name when known. This
/// version of the app shows none of them yet; later ones will.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum CoreNotice {
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
        reason: String,
    },
    Notification {
        from: String,
        device: Option<String>,
        id: String,
        app: String,
        title: String,
        text: String,
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
    MediaCommand {
        from: String,
        player: String,
        action: String,
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

fn pairing(pairing: &PairingState, now: Now) -> Pairing {
    match pairing {
        PairingState::Idle => Pairing::Idle,
        PairingState::ShowingCode { code, expires_at } => Pairing::ShowingCode {
            code: code.clone(),
            expires_at_ms: now.unix_ms(*expires_at),
        },
        PairingState::Joining => Pairing::Joining,
        PairingState::Success { device } => Pairing::Paired {
            device_id: hex(device),
        },
        PairingState::Failed(reason) => Pairing::Failed {
            failure: failure(reason),
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
            DeviceInfo {
                id: hex(&device.id),
                short_id: short_id(&device.id),
                name: device.name.clone(),
                platform: peer
                    .capabilities
                    .as_ref()
                    .map(|capabilities| capabilities.platform.clone()),
                battery,
                handles: peer
                    .capabilities
                    .map(|capabilities| capabilities.incoming)
                    .unwrap_or_default(),
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
        pairing: pairing(&view.pairing, now),
        last_activity: view.last_activity.map(|activity| LastActivity {
            direction: match activity.direction {
                Direction::Sent => TextDirection::Sent,
                Direction::Received => TextDirection::Received,
            },
            at_ms: system_ms(activity.at),
        }),
    }
}

/// A notice of the worker as the app gets it.
pub(crate) fn notice(notice: Notice) -> CoreNotice {
    match notice {
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
            reason: format!("{failure:?}"),
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
            action: format!("{action:?}"),
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
                device_id: "ab".repeat(16)
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
