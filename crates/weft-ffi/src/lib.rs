//! The Weft client core for the Apple apps, through UniFFI.
//!
//! The Swift app gets one object, [`WeftCore`], over the worker of
//! `weft-client`: it starts and stops it, reads its [`Snapshot`] as plain
//! records and sends it commands. What the worker reports comes back
//! through a [`CoreDelegate`] that Swift implements.
//!
//! The clipboard rules stay in Rust: [`weft_client::sync::Sync`] decides
//! what is sent and what is written, over a [`Pasteboard`] that Swift
//! implements on `NSPasteboard`. Swift only gives the clock: it calls
//! [`WeftCore::poll_pasteboard`] on its main thread every half second, and
//! whenever [`CoreDelegate::clipboard_waiting`] says a received text waits.
//! So every pasteboard access happens on the main thread, and the Sync
//! never runs on two threads at once.
//!
//! Threads: the worker runs on its own thread, plus one thread per worker
//! that waits for its snapshots. Both call the delegate; the Swift side
//! hops to the main actor before touching the interface.
//!
//! Clipboard text, keys and pairing codes are never logged here.
//!
//! [`Snapshot`]: weft_client::worker::Snapshot

mod core;
mod group;
mod map;

pub use crate::core::{
    CoreDelegate, MAC_IMPLEMENTED, MAC_INCOMING, MAC_OUTGOING, Pasteboard, PasteboardContent,
    WeftCore,
};
pub use crate::map::{
    BatteryInfo, ConnectionStatus, ConnectivityInfo, CoreNotice, CoreSnapshot, CoverImage,
    DeviceInfo, FeatureSwitches, LastActivity, LocalPlayer, MediaControl, MediaInfo, Pairing,
    PairingFailureKind, PlayerInfo, PollOutcome, SendOutcome, TextDirection, TransferFailureKind,
    TransferInfo, TransferStage, VolumeInfo,
};

uniffi::setup_scaffolding!();

/// What the app gives the core to start.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CoreConfig {
    /// Where the identity, the trusted devices and the settings live.
    pub state_dir: String,
    /// Where received files go once complete.
    pub downloads_dir: String,
    /// The relay a new identity is created on; `None` for the public one.
    /// An existing identity keeps its relay.
    pub relay_url: Option<String>,
    /// The name a new identity starts with, such as the Mac's name in
    /// System Settings. `None`, or an invalid name, for the host name.
    pub default_name: Option<String>,
}

/// Why a call failed, in terms the app can put into words.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    /// The state could not be read or created, or the worker not started.
    #[error("could not start: {message}")]
    Start { message: String },
    /// Not a valid device name (`docs/protocol.md`, kind `0x06`).
    #[error("invalid device name")]
    InvalidName,
    /// Not a `ws://` or `wss://` url with a host, or `ws://` off this Mac.
    #[error("invalid relay url: {message}")]
    InvalidRelayUrl { message: String },
    /// The relay can only change while no other device is trusted: the
    /// group lives on the relay.
    #[error("leave the group before changing the relay")]
    InGroup,
    /// Not a device id this core knows.
    #[error("unknown device")]
    UnknownDevice,
    /// The state file could not be updated.
    #[error("{message}")]
    State { message: String },
    /// Not a link another device opens: only http and https, at most
    /// 2048 bytes.
    #[error("not a web link")]
    InvalidLink,
}

/// The relay a new installation uses.
#[uniffi::export]
pub fn default_relay_url() -> String {
    weft_client::worker::DEFAULT_RELAY_URL.to_owned()
}

/// The name to save for what was typed, surrounding spaces aside: the rule
/// names on the wire follow. The Swift app checks names as they are typed
/// with the same rule; this is the one the core enforces.
#[uniffi::export]
pub fn check_device_name(typed: &str) -> Result<String, CoreError> {
    let name = typed.trim();
    match weft_cli::state::sanitize_device_name(name) {
        Some(valid) if valid == name => Ok(valid),
        _ => Err(CoreError::InvalidName),
    }
}

/// Whether `url` is a link another device opens (`docs/features.md`
/// section 4): http or https, with a host, at most 2048 bytes, no spaces.
#[uniffi::export]
pub fn is_shareable_url(url: &str) -> bool {
    weft_client::packets::is_shareable_url(url.trim())
}

/// Checks a relay url the user typed: what the client accepts, and plain
/// `ws://` only towards this Mac (a development relay).
#[uniffi::export]
pub fn check_relay_url(url: &str) -> Result<String, CoreError> {
    let url = url.trim();
    let endpoint =
        weft_cli::session::relay_endpoint(url).map_err(|err| CoreError::InvalidRelayUrl {
            message: err.to_string(),
        })?;
    let host = endpoint.host.trim_start_matches('[').trim_end_matches(']');
    let loopback = matches!(host, "localhost" | "127.0.0.1" | "::1");
    if !url.starts_with("wss://") && !loopback {
        return Err(CoreError::InvalidRelayUrl {
            message: "use wss:// (ws:// is for a relay on this Mac)".into(),
        });
    }
    Ok(url.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_wire_rule() {
        assert_eq!(check_device_name("  MacBook Air ").unwrap(), "MacBook Air");
        assert_eq!(check_device_name(""), Err(CoreError::InvalidName));
        assert_eq!(check_device_name("a\tb"), Err(CoreError::InvalidName));
        assert_eq!(
            check_device_name(&"é".repeat(33)),
            Err(CoreError::InvalidName)
        );
        assert!(check_device_name(&"é".repeat(32)).is_ok());
    }

    #[test]
    fn relay_urls_need_tls_off_this_mac() {
        assert!(check_relay_url("wss://weft.frohub.eu/v1").is_ok());
        assert!(check_relay_url("ws://127.0.0.1:8095/v1").is_ok());
        assert!(check_relay_url("ws://localhost:8095/v1").is_ok());
        assert!(check_relay_url("ws://relay.example/v1").is_err());
        assert!(check_relay_url("https://relay.example/v1").is_err());
        assert!(check_relay_url("").is_err());
    }
}
