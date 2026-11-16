//! The Windows clipboard: Unicode text in and out, and the formats that
//! mark content clipboard monitors must leave alone.
//!
//! Text conversion is lossless or refused. Windows text is UTF-16 and may
//! hold unpaired surrogates, which have no UTF-8 form: such text is
//! reported as [`Content::NoText`] rather than sent with replacement
//! characters. In the other direction, text with a NUL character is not
//! written, since the clipboard would cut it there. Line endings are left
//! as they are both ways.

use std::marker::PhantomData;
use std::time::Duration;

use weft_protocol::MAX_CLIPBOARD_TEXT_LEN;
use windows_sys::Win32::Foundation::{GlobalFree, HANDLE, HWND};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};

use crate::sync::{Clipboard, Content};

/// Standard clipboard format: NUL-terminated UTF-16 text.
const CF_UNICODETEXT: u32 = 13;

/// Another process often holds the clipboard for a few milliseconds right
/// after changing it.
const OPEN_ATTEMPTS: u32 = 10;
const OPEN_RETRY_DELAY: Duration = Duration::from_millis(10);

/// NUL-terminated UTF-16, as Win32 takes strings.
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// The clipboard, opened on behalf of a window. Every call must come from
/// the thread that owns that window.
pub struct SystemClipboard {
    owner: HWND,
    /// Formats whose presence alone excludes the content.
    excluded_if_present: [u32; 2],
    /// Formats holding a DWORD, which excludes the content when it is 0.
    excluded_if_zero: [u32; 2],
}

impl SystemClipboard {
    /// `owner` is the window the clipboard is opened for. Writing needs a
    /// real one: Windows refuses data for a clipboard without an owner.
    pub fn new(owner: HWND) -> SystemClipboard {
        let register = |name: &str| {
            // SAFETY: `wide` returns a NUL-terminated string that outlives
            // the call.
            unsafe { RegisterClipboardFormatW(wide(name).as_ptr()) }
        };
        SystemClipboard {
            owner,
            excluded_if_present: [
                register("ExcludeClipboardContentFromMonitorProcessing"),
                register("Clipboard Viewer Ignore"),
            ],
            excluded_if_zero: [
                register("CanIncludeInClipboardHistory"),
                register("CanUploadToCloudClipboard"),
            ],
        }
    }

    /// Opens the clipboard, trying again for a moment while another process
    /// holds it.
    fn open(&self) -> Option<OpenedClipboard> {
        for attempt in 0..OPEN_ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(OPEN_RETRY_DELAY);
            }
            // SAFETY: `owner` is null or a window of this thread.
            if unsafe { OpenClipboard(self.owner) } != 0 {
                return Some(OpenedClipboard);
            }
        }
        tracing::warn!("the clipboard stayed locked by another application");
        None
    }

    fn is_excluded(&self, opened: &OpenedClipboard) -> bool {
        // SAFETY: no pointer involved; 0 (a format that failed to register)
        // is never available.
        let available = |format: u32| unsafe { IsClipboardFormatAvailable(format) } != 0;
        self.excluded_if_present.into_iter().any(available)
            || self
                .excluded_if_zero
                .into_iter()
                .filter(|format| available(*format))
                // Present but unreadable: stay on the safe side.
                .any(|format| read_dword(format, opened) != Some(1))
    }
}

/// Proof that this thread has the clipboard open; closes it when dropped.
struct OpenedClipboard;

impl Drop for OpenedClipboard {
    fn drop(&mut self) {
        // SAFETY: only built after `OpenClipboard` succeeded on this thread.
        unsafe { CloseClipboard() };
    }
}

/// A clipboard memory block locked for reading; unlocked when dropped. The
/// clipboard owns the block: it is never freed here.
struct Locked<'a> {
    handle: HANDLE,
    data: *const u8,
    len: usize,
    /// The block is only valid while the clipboard stays open.
    _opened: PhantomData<&'a OpenedClipboard>,
}

impl<'a> Locked<'a> {
    fn new(format: u32, _opened: &'a OpenedClipboard) -> Option<Locked<'a>> {
        // SAFETY: the clipboard is open, so the handle it returns is valid.
        unsafe {
            let handle = GetClipboardData(format);
            if handle.is_null() {
                return None;
            }
            let data = GlobalLock(handle);
            if data.is_null() {
                return None;
            }
            Some(Locked {
                handle,
                data: data.cast(),
                len: GlobalSize(handle),
                _opened: PhantomData,
            })
        }
    }
}

impl Drop for Locked<'_> {
    fn drop(&mut self) {
        // SAFETY: `handle` was locked in `new`.
        unsafe { GlobalUnlock(self.handle) };
    }
}

/// Reads a format that holds a DWORD, normalised to 0 or 1.
fn read_dword(format: u32, opened: &OpenedClipboard) -> Option<u32> {
    let block = Locked::new(format, opened)?;
    if block.len < size_of::<u32>() {
        return None;
    }
    // SAFETY: the block is locked and at least 4 bytes long.
    let value = unsafe { block.data.cast::<u32>().read_unaligned() };
    Some(u32::from(value != 0))
}

impl Clipboard for SystemClipboard {
    fn sequence(&self) -> u32 {
        // SAFETY: no argument, no side effect.
        unsafe { GetClipboardSequenceNumber() }
    }

    fn read(&mut self) -> Content {
        let Some(opened) = self.open() else {
            return Content::NoText;
        };
        if self.is_excluded(&opened) {
            return Content::Excluded;
        }
        let Some(block) = Locked::new(CF_UNICODETEXT, &opened) else {
            return Content::NoText;
        };
        // SAFETY: the block is locked, `len` bytes long, and global memory
        // is aligned for u16.
        let units = unsafe { std::slice::from_raw_parts(block.data.cast::<u16>(), block.len / 2) };
        // The block may be larger than the text: it ends at the first NUL.
        let end = units.iter().position(|unit| *unit == 0);
        let units = &units[..end.unwrap_or(units.len())];
        // Every UTF-16 unit takes at least one byte of UTF-8.
        if units.len() > MAX_CLIPBOARD_TEXT_LEN {
            return Content::TooLarge;
        }
        match String::from_utf16(units) {
            Ok(text) => Content::Text(text),
            Err(_) => Content::NoText,
        }
    }

    fn write(&mut self, text: &str) -> Option<u32> {
        if text.contains('\0') {
            tracing::warn!("received text holds a NUL character; not written");
            return None;
        }
        let units = wide(text);
        let bytes = size_of_val(units.as_slice());
        // SAFETY: the block is `bytes` long and locked while it is filled.
        // It belongs to this function until `SetClipboardData` succeeds,
        // and is freed on every path where that does not happen.
        unsafe {
            let block = GlobalAlloc(GMEM_MOVEABLE, bytes);
            if block.is_null() {
                return None;
            }
            let data = GlobalLock(block);
            if data.is_null() {
                GlobalFree(block);
                return None;
            }
            std::ptr::copy_nonoverlapping(units.as_ptr(), data.cast::<u16>(), units.len());
            GlobalUnlock(block);

            let Some(opened) = self.open() else {
                GlobalFree(block);
                return None;
            };
            if EmptyClipboard() == 0 || SetClipboardData(CF_UNICODETEXT, block).is_null() {
                GlobalFree(block);
                return None;
            }
            drop(opened);
        }
        // Read once the clipboard is closed, when the change is complete.
        // Another application writing in between would be taken for this
        // write and its content skipped; the window is a few microseconds.
        Some(self.sequence())
    }
}
