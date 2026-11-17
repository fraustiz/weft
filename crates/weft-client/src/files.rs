//! The disk side of file transfers: where partial files wait, how long they
//! are kept, and how a finished file reaches the Downloads folder without
//! overwriting anything. The decisions are in [`crate::transfer`].
//!
//! The free space and the move that never replaces a file ask the system
//! on Windows; elsewhere the free space is unknown, so not checked, and the
//! move checks for the target before renaming. Where the Downloads folder
//! is, the platform says ([`crate::worker::Hooks::downloads`]).

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use weft_cli::hex;

use crate::transfer::{PARTIAL_LIFETIME, numbered};

const PARTIAL_EXTENSION: &str = "part";

/// The private folder of partial files, inside the state directory.
pub fn partial_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("partial")
}

/// Where the transfer `id` from `from` is written until it is complete.
pub fn partial_path(dir: &Path, from: &[u8; 16], id: &[u8; 16]) -> PathBuf {
    dir.join(format!("{}-{}.{PARTIAL_EXTENSION}", hex(from), hex(id)))
}

fn is_stale(modified: io::Result<SystemTime>, now: SystemTime) -> bool {
    modified
        .ok()
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(|age| age >= PARTIAL_LIFETIME)
}

/// The bytes kept of a partial file for a resume: 0 if there is none, or
/// if it was last written more than a day ago (it is deleted then).
pub fn kept_len(path: &Path) -> u64 {
    let Ok(metadata) = std::fs::metadata(path) else {
        return 0;
    };
    if is_stale(metadata.modified(), SystemTime::now()) {
        let _ = std::fs::remove_file(path);
        return 0;
    }
    metadata.len()
}

/// Deletes the partial files last written more than a day ago.
pub fn remove_stale(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let partial = path.extension().is_some_and(|ext| ext == PARTIAL_EXTENSION);
        if partial && is_stale(entry.metadata().and_then(|m| m.modified()), now) {
            tracing::debug!("deleting a partial file older than a day");
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The free space on the volume of `dir`, if the system tells.
#[cfg(windows)]
pub fn free_space(dir: &Path) -> Option<u64> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let path = wide_path(dir);
    let mut free = 0u64;
    // SAFETY: `path` is NUL-terminated and outlives the call; the other
    // outputs are optional.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(free)
}

#[cfg(not(windows))]
pub fn free_space(_dir: &Path) -> Option<u64> {
    None
}

#[cfg(windows)]
fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain([0]).collect()
}

/// Moves `from` to `to`, failing with [`io::ErrorKind::AlreadyExists`]
/// rather than replacing a file, and copying across volumes.
#[cfg(windows)]
fn move_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_COPY_ALLOWED, MoveFileExW};
    let (from, to) = (wide_path(from), wide_path(to));
    // SAFETY: both paths are NUL-terminated and outlive the call. Without
    // MOVEFILE_REPLACE_EXISTING an existing target makes it fail.
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_COPY_ALLOWED) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
fn move_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    if to.exists() {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    std::fs::rename(from, to)
}

/// Moves the finished file `from` into `dir` as `name`, or as
/// `name (1)`, `name (2)`… if taken. Returns where it went.
pub fn move_unique(from: &Path, dir: &Path, name: &str) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    for attempt in 0..10_000 {
        let target = dir.join(numbered(name, attempt));
        match move_no_replace(from, &target) {
            Ok(()) => return Ok(target),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(io::ErrorKind::AlreadyExists.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finished_files_never_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let downloads = temp.path().join("Downloads");
        let mut moved = Vec::new();
        for content in ["one", "two", "three"] {
            let partial = temp.path().join(format!("{content}.part"));
            std::fs::write(&partial, content).unwrap();
            moved.push(move_unique(&partial, &downloads, "notes.txt").unwrap());
            assert!(!partial.exists());
        }
        let names: Vec<_> = moved
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["notes.txt", "notes (1).txt", "notes (2).txt"]);
        assert_eq!(std::fs::read_to_string(&moved[0]).unwrap(), "one");
        assert_eq!(std::fs::read_to_string(&moved[2]).unwrap(), "three");
    }

    #[test]
    fn partial_files_are_kept_with_their_length() {
        let temp = tempfile::tempdir().unwrap();
        let path = partial_path(temp.path(), &[1; 16], &[2; 16]);
        assert_eq!(kept_len(&path), 0);
        std::fs::write(&path, [0u8; 10]).unwrap();
        assert_eq!(kept_len(&path), 10);
        remove_stale(temp.path());
        assert!(path.exists(), "a fresh partial file stays");
        assert!(is_stale(
            Ok(SystemTime::now() - PARTIAL_LIFETIME),
            SystemTime::now()
        ));
        assert!(!is_stale(Ok(SystemTime::now()), SystemTime::now()));
    }

    #[cfg(windows)]
    #[test]
    fn the_system_reports_free_space() {
        let temp = tempfile::tempdir().unwrap();
        assert!(free_space(temp.path()).is_some_and(|free| free > 0));
    }
}
