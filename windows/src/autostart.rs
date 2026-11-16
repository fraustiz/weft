//! "Start with Windows": the value `Weft` under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, which Windows runs
//! at sign-in. It starts the app with [`STARTUP_ARG`], window hidden.
//!
//! Nothing is cached: what the settings show is read from the registry.

use std::io;
use std::path::Path;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};

use crate::clipboard::wide;

/// The argument the entry passes: start in the notification area only.
pub const STARTUP_ARG: &str = "--startup";

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "Weft";

/// The command line the entry holds for the executable at `exe`.
fn command_for(exe: &Path) -> String {
    format!("\"{}\" {STARTUP_ARG}", exe.display())
}

fn check(status: WIN32_ERROR) -> io::Result<()> {
    match status {
        ERROR_SUCCESS => Ok(()),
        status => Err(io::Error::from_raw_os_error(status as i32)),
    }
}

/// Whether the entry exists.
pub fn is_enabled() -> bool {
    let (key, name) = (wide(RUN_KEY), wide(VALUE_NAME));
    // SAFETY: both strings are NUL-terminated and outlive the call; no
    // data is asked for, only whether the value exists.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            null_mut(),
            null_mut(),
        )
    };
    status == ERROR_SUCCESS
}

/// Creates the entry for the running executable, or removes it.
pub fn set_enabled(enabled: bool) -> io::Result<()> {
    let (key, name) = (wide(RUN_KEY), wide(VALUE_NAME));
    if !enabled {
        // SAFETY: both strings are NUL-terminated and outlive the call.
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) };
        return match status {
            ERROR_FILE_NOT_FOUND => Ok(()),
            status => check(status),
        };
    }
    let command = wide(&command_for(&std::env::current_exe()?));
    // SAFETY: the strings are NUL-terminated and outlive the call; the data
    // is the command with its NUL, of the size given.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            REG_SZ,
            command.as_ptr().cast(),
            size_of_val(command.as_slice()) as u32,
        )
    };
    check(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The registry itself is never written by tests.
    #[test]
    fn the_command_quotes_the_path_and_starts_hidden() {
        assert_eq!(
            command_for(Path::new(r"C:\Program Files\Weft\weft-windows.exe")),
            r#""C:\Program Files\Weft\weft-windows.exe" --startup"#
        );
    }
}
