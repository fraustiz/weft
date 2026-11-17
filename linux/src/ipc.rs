//! Authenticated loopback TCP IPC (`127.0.0.1:0` + `<state_dir>/daemon.json`
//! with `0o600` permissions on Unix) between the running `weft-desktop` daemon
//! and control CLI subcommands.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use weft_cli::hex;
use weft_client::features::{Features, UserCommand};
use weft_client::packets::{Battery, Connectivity, Conversation, SmsMessage, Volume};

use crate::update::UpdateSnapshot;

pub const DAEMON_FILE: &str = "daemon.json";
const DAEMON_TEMP_FILE: &str = "daemon.json.tmp";
const MAX_IPC_LINE_BYTES: usize = 512 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Connection details persisted in `<state_dir>/daemon.json`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonEndpoint {
    pub addr: String,
    pub token: String,
}

/// Redacts the authentication token from debug output.
impl std::fmt::Debug for DaemonEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonEndpoint")
            .field("addr", &self.addr)
            .field("token", &"[redacted]")
            .finish()
    }
}

/// Generates a random 32-byte (64-hex character) authentication token.
pub fn generate_token() -> io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(hex(&bytes))
}

/// Constant-time comparison of 64-hex authentication tokens.
pub fn verify_token(expected: &str, provided: &str) -> bool {
    let a = expected.as_bytes();
    let b = provided.as_bytes();
    if a.len() != 64 || b.len() != 64 {
        return false;
    }
    let mut diff: u8 = 0;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Writes `<state_dir>/daemon.json` atomically with `0o600` permissions on Unix.
pub fn write_daemon_endpoint(
    state_dir: &Path,
    addr: SocketAddr,
    token: &str,
) -> io::Result<PathBuf> {
    create_private_dir(state_dir)?;
    let endpoint = DaemonEndpoint {
        addr: addr.to_string(),
        token: token.to_owned(),
    };
    let mut json = serde_json::to_vec_pretty(&endpoint).expect("endpoint serializes");
    json.push(b'\n');

    let temp = state_dir.join(DAEMON_TEMP_FILE);
    let target = state_dir.join(DAEMON_FILE);
    let _ = fs::remove_file(&temp);

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    file.write_all(&json)?;
    file.sync_all()?;
    drop(file);

    fs::rename(&temp, &target)?;
    Ok(target)
}

/// Reads `<state_dir>/daemon.json`.
pub fn read_daemon_endpoint(state_dir: &Path) -> io::Result<DaemonEndpoint> {
    let path = state_dir.join(DAEMON_FILE);
    let raw = fs::read_to_string(&path).map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("weft-desktop daemon is not running (no {})", path.display()),
            )
        } else {
            err
        }
    })?;
    let endpoint: DaemonEndpoint = serde_json::from_str(&raw)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    if endpoint.token.len() != 64 || !endpoint.token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid token in daemon.json",
        ));
    }
    Ok(endpoint)
}

pub fn remove_daemon_endpoint(state_dir: &Path) {
    let _ = fs::remove_file(state_dir.join(DAEMON_FILE));
    let _ = fs::remove_file(state_dir.join(DAEMON_TEMP_FILE));
}

pub(crate) fn create_private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

/// Authenticated envelope sent over the local TCP IPC socket.
#[derive(Serialize, Deserialize)]
pub struct AuthenticatedRequest {
    pub token: String,
    pub request: IpcRequest,
}

impl std::fmt::Debug for AuthenticatedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthenticatedRequest")
            .field("token", &"[redacted]")
            .finish()
    }
}

/// Control requests supported by the running daemon.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum IpcRequest {
    Status,
    Devices,
    PairStart,
    PairWait {
        timeout_ms: u64,
    },
    PairCancel,
    Join {
        code: String,
    },
    Remove {
        device: String,
    },
    Rename {
        name: String,
    },
    SetPaused {
        paused: bool,
    },
    SendClipboardNow,
    SendFiles {
        device: String,
        files: Vec<PathBuf>,
    },
    ShareUrl {
        device: String,
        url: String,
    },
    Ring {
        device: String,
    },
    StopRing {
        device: String,
    },
    FoundIt,
    Ping {
        device: String,
        message: Option<String>,
    },
    Messages {
        device: String,
        thread: Option<i64>,
    },
    SendSms {
        device: String,
        to: String,
        text: String,
    },
    CommandsList,
    CommandsAdd {
        name: String,
        line: String,
    },
    CommandsRemove {
        query: String,
    },
    SetControl {
        device: String,
        allowed: bool,
    },
    GetFeatures,
    SetFeature {
        feature: String,
        enabled: bool,
    },
    GetRelay,
    SetRelay {
        url: String,
    },
    Leave,
    UpdateCheck,
    UpdateInstall,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub short_id: String,
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery: Option<Battery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connectivity: Option<Connectivity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<Volume>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub now_playing: Option<String>,
    pub control_allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferInfo {
    pub id: String,
    pub device_id: String,
    pub outgoing: bool,
    pub name: String,
    pub size: u64,
    pub done: u64,
    pub stage: String,
}

/// Complete daemon status payload, directly usable in Waybar (`return-type: "json"`),
/// Polybar scripts, and JSON tooling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusPayload {
    pub text: String,
    pub alt: String,
    pub class: String,
    pub tooltip: String,
    pub status: String,
    pub connected: bool,
    pub paused: bool,
    pub name: String,
    pub device_id: String,
    pub short_id: String,
    pub relay_url: String,
    pub devices: Vec<DeviceInfo>,
    pub features: Features,
    pub ringing: bool,
    pub transfers: Vec<TransferInfo>,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PairPollOutcome {
    Waiting {
        expires_in_secs: u64,
    },
    Success {
        device_id: String,
        short_id: String,
        name: Option<String>,
        kind: String,
    },
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingSmsView {
    pub request_id: String,
    pub addresses: Vec<String>,
    pub text: String,
    pub thread: Option<i64>,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagesPayload {
    pub device_id: String,
    pub device_name: Option<String>,
    pub conversations: Vec<Conversation>,
    pub thread: Option<i64>,
    pub messages: Vec<SmsMessage>,
    pub outbox: Vec<OutgoingSmsView>,
}

/// Response returned by the daemon over IPC.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IpcResponse {
    Error { message: String },
    Ack { message: String },
    Status { status: StatusPayload },
    Devices { devices: Vec<DeviceInfo> },
    PairStarted { code: String, expires_in_secs: u64 },
    PairOutcome { outcome: PairPollOutcome },
    Messages { payload: MessagesPayload },
    Commands { commands: Vec<UserCommand> },
    Features { features: Features },
    Relay { relay_url: String },
    Left { told_peers: bool },
    Update { snapshot: UpdateSnapshot },
}

/// Reads a single newline-delimited JSON message bounded to `MAX_IPC_LINE_BYTES`.
pub async fn read_json_line<R: tokio::io::AsyncRead + Unpin, T: serde::de::DeserializeOwned>(
    stream: R,
) -> io::Result<T> {
    let limited = stream.take((MAX_IPC_LINE_BYTES + 1) as u64);
    let mut reader = BufReader::new(limited);
    let mut line = Vec::new();
    let n = reader.read_until(b'\n', &mut line).await?;
    if n == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "IPC connection closed",
        ));
    }
    if line.len() > MAX_IPC_LINE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC message exceeds size limit",
        ));
    }
    serde_json::from_slice(&line)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
}

pub async fn write_json_line<W: tokio::io::AsyncWrite + Unpin, T: Serialize>(
    mut stream: W,
    value: &T,
) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    bytes.push(b'\n');
    stream.write_all(&bytes).await?;
    stream.flush().await
}

/// Client for communicating with the running daemon through `<state_dir>/daemon.json`.
#[derive(Clone)]
pub struct IpcClient {
    endpoint: DaemonEndpoint,
}

impl IpcClient {
    pub fn from_state_dir(state_dir: &Path) -> io::Result<Self> {
        Ok(Self {
            endpoint: read_daemon_endpoint(state_dir)?,
        })
    }

    pub fn new(endpoint: DaemonEndpoint) -> Self {
        Self { endpoint }
    }

    pub async fn request(&self, request: IpcRequest) -> io::Result<IpcResponse> {
        let stream = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(&self.endpoint.addr))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "IPC connect timed out"))?
            .map_err(|err| {
                io::Error::new(
                    err.kind(),
                    format!(
                        "could not connect to weft-desktop daemon at {}: {err}",
                        self.endpoint.addr
                    ),
                )
            })?;
        let (read_half, write_half) = stream.into_split();
        let auth = AuthenticatedRequest {
            token: self.endpoint.token.clone(),
            request,
        };
        write_json_line(write_half, &auth).await?;
        read_json_line(read_half).await
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn writes_and_verifies_daemon_endpoint_and_tokens() {
        let dir = TempDir::new().unwrap();
        let state_dir = dir.path().join("state");
        let token = generate_token().unwrap();
        assert_eq!(token.len(), 64);
        assert!(verify_token(&token, &token));
        assert!(!verify_token(&token, &"0".repeat(64)));
        assert!(!verify_token(&token, "short"));

        let addr: SocketAddr = "127.0.0.1:41234".parse().unwrap();
        let path = write_daemon_endpoint(&state_dir, addr, &token).unwrap();
        assert!(path.is_file());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let loaded = read_daemon_endpoint(&state_dir).unwrap();
        assert_eq!(loaded.addr, "127.0.0.1:41234");
        assert_eq!(loaded.token, token);
        assert!(!format!("{loaded:?}").contains(&token));

        remove_daemon_endpoint(&state_dir);
        assert!(read_daemon_endpoint(&state_dir).is_err());
    }
}
