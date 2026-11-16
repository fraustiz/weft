//! The system's Open dialog, to choose files to send.

use std::path::PathBuf;

use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_ALLOWMULTISELECT, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR,
    OFN_PATHMUSTEXIST, OPENFILENAMEW,
};

use crate::clipboard::wide;
use crate::i18n::t;

/// Room for the chosen paths, in UTF-16 units.
const BUFFER: usize = 64 * 1024;

/// The paths in the buffer the dialog filled: one full path, or a folder
/// followed by file names, each NUL-terminated, the list ending with an
/// empty string.
fn chosen(buffer: &[u16]) -> Vec<PathBuf> {
    let parts: Vec<String> = buffer
        .split(|&unit| unit == 0)
        .take_while(|part| !part.is_empty())
        .map(String::from_utf16_lossy)
        .collect();
    match parts.as_slice() {
        [] => Vec::new(),
        [path] => vec![PathBuf::from(path)],
        [folder, names @ ..] => names
            .iter()
            .map(|name| PathBuf::from(folder).join(name))
            .collect(),
    }
}

/// Asks for files, over the main window, and blocks until the dialog
/// closes. Returns nothing if the user cancelled.
///
/// The dialog runs on a thread of its own, set up for COM as the shell
/// expects, so it never runs inside the window's message loop.
pub fn pick_files() -> Vec<PathBuf> {
    let owner = crate::window::handle();
    let dialog = std::thread::spawn(move || {
        let title = wide(t("Send files", "Envoyer des fichiers"));
        let mut buffer = vec![0u16; BUFFER];
        // SAFETY: all-zero is a valid OPENFILENAMEW; the buffer and the
        // title outlive the call, which writes at most `nMaxFile` units.
        unsafe {
            CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
            let mut open: OPENFILENAMEW = std::mem::zeroed();
            open.lStructSize = size_of::<OPENFILENAMEW>() as u32;
            open.hwndOwner = owner as _;
            open.lpstrFile = buffer.as_mut_ptr();
            open.nMaxFile = BUFFER as u32;
            open.lpstrTitle = title.as_ptr();
            open.Flags = OFN_EXPLORER
                | OFN_ALLOWMULTISELECT
                | OFN_FILEMUSTEXIST
                | OFN_PATHMUSTEXIST
                | OFN_NOCHANGEDIR;
            if GetOpenFileNameW(&mut open) == 0 {
                return Vec::new();
            }
        }
        chosen(&buffer)
    });
    dialog.join().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(parts: &[&str]) -> Vec<u16> {
        let mut buffer: Vec<u16> = Vec::new();
        for part in parts {
            buffer.extend(part.encode_utf16());
            buffer.push(0);
        }
        buffer.extend([0, 0, 0]);
        buffer
    }

    #[test]
    fn one_file_or_a_folder_and_names() {
        assert!(chosen(&units(&[])).is_empty());
        assert_eq!(
            chosen(&units(&[r"C:\data\a.txt"])),
            [PathBuf::from(r"C:\data\a.txt")]
        );
        assert_eq!(
            chosen(&units(&[r"C:\data", "a.txt", "b c.jpg"])),
            [
                PathBuf::from(r"C:\data\a.txt"),
                PathBuf::from(r"C:\data\b c.jpg")
            ]
        );
    }
}
