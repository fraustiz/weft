//! Weft command-line client.
//!
//! Plays the role of a desktop device (`docs/protocol.md`): it keeps an
//! identity and a list of trusted devices on disk ([`state`]), and talks to
//! the relay to pair, receive and send clipboard text ([`session`]).

pub mod session;
pub mod state;

use std::path::PathBuf;

use weft_protocol::ControlError;

pub use session::{DropReason, Event, Next, PairOutcome, Session};
pub use state::{State, StateDir, TrustedDevice};

/// Everything that can go wrong in the client.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("state file {0} is invalid: {1}")]
    InvalidState(PathBuf, String),
    #[error("not initialised: no state in {0} (run `weft init --relay <url>`)")]
    NotInitialised(PathBuf),
    #[error("already initialised: {0} exists (use --force to replace the identity)")]
    AlreadyInitialised(PathBuf),
    #[error("invalid relay url {0:?}: {1}")]
    InvalidUrl(String, &'static str),
    #[error("the pairing code is for relay {code}, but this device uses {state}")]
    RelayUrlMismatch { state: String, code: String },
    #[error(transparent)]
    Protocol(#[from] weft_protocol::Error),
    #[error("websocket: {0}")]
    WebSocket(Box<tokio_tungstenite::tungstenite::Error>),
    #[error("the relay refused the authentication")]
    AuthRejected,
    #[error("the relay closed the connection")]
    Closed,
    #[error("timed out waiting for {0}")]
    Timeout(&'static str),
    #[error("relay error: {}", control_error_text(*.0))]
    Relay(ControlError),
    #[error("no trusted device matches {0:?}")]
    UnknownDevice(String),
    #[error("{0:?} matches more than one trusted device")]
    AmbiguousDevice(String),
    #[error(
        "the group is full ({} devices at most)",
        weft_protocol::MAX_GROUP_SIZE
    )]
    GroupFull,
    #[error("no default state directory (no home directory): pass --state or set WEFT_STATE")]
    NoStateDir,
    #[error("the pairing code was made by this device")]
    OwnCode,
    #[error("this device is in no group: join the other device instead (`weft join <code>`)")]
    NoGroup,
    #[error(
        "the device did not answer: it may already belong to a group (pair from that group instead)"
    )]
    NoAnswer,
    #[error("the inviting device did not send the welcome message in time")]
    NoWelcome,
}

impl From<tokio_tungstenite::tungstenite::Error> for Error {
    fn from(err: tokio_tungstenite::tungstenite::Error) -> Error {
        Error::WebSocket(Box::new(err))
    }
}

fn control_error_text(code: ControlError) -> &'static str {
    match code {
        ControlError::UnknownPairing => "unknown or expired pairing",
        ControlError::NotAllowed => "not allowed",
        ControlError::RateLimited => "rate limited",
        ControlError::GroupFull => "group full",
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Lowercase hex of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Parses exactly `N` bytes of hex (either case).
pub fn unhex<const N: usize>(text: &str) -> Option<[u8; N]> {
    fn nibble(digit: u8) -> Option<u8> {
        (digit as char).to_digit(16).map(|value| value as u8)
    }
    let (pairs, rest) = text.as_bytes().as_chunks::<2>();
    if pairs.len() != N || !rest.is_empty() {
        return None;
    }
    let mut out = [0u8; N];
    for (byte, [high, low]) in out.iter_mut().zip(pairs) {
        *byte = (nibble(*high)? << 4) | nibble(*low)?;
    }
    Some(out)
}

/// First 4 bytes of a device id in hex, for display.
pub fn short_id(device_id: &[u8; 16]) -> String {
    hex(&device_id[..4])
}

/// Default state directory: the platform data directory followed by `weft`.
///
/// `~/Library/Application Support/weft` on macOS, `%APPDATA%\weft` on
/// Windows, and `$XDG_DATA_HOME/weft` (or `~/.local/share/weft`) elsewhere.
pub fn default_state_dir() -> Option<PathBuf> {
    let env = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    if cfg!(target_os = "macos") {
        return env("HOME").map(|home| home.join("Library/Application Support/weft"));
    }
    if cfg!(windows) {
        return env("APPDATA").map(|dir| dir.join("weft"));
    }
    env("XDG_DATA_HOME")
        .or_else(|| env("HOME").map(|home| home.join(".local/share")))
        .map(|dir| dir.join("weft"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let bytes = [0x00, 0x7f, 0xab, 0xff];
        assert_eq!(hex(&bytes), "007fabff");
        assert_eq!(unhex::<4>("007FABff"), Some(bytes));
        assert_eq!(unhex::<4>("007fab"), None);
        assert_eq!(unhex::<2>("zz00"), None);
        assert_eq!(unhex::<2>("+f00"), None);
    }
}
