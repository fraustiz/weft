//! The real Windows clipboard.
//!
//! These tests replace whatever the clipboard holds and do not put it
//! back (other formats, such as images or files, could not be restored
//! faithfully), so they only run on request:
//!
//! ```text
//! cargo test --test clipboard -- --ignored --test-threads 1
//! ```

#![cfg(windows)]

use std::ptr::{null, null_mut};

use weft_windows::clipboard::{SystemClipboard, wide};
use weft_windows::sync::{Clipboard, Content};
use windows_sys::Win32::Foundation::{GlobalFree, HWND};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE};

/// A message-only window to own the clipboard, destroyed with the value.
struct Owner(HWND);

impl Owner {
    fn new() -> Owner {
        // SAFETY: "STATIC" is a system window class; the name outlives the
        // call.
        let window = unsafe {
            CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                null_mut(),
                null_mut(),
                null(),
            )
        };
        assert!(!window.is_null());
        Owner(window)
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        // SAFETY: the window was created by this thread.
        unsafe { DestroyWindow(self.0) };
    }
}

/// Adds a format holding `value` to what the clipboard already has, as a
/// password manager does next to the text it copies.
fn add_marker(owner: &Owner, format_name: &str, value: u32) {
    // SAFETY: the block is 4 bytes long and locked while written; the
    // clipboard owns it once `SetClipboardData` succeeds.
    unsafe {
        let format = RegisterClipboardFormatW(wide(format_name).as_ptr());
        assert_ne!(format, 0);
        let block = GlobalAlloc(GMEM_MOVEABLE, size_of::<u32>());
        assert!(!block.is_null());
        let data = GlobalLock(block);
        assert!(!data.is_null());
        data.cast::<u32>().write_unaligned(value);
        GlobalUnlock(block);
        // Clipboard monitors open it for a moment after every change.
        let opened = (0..100).any(|_| {
            OpenClipboard(owner.0) != 0 || {
                std::thread::sleep(std::time::Duration::from_millis(10));
                false
            }
        });
        assert!(opened, "the clipboard stayed locked");
        let set = SetClipboardData(format, block);
        CloseClipboard();
        if set.is_null() {
            GlobalFree(block);
            panic!("could not add {format_name}");
        }
    }
}

fn is_text(content: Content, text: &str) -> bool {
    content == Content::Text(text.to_owned())
}

#[test]
#[ignore = "replaces the content of the real clipboard"]
fn unicode_text_round_trip() {
    let owner = Owner::new();
    let mut clipboard = SystemClipboard::new(owner.0);
    for text in [
        "plain text",
        "accents é à ü, emoji 🧵 👍🏽, kanji 漢字",
        "two\r\nlines\nand\ta tab",
        " ",
    ] {
        let before = clipboard.sequence();
        let written = clipboard.write(text).expect("writing the clipboard");
        assert_ne!(written, before);
        assert_eq!(written, clipboard.sequence());
        assert!(is_text(clipboard.read(), text), "{text:?}");
    }

    // The clipboard would cut the text at the NUL: refused, nothing changes.
    let before = clipboard.sequence();
    assert_eq!(clipboard.write("cut\0here"), None);
    assert_eq!(clipboard.sequence(), before);
    assert!(is_text(clipboard.read(), " "));
}

#[test]
#[ignore = "replaces the content of the real clipboard"]
fn content_marked_for_exclusion_is_not_read() {
    let owner = Owner::new();
    let mut clipboard = SystemClipboard::new(owner.0);

    for present in [
        "ExcludeClipboardContentFromMonitorProcessing",
        "Clipboard Viewer Ignore",
    ] {
        clipboard.write("marked").unwrap();
        add_marker(&owner, present, 0);
        assert!(clipboard.read() == Content::Excluded, "{present}");
    }
    for flag in ["CanIncludeInClipboardHistory", "CanUploadToCloudClipboard"] {
        clipboard.write("refused").unwrap();
        add_marker(&owner, flag, 0);
        assert!(clipboard.read() == Content::Excluded, "{flag} = 0");

        clipboard.write("allowed").unwrap();
        add_marker(&owner, flag, 1);
        assert!(is_text(clipboard.read(), "allowed"), "{flag} = 1");
    }

    // A new write carries no mark: it replaces everything.
    clipboard.write("unmarked").unwrap();
    assert!(is_text(clipboard.read(), "unmarked"));
}
