//! Self-updates from GitHub Releases.
//!
//! Each release publishes `windows-update.json` next to
//! `weft-X.Y.Z-windows.exe`. [`Updater`] checks it at most once a day when
//! automatic checks are enabled (and on demand from Settings or the tray
//! menu), downloads the new executable over WinHTTP, verifies its SHA-256
//! digest and x64 PE header, swaps it into place next to the running
//! executable, and asks the app to restart cleanly.

use std::collections::BTreeSet;
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use weft_cli::hex;
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, GetLastError};
use windows_sys::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_DISABLE_REDIRECTS,
    WINHTTP_FLAG_SECURE, WINHTTP_OPTION_DISABLE_FEATURE, WINHTTP_QUERY_CONTENT_LENGTH,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_LOCATION, WINHTTP_QUERY_STATUS_CODE,
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption,
    WinHttpSetTimeouts,
};

use crate::clipboard::wide;

/// Default URL of the release manifest.
pub const DEFAULT_MANIFEST_URL: &str =
    "https://github.com/fraustiz/weft/releases/latest/download/windows-update.json";

/// Human-facing releases page opened when the install folder is write-protected.
pub const RELEASES_PAGE_URL: &str = "https://github.com/fraustiz/weft/releases/latest";

/// Version string embedded at build time (`WEFT_VERSION` or `CARGO_PKG_VERSION`).
pub const INSTALLED_VERSION_NAME: &str = env!("WEFT_VERSION_NAME");

/// Monotonic build number embedded at build time (`WEFT_VERSION_CODE` or `1`).
pub const INSTALLED_VERSION_CODE: u64 = parse_u64_const(env!("WEFT_VERSION_CODE"));

/// Maximum accepted byte size for `windows-update.json`.
pub const MAX_MANIFEST_BYTES: usize = 16 * 1024;

/// Maximum accepted byte size for `weft-X.Y.Z-windows.exe`.
pub const MAX_EXE_BYTES: u64 = 100 * 1024 * 1024;

/// Maximum HTTP redirect hops followed manually.
const MAX_REDIRECTS: usize = 5;

const BUFFER_LEN: usize = 64 * 1024;
const PREFS_FILE: &str = "updates.json";
const STAGING_DIR: &str = "updates";
const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;

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

/// A version published on GitHub Releases, read from `windows-update.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Why an update check, download, or installation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    /// Connection, timeout, DNS, or TLS failure.
    Network(String),
    /// 404 on the manifest: no Windows release published yet.
    NoRelease,
    /// Non-success HTTP status code.
    Http(u16),
    /// Malformed manifest, size exceeded, disallowed URL/redirect, or non-x64 PE.
    Invalid(String),
    /// Downloaded executable SHA-256 did not match the manifest.
    BadHash,
    /// The executable's directory denied write or rename access.
    WriteProtected,
    /// Any other local filesystem or install error.
    Install(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::Network(reason) => write!(f, "network: {reason}"),
            UpdateError::NoRelease => f.write_str("no release yet"),
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

    fn connect_host(&self) -> String {
        if self.ipv6 {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        }
    }

    fn authority(&self) -> String {
        let default_port = if self.scheme == "https" { 443 } else { 80 };
        let host = self.connect_host();
        if self.port == default_port {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }

    fn is_loopback(&self) -> bool {
        matches!(self.host.as_str(), "127.0.0.1" | "localhost" | "::1")
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

/// Which URLs the updater fetches from: HTTPS on GitHub's release hosts only,
/// plus a single override host in debug/test configurations.
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

    /// GitHub hosts plus the host of `manifest_url` over HTTP or HTTPS.
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

/// Parses `windows-update.json` and validates its fields and download URL.
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
        Self::is_due_within(auto_check, last_check_ms, now_ms, Self::INTERVAL_MS)
    }

    pub fn is_due_within(
        auto_check: bool,
        last_check_ms: u64,
        now_ms: u64,
        interval_ms: u64,
    ) -> bool {
        if !auto_check {
            return false;
        }
        if last_check_ms == 0 || last_check_ms > now_ms {
            return true;
        }
        now_ms - last_check_ms >= interval_ms
    }
}

/// Verifies that `path` starts with a DOS `MZ` header whose `e_lfanew` points
/// to a `PE\0\0` signature for `IMAGE_FILE_MACHINE_AMD64` (`0x8664`).
pub fn check_pe_executable(path: &Path) -> Result<(), UpdateError> {
    let invalid = |why: &str| UpdateError::Invalid(why.to_owned());
    let mut file = File::open(path).map_err(|err| UpdateError::Install(err.to_string()))?;
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos)
        .map_err(|_| invalid("executable header truncated"))?;
    if &dos[0..2] != b"MZ" {
        return Err(invalid("missing MZ signature"));
    }
    let pe_offset = u32::from_le_bytes(dos[0x3c..0x40].try_into().unwrap());
    if !(64..=1024 * 1024).contains(&pe_offset) {
        return Err(invalid("invalid PE header offset"));
    }
    file.seek(SeekFrom::Start(u64::from(pe_offset)))
        .map_err(|_| invalid("PE header out of bounds"))?;
    let mut pe = [0u8; 6];
    file.read_exact(&mut pe)
        .map_err(|_| invalid("PE header truncated"))?;
    if &pe[0..4] != b"PE\0\0" {
        return Err(invalid("missing PE signature"));
    }
    let machine = u16::from_le_bytes([pe[4], pe[5]]);
    if machine != IMAGE_FILE_MACHINE_AMD64 {
        return Err(invalid("executable is not x86_64"));
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

/// Atomically replaces `target_exe` with `staged`:
/// 1. Places `staged` at `<target_exe>.new` in the same folder.
/// 2. Renames `<target_exe>` to `<target_exe>.old` (permitted on a running
///    executable on Windows).
/// 3. Renames `<target_exe>.new` to `<target_exe>`, rolling `<target_exe>.old`
///    back to `<target_exe>` if step 3 fails.
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

    if let Err(err) = fs::rename(target_exe, &old_path) {
        let _ = fs::remove_file(&new_path);
        return Err(map_write_error(err));
    }

    if let Err(err) = activate(&new_path, target_exe) {
        let _ = fs::rename(&old_path, target_exe);
        let _ = fs::remove_file(&new_path);
        return Err(map_write_error(err));
    }
    Ok(())
}

/// Removes `<current_exe>.old`, `<current_exe>.new`, and any leftover download
/// staging folder in `state_dir`. Retries briefly on a background thread if the
/// previous process is still releasing its image lock on `.exe.old`.
pub fn cleanup_old_executable(state_dir: &Path) {
    let _ = fs::remove_dir_all(state_dir.join(STAGING_DIR));
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let _ = fs::remove_file(side_path(&exe, ".new"));
    let old = side_path(&exe, ".old");
    if !old.exists() || fs::remove_file(&old).is_ok() {
        return;
    }
    std::thread::spawn(move || {
        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if !old.exists() || fs::remove_file(&old).is_ok() {
                return;
            }
        }
    });
}

struct WinHttpHandle(*mut c_void);

impl Drop for WinHttpHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: non-null handle returned by WinHttpOpen / Connect / OpenRequest.
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn winhttp_error(step: &str) -> UpdateError {
    // SAFETY: reads the calling thread's last Win32 error code.
    let code = unsafe { GetLastError() };
    UpdateError::Network(format!("{step} ({code})"))
}

/// Queries a response header as UTF-16 text.
fn query_header_string(request: *mut c_void, info_level: u32) -> Option<String> {
    let mut byte_len: u32 = 0;
    // SAFETY: first call probes required byte length into `byte_len`.
    let ok = unsafe {
        WinHttpQueryHeaders(
            request,
            info_level,
            null(),
            null_mut(),
            &mut byte_len,
            null_mut(),
        )
    };
    if ok != 0 || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER || byte_len == 0 {
        return None;
    }
    let units = (byte_len as usize).div_ceil(size_of::<u16>());
    let mut buf = vec![0u16; units];
    let mut actual = (units * size_of::<u16>()) as u32;
    // SAFETY: `buf` holds `actual` bytes and outlives the call.
    let ok = unsafe {
        WinHttpQueryHeaders(
            request,
            info_level,
            null(),
            buf.as_mut_ptr().cast(),
            &mut actual,
            null_mut(),
        )
    };
    if ok == 0 {
        return None;
    }
    let len = ((actual as usize) / size_of::<u16>()).min(buf.len());
    let slice = buf[..len].strip_suffix(&[0]).unwrap_or(&buf[..len]);
    Some(String::from_utf16_lossy(slice))
}

struct OpenResponse {
    _session: WinHttpHandle,
    _connection: WinHttpHandle,
    request: WinHttpHandle,
    content_length: Option<u64>,
}

impl OpenResponse {
    fn read_chunk(&self, buf: &mut [u8]) -> Result<usize, UpdateError> {
        let mut read: u32 = 0;
        // SAFETY: `buf` is valid for `buf.len()` writable bytes.
        let ok = unsafe {
            WinHttpReadData(
                self.request.0,
                buf.as_mut_ptr().cast(),
                buf.len().min(u32::MAX as usize) as u32,
                &mut read,
            )
        };
        if ok == 0 {
            return Err(winhttp_error("WinHttpReadData"));
        }
        Ok(read as usize)
    }
}

/// Fetches the update manifest and executable over WinHTTP with manual redirect
/// validation on every hop.
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
        let response = self.get(&self.manifest_url)?;
        if response
            .content_length
            .is_some_and(|len| len > MAX_MANIFEST_BYTES as u64)
        {
            return Err(UpdateError::Invalid("manifest too large".into()));
        }
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let read = response.read_chunk(&mut chunk)?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
            if bytes.len() > MAX_MANIFEST_BYTES {
                return Err(UpdateError::Invalid("manifest too large".into()));
            }
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| UpdateError::Invalid("manifest is not UTF-8".into()))?;
        parse_manifest(&text, &self.policy)
    }

    /// Downloads `info.url` to `target`, reporting `(done, total)` in bytes,
    /// and verifies its SHA-256 hash. Deletes `target` on any error.
    pub fn download(
        &self,
        info: &UpdateInfo,
        target: &Path,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<(), UpdateError> {
        let result = self.download_inner(info, target, &mut progress);
        if result.is_err() {
            let _ = fs::remove_file(target);
        }
        result
    }

    fn download_inner(
        &self,
        info: &UpdateInfo,
        target: &Path,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<(), UpdateError> {
        if !self.policy.allows(&info.url) {
            return Err(UpdateError::Invalid("url not allowed".into()));
        }
        let response = self.get(&info.url)?;
        let total = response.content_length;
        if total.is_some_and(|len| len > MAX_EXE_BYTES) {
            return Err(UpdateError::Invalid("executable too large".into()));
        }
        let mut file = File::create(target).map_err(|err| UpdateError::Install(err.to_string()))?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; BUFFER_LEN];
        let mut done: u64 = 0;
        progress(0, total);
        loop {
            let read = response.read_chunk(&mut buf)?;
            if read == 0 {
                break;
            }
            done = done.saturating_add(read as u64);
            if done > MAX_EXE_BYTES {
                return Err(UpdateError::Invalid("executable too large".into()));
            }
            hasher.update(&buf[..read]);
            file.write_all(&buf[..read])
                .map_err(|err| UpdateError::Install(err.to_string()))?;
            progress(done, total);
        }
        file.sync_all()
            .map_err(|err| UpdateError::Install(err.to_string()))?;
        drop(file);
        if hex(&hasher.finish()) != info.sha256 {
            return Err(UpdateError::BadHash);
        }
        Ok(())
    }

    fn get(&self, initial_url: &str) -> Result<OpenResponse, UpdateError> {
        let mut current = ParsedUrl::parse(initial_url)
            .ok_or_else(|| UpdateError::Invalid("invalid url".into()))?;
        if !self.policy.allows_parsed(&current) {
            return Err(UpdateError::Invalid("url not allowed".into()));
        }

        let agent = wide(&format!("Weft/{INSTALLED_VERSION_NAME}"));
        let verb = wide("GET");
        let accept = wide("Accept: application/octet-stream, application/json\r\n");

        for hop in 0..=MAX_REDIRECTS {
            let access_type = if current.is_loopback() {
                WINHTTP_ACCESS_TYPE_NO_PROXY
            } else {
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY
            };
            // SAFETY: `agent` is NUL-terminated and outlives `WinHttpOpen`.
            let session = WinHttpHandle(unsafe {
                WinHttpOpen(agent.as_ptr(), access_type, null(), null(), 0)
            });
            if session.0.is_null() {
                return Err(winhttp_error("WinHttpOpen"));
            }
            // SAFETY: valid session handle.
            unsafe {
                WinHttpSetTimeouts(session.0, 15_000, 15_000, 15_000, 30_000);
            }

            let host = wide(&current.connect_host());
            // SAFETY: `host` is NUL-terminated and outlives `WinHttpConnect`.
            let connection =
                WinHttpHandle(unsafe { WinHttpConnect(session.0, host.as_ptr(), current.port, 0) });
            if connection.0.is_null() {
                return Err(winhttp_error("WinHttpConnect"));
            }

            let path = wide(&current.path_and_query);
            let flags = if current.scheme == "https" {
                WINHTTP_FLAG_SECURE
            } else {
                0
            };
            // SAFETY: `verb` and `path` are NUL-terminated and outlive the call.
            let request = WinHttpHandle(unsafe {
                WinHttpOpenRequest(
                    connection.0,
                    verb.as_ptr(),
                    path.as_ptr(),
                    null(),
                    null(),
                    null(),
                    flags,
                )
            });
            if request.0.is_null() {
                return Err(winhttp_error("WinHttpOpenRequest"));
            }

            let disable_redirects: u32 = WINHTTP_DISABLE_REDIRECTS;
            // SAFETY: `WINHTTP_OPTION_DISABLE_FEATURE` expects a `u32` flag mask.
            let disabled = unsafe {
                WinHttpSetOption(
                    request.0,
                    WINHTTP_OPTION_DISABLE_FEATURE,
                    (&raw const disable_redirects).cast(),
                    size_of::<u32>() as u32,
                )
            };
            if disabled == 0 {
                return Err(winhttp_error("WinHttpSetOption"));
            }

            // SAFETY: `accept` is NUL-terminated; `u32::MAX` asks WinHTTP to
            // measure up to the NUL.
            let sent = unsafe {
                WinHttpSendRequest(request.0, accept.as_ptr(), u32::MAX, null(), 0, 0, 0)
            };
            if sent == 0 {
                return Err(winhttp_error("WinHttpSendRequest"));
            }
            // SAFETY: valid sent request handle.
            if unsafe { WinHttpReceiveResponse(request.0, null_mut()) } == 0 {
                return Err(winhttp_error("WinHttpReceiveResponse"));
            }

            let mut status: u32 = 0;
            let mut status_size = size_of::<u32>() as u32;
            // SAFETY: queries a 32-bit status code into `status`.
            let queried = unsafe {
                WinHttpQueryHeaders(
                    request.0,
                    WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                    null(),
                    (&raw mut status).cast(),
                    &mut status_size,
                    null_mut(),
                )
            };
            if queried == 0 {
                return Err(winhttp_error("WinHttpQueryHeaders"));
            }

            if matches!(status, 301 | 302 | 303 | 307 | 308) {
                if hop == MAX_REDIRECTS {
                    return Err(UpdateError::Invalid("too many redirects".into()));
                }
                let location = query_header_string(request.0, WINHTTP_QUERY_LOCATION)
                    .ok_or_else(|| UpdateError::Invalid("redirect missing Location".into()))?;
                let next = current
                    .resolve_redirect(&location)
                    .ok_or_else(|| UpdateError::Invalid("disallowed redirect".into()))?;
                if !self.policy.allows_parsed(&next) {
                    return Err(UpdateError::Invalid(
                        "redirected outside the allowed hosts".into(),
                    ));
                }
                current = next;
                continue;
            }

            return match status {
                404 => Err(UpdateError::NoRelease),
                200..=299 => {
                    let content_length =
                        query_header_string(request.0, WINHTTP_QUERY_CONTENT_LENGTH)
                            .and_then(|raw| raw.trim().parse::<u64>().ok());
                    Ok(OpenResponse {
                        _session: session,
                        _connection: connection,
                        request,
                        content_length,
                    })
                }
                code => Err(UpdateError::Http(code.min(u16::MAX as u32) as u16)),
            };
        }
        Err(UpdateError::Invalid("too many redirects".into()))
    }
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
        if let Err(err) = fs::create_dir_all(state_dir) {
            tracing::warn!(error = %err, "could not create state directory for updates.json");
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

/// Where the updater stands, for the Settings page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available(UpdateInfo),
    Downloading {
        info: UpdateInfo,
        done: u64,
        total: Option<u64>,
    },
    Installing(UpdateInfo),
    Restarting(UpdateInfo),
    Failed {
        error: UpdateError,
        info: Option<UpdateInfo>,
    },
}

/// Published snapshot of the updater's state and preferences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateSnapshot {
    pub generation: u64,
    pub state: UpdateState,
    pub auto_check: bool,
}

struct Board {
    snapshot: UpdateSnapshot,
    prefs: UpdatePrefs,
    stopped: bool,
}

/// Background coordinator for manual and daily update checks, downloads, and
/// atomic executable replacement.
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

    /// Blocks until a snapshot newer than `generation` is published, or returns
    /// `None` once [`Updater::stop`] has been called.
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

    /// Wakes any waiting UI watcher on shutdown.
    pub fn stop(&self) {
        let mut board = self.board.lock().unwrap();
        board.stopped = true;
        self.changed.notify_all();
    }

    /// Returns and clears the executable path to spawn on exit, if an update
    /// was just installed.
    pub fn take_restart_exe(&self) -> Option<PathBuf> {
        self.restart_exe.lock().unwrap().take()
    }

    pub fn set_auto_check(&self, auto_check: bool) {
        let prefs = {
            let mut board = self.board.lock().unwrap();
            if board.prefs.auto_check == auto_check {
                return;
            }
            board.prefs.auto_check = auto_check;
            board.snapshot.auto_check = auto_check;
            board.snapshot.generation += 1;
            self.changed.notify_all();
            board.prefs.clone()
        };
        prefs.save(&self.state_dir);
    }

    /// Starts an interactive update check if idle.
    pub fn check(self: &Arc<Self>) {
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
            updater.run_check(false);
        });
    }

    /// Starts a quiet automatic update check if enabled and at least 24 hours
    /// have elapsed since the last completed check.
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

    /// Downloads, verifies, installs, and triggers a clean restart for the
    /// currently available update.
    pub fn install_and_restart(self: &Arc<Self>) {
        let Ok(target_exe) = std::env::current_exe() else {
            self.publish_state(UpdateState::Failed {
                error: UpdateError::Install("current_exe unavailable".into()),
                info: None,
            });
            return;
        };
        self.install_into(target_exe);
    }

    fn install_into(self: &Arc<Self>, target_exe: PathBuf) {
        let info = {
            let board = self.board.lock().unwrap();
            match &board.snapshot.state {
                UpdateState::Available(info) => info.clone(),
                UpdateState::Failed {
                    info: Some(info), ..
                } => info.clone(),
                _ => return,
            }
        };
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
            updater.run_install(info, &target_exe);
        });
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
                board.snapshot.state = UpdateState::Available(info.clone());
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
        let staged = staging.join(format!("weft-{}.exe", info.version_code));
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
            check_pe_executable(&staged)?;
            self.publish_state(UpdateState::Installing(info.clone()));
            replace_executable(&staged, target_exe)
        });
        let _ = fs::remove_dir_all(&staging);

        match outcome {
            Ok(()) => {
                *self.restart_exe.lock().unwrap() = Some(target_exe.to_path_buf());
                self.publish_state(UpdateState::Restarting(info));
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
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::AtomicUsize;

    use tempfile::TempDir;

    use super::*;

    const SAMPLE_URL: &str =
        "https://github.com/fraustiz/weft/releases/download/app-v0.2.0/weft-0.2.0-windows.exe";

    fn sample_sha() -> String {
        "a".repeat(64)
    }

    fn manifest_json(code: &str, name: &str, url: &str, sha: &str, extra: &str) -> String {
        format!(
            r#"{{"versionCode": {code}, "versionName": {name}, "url": {url}, "sha256": {sha}{extra}}}"#
        )
    }

    /// Minimal valid x86_64 PE header padded to `len` bytes.
    fn fake_x64_pe(len: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; len.max(128)];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        bytes[0x40..0x44].copy_from_slice(b"PE\0\0");
        bytes[0x44..0x46].copy_from_slice(&IMAGE_FILE_MACHINE_AMD64.to_le_bytes());
        for (i, slot) in bytes[0x46..].iter_mut().enumerate() {
            *slot = (i.wrapping_mul(31)) as u8;
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

        let mut million = Sha256::new();
        for _ in 0..1000 {
            million.update(&[b'a'; 1000]);
        }
        assert_eq!(
            hex(&million.finish()),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("sample.bin");
        fs::write(&path, b"abc").unwrap();
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(Sha256::matches(&path, &expected.to_uppercase()).unwrap());
        assert!(!Sha256::matches(&path, &"0".repeat(64)).unwrap());
    }

    #[test]
    fn parses_manifest_and_rejects_malformed_fields() {
        let sha = sample_sha();
        let policy = UpdatePolicy::github();
        let ok = manifest_json(
            "120",
            "\"0.2.0\"",
            &format!("\"{SAMPLE_URL}\""),
            &format!("\"{sha}\""),
            "",
        );
        assert_eq!(
            parse_manifest(&ok, &policy).unwrap(),
            UpdateInfo {
                version_code: 120,
                version_name: "0.2.0".into(),
                url: SAMPLE_URL.into(),
                sha256: sha.clone(),
            }
        );

        let upper = manifest_json(
            "120",
            "\"0.2.0\"",
            &format!("\"{SAMPLE_URL}\""),
            &format!("\"{}\"", "AB".repeat(32)),
            r#", "notes": "extra""#,
        );
        assert_eq!(
            parse_manifest(&upper, &policy).unwrap().sha256,
            "ab".repeat(32)
        );

        let url_q = format!("\"{SAMPLE_URL}\"");
        let sha_q = format!("\"{sha}\"");
        let bad = [
            format!(r#"{{"versionName": "0.2.0", "url": {url_q}, "sha256": {sha_q}}}"#),
            format!(r#"{{"versionCode": 120, "url": {url_q}, "sha256": {sha_q}}}"#),
            format!(r#"{{"versionCode": 120, "versionName": "0.2.0", "sha256": {sha_q}}}"#),
            format!(r#"{{"versionCode": 120, "versionName": "0.2.0", "url": {url_q}}}"#),
            manifest_json("\"120\"", "\"0.2.0\"", &url_q, &sha_q, ""),
            manifest_json("1.5", "\"0.2.0\"", &url_q, &sha_q, ""),
            manifest_json("0", "\"0.2.0\"", &url_q, &sha_q, ""),
            manifest_json("-3", "\"0.2.0\"", &url_q, &sha_q, ""),
            manifest_json("120", "\"   \"", &url_q, &sha_q, ""),
            manifest_json("120", "\"0.2.0\"", &url_q, "\"abc\"", ""),
            manifest_json(
                "120",
                "\"0.2.0\"",
                &url_q,
                &format!("\"{}\"", "g".repeat(64)),
                "",
            ),
            manifest_json("120", "\"0.2.0\"", "42", &sha_q, ""),
            "[]".into(),
            "not json".into(),
            String::new(),
        ];
        for entry in bad {
            assert!(
                matches!(
                    parse_manifest(&entry, &policy),
                    Err(UpdateError::Invalid(_))
                ),
                "accepted invalid manifest: {entry}"
            );
        }
    }

    #[test]
    fn policy_enforces_https_github_hosts_and_single_override() {
        let policy = UpdatePolicy::github();
        assert!(policy.allows(SAMPLE_URL));
        assert!(policy.allows("https://objects.githubusercontent.com/a/b.exe"));
        assert!(policy.allows("https://release-assets.githubusercontent.com/a/b.exe?x=1"));
        assert!(policy.allows("https://GitHub.com/x.exe"));
        assert!(!policy.allows("http://github.com/x.exe"));
        assert!(!policy.allows("https://evil.example/x.exe"));
        assert!(!policy.allows("https://github.com.evil.example/x.exe"));
        assert!(!policy.allows("https://evilgithub.com/x.exe"));
        assert!(!policy.allows("https://raw.githubusercontent.com/x.exe"));
        assert!(!policy.allows("https://github.com@evil.example/x.exe"));
        assert!(!policy.allows("https://user@github.com/x.exe"));
        assert!(!policy.allows("file:///C:/x.exe"));
        assert!(!policy.allows("ftp://github.com/x.exe"));
        assert!(!policy.allows("//github.com/x.exe"));
        assert!(!policy.allows("not a url"));
        assert!(!policy.allows(""));

        let over = UpdatePolicy::for_override("http://127.0.0.1:8080/windows-update.json");
        assert!(over.allows("http://127.0.0.1:8080/weft.exe"));
        assert!(over.allows(SAMPLE_URL));
        assert!(!over.allows("http://127.0.0.2:8080/weft.exe"));

        // Redirect downgrade from https to http is rejected even when http is allowed.
        let https = ParsedUrl::parse("https://github.com/releases/latest").unwrap();
        assert!(https.resolve_redirect("http://github.com/x.exe").is_none());
        assert_eq!(
            https
                .resolve_redirect("/fraustiz/weft/releases/download/v1/weft.exe")
                .unwrap()
                .path_and_query,
            "/fraustiz/weft/releases/download/v1/weft.exe"
        );
    }

    #[test]
    fn schedule_checks_at_most_once_a_day() {
        let day = UpdateSchedule::INTERVAL_MS;
        let now = 10 * day;
        assert!(UpdateSchedule::is_due(true, 0, now));
        assert!(UpdateSchedule::is_due(true, 0, 500));
        assert!(UpdateSchedule::is_due(true, now - day, now));
        assert!(!UpdateSchedule::is_due(true, now - day + 1, now));
        assert!(!UpdateSchedule::is_due(false, 0, now));
        assert!(UpdateSchedule::is_due(true, now + 1000, now));
    }

    #[test]
    fn checks_pe_x64_signature() {
        let dir = TempDir::new().unwrap();
        let valid = dir.path().join("valid.exe");
        fs::write(&valid, fake_x64_pe(256)).unwrap();
        assert!(check_pe_executable(&valid).is_ok());

        let text = dir.path().join("not_pe.exe");
        fs::write(&text, b"not an executable at all").unwrap();
        assert!(matches!(
            check_pe_executable(&text),
            Err(UpdateError::Invalid(_))
        ));

        let mut x86 = fake_x64_pe(256);
        x86[0x44..0x46].copy_from_slice(&0x014cu16.to_le_bytes());
        let x86_path = dir.path().join("x86.exe");
        fs::write(&x86_path, x86).unwrap();
        assert!(matches!(
            check_pe_executable(&x86_path),
            Err(UpdateError::Invalid(_))
        ));
    }

    #[test]
    fn replaces_executable_and_rolls_back_on_failure() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("weft-windows.exe");
        let staged = dir.path().join("staged.exe");
        fs::write(&target, b"old-version").unwrap();
        fs::write(&staged, b"new-version").unwrap();

        replace_executable(&staged, &target).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new-version");
        assert_eq!(
            fs::read(side_path(&target, ".old")).unwrap(),
            b"old-version"
        );
        assert!(!side_path(&target, ".new").exists());
        assert!(!staged.exists());

        // Simulate failure at step 3: original executable must be restored.
        let staged_fail = dir.path().join("staged-fail.exe");
        fs::write(&staged_fail, b"broken-step").unwrap();
        let err = replace_executable_inner(&staged_fail, &target, |_, _| {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
        });
        assert_eq!(err, Err(UpdateError::WriteProtected));
        assert_eq!(fs::read(&target).unwrap(), b"new-version");
        assert!(!side_path(&target, ".new").exists());
    }

    #[derive(Clone)]
    struct ServerRoutes {
        manifest: Option<String>,
        exe: Vec<u8>,
        redirect_to: String,
    }

    struct TestServer {
        base: String,
        routes: Arc<Mutex<ServerRoutes>>,
        stop: Arc<AtomicBool>,
        addr: std::net::SocketAddr,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl TestServer {
        fn start(exe: Vec<u8>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let base = format!("http://127.0.0.1:{}", addr.port());
            let sha = Sha256::hex_digest(&exe);
            let routes = Arc::new(Mutex::new(ServerRoutes {
                manifest: Some(format!(
                    r#"{{"versionCode": 7, "versionName": "1.0.0", "url": "{base}/redirect", "sha256": "{sha}"}}"#
                )),
                exe,
                redirect_to: format!("{base}/weft.exe"),
            }));
            let stop = Arc::new(AtomicBool::new(false));
            let (routes_bg, stop_bg) = (Arc::clone(&routes), Arc::clone(&stop));
            let thread = std::thread::spawn(move || {
                while let Ok((mut stream, _)) = listener.accept() {
                    if stop_bg.load(Ordering::Acquire) {
                        break;
                    }
                    let mut req = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !req.windows(4).any(|w| w == b"\r\n\r\n") && req.len() < 16 * 1024 {
                        match stream.read(&mut buf) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => req.extend_from_slice(&buf[..n]),
                        }
                    }
                    let line = String::from_utf8_lossy(&req);
                    let path = line
                        .lines()
                        .next()
                        .and_then(|first| first.split_whitespace().nth(1))
                        .unwrap_or("/");
                    let cfg = routes_bg.lock().unwrap().clone();
                    let response = match path {
                        "/windows-update.json" => match cfg.manifest {
                            Some(body) => http_ok(body.as_bytes()),
                            None => http_status("404 Not Found", &[]),
                        },
                        "/redirect" => format!(
                            "HTTP/1.1 302 Found\r\nLocation: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                            cfg.redirect_to
                        )
                        .into_bytes(),
                        "/weft.exe" => http_ok(&cfg.exe),
                        _ => http_status("404 Not Found", &[]),
                    };
                    let _ = stream.write_all(&response);
                    let _ = stream.flush();
                }
            });
            Self {
                base,
                routes,
                stop,
                addr,
                thread: Some(thread),
            }
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            let _ = TcpStream::connect(self.addr);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn http_ok(body: &[u8]) -> Vec<u8> {
        http_status("200 OK", body)
    }

    fn http_status(status: &str, body: &[u8]) -> Vec<u8> {
        let mut out = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn fetches_follows_redirect_and_verifies_download() {
        let exe = fake_x64_pe(200_000);
        let server = TestServer::start(exe.clone());
        let manifest_url = format!("{}/windows-update.json", server.base);
        let client = UpdateClient::new(&manifest_url, UpdatePolicy::for_override(&manifest_url));

        let info = client.fetch_latest().unwrap();
        assert_eq!(info.version_code, 7);
        assert_eq!(info.version_name, "1.0.0");

        let dir = TempDir::new().unwrap();
        let target = dir.path().join("downloaded.exe");
        let mut last = (0u64, None);
        client
            .download(&info, &target, |done, total| last = (done, total))
            .unwrap();
        assert_eq!(fs::read(&target).unwrap(), exe);
        assert_eq!(last, (exe.len() as u64, Some(exe.len() as u64)));
        check_pe_executable(&target).unwrap();
    }

    #[test]
    fn bad_hash_deletes_downloaded_file() {
        let server = TestServer::start(fake_x64_pe(4096));
        let manifest_url = format!("{}/windows-update.json", server.base);
        let client = UpdateClient::new(&manifest_url, UpdatePolicy::for_override(&manifest_url));
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("bad.exe");
        let info = UpdateInfo {
            version_code: 7,
            version_name: "1.0.0".into(),
            url: format!("{}/weft.exe", server.base),
            sha256: "0".repeat(64),
        };
        assert_eq!(
            client.download(&info, &target, |_, _| {}),
            Err(UpdateError::BadHash)
        );
        assert!(!target.exists());
    }

    #[test]
    fn missing_release_and_disallowed_redirect_are_classified() {
        let server = TestServer::start(fake_x64_pe(1024));
        server.routes.lock().unwrap().manifest = None;
        let manifest_url = format!("{}/windows-update.json", server.base);
        let client = UpdateClient::new(&manifest_url, UpdatePolicy::for_override(&manifest_url));
        assert_eq!(client.fetch_latest(), Err(UpdateError::NoRelease));

        // Redirect from 127.0.0.1 to localhost leaves the allowed host set.
        server.routes.lock().unwrap().redirect_to =
            format!("{}/weft.exe", server.base.replace("127.0.0.1", "localhost"));
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("redirected.exe");
        let info = UpdateInfo {
            version_code: 7,
            version_name: "1.0.0".into(),
            url: format!("{}/redirect", server.base),
            sha256: "0".repeat(64),
        };
        assert!(matches!(
            client.download(&info, &target, |_, _| {}),
            Err(UpdateError::Invalid(_))
        ));
        assert!(!target.exists());
    }

    #[test]
    fn updater_persists_state_notifies_once_and_installs() {
        let new_exe = fake_x64_pe(16_384);
        let server = TestServer::start(new_exe.clone());
        let manifest_url = format!("{}/windows-update.json", server.base);
        let client = UpdateClient::new(&manifest_url, UpdatePolicy::for_override(&manifest_url));

        let dir = TempDir::new().unwrap();
        let state_dir = dir.path().join("state");
        let installed_exe = dir.path().join("weft-windows.exe");
        fs::write(&installed_exe, fake_x64_pe(512)).unwrap();

        let notified = Arc::new(AtomicUsize::new(0));
        let restarted = Arc::new(AtomicBool::new(false));
        let notified_cb = Arc::clone(&notified);
        let restarted_cb = Arc::clone(&restarted);

        let updater = Updater::with_client(
            state_dir.clone(),
            1,
            client,
            move |_ver| {
                notified_cb.fetch_add(1, Ordering::SeqCst);
            },
            move || {
                restarted_cb.store(true, Ordering::SeqCst);
            },
        );

        updater.check_if_due();
        let mut snap = updater.wait_newer(0).unwrap();
        while !matches!(snap.state, UpdateState::Available(_)) {
            snap = updater.wait_newer(snap.generation).unwrap();
        }
        for _ in 0..50 {
            if notified.load(Ordering::SeqCst) == 1 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(notified.load(Ordering::SeqCst), 1);

        let saved: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(state_dir.join(PREFS_FILE)).unwrap()).unwrap();
        assert_eq!(saved["autoCheck"], true);
        assert_eq!(saved["lastNotifiedCode"], 7);
        assert!(saved["lastCheckMs"].as_u64().unwrap() > 0);

        // Install into `installed_exe` and verify atomic swap + restart path.
        let before = snap.generation;
        updater.install_into(installed_exe.clone());
        loop {
            snap = updater.wait_newer(snap.generation.max(before)).unwrap();
            if matches!(
                snap.state,
                UpdateState::Restarting(_) | UpdateState::Failed { .. }
            ) {
                break;
            }
        }
        assert!(matches!(snap.state, UpdateState::Restarting(_)));
        assert_eq!(fs::read(&installed_exe).unwrap(), new_exe);
        assert_eq!(updater.take_restart_exe(), Some(installed_exe));
        assert!(restarted.load(Ordering::SeqCst));
        updater.stop();
        assert!(updater.wait_newer(snap.generation).is_none());
    }
}
