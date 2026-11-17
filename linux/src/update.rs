//! Self-updates from GitHub Releases (`linux-update.json` /
//! `weft-X.Y.Z-linux-x86_64`).
//!
//! Fetches over `rustls` + `tokio` HTTPS (with manual redirect validation
//! against the GitHub release host allowlist), verifies the pure-Rust FIPS
//! 180-4 SHA-256 digest and ELF64 `x86_64` header (`\x7FELF`, 64-bit
//! little-endian, `e_machine == 0x3E`), sets `0o755` permissions on Unix, and
//! atomically swaps the installed binary into place.

type ChunkCallback<'a> = dyn FnMut(Option<u64>, &[u8]) -> Result<(), UpdateError> + 'a;
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustls_pki_types::ServerName;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use weft_cli::hex;

/// Default URL of the Linux release manifest.
pub const DEFAULT_MANIFEST_URL: &str =
    "https://github.com/fraustiz/weft/releases/latest/download/linux-update.json";

/// Human-facing releases page.
pub const RELEASES_PAGE_URL: &str = "https://github.com/fraustiz/weft/releases/latest";

/// Version string embedded at build time (`WEFT_VERSION` or `CARGO_PKG_VERSION`).
pub const INSTALLED_VERSION_NAME: &str = env!("WEFT_VERSION_NAME");

/// Monotonic build number embedded at build time (`WEFT_VERSION_CODE` or `1`).
pub const INSTALLED_VERSION_CODE: u64 = parse_u64_const(env!("WEFT_VERSION_CODE"));

/// Maximum accepted byte size for `linux-update.json`.
pub const MAX_MANIFEST_BYTES: usize = 16 * 1024;

/// Maximum accepted byte size for `weft-X.Y.Z-linux-x86_64`.
pub const MAX_EXE_BYTES: u64 = 100 * 1024 * 1024;

const MAX_REDIRECTS: usize = 5;
const MAX_HEADER_BYTES: usize = 32 * 1024;
const BUFFER_LEN: usize = 64 * 1024;
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const PREFS_FILE: &str = "updates.json";
const STAGING_DIR: &str = "updates";
const ELF_MAGIC: &[u8; 4] = b"\x7FELF";
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const EM_X86_64: u16 = 0x003E;

const fn parse_u64_const(raw: &str) -> u64 {
    let bytes = raw.as_bytes();
    let mut index = 0;
    let mut value: u64 = 0;
    while index < bytes.len() {
        let digit = bytes[index];
        assert!(digit >= b'0' && digit <= b'9', "invalid WEFT_VERSION_CODE");
        value = value * 10 + (digit - b'0') as u64;
        index += 1;
    }
    assert!(value > 0, "WEFT_VERSION_CODE must be positive");
    value
}

/// Active manifest URL: `WEFT_UPDATE_URL` in debug builds when set, otherwise
/// [`DEFAULT_MANIFEST_URL`].
pub fn manifest_url() -> String {
    if cfg!(debug_assertions)
        && let Ok(url) = std::env::var("WEFT_UPDATE_URL")
        && !url.trim().is_empty()
    {
        return url.trim().to_owned();
    }
    DEFAULT_MANIFEST_URL.to_owned()
}

/// Pure-Rust FIPS 180-4 SHA-256 implementation.
#[derive(Clone)]
pub struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    total_bytes: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    const H0: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    pub fn new() -> Self {
        Self {
            state: Self::H0,
            buffer: [0u8; 64],
            total_bytes: 0,
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        let mut used = (self.total_bytes % 64) as usize;
        self.total_bytes = self.total_bytes.wrapping_add(data.len() as u64);
        if used > 0 {
            let take = (64 - used).min(data.len());
            self.buffer[used..used + take].copy_from_slice(&data[..take]);
            data = &data[take..];
            used += take;
            if used == 64 {
                let block = self.buffer;
                self.compress(&block);
            }
        }
        while data.len() >= 64 {
            let (block, rest) = data.split_at(64);
            self.compress(block.try_into().unwrap());
            data = rest;
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
        }
    }

    pub fn finish(mut self) -> [u8; 32] {
        let used = (self.total_bytes % 64) as usize;
        self.buffer[used] = 0x80;
        if used + 1 > 56 {
            self.buffer[used + 1..64].fill(0);
            let block = self.buffer;
            self.compress(&block);
            self.buffer[..56].fill(0);
        } else {
            self.buffer[used + 1..56].fill(0);
        }
        self.buffer[56..64].copy_from_slice(&(self.total_bytes.wrapping_mul(8)).to_be_bytes());
        let block = self.buffer;
        self.compress(&block);

        let mut out = [0u8; 32];
        for (chunk, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(self.state) {
            *chunk = word.to_be_bytes();
        }
        out
    }

    pub fn digest(data: &[u8]) -> [u8; 32] {
        let mut hasher = Self::new();
        hasher.update(data);
        hasher.finish()
    }

    pub fn hex_digest(data: &[u8]) -> String {
        hex(&Self::digest(data))
    }

    pub fn file(path: &Path) -> io::Result<String> {
        let mut file = File::open(path)?;
        let mut hasher = Self::new();
        let mut buf = vec![0u8; BUFFER_LEN];
        loop {
            let read = file.read(&mut buf)?;
            if read == 0 {
                break;
            }
            hasher.update(&buf[..read]);
        }
        Ok(hex(&hasher.finish()))
    }

    pub fn matches(path: &Path, expected: &str) -> io::Result<bool> {
        Ok(Self::file(path)?.eq_ignore_ascii_case(expected.trim()))
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (slot, chunk) in w[..16].iter_mut().zip(block.as_chunks::<4>().0) {
            *slot = u32::from_be_bytes(*chunk);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for (&word, &k) in w.iter().zip(&Self::K) {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k)
                .wrapping_add(word);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, add) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(add);
        }
    }
}

/// Published Linux release metadata read from `linux-update.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version_code: u64,
    pub version_name: String,
    pub url: String,
    pub sha256: String,
}

impl UpdateInfo {
    pub fn is_newer_than(&self, installed_code: u64) -> bool {
        self.version_code > installed_code
    }
}

/// Errors returned while checking, downloading, or installing an update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateError {
    Network(String),
    NoRelease,
    Http(u16),
    Invalid(String),
    BadHash,
    WriteProtected,
    Install(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::Network(reason) => write!(f, "network: {reason}"),
            UpdateError::NoRelease => f.write_str("no Linux release published yet"),
            UpdateError::Http(code) => write!(f, "HTTP {code}"),
            UpdateError::Invalid(reason) => write!(f, "invalid update: {reason}"),
            UpdateError::BadHash => f.write_str("sha256 mismatch"),
            UpdateError::WriteProtected => f.write_str("install folder is write-protected"),
            UpdateError::Install(reason) => write!(f, "install failed: {reason}"),
        }
    }
}

impl std::error::Error for UpdateError {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedUrl {
    scheme: String,
    host: String,
    ipv6: bool,
    port: u16,
    path_and_query: String,
}

impl ParsedUrl {
    fn parse(raw: &str) -> Option<Self> {
        if raw.is_empty() || raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return None;
        }
        let (scheme_raw, after_scheme) = raw.split_once("://")?;
        let scheme = scheme_raw.to_ascii_lowercase();
        let default_port = match scheme.as_str() {
            "https" => 443,
            "http" => 80,
            _ => return None,
        };
        let without_fragment = after_scheme.split('#').next()?;
        let (authority, path_and_query) = match without_fragment.find(['/', '?']) {
            Some(index) if without_fragment.as_bytes()[index] == b'/' => (
                &without_fragment[..index],
                without_fragment[index..].to_owned(),
            ),
            Some(index) => (
                &without_fragment[..index],
                format!("/{}", &without_fragment[index..]),
            ),
            None => (without_fragment, "/".to_owned()),
        };
        if authority.is_empty() || authority.contains('@') || authority.contains('\\') {
            return None;
        }
        let (host, ipv6, port_part) = if let Some(rest) = authority.strip_prefix('[') {
            let (v6, tail) = rest.split_once(']')?;
            if v6.is_empty()
                || !v6
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.')
            {
                return None;
            }
            let port_part = match tail {
                "" => None,
                _ => Some(tail.strip_prefix(':')?),
            };
            (v6.to_ascii_lowercase(), true, port_part)
        } else {
            let mut parts = authority.split(':');
            let host_raw = parts.next()?;
            let port_part = parts.next();
            if parts.next().is_some() {
                return None;
            }
            if host_raw.is_empty()
                || host_raw.starts_with('.')
                || host_raw.ends_with('.')
                || !host_raw
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
            {
                return None;
            }
            (host_raw.to_ascii_lowercase(), false, port_part)
        };
        let port = match port_part {
            Some(raw_port) => raw_port.parse::<u16>().ok().filter(|&p| p > 0)?,
            None => default_port,
        };
        Some(Self {
            scheme,
            host,
            ipv6,
            port,
            path_and_query,
        })
    }

    fn connect_addr(&self) -> String {
        if self.ipv6 {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    fn authority(&self) -> String {
        let default_port = if self.scheme == "https" { 443 } else { 80 };
        let host = if self.ipv6 {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == default_port {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }

    fn resolve_redirect(&self, location: &str) -> Option<Self> {
        let location = location.trim();
        if location.is_empty() || location.starts_with('\\') {
            return None;
        }
        let next = if location.contains("://") {
            Self::parse(location)?
        } else if location.starts_with("//") {
            Self::parse(&format!("{}:{location}", self.scheme))?
        } else if location.starts_with('/') {
            Self::parse(&format!("{}://{}{location}", self.scheme, self.authority()))?
        } else {
            let base_path = self.path_and_query.split('?').next().unwrap_or("/");
            let dir = match base_path.rfind('/') {
                Some(slash) => &base_path[..=slash],
                None => "/",
            };
            Self::parse(&format!(
                "{}://{}{dir}{location}",
                self.scheme,
                self.authority()
            ))?
        };
        if self.scheme == "https" && next.scheme != "https" {
            return None;
        }
        Some(next)
    }
}

/// Host allowlist for manifest and release binary downloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatePolicy {
    allowed_hosts: BTreeSet<String>,
    allow_http: bool,
}

impl UpdatePolicy {
    pub fn github() -> Self {
        let allowed_hosts = [
            "github.com",
            "objects.githubusercontent.com",
            "release-assets.githubusercontent.com",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        Self {
            allowed_hosts,
            allow_http: false,
        }
    }

    pub fn for_override(manifest_url: &str) -> Self {
        let mut policy = Self::github();
        if let Some(parsed) = ParsedUrl::parse(manifest_url) {
            policy.allowed_hosts.insert(parsed.host);
            policy.allow_http = true;
        }
        policy
    }

    pub fn allows(&self, url: &str) -> bool {
        ParsedUrl::parse(url).is_some_and(|parsed| self.allows_parsed(&parsed))
    }

    fn allows_parsed(&self, parsed: &ParsedUrl) -> bool {
        if parsed.scheme != "https" && !(self.allow_http && parsed.scheme == "http") {
            return false;
        }
        self.allowed_hosts.contains(&parsed.host)
    }
}

/// Parses `linux-update.json` and validates its fields and download URL.
pub fn parse_manifest(json: &str, policy: &UpdatePolicy) -> Result<UpdateInfo, UpdateError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| UpdateError::Invalid("not a JSON object".into()))?;
    let Some(map) = value.as_object() else {
        return Err(UpdateError::Invalid("not a JSON object".into()));
    };
    let Some(code) = map.get("versionCode").and_then(|v| v.as_u64()) else {
        return Err(UpdateError::Invalid(
            "versionCode missing or not an integer".into(),
        ));
    };
    if code == 0 {
        return Err(UpdateError::Invalid("versionCode must be positive".into()));
    }
    let name = map
        .get("versionName")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| UpdateError::Invalid("versionName missing".into()))?;
    let url = map
        .get("url")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| UpdateError::Invalid("url missing".into()))?;
    if !policy.allows(url) {
        return Err(UpdateError::Invalid("url not allowed".into()));
    }
    let sha = map
        .get("sha256")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| UpdateError::Invalid("sha256 missing or malformed".into()))?;
    Ok(UpdateInfo {
        version_code: code,
        version_name: name.to_owned(),
        url: url.to_owned(),
        sha256: sha,
    })
}

/// Daily automatic check schedule.
pub struct UpdateSchedule;

impl UpdateSchedule {
    pub const INTERVAL_MS: u64 = 24 * 60 * 60 * 1000;

    pub fn is_due(auto_check: bool, last_check_ms: u64, now_ms: u64) -> bool {
        if !auto_check {
            return false;
        }
        if last_check_ms == 0 || last_check_ms > now_ms {
            return true;
        }
        now_ms - last_check_ms >= Self::INTERVAL_MS
    }
}

/// Verifies that `path` starts with a valid 64-bit little-endian `x86_64` ELF
/// executable header (`\x7FELF`, `ELFCLASS64`, `ELFDATA2LSB`, `ET_EXEC`/`ET_DYN`,
/// `e_machine == 0x3E`).
pub fn check_elf64_x86_64(path: &Path) -> Result<(), UpdateError> {
    let invalid = |why: &str| UpdateError::Invalid(why.to_owned());
    let mut file = File::open(path).map_err(|err| UpdateError::Install(err.to_string()))?;
    let mut hdr = [0u8; 64];
    file.read_exact(&mut hdr)
        .map_err(|_| invalid("ELF header truncated"))?;
    if &hdr[0..4] != ELF_MAGIC {
        return Err(invalid("missing ELF magic"));
    }
    if hdr[4] != ELFCLASS64 {
        return Err(invalid("ELF binary is not 64-bit"));
    }
    if hdr[5] != ELFDATA2LSB {
        return Err(invalid("ELF binary is not little-endian"));
    }
    if hdr[6] != EV_CURRENT {
        return Err(invalid("unsupported ELF version"));
    }
    let e_type = u16::from_le_bytes([hdr[16], hdr[17]]);
    if e_type != ET_EXEC && e_type != ET_DYN {
        return Err(invalid("ELF binary is not an executable"));
    }
    let e_machine = u16::from_le_bytes([hdr[18], hdr[19]]);
    if e_machine != EM_X86_64 {
        return Err(invalid("ELF executable is not x86_64"));
    }
    Ok(())
}

fn side_path(exe: &Path, suffix: &str) -> PathBuf {
    let mut name = exe.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn map_write_error(err: io::Error) -> UpdateError {
    if err.kind() == io::ErrorKind::PermissionDenied {
        UpdateError::WriteProtected
    } else {
        UpdateError::Install(err.to_string())
    }
}

fn set_executable_mode(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Atomically installs `staged` over `target_exe`, setting `0o755` permissions
/// on Unix before swapping into place.
pub fn replace_executable(staged: &Path, target_exe: &Path) -> Result<(), UpdateError> {
    replace_executable_inner(staged, target_exe, |from, to| fs::rename(from, to))
}

fn replace_executable_inner(
    staged: &Path,
    target_exe: &Path,
    activate: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> Result<(), UpdateError> {
    let new_path = side_path(target_exe, ".new");
    let old_path = side_path(target_exe, ".old");

    if new_path.exists() {
        fs::remove_file(&new_path).map_err(map_write_error)?;
    }
    if old_path.exists() {
        fs::remove_file(&old_path).map_err(map_write_error)?;
    }

    if let Err(err) = fs::rename(staged, &new_path) {
        if err.kind() == io::ErrorKind::PermissionDenied {
            return Err(UpdateError::WriteProtected);
        }
        fs::copy(staged, &new_path).map_err(map_write_error)?;
        let _ = fs::remove_file(staged);
    }

    set_executable_mode(&new_path).map_err(map_write_error)?;

    // On Unix, rename(2) atomically replaces `target_exe` in place even while
    // it is executing. Keep a hardlink/move fallback on non-Unix hosts so unit
    // tests pass identically on Windows and Linux.
    #[cfg(unix)]
    {
        if let Err(err) = activate(&new_path, target_exe) {
            let _ = fs::remove_file(&new_path);
            return Err(map_write_error(err));
        }
        Ok(())
    }

    #[cfg(not(unix))]
    {
        let existed = target_exe.exists();
        if existed && let Err(err) = fs::rename(target_exe, &old_path) {
            let _ = fs::remove_file(&new_path);
            return Err(map_write_error(err));
        }
        if let Err(err) = activate(&new_path, target_exe) {
            if existed {
                let _ = fs::rename(&old_path, target_exe);
            }
            let _ = fs::remove_file(&new_path);
            return Err(map_write_error(err));
        }
        let _ = fs::remove_file(&old_path);
        Ok(())
    }
}

/// Cleans up leftover `.new` / `.old` files and the `updates/` staging directory.
pub fn cleanup_old_executable(state_dir: &Path) {
    let _ = fs::remove_dir_all(state_dir.join(STAGING_DIR));
    if let Ok(exe) = std::env::current_exe() {
        let _ = fs::remove_file(side_path(&exe, ".new"));
        let _ = fs::remove_file(side_path(&exe, ".old"));
    }
}

fn tls_connector() -> TlsConnector {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("ring supports default TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}

#[derive(Debug)]
struct HttpHeaders {
    status: u16,
    location: Option<String>,
    content_length: Option<u64>,
    chunked: bool,
}

fn parse_http_headers(raw: &[u8]) -> Result<HttpHeaders, UpdateError> {
    let text = std::str::from_utf8(raw)
        .map_err(|_| UpdateError::Network("invalid HTTP headers".into()))?;
    let mut lines = text.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| UpdateError::Network("empty HTTP response".into()))?;
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| UpdateError::Network("malformed HTTP status line".into()))?;

    let mut location = None;
    let mut content_length = None;
    let mut chunked = false;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, val)) = line.split_once(':') {
            let name = name.trim();
            let val = val.trim();
            if name.eq_ignore_ascii_case("location") {
                location = Some(val.to_owned());
            } else if name.eq_ignore_ascii_case("content-length") {
                content_length = val.parse::<u64>().ok();
            } else if name.eq_ignore_ascii_case("transfer-encoding")
                && val.to_ascii_lowercase().contains("chunked")
            {
                chunked = true;
            }
        }
    }
    Ok(HttpHeaders {
        status: status_code,
        location,
        content_length,
        chunked,
    })
}

async fn read_headers<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<HttpHeaders, UpdateError> {
    let mut raw = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];
    while raw.len() < MAX_HEADER_BYTES {
        let n = reader
            .read(&mut byte)
            .await
            .map_err(|e| UpdateError::Network(e.to_string()))?;
        if n == 0 {
            return Err(UpdateError::Network("unexpected EOF in headers".into()));
        }
        raw.push(byte[0]);
        if raw.ends_with(b"\r\n\r\n") {
            return parse_http_headers(&raw);
        }
    }
    Err(UpdateError::Invalid("HTTP headers too large".into()))
}

async fn stream_body<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    headers: &HttpHeaders,
    max_bytes: u64,
    mut on_chunk: impl FnMut(&[u8]) -> Result<(), UpdateError>,
) -> Result<(), UpdateError> {
    if headers.chunked {
        let mut total: u64 = 0;
        let mut line = String::new();
        loop {
            line.clear();
            let read = tokio::io::AsyncBufReadExt::read_line(reader, &mut line)
                .await
                .map_err(|e| UpdateError::Network(e.to_string()))?;
            if read == 0 {
                return Err(UpdateError::Network(
                    "unexpected EOF in chunked body".into(),
                ));
            }
            let size_hex = line.trim().split(';').next().unwrap_or("");
            let chunk_len = usize::from_str_radix(size_hex, 16)
                .map_err(|_| UpdateError::Network("invalid chunk size".into()))?;
            if chunk_len == 0 {
                return Ok(());
            }
            total = total.saturating_add(chunk_len as u64);
            if total > max_bytes {
                return Err(UpdateError::Invalid("response exceeds size limit".into()));
            }
            let mut remaining = chunk_len;
            let mut buf = vec![0u8; BUFFER_LEN.min(chunk_len)];
            while remaining > 0 {
                let take = buf.len().min(remaining);
                reader
                    .read_exact(&mut buf[..take])
                    .await
                    .map_err(|e| UpdateError::Network(e.to_string()))?;
                on_chunk(&buf[..take])?;
                remaining -= take;
            }
            let mut crlf = [0u8; 2];
            reader
                .read_exact(&mut crlf)
                .await
                .map_err(|e| UpdateError::Network(e.to_string()))?;
        }
    } else {
        if let Some(len) = headers.content_length
            && len > max_bytes
        {
            return Err(UpdateError::Invalid("response exceeds size limit".into()));
        }
        let mut total: u64 = 0;
        let mut buf = vec![0u8; BUFFER_LEN];
        loop {
            if let Some(expected) = headers.content_length
                && total >= expected
            {
                break;
            }
            let to_read = match headers.content_length {
                Some(expected) => buf.len().min((expected - total) as usize),
                None => buf.len(),
            };
            let n = reader
                .read(&mut buf[..to_read])
                .await
                .map_err(|e| UpdateError::Network(e.to_string()))?;
            if n == 0 {
                if let Some(expected) = headers.content_length
                    && total < expected
                {
                    return Err(UpdateError::Network("truncated HTTP body".into()));
                }
                break;
            }
            total = total.saturating_add(n as u64);
            if total > max_bytes {
                return Err(UpdateError::Invalid("response exceeds size limit".into()));
            }
            on_chunk(&buf[..n])?;
        }
        Ok(())
    }
}

/// HTTPS/HTTP client using `rustls` + `tokio` with per-hop redirect allowlisting.
#[derive(Clone)]
pub struct UpdateClient {
    manifest_url: String,
    policy: UpdatePolicy,
}

impl UpdateClient {
    pub fn new(manifest_url: impl Into<String>, policy: UpdatePolicy) -> Self {
        Self {
            manifest_url: manifest_url.into(),
            policy,
        }
    }

    pub fn fetch_latest(&self) -> Result<UpdateInfo, UpdateError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| UpdateError::Network(e.to_string()))?;
        rt.block_on(self.fetch_latest_async())
    }

    pub async fn fetch_latest_async(&self) -> Result<UpdateInfo, UpdateError> {
        let mut body = Vec::new();
        self.get_stream(
            &self.manifest_url,
            MAX_MANIFEST_BYTES as u64,
            |_len, chunk| {
                body.extend_from_slice(chunk);
                Ok(())
            },
        )
        .await?;
        let text = String::from_utf8(body)
            .map_err(|_| UpdateError::Invalid("manifest is not UTF-8".into()))?;
        parse_manifest(&text, &self.policy)
    }

    pub fn download(
        &self,
        info: &UpdateInfo,
        target: &Path,
        progress: impl FnMut(u64, Option<u64>),
    ) -> Result<(), UpdateError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| UpdateError::Network(e.to_string()))?;
        rt.block_on(self.download_async(info, target, progress))
    }

    pub async fn download_async(
        &self,
        info: &UpdateInfo,
        target: &Path,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<(), UpdateError> {
        let res = self.download_inner(info, target, &mut progress).await;
        if res.is_err() {
            let _ = fs::remove_file(target);
        }
        res
    }

    async fn download_inner(
        &self,
        info: &UpdateInfo,
        target: &Path,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<(), UpdateError> {
        if !self.policy.allows(&info.url) {
            return Err(UpdateError::Invalid("url not allowed".into()));
        }
        let mut file = File::create(target).map_err(|err| UpdateError::Install(err.to_string()))?;
        let mut hasher = Sha256::new();
        let mut done: u64 = 0;
        let mut started = false;

        self.get_stream(&info.url, MAX_EXE_BYTES, |total, chunk| {
            if !started {
                started = true;
                progress(0, total);
            }
            done = done.saturating_add(chunk.len() as u64);
            hasher.update(chunk);
            file.write_all(chunk)
                .map_err(|err| UpdateError::Install(err.to_string()))?;
            progress(done, total);
            Ok(())
        })
        .await?;

        file.sync_all()
            .map_err(|err| UpdateError::Install(err.to_string()))?;
        drop(file);
        if hex(&hasher.finish()) != info.sha256 {
            return Err(UpdateError::BadHash);
        }
        Ok(())
    }

    async fn get_stream(
        &self,
        initial_url: &str,
        max_bytes: u64,
        mut on_chunk: impl FnMut(Option<u64>, &[u8]) -> Result<(), UpdateError>,
    ) -> Result<(), UpdateError> {
        let mut current = ParsedUrl::parse(initial_url)
            .ok_or_else(|| UpdateError::Invalid("invalid url".into()))?;
        if !self.policy.allows_parsed(&current) {
            return Err(UpdateError::Invalid("url not allowed".into()));
        }
        let connector = tls_connector();

        for hop in 0..=MAX_REDIRECTS {
            let outcome = tokio::time::timeout(
                HTTP_TIMEOUT,
                Self::single_request(&connector, &current, max_bytes, &mut on_chunk),
            )
            .await
            .map_err(|_| UpdateError::Network("request timed out".into()))??;

            match outcome {
                HopOutcome::Done => return Ok(()),
                HopOutcome::Redirect(location) => {
                    if hop == MAX_REDIRECTS {
                        return Err(UpdateError::Invalid("too many redirects".into()));
                    }
                    let next = current
                        .resolve_redirect(&location)
                        .ok_or_else(|| UpdateError::Invalid("disallowed redirect".into()))?;
                    if !self.policy.allows_parsed(&next) {
                        return Err(UpdateError::Invalid(
                            "redirected outside the allowed hosts".into(),
                        ));
                    }
                    current = next;
                }
            }
        }
        Err(UpdateError::Invalid("too many redirects".into()))
    }

    async fn single_request(
        connector: &TlsConnector,
        url: &ParsedUrl,
        max_bytes: u64,
        on_chunk: &mut ChunkCallback<'_>,
    ) -> Result<HopOutcome, UpdateError> {
        let tcp = TcpStream::connect(url.connect_addr())
            .await
            .map_err(|e| UpdateError::Network(e.to_string()))?;
        let req = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: Weft/{INSTALLED_VERSION_NAME}\r\nAccept: application/octet-stream, application/json\r\nConnection: close\r\n\r\n",
            url.path_and_query,
            url.authority()
        );

        if url.scheme == "https" {
            let server_name = ServerName::try_from(url.host.clone())
                .map_err(|_| UpdateError::Invalid("invalid TLS server name".into()))?;
            let mut tls = connector
                .connect(server_name, tcp)
                .await
                .map_err(|e| UpdateError::Network(e.to_string()))?;
            tls.write_all(req.as_bytes())
                .await
                .map_err(|e| UpdateError::Network(e.to_string()))?;
            tls.flush()
                .await
                .map_err(|e| UpdateError::Network(e.to_string()))?;
            let mut reader = BufReader::new(tls);
            Self::handle_response(&mut reader, max_bytes, on_chunk).await
        } else {
            let mut plain = tcp;
            plain
                .write_all(req.as_bytes())
                .await
                .map_err(|e| UpdateError::Network(e.to_string()))?;
            plain
                .flush()
                .await
                .map_err(|e| UpdateError::Network(e.to_string()))?;
            let mut reader = BufReader::new(plain);
            Self::handle_response(&mut reader, max_bytes, on_chunk).await
        }
    }

    async fn handle_response<R: AsyncRead + Unpin>(
        reader: &mut BufReader<R>,
        max_bytes: u64,
        on_chunk: &mut ChunkCallback<'_>,
    ) -> Result<HopOutcome, UpdateError> {
        let headers = read_headers(reader).await?;
        if matches!(headers.status, 301 | 302 | 303 | 307 | 308) {
            let loc = headers
                .location
                .ok_or_else(|| UpdateError::Invalid("redirect missing Location".into()))?;
            return Ok(HopOutcome::Redirect(loc));
        }
        match headers.status {
            404 => Err(UpdateError::NoRelease),
            200..=299 => {
                let len = headers.content_length;
                stream_body(reader, &headers, max_bytes, |chunk| on_chunk(len, chunk)).await?;
                Ok(HopOutcome::Done)
            }
            code => Err(UpdateError::Http(code)),
        }
    }
}

enum HopOutcome {
    Done,
    Redirect(String),
}

fn default_auto_check() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct UpdatePrefs {
    #[serde(default = "default_auto_check")]
    auto_check: bool,
    #[serde(default)]
    last_check_ms: u64,
    #[serde(default)]
    last_notified_code: u64,
}

impl Default for UpdatePrefs {
    fn default() -> Self {
        Self {
            auto_check: true,
            last_check_ms: 0,
            last_notified_code: 0,
        }
    }
}

impl UpdatePrefs {
    fn load(state_dir: &Path) -> Self {
        fs::read_to_string(state_dir.join(PREFS_FILE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(&self, state_dir: &Path) {
        if fs::create_dir_all(state_dir).is_err() {
            return;
        }
        let Ok(json) = serde_json::to_vec_pretty(self) else {
            return;
        };
        let path = state_dir.join(PREFS_FILE);
        let temp = state_dir.join("updates.json.tmp");
        if fs::write(&temp, json)
            .and_then(|()| fs::rename(&temp, &path))
            .is_err()
        {
            let _ = fs::remove_file(&temp);
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// Current state of the background updater.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available {
        info: UpdateInfo,
    },
    Downloading {
        info: UpdateInfo,
        done: u64,
        total: Option<u64>,
    },
    Installing {
        info: UpdateInfo,
    },
    Restarting {
        info: UpdateInfo,
    },
    Failed {
        error: UpdateError,
        info: Option<UpdateInfo>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateSnapshot {
    pub generation: u64,
    pub state: UpdateState,
    pub auto_check: bool,
    pub installed_version: String,
    pub installed_code: u64,
}

struct Board {
    snapshot: UpdateSnapshot,
    prefs: UpdatePrefs,
    stopped: bool,
}

/// Coordinates manual and daily automatic update checks and atomic binary swaps.
pub struct Updater {
    state_dir: PathBuf,
    installed_code: u64,
    client: UpdateClient,
    board: Mutex<Board>,
    changed: Condvar,
    busy: AtomicBool,
    restart_exe: Mutex<Option<PathBuf>>,
    on_available: Box<dyn Fn(&str) + Send + Sync>,
    on_restart: Box<dyn Fn() + Send + Sync>,
}

struct BusyGuard<'a>(&'a AtomicBool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Updater {
    pub fn new(
        state_dir: PathBuf,
        on_available: impl Fn(&str) + Send + Sync + 'static,
        on_restart: impl Fn() + Send + Sync + 'static,
    ) -> Arc<Self> {
        let url = manifest_url();
        let policy = if cfg!(debug_assertions) {
            UpdatePolicy::for_override(&url)
        } else {
            UpdatePolicy::github()
        };
        Self::with_client(
            state_dir,
            INSTALLED_VERSION_CODE,
            UpdateClient::new(url, policy),
            on_available,
            on_restart,
        )
    }

    pub fn with_client(
        state_dir: PathBuf,
        installed_code: u64,
        client: UpdateClient,
        on_available: impl Fn(&str) + Send + Sync + 'static,
        on_restart: impl Fn() + Send + Sync + 'static,
    ) -> Arc<Self> {
        let prefs = UpdatePrefs::load(&state_dir);
        let snapshot = UpdateSnapshot {
            generation: 0,
            state: UpdateState::Idle,
            auto_check: prefs.auto_check,
            installed_version: INSTALLED_VERSION_NAME.to_owned(),
            installed_code,
        };
        Arc::new(Self {
            state_dir,
            installed_code,
            client,
            board: Mutex::new(Board {
                snapshot,
                prefs,
                stopped: false,
            }),
            changed: Condvar::new(),
            busy: AtomicBool::new(false),
            restart_exe: Mutex::new(None),
            on_available: Box::new(on_available),
            on_restart: Box::new(on_restart),
        })
    }

    pub fn snapshot(&self) -> UpdateSnapshot {
        self.board.lock().unwrap().snapshot.clone()
    }

    pub fn wait_newer(&self, generation: u64) -> Option<UpdateSnapshot> {
        let mut board = self.board.lock().unwrap();
        loop {
            if board.snapshot.generation > generation {
                return Some(board.snapshot.clone());
            }
            if board.stopped {
                return None;
            }
            board = self.changed.wait(board).unwrap();
        }
    }

    pub fn stop(&self) {
        let mut board = self.board.lock().unwrap();
        board.stopped = true;
        self.changed.notify_all();
    }

    pub fn take_restart_exe(&self) -> Option<PathBuf> {
        self.restart_exe.lock().unwrap().take()
    }

    /// Runs a synchronous interactive check and returns the resulting snapshot.
    pub fn check_now(&self) -> UpdateSnapshot {
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let _guard = BusyGuard(&self.busy);
            self.run_check(false);
        }
        self.snapshot()
    }

    /// Spawns a quiet daily background check when due.
    pub fn check_if_due(self: &Arc<Self>) {
        let due = {
            let board = self.board.lock().unwrap();
            UpdateSchedule::is_due(board.prefs.auto_check, board.prefs.last_check_ms, now_ms())
        };
        if !due {
            return;
        }
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let updater = Arc::clone(self);
        std::thread::spawn(move || {
            let _guard = BusyGuard(&updater.busy);
            updater.run_check(true);
        });
    }

    /// Checks (if needed), downloads, verifies SHA-256 and ELF64 `x86_64`, and
    /// atomically swaps the current executable.
    pub fn install_now(&self) -> UpdateSnapshot {
        let Ok(target_exe) = std::env::current_exe() else {
            self.publish_state(UpdateState::Failed {
                error: UpdateError::Install("current_exe unavailable".into()),
                info: None,
            });
            return self.snapshot();
        };
        self.install_into_now(&target_exe)
    }

    pub fn install_into_now(&self, target_exe: &Path) -> UpdateSnapshot {
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return self.snapshot();
        }
        let _guard = BusyGuard(&self.busy);
        let existing = match self.snapshot().state {
            UpdateState::Available { info } => Some(info),
            UpdateState::Failed {
                info: Some(info), ..
            } => Some(info),
            _ => None,
        };
        let info = match existing {
            Some(info) => info,
            None => {
                self.run_check(false);
                match self.snapshot().state {
                    UpdateState::Available { info } => info,
                    _ => return self.snapshot(),
                }
            }
        };
        self.run_install(info, target_exe);
        self.snapshot()
    }

    fn publish_state(&self, state: UpdateState) {
        let mut board = self.board.lock().unwrap();
        board.snapshot.state = state;
        board.snapshot.generation += 1;
        self.changed.notify_all();
    }

    fn run_check(&self, automatic: bool) {
        let previous = self.snapshot().state;
        if !automatic {
            self.publish_state(UpdateState::Checking);
        }
        let info = match self.client.fetch_latest() {
            Ok(info) => info,
            Err(error) => {
                if automatic {
                    tracing::debug!(%error, "automatic update check failed");
                    self.publish_state(previous);
                } else {
                    tracing::warn!(%error, "update check failed");
                    self.publish_state(UpdateState::Failed { error, info: None });
                }
                return;
            }
        };

        let notify = {
            let mut board = self.board.lock().unwrap();
            board.prefs.last_check_ms = now_ms();
            let newer = info.is_newer_than(self.installed_code);
            let should_notify =
                newer && automatic && board.prefs.last_notified_code != info.version_code;
            if should_notify {
                board.prefs.last_notified_code = info.version_code;
            }
            if newer {
                board.snapshot.state = UpdateState::Available { info: info.clone() };
                board.snapshot.generation += 1;
                self.changed.notify_all();
            } else if !automatic {
                board.snapshot.state = UpdateState::UpToDate;
                board.snapshot.generation += 1;
                self.changed.notify_all();
            }
            let prefs = board.prefs.clone();
            drop(board);
            prefs.save(&self.state_dir);
            should_notify
        };

        if notify {
            (self.on_available)(&info.version_name);
        }
    }

    fn run_install(&self, info: UpdateInfo, target_exe: &Path) {
        let staging = self.state_dir.join(STAGING_DIR);
        let _ = fs::remove_dir_all(&staging);
        if let Err(err) = fs::create_dir_all(&staging) {
            self.publish_state(UpdateState::Failed {
                error: UpdateError::Install(err.to_string()),
                info: Some(info),
            });
            return;
        }
        let staged = staging.join(format!("weft-{}-linux-x86_64", info.version_code));
        self.publish_state(UpdateState::Downloading {
            info: info.clone(),
            done: 0,
            total: None,
        });

        let mut last_step: Option<u64> = None;
        let step_info = info.clone();
        let downloaded = self.client.download(&info, &staged, |done, total| {
            let step = match total {
                Some(total) if total > 0 => done.saturating_mul(1000) / total,
                _ => done / (256 * 1024),
            };
            if last_step != Some(step) {
                last_step = Some(step);
                self.publish_state(UpdateState::Downloading {
                    info: step_info.clone(),
                    done,
                    total,
                });
            }
        });

        let outcome = downloaded.and_then(|()| {
            check_elf64_x86_64(&staged)?;
            self.publish_state(UpdateState::Installing { info: info.clone() });
            replace_executable(&staged, target_exe)
        });
        let _ = fs::remove_dir_all(&staging);

        match outcome {
            Ok(()) => {
                *self.restart_exe.lock().unwrap() = Some(target_exe.to_path_buf());
                self.publish_state(UpdateState::Restarting { info });
                (self.on_restart)();
            }
            Err(error) => {
                tracing::warn!(%error, "update install failed");
                self.publish_state(UpdateState::Failed {
                    error,
                    info: Some(info),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{TcpListener, TcpStream as StdTcpStream};
    use std::sync::atomic::AtomicUsize;

    use tempfile::TempDir;

    use super::*;

    const SAMPLE_URL: &str =
        "https://github.com/fraustiz/weft/releases/download/app-v0.2.0/weft-0.2.0-linux-x86_64";

    /// Minimal valid 64-bit little-endian `x86_64` ELF (`ET_DYN`) padded to `len`.
    fn fake_elf64_x86_64(len: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; len.max(128)];
        bytes[0..4].copy_from_slice(ELF_MAGIC);
        bytes[4] = ELFCLASS64;
        bytes[5] = ELFDATA2LSB;
        bytes[6] = EV_CURRENT;
        bytes[16..18].copy_from_slice(&ET_DYN.to_le_bytes());
        bytes[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
        bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
        for (i, slot) in bytes[64..].iter_mut().enumerate() {
            *slot = (i.wrapping_mul(37)) as u8;
        }
        bytes
    }

    #[test]
    fn sha256_matches_nist_vectors() {
        assert_eq!(
            Sha256::hex_digest(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            Sha256::hex_digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            Sha256::hex_digest(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn verifies_elf64_x86_64_headers() {
        let dir = TempDir::new().unwrap();
        let valid = dir.path().join("weft-desktop");
        fs::write(&valid, fake_elf64_x86_64(256)).unwrap();
        assert!(check_elf64_x86_64(&valid).is_ok());

        // ET_EXEC is also valid.
        let mut exec = fake_elf64_x86_64(256);
        exec[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        fs::write(&valid, &exec).unwrap();
        assert!(check_elf64_x86_64(&valid).is_ok());

        // Rejects non-ELF, 32-bit ELF, big-endian ELF, and aarch64 (0xB7).
        fs::write(&valid, b"#!/bin/sh\necho nope\n").unwrap();
        assert!(matches!(
            check_elf64_x86_64(&valid),
            Err(UpdateError::Invalid(_))
        ));

        let mut elf32 = fake_elf64_x86_64(256);
        elf32[4] = 1;
        fs::write(&valid, &elf32).unwrap();
        assert!(matches!(
            check_elf64_x86_64(&valid),
            Err(UpdateError::Invalid(_))
        ));

        let mut arm64 = fake_elf64_x86_64(256);
        arm64[18..20].copy_from_slice(&0x00B7u16.to_le_bytes());
        fs::write(&valid, &arm64).unwrap();
        assert!(matches!(
            check_elf64_x86_64(&valid),
            Err(UpdateError::Invalid(_))
        ));
    }

    #[test]
    fn enforces_manifest_and_redirect_host_policy() {
        let policy = UpdatePolicy::github();
        assert!(policy.allows(SAMPLE_URL));
        assert!(policy.allows("https://objects.githubusercontent.com/x/bin"));
        assert!(policy.allows("https://release-assets.githubusercontent.com/x/bin?sig=1"));
        assert!(!policy.allows("http://github.com/x/bin"));
        assert!(!policy.allows("https://evil.example/x/bin"));
        assert!(!policy.allows("https://raw.githubusercontent.com/x/bin"));

        let sha = "a".repeat(64);
        let ok = format!(
            r#"{{"versionCode": 5, "versionName": "0.2.0", "url": "{SAMPLE_URL}", "sha256": "{sha}"}}"#
        );
        let parsed = parse_manifest(&ok, &policy).unwrap();
        assert_eq!(parsed.version_code, 5);
        assert_eq!(parsed.version_name, "0.2.0");

        let https = ParsedUrl::parse("https://github.com/releases/latest").unwrap();
        assert!(https.resolve_redirect("http://github.com/bin").is_none());
    }

    #[test]
    fn swaps_binary_atomically_and_rolls_back_on_error() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("weft-desktop");
        let staged = dir.path().join("staged");
        fs::write(&target, b"old-binary").unwrap();
        fs::write(&staged, b"new-binary").unwrap();

        replace_executable(&staged, &target).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new-binary");
        assert!(!side_path(&target, ".new").exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o755);
        }

        let staged_bad = dir.path().join("staged-bad");
        fs::write(&staged_bad, b"broken").unwrap();
        let err = replace_executable_inner(&staged_bad, &target, |_, _| {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
        });
        assert_eq!(err, Err(UpdateError::WriteProtected));
        assert_eq!(fs::read(&target).unwrap(), b"new-binary");
    }

    #[test]
    fn end_to_end_redirect_download_verify_and_install() {
        let bin = fake_elf64_x86_64(90_000);
        let sha = Sha256::hex_digest(&bin);

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://127.0.0.1:{}", addr.port());
        let manifest = format!(
            r#"{{"versionCode": 9, "versionName": "0.3.0", "url": "{base}/redirect", "sha256": "{sha}"}}"#
        );
        let stop = Arc::new(AtomicBool::new(false));
        let stop_bg = stop.clone();
        let bin_bg = bin.clone();
        let base_bg = base.clone();

        let server = std::thread::spawn(move || {
            while let Ok((mut stream, _)) = listener.accept() {
                if stop_bg.load(Ordering::Acquire) {
                    break;
                }
                let mut req = Vec::new();
                let mut buf = [0u8; 1024];
                while !req.windows(4).any(|w| w == b"\r\n\r\n") && req.len() < 8192 {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => req.extend_from_slice(&buf[..n]),
                    }
                }
                let first = String::from_utf8_lossy(&req);
                let path = first
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("/");
                let resp = match path {
                    "/linux-update.json" => format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        manifest.len(),
                        manifest
                    )
                    .into_bytes(),
                    "/redirect" => format!(
                        "HTTP/1.1 302 Found\r\nLocation: {base_bg}/weft-0.3.0-linux-x86_64\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .into_bytes(),
                    "/weft-0.3.0-linux-x86_64" => {
                        let mut out = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            bin_bg.len()
                        )
                        .into_bytes();
                        out.extend_from_slice(&bin_bg);
                        out
                    }
                    _ => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_vec(),
                };
                let _ = stream.write_all(&resp);
                let _ = stream.flush();
            }
        });

        let dir = TempDir::new().unwrap();
        let state_dir = dir.path().join("state");
        let installed = dir.path().join("weft-desktop");
        fs::write(&installed, fake_elf64_x86_64(256)).unwrap();

        let manifest_url = format!("{base}/linux-update.json");
        let client = UpdateClient::new(&manifest_url, UpdatePolicy::for_override(&manifest_url));
        let notified = Arc::new(AtomicUsize::new(0));
        let restarted = Arc::new(AtomicBool::new(false));
        let (notified_cb, restarted_cb) = (notified.clone(), restarted.clone());

        let updater = Updater::with_client(
            state_dir,
            1,
            client,
            move |_| {
                notified_cb.fetch_add(1, Ordering::SeqCst);
            },
            move || {
                restarted_cb.store(true, Ordering::SeqCst);
            },
        );

        let snap = updater.install_into_now(&installed);
        assert!(matches!(snap.state, UpdateState::Restarting { .. }));
        assert_eq!(fs::read(&installed).unwrap(), bin);
        assert!(restarted.load(Ordering::SeqCst));
        assert_eq!(notified.load(Ordering::SeqCst), 0);

        stop.store(true, Ordering::Release);
        let _ = StdTcpStream::connect(addr);
        let _ = server.join();
    }
}
