//! What a device keeps on disk: its identity, its relay, the devices it
//! trusts, and per-peer message counters and replay state.
//!
//! Everything lives in one JSON file, `state.json`, in the state directory.
//! It holds the identity seeds, so it is created with mode 0600 on Unix and
//! always replaced atomically (temporary file, then rename). Changes go
//! through [`StateDir::update`], which takes an exclusive lock, re-reads the
//! file and saves it again, so a `weft send` running next to a `weft listen`
//! on the same state does not lose the other's changes.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use weft_protocol::{Identity, PeerKeys, device_id_from_sign_pk};

use crate::{Error, Result, hex};

/// Name of the state file inside the state directory.
pub const STATE_FILE: &str = "state.json";
const TEMP_FILE: &str = "state.json.tmp";
const LOCK_FILE: &str = "state.lock";
/// How many counters back a receiver remembers message ids (spec §3).
pub const REPLAY_WINDOW: u64 = 1024;
/// Version of the state file format.
const FORMAT_VERSION: u32 = 1;

/// The whole state of a device. Holds the identity seeds: never log it.
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    format: u32,
    #[serde(with = "hex_array")]
    sign_seed: [u8; 32],
    #[serde(with = "hex_array")]
    kx_seed: [u8; 32],
    pub relay_url: String,
    #[serde(default)]
    pub devices: Vec<TrustedDevice>,
    /// Keyed by the peer's device id in hex. Kept when a device is removed,
    /// so counters keep growing if it is trusted again with the same keys.
    #[serde(default)]
    pub peers: BTreeMap<String, PeerState>,
}

/// A device this one encrypts to and accepts messages from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedDevice {
    #[serde(with = "hex_array")]
    pub sign_pk: [u8; 32],
    #[serde(with = "hex_array")]
    pub kx_pk: [u8; 32],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl TrustedDevice {
    pub fn device_id(&self) -> [u8; 16] {
        device_id_from_sign_pk(&self.sign_pk)
    }

    pub fn keys(&self) -> PeerKeys {
        PeerKeys {
            sign_pk: self.sign_pk,
            kx_pk: self.kx_pk,
        }
    }
}

/// Counters exchanged with one peer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerState {
    /// Counter of the last envelope sent to this peer; the next one is +1.
    #[serde(default)]
    pub send_counter: u64,
    /// Highest counter accepted from this peer.
    #[serde(default)]
    pub recv_highest: u64,
    /// Message ids accepted from this peer within the replay window.
    #[serde(default)]
    pub recv_seen: Vec<SeenMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeenMessage {
    pub counter: u64,
    #[serde(with = "hex_array")]
    pub id: [u8; 16],
}

/// Why the replay check refused an envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayError {
    /// Counter 0 is never valid.
    ZeroCounter,
    /// The counter is older than the replay window.
    TooOld,
    /// The message id was already accepted.
    Duplicate,
}

impl PeerState {
    /// Checks an incoming `(counter, message_id)` without recording it.
    pub fn check(&self, counter: u64, message_id: &[u8; 16]) -> Result<(), ReplayError> {
        if counter == 0 {
            return Err(ReplayError::ZeroCounter);
        }
        if self.recv_highest >= REPLAY_WINDOW && counter <= self.recv_highest - REPLAY_WINDOW {
            return Err(ReplayError::TooOld);
        }
        if self.recv_seen.iter().any(|seen| seen.id == *message_id) {
            return Err(ReplayError::Duplicate);
        }
        Ok(())
    }

    /// Checks an incoming envelope and, if it passes, records it.
    pub fn accept(&mut self, counter: u64, message_id: &[u8; 16]) -> Result<(), ReplayError> {
        self.check(counter, message_id)?;
        self.recv_highest = self.recv_highest.max(counter);
        self.recv_seen.push(SeenMessage {
            counter,
            id: *message_id,
        });
        let floor = self.recv_highest.saturating_sub(REPLAY_WINDOW);
        self.recv_seen.retain(|seen| seen.counter > floor);
        Ok(())
    }
}

impl State {
    /// A new identity, from seeds drawn from the OS random generator.
    pub fn generate(relay_url: &str) -> Result<State> {
        crate::session::relay_endpoint(relay_url)?;
        let mut sign_seed = [0u8; 32];
        let mut kx_seed = [0u8; 32];
        getrandom::fill(&mut sign_seed)
            .and_then(|()| getrandom::fill(&mut kx_seed))
            .map_err(|err| Error::Io {
                context: "reading OS randomness".into(),
                source: io::Error::other(err.to_string()),
            })?;
        Ok(State {
            format: FORMAT_VERSION,
            sign_seed,
            kx_seed,
            relay_url: relay_url.to_owned(),
            devices: Vec::new(),
            peers: BTreeMap::new(),
        })
    }

    /// The device's key pairs, rebuilt from the seeds.
    pub fn identity(&self) -> Identity {
        Identity::from_seeds(self.sign_seed, self.kx_seed)
    }

    pub fn device(&self, device_id: &[u8; 16]) -> Option<&TrustedDevice> {
        self.devices
            .iter()
            .find(|device| device.device_id() == *device_id)
    }

    /// Adds `keys` to the trusted devices, or updates their kx key and name.
    /// Returns `false` if they were already trusted unchanged.
    pub fn trust(&mut self, keys: PeerKeys, name: Option<String>) -> bool {
        let id = keys.device_id();
        if let Some(device) = self.devices.iter_mut().find(|d| d.device_id() == id) {
            let changed = device.kx_pk != keys.kx_pk || (name.is_some() && device.name != name);
            device.kx_pk = keys.kx_pk;
            if name.is_some() {
                device.name = name;
            }
            return changed;
        }
        self.devices.push(TrustedDevice {
            sign_pk: keys.sign_pk,
            kx_pk: keys.kx_pk,
            name,
        });
        true
    }

    /// Drops a device from the trusted list. Returns `false` if it was not
    /// trusted.
    pub fn untrust(&mut self, device_id: &[u8; 16]) -> bool {
        let before = self.devices.len();
        self.devices
            .retain(|device| device.device_id() != *device_id);
        self.devices.len() != before
    }

    pub fn peer(&self, device_id: &[u8; 16]) -> Option<&PeerState> {
        self.peers.get(&hex(device_id))
    }

    pub fn peer_mut(&mut self, device_id: &[u8; 16]) -> &mut PeerState {
        self.peers.entry(hex(device_id)).or_default()
    }

    /// Increments and returns the send counter for `device_id`.
    pub fn next_counter(&mut self, device_id: &[u8; 16]) -> u64 {
        let peer = self.peer_mut(device_id);
        peer.send_counter += 1;
        peer.send_counter
    }

    /// The trusted devices whose id starts with the hex `prefix`.
    pub fn find_by_prefix(&self, prefix: &str) -> Result<&TrustedDevice> {
        let prefix = prefix.to_ascii_lowercase();
        let mut matches = self
            .devices
            .iter()
            .filter(|device| !prefix.is_empty() && hex(&device.device_id()).starts_with(&prefix));
        match (matches.next(), matches.next()) {
            (Some(device), None) => Ok(device),
            (Some(_), Some(_)) => Err(Error::AmbiguousDevice(prefix)),
            (None, _) => Err(Error::UnknownDevice(prefix)),
        }
    }

    fn validate(&self, path: &Path) -> Result<()> {
        let invalid = |what: &str| Err(Error::InvalidState(path.to_owned(), what.to_owned()));
        if self.format != FORMAT_VERSION {
            return invalid("unsupported format version");
        }
        if crate::session::relay_endpoint(&self.relay_url).is_err() {
            return invalid("invalid relay url");
        }
        Ok(())
    }
}

/// The directory holding `state.json`.
#[derive(Debug, Clone)]
pub struct StateDir {
    dir: PathBuf,
}

impl StateDir {
    pub fn new(dir: impl Into<PathBuf>) -> StateDir {
        StateDir { dir: dir.into() }
    }

    pub fn path(&self) -> &Path {
        &self.dir
    }

    pub fn file(&self) -> PathBuf {
        self.dir.join(STATE_FILE)
    }

    pub fn exists(&self) -> bool {
        self.file().exists()
    }

    /// Reads the state file.
    pub fn load(&self) -> Result<State> {
        let path = self.file();
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Err(Error::NotInitialised(self.dir.clone()));
            }
            Err(source) => return Err(io_error(format!("reading {}", path.display()), source)),
        };
        let state: State = serde_json::from_slice(&bytes)
            .map_err(|err| Error::InvalidState(path.clone(), err.to_string()))?;
        state.validate(&path)?;
        Ok(state)
    }

    /// Writes a fresh state. Fails if one exists, unless `force`.
    pub fn create(&self, state: &State, force: bool) -> Result<()> {
        let _lock = self.lock()?;
        if self.exists() && !force {
            return Err(Error::AlreadyInitialised(self.file()));
        }
        self.save(state)
    }

    /// Loads the state, or creates a new identity on `relay_url` if there is
    /// none. Fails if the existing state uses another relay.
    pub fn load_or_create(&self, relay_url: &str) -> Result<State> {
        let _lock = self.lock()?;
        match self.load() {
            Ok(state) if state.relay_url == relay_url => Ok(state),
            Ok(state) => Err(Error::RelayUrlMismatch {
                state: state.relay_url,
                code: relay_url.to_owned(),
            }),
            Err(Error::NotInitialised(_)) => {
                let state = State::generate(relay_url)?;
                self.save(&state)?;
                Ok(state)
            }
            Err(err) => Err(err),
        }
    }

    /// Applies `change` to the state on disk, under an exclusive lock, and
    /// saves it if anything changed. Returns what `change` returned and the
    /// state as saved.
    pub fn update<T>(&self, change: impl FnOnce(&mut State) -> T) -> Result<(T, State)> {
        let _lock = self.lock()?;
        let mut state = self.load()?;
        let before = serde_json::to_vec(&state).expect("state serialises");
        let result = change(&mut state);
        let after = serde_json::to_vec(&state).expect("state serialises");
        if before != after {
            self.save(&state)?;
        }
        Ok((result, state))
    }

    /// An exclusive lock on the state, released when the file is dropped.
    /// Held only for short, synchronous sections.
    fn lock(&self) -> Result<File> {
        create_private_dir(&self.dir)?;
        let path = self.dir.join(LOCK_FILE);
        let file = private_options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|err| io_error(format!("opening {}", path.display()), err))?;
        file.lock()
            .map_err(|err| io_error(format!("locking {}", path.display()), err))?;
        Ok(file)
    }

    /// Writes the state atomically: temporary file (0600), fsync, rename.
    /// The caller holds the lock.
    fn save(&self, state: &State) -> Result<()> {
        create_private_dir(&self.dir)?;
        let temp = self.dir.join(TEMP_FILE);
        let target = self.file();
        let context = |what: &str| format!("{what} {}", temp.display());
        // A leftover from an interrupted save: we hold the lock, so it is stale.
        match fs::remove_file(&temp) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(io_error(context("removing"), err)),
        }
        let mut json = serde_json::to_vec_pretty(state).expect("state serialises");
        json.push(b'\n');
        let mut file = private_options()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|err| io_error(context("creating"), err))?;
        file.write_all(&json)
            .and_then(|()| file.sync_all())
            .map_err(|err| io_error(context("writing"), err))?;
        drop(file);
        fs::rename(&temp, &target)
            .map_err(|err| io_error(format!("replacing {}", target.display()), err))?;
        #[cfg(unix)]
        if let Ok(dir) = File::open(&self.dir) {
            let _ = dir.sync_all();
        }
        Ok(())
    }
}

fn io_error(context: String, source: io::Error) -> Error {
    Error::Io { context, source }
}

/// Open options that create files readable by the owner only.
fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

/// Creates the state directory (0700 on Unix) if it does not exist.
fn create_private_dir(dir: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(dir)
        .map_err(|err| io_error(format!("creating {}", dir.display()), err))
}

/// Serde helpers for fixed-size byte arrays written as hex strings.
mod hex_array {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer, const N: usize>(
        bytes: &[u8; N],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&crate::hex(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(
        deserializer: D,
    ) -> Result<[u8; N], D::Error> {
        let text = String::deserialize(deserializer)?;
        crate::unhex(&text).ok_or_else(|| D::Error::custom(format!("expected {N} bytes of hex")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_window() {
        let mut peer = PeerState::default();
        let id = |n: u8| [n; 16];
        assert_eq!(peer.accept(0, &id(0)), Err(ReplayError::ZeroCounter));
        assert_eq!(peer.accept(1, &id(1)), Ok(()));
        assert_eq!(peer.accept(1, &id(1)), Err(ReplayError::Duplicate));
        // Out of order but inside the window.
        assert_eq!(peer.accept(5, &id(5)), Ok(()));
        assert_eq!(peer.accept(3, &id(3)), Ok(()));
        assert_eq!(peer.accept(2000, &id(20)), Ok(()));
        // 2000 - 1024 = 976 is the newest counter outside the window.
        assert_eq!(peer.accept(976, &id(9)), Err(ReplayError::TooOld));
        assert_eq!(peer.accept(977, &id(9)), Ok(()));
        assert_eq!(peer.recv_seen.len(), 2);
        assert_eq!(peer.accept(2000, &id(20)), Err(ReplayError::Duplicate));
    }

    #[test]
    fn prefix_lookup() {
        let mut state = State::generate("ws://127.0.0.1:1/v1").unwrap();
        let a = Identity::generate();
        let b = Identity::generate();
        for identity in [&a, &b] {
            state.trust(
                PeerKeys {
                    sign_pk: identity.sign_pk(),
                    kx_pk: identity.kx_pk(),
                },
                None,
            );
        }
        let full = hex(&a.device_id());
        assert_eq!(state.find_by_prefix(&full).unwrap().sign_pk, a.sign_pk());
        assert!(matches!(
            state.find_by_prefix(""),
            Err(Error::UnknownDevice(_))
        ));
        assert!(matches!(
            state.find_by_prefix("zz"),
            Err(Error::UnknownDevice(_))
        ));
    }
}
