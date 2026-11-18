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

/// Expands dropped or pasted paths into regular files to send: regular files
/// are kept, folders contribute their regular top-level files (sorted), and
/// symlinks, missing entries and duplicates are ignored.
pub fn expand_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut push_unique = |candidate: PathBuf| {
        if !files.contains(&candidate) {
            files.push(candidate);
        }
    };
    for path in paths {
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_file() {
            push_unique(path);
        } else if meta.is_dir()
            && let Ok(entries) = std::fs::read_dir(&path)
        {
            let mut children: Vec<PathBuf> = entries
                .flatten()
                .filter(|entry| {
                    entry
                        .file_type()
                        .is_ok_and(|kind| !kind.is_symlink() && kind.is_file())
                })
                .map(|entry| entry.path())
                .collect();
            children.sort();
            for child in children {
                push_unique(child);
            }
        }
    }
    files
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

    #[test]
    fn expand_paths_collects_regular_files_and_top_level_folder_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let top_file = temp.path().join("note.txt");
        std::fs::write(&top_file, "hello").unwrap();

        let folder = temp.path().join("photos");
        std::fs::create_dir(&folder).unwrap();
        let img_b = folder.join("b.png");
        let img_a = folder.join("a.png");
        std::fs::write(&img_b, "b").unwrap();
        std::fs::write(&img_a, "a").unwrap();

        // Nested subfolder files are not collected recursively.
        let nested = folder.join("nested");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(nested.join("deep.png"), "deep").unwrap();

        let empty_folder = temp.path().join("empty");
        std::fs::create_dir(&empty_folder).unwrap();

        #[cfg(windows)]
        let maybe_symlink = {
            let link = folder.join("shortcut.txt");
            if std::os::windows::fs::symlink_file(&top_file, &link).is_ok() {
                Some(link)
            } else {
                None
            }
        };

        let mut inputs = vec![
            empty_folder.clone(),
            top_file.clone(),
            folder,
            img_a.clone(),
            temp.path().join("missing.bin"),
        ];
        #[cfg(windows)]
        if let Some(link) = maybe_symlink {
            inputs.push(link);
        }

        assert_eq!(expand_paths(inputs), vec![top_file, img_a, img_b]);
        assert!(expand_paths([empty_folder]).is_empty());
    }
}
