//! The Windows clipboard: Unicode text, copied files and lossless PNG/DIB
//! images in and out, plus the formats that mark content clipboard monitors
//! must leave alone.
//!
//! Text conversion is lossless or refused. Windows text is UTF-16 and may
//! hold unpaired surrogates, which have no UTF-8 form: such text is
//! reported as [`Content::NoText`] rather than sent with replacement
//! characters. In the other direction, text with a NUL character is not
//! written, since the clipboard would cut it there. Line endings are left
//! as they are both ways.

use std::ffi::OsString;
use std::marker::PhantomData;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::OnceLock;
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
use windows_sys::Win32::UI::Shell::{DragQueryFileW, HDROP};

use crate::sync::{Clipboard, ClipboardImage, Content, ImageMime};

/// Standard clipboard format: device-independent bitmap (`BITMAPINFO`).
const CF_DIB: u32 = 8;
/// Standard clipboard format: NUL-terminated UTF-16 text.
const CF_UNICODETEXT: u32 = 13;
/// Standard clipboard format: shell file list (`HDROP`).
const CF_HDROP: u32 = 15;

/// Largest PNG accepted from or written to the clipboard (16 MiB).
pub const MAX_CLIPBOARD_IMAGE_LEN: usize = 16 * 1024 * 1024;

const PNG_SIGNATURE: [u8; 8] = *b"\x89PNG\r\n\x1a\n";
const BI_RGB: u32 = 0;
const BI_BITFIELDS: u32 = 3;
const BITMAPINFOHEADER_LEN: usize = 40;

/// Another process often holds the clipboard for a few milliseconds right
/// after changing it.
const OPEN_ATTEMPTS: u32 = 10;
const OPEN_RETRY_DELAY: Duration = Duration::from_millis(10);

/// NUL-terminated UTF-16, as Win32 takes strings.
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Reads the paths stored in an [`HDROP`] without freeing the handle.
///
/// # Safety
///
/// `hdrop` must be a valid drop handle (for example from `WM_DROPFILES`
/// before `DragFinish`, or from `GetClipboardData(CF_HDROP)` while the
/// clipboard is open).
pub(crate) unsafe fn hdrop_paths(hdrop: HDROP) -> Vec<PathBuf> {
    if hdrop.is_null() {
        return Vec::new();
    }
    // SAFETY: `0xFFFF_FFFF` queries the file count without writing to a buffer.
    let count = unsafe { DragQueryFileW(hdrop, 0xFFFF_FFFF, std::ptr::null_mut(), 0) };
    let mut paths = Vec::with_capacity(count as usize);
    for index in 0..count {
        // SAFETY: null buffer queries the length in UTF-16 units, NUL excluded.
        let len = unsafe { DragQueryFileW(hdrop, index, std::ptr::null_mut(), 0) };
        if len == 0 {
            continue;
        }
        let mut units = vec![0u16; len as usize + 1];
        // SAFETY: `units` has room for `len + 1` UTF-16 units including NUL.
        let copied =
            unsafe { DragQueryFileW(hdrop, index, units.as_mut_ptr(), units.len() as u32) };
        if copied > 0 {
            paths.push(PathBuf::from(OsString::from_wide(
                &units[..copied as usize],
            )));
        }
    }
    paths
}

/// Reads the files and folders currently copied on the clipboard (`CF_HDROP`).
/// `owner` must be null or a window owned by the calling thread.
pub fn read_clipboard_files(owner: HWND) -> Vec<PathBuf> {
    SystemClipboard::new(owner).read_files()
}

/// The clipboard, opened on behalf of a window. Every call must come from
/// the thread that owns that window (or any thread when `owner` is null and
/// only reading).
pub struct SystemClipboard {
    owner: HWND,
    /// Registered `"PNG"` clipboard format, or 0 if registration failed.
    png_format: u32,
    /// Hash of the PNG actually written, including after a JPEG conversion.
    received_image: Option<[u8; 16]>,
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
            png_format: register("PNG"),
            received_image: None,
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
        is_excluded_by(
            self.excluded_if_present,
            self.excluded_if_zero,
            available,
            |format| read_dword(format, opened),
        )
    }

    /// Reads regular files and folders from `CF_HDROP` when the clipboard
    /// is not marked excluded. Symlinks and missing entries are skipped.
    pub fn read_files(&self) -> Vec<PathBuf> {
        let Some(opened) = self.open() else {
            return Vec::new();
        };
        if self.is_excluded(&opened) {
            return Vec::new();
        }
        // SAFETY: no pointer involved.
        if unsafe { IsClipboardFormatAvailable(CF_HDROP) } == 0 {
            return Vec::new();
        }
        // SAFETY: the clipboard is open on this thread and retains ownership
        // of the returned handle; `hdrop_paths` only reads it via
        // `DragQueryFileW` before `opened` closes the clipboard.
        let raw = unsafe {
            let handle = GetClipboardData(CF_HDROP);
            hdrop_paths(handle as HDROP)
        };
        drop(opened);
        raw.into_iter()
            .filter(|path| {
                std::fs::symlink_metadata(path).is_ok_and(|meta| {
                    !meta.file_type().is_symlink() && (meta.is_file() || meta.is_dir())
                })
            })
            .collect()
    }

    /// Reads a lossless PNG image from the clipboard when not excluded:
    /// returns the `"PNG"` format directly when valid and at most
    /// [`MAX_CLIPBOARD_IMAGE_LEN`], or converts a 24-bit/32-bit `CF_DIB`
    /// bitmap to PNG.
    pub fn read_png_bytes(&self) -> Option<Vec<u8>> {
        let opened = self.open()?;
        if self.is_excluded(&opened) {
            return None;
        }
        // SAFETY: no pointer involved; 0 is never available.
        let has_png =
            self.png_format != 0 && unsafe { IsClipboardFormatAvailable(self.png_format) } != 0;
        if has_png && let Some(block) = Locked::new(self.png_format, &opened) {
            // SAFETY: `block` is locked for `block.len` bytes while `opened` lives.
            let bytes = unsafe { std::slice::from_raw_parts(block.data, block.len) };
            if let Some(png) = valid_png_slice(bytes) {
                return Some(png.to_vec());
            }
        }
        // SAFETY: no pointer involved.
        if unsafe { IsClipboardFormatAvailable(CF_DIB) } != 0
            && let Some(block) = Locked::new(CF_DIB, &opened)
        {
            // SAFETY: `block` is locked for `block.len` bytes while `opened` lives.
            let bytes = unsafe { std::slice::from_raw_parts(block.data, block.len) };
            return dib_to_png(bytes);
        }
        None
    }

    /// Writes `png` to the clipboard as `"PNG"` (and as `CF_DIB` when GDI+
    /// can decode it, so legacy Win32 applications can paste it too).
    /// Returns the new clipboard sequence number.
    pub fn write_png_bytes(&mut self, png: &[u8]) -> Option<u32> {
        self.write_png_marked(png, false)
    }

    /// Like [`Self::write_png_bytes`], plus the markers that keep clipboard
    /// monitors, clipboard history and cloud clipboard away from the image.
    pub fn write_png_bytes_private(&mut self, png: &[u8]) -> Option<u32> {
        self.write_png_marked(png, true)
    }

    fn write_png_marked(&mut self, png: &[u8], private: bool) -> Option<u32> {
        let png = valid_png_slice(png)?;
        if self.png_format == 0 {
            return None;
        }
        let png_block = global_bytes(png)?;
        // Decode before opening the clipboard so the lock is held briefly.
        let dib_block = png_to_dib(png).and_then(|dib| global_bytes(&dib));
        let Some(opened) = self.open() else {
            // SAFETY: neither block was handed to the clipboard.
            unsafe {
                GlobalFree(png_block);
                if let Some(dib) = dib_block {
                    GlobalFree(dib);
                }
            }
            return None;
        };
        // SAFETY: the clipboard is open on this thread. Any block rejected
        // by `SetClipboardData` is freed here; accepted blocks belong to the
        // system.
        unsafe {
            if EmptyClipboard() == 0 || SetClipboardData(self.png_format, png_block).is_null() {
                GlobalFree(png_block);
                if let Some(dib) = dib_block {
                    GlobalFree(dib);
                }
                return None;
            }
            if let Some(dib) = dib_block
                && SetClipboardData(CF_DIB, dib).is_null()
            {
                GlobalFree(dib);
                tracing::debug!("could not publish CF_DIB alongside PNG");
            }
            if private {
                self.set_private_markers();
            }
        }
        drop(opened);
        Some(self.sequence())
    }

    /// Caller must hold the open clipboard on this thread.
    unsafe fn set_private_markers(&self) {
        // A DWORD of 0 in each: the first format excludes the content by its
        // presence, the others by their value.
        let markers = [self.excluded_if_present[0]]
            .into_iter()
            .chain(self.excluded_if_zero)
            .filter(|format| *format != 0);
        for format in markers {
            let Some(marker) = global_bytes(&0u32.to_ne_bytes()) else {
                continue;
            };
            // SAFETY: the caller has the clipboard open; failed blocks are freed.
            unsafe {
                if SetClipboardData(format, marker).is_null() {
                    GlobalFree(marker);
                    tracing::warn!("could not mark the clipboard content as private");
                }
            }
        }
    }
}

fn is_excluded_by(
    excluded_if_present: [u32; 2],
    excluded_if_zero: [u32; 2],
    mut available: impl FnMut(u32) -> bool,
    mut dword: impl FnMut(u32) -> Option<u32>,
) -> bool {
    excluded_if_present
        .into_iter()
        .any(|format| format != 0 && available(format))
        || excluded_if_zero
            .into_iter()
            .filter(|format| *format != 0 && available(*format))
            // Present but unreadable: stay on the safe side.
            .any(|format| dword(format) != Some(1))
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
            drop(opened);
            let Some(image) = self
                .read_png_bytes()
                .and_then(|bytes| ClipboardImage::new(ImageMime::Png, bytes))
            else {
                return Content::NoText;
            };
            if self.received_image == Some(image.hash()) {
                return Content::Excluded;
            }
            self.received_image = None;
            return Content::Image(image);
        };
        self.received_image = None;
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
        self.write_marked(text, false)
    }

    fn write_private(&mut self, text: &str) -> Option<u32> {
        self.write_marked(text, true)
    }

    fn write_image(&mut self, image: &ClipboardImage) -> Option<u32> {
        if !image.is_valid() {
            return None;
        }
        let png = match image.mime {
            ImageMime::Png => image.bytes.clone(),
            ImageMime::Jpeg => dib_to_png(&decode_image_to_dib(&image.bytes)?)?,
        };
        let sequence = self.write_png_bytes(&png)?;
        self.received_image = Some(weft_protocol::content_hash(&png));
        Some(sequence)
    }
}

impl SystemClipboard {
    /// Writes `text`; when `private`, adds the formats that keep clipboard
    /// monitors, the clipboard history and the cloud clipboard away from it.
    fn write_marked(&mut self, text: &str, private: bool) -> Option<u32> {
        if text.contains('\0') {
            tracing::warn!("text holds a NUL character; not written");
            return None;
        }
        let units = wide(text);
        let block = global_block(&units)?;
        let Some(opened) = self.open() else {
            // SAFETY: the block was never given to the clipboard.
            unsafe { GlobalFree(block) };
            return None;
        };
        // SAFETY: the clipboard is open on this thread. A block belongs to
        // the clipboard once `SetClipboardData` accepts it, and is freed
        // here when it does not.
        unsafe {
            if EmptyClipboard() == 0 || SetClipboardData(CF_UNICODETEXT, block).is_null() {
                GlobalFree(block);
                return None;
            }
            if private {
                self.set_private_markers();
            }
        }
        drop(opened);
        // Read once the clipboard is closed, when the change is complete.
        // Another application writing in between would be taken for this
        // write and its content skipped; the window is a few microseconds.
        Some(self.sequence())
    }
}

/// A movable global memory block holding a copy of `data`, as the
/// clipboard takes it. The caller owns the block.
fn global_block(data: &[u16]) -> Option<HANDLE> {
    // SAFETY: `u16` has no padding or invalid bit patterns for `u8`.
    let bytes =
        unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), size_of_val(data)) };
    global_bytes(bytes)
}

/// A movable global memory block holding a copy of `data`. The caller owns
/// the block until `SetClipboardData` or `CreateStreamOnHGlobal` takes it.
fn global_bytes(data: &[u8]) -> Option<HANDLE> {
    // SAFETY: the block is at least as long as `data` and locked while filled.
    // It is freed if it cannot be locked.
    unsafe {
        let block = GlobalAlloc(GMEM_MOVEABLE, data.len().max(1));
        if block.is_null() {
            return None;
        }
        if !data.is_empty() {
            let target = GlobalLock(block);
            if target.is_null() {
                GlobalFree(block);
                return None;
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), target.cast::<u8>(), data.len());
            GlobalUnlock(block);
        }
        Some(block)
    }
}

/// Verifies the PNG signature and walks chunk headers through `IEND`,
/// returning the exact PNG slice (without any trailing `GlobalAlloc` slack)
/// if valid and at most [`MAX_CLIPBOARD_IMAGE_LEN`].
pub(crate) fn valid_png_slice(bytes: &[u8]) -> Option<&[u8]> {
    if !bytes.starts_with(&PNG_SIGNATURE) {
        return None;
    }
    let mut pos = PNG_SIGNATURE.len();
    let mut saw_header = false;
    let mut saw_data = false;
    while pos.checked_add(12)? <= bytes.len() {
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
        let kind: [u8; 4] = bytes[pos + 4..pos + 8].try_into().ok()?;
        let end = pos.checked_add(12)?.checked_add(len)?;
        if end > bytes.len() || end > MAX_CLIPBOARD_IMAGE_LEN {
            return None;
        }
        let data = &bytes[pos + 8..end - 4];
        let checksum = u32::from_be_bytes(bytes[end - 4..end].try_into().ok()?);
        if crc32(&kind, data) != checksum {
            return None;
        }
        if !saw_header {
            if &kind != b"IHDR" || len != 13 {
                return None;
            }
            let width = u32::from_be_bytes(data[..4].try_into().ok()?);
            let height = u32::from_be_bytes(data[4..8].try_into().ok()?);
            if width == 0
                || height == 0
                || u64::from(width) * u64::from(height) > (MAX_DIB_PIXELS_LEN / 4) as u64
            {
                return None;
            }
            saw_header = true;
        } else if &kind == b"IHDR" {
            return None;
        }
        if &kind == b"IDAT" {
            saw_data = true;
        }
        pos = end;
        if &kind == b"IEND" {
            return (len == 0 && saw_data).then_some(&bytes[..pos]);
        }
    }
    None
}

fn extract_masked(pixel: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let shifted = (pixel & mask) >> shift;
    let max = mask >> shift;
    if max == 0xFF {
        shifted as u8
    } else {
        ((u64::from(shifted) * 255 + u64::from(max / 2)) / u64::from(max)) as u8
    }
}

/// Converts a 24-bit or 32-bit `CF_DIB` (`BITMAPINFOHEADER` and later) buffer
/// into a lossless PNG of at most [`MAX_CLIPBOARD_IMAGE_LEN`] bytes.
pub(crate) fn dib_to_png(dib: &[u8]) -> Option<Vec<u8>> {
    if dib.len() < BITMAPINFOHEADER_LEN {
        return None;
    }
    let header_len = u32::from_le_bytes(dib[0..4].try_into().ok()?) as usize;
    if header_len < BITMAPINFOHEADER_LEN || header_len > dib.len() {
        return None;
    }
    let width_raw = i32::from_le_bytes(dib[4..8].try_into().ok()?);
    let height_raw = i32::from_le_bytes(dib[8..12].try_into().ok()?);
    if width_raw <= 0 || height_raw == 0 || height_raw == i32::MIN {
        return None;
    }
    let width = width_raw as u32;
    let height = height_raw.unsigned_abs();
    let top_down = height_raw < 0;
    if u64::from(width) * u64::from(height) * 3 > MAX_CLIPBOARD_IMAGE_LEN as u64 {
        return None;
    }
    let planes = u16::from_le_bytes(dib[12..14].try_into().ok()?);
    let bit_count = u16::from_le_bytes(dib[14..16].try_into().ok()?);
    let compression = u32::from_le_bytes(dib[16..20].try_into().ok()?);
    if planes != 1 || !matches!(bit_count, 24 | 32) {
        return None;
    }
    if compression != BI_RGB && !(compression == BI_BITFIELDS && bit_count == 32) {
        return None;
    }
    let clr_used = u32::from_le_bytes(dib[32..36].try_into().ok()?) as usize;
    let palette_bytes = clr_used.checked_mul(4)?;
    let (masks, extra_masks_len) = if compression == BI_BITFIELDS {
        if header_len == BITMAPINFOHEADER_LEN {
            if dib.len() < 52 {
                return None;
            }
            let r = u32::from_le_bytes(dib[40..44].try_into().ok()?);
            let g = u32::from_le_bytes(dib[44..48].try_into().ok()?);
            let b = u32::from_le_bytes(dib[48..52].try_into().ok()?);
            (Some((r, g, b, 0u32)), 12usize)
        } else if header_len >= 52 {
            let r = u32::from_le_bytes(dib[40..44].try_into().ok()?);
            let g = u32::from_le_bytes(dib[44..48].try_into().ok()?);
            let b = u32::from_le_bytes(dib[48..52].try_into().ok()?);
            let a = if header_len >= 56 {
                u32::from_le_bytes(dib[52..56].try_into().ok()?)
            } else {
                0
            };
            (Some((r, g, b, a)), 0)
        } else {
            return None;
        }
    } else {
        (None, 0)
    };
    let pixels_offset = header_len
        .checked_add(extra_masks_len)?
        .checked_add(palette_bytes)?;
    let bytes_per_pixel = (bit_count / 8) as usize;
    let stride = (width as usize)
        .checked_mul(bytes_per_pixel)?
        .checked_add(3)?
        & !3;
    let needed = stride.checked_mul(height as usize)?;
    let pixels = dib.get(pixels_offset..pixels_offset.checked_add(needed)?)?;

    let row_at = |y: usize| -> &[u8] {
        let src_y = if top_down {
            y
        } else {
            (height as usize - 1) - y
        };
        &pixels[src_y * stride..src_y * stride + stride]
    };

    if bit_count == 24 {
        let scanline_len = 1 + width as usize * 3;
        let mut raw = Vec::with_capacity(scanline_len.checked_mul(height as usize)?);
        for y in 0..height as usize {
            raw.push(0);
            let row = row_at(y);
            for x in 0..width as usize {
                let px = &row[x * 3..x * 3 + 3];
                raw.extend_from_slice(&[px[2], px[1], px[0]]);
            }
        }
        return encode_stored_png(width, height, 2, &raw);
    }

    // 32-bit DIB: GDI screenshots leave every alpha byte at 0, whereas
    // alpha-aware writers set non-zero alpha on visible pixels.
    let mut any_non_zero_alpha = false;
    let mut any_translucent = false;
    for y in 0..height as usize {
        let row = row_at(y);
        for x in 0..width as usize {
            let px = u32::from_le_bytes(row[x * 4..x * 4 + 4].try_into().ok()?);
            let a = match masks {
                Some((_, _, _, a_mask)) if a_mask != 0 => extract_masked(px, a_mask),
                Some(_) => 255,
                None => (px >> 24) as u8,
            };
            if a != 0 {
                any_non_zero_alpha = true;
            }
            if a != 255 {
                any_translucent = true;
            }
        }
    }
    let has_alpha = any_non_zero_alpha && any_translucent;
    let channels = if has_alpha { 4 } else { 3 };
    let scanline_len = 1 + width as usize * channels;
    let mut raw = Vec::with_capacity(scanline_len.checked_mul(height as usize)?);
    for y in 0..height as usize {
        raw.push(0);
        let row = row_at(y);
        for x in 0..width as usize {
            let px = u32::from_le_bytes(row[x * 4..x * 4 + 4].try_into().ok()?);
            let (r, g, b, a) = match masks {
                Some((r_m, g_m, b_m, a_m)) => (
                    extract_masked(px, r_m),
                    extract_masked(px, g_m),
                    extract_masked(px, b_m),
                    if a_m != 0 {
                        extract_masked(px, a_m)
                    } else {
                        255
                    },
                ),
                None => (
                    ((px >> 16) & 0xFF) as u8,
                    ((px >> 8) & 0xFF) as u8,
                    (px & 0xFF) as u8,
                    (px >> 24) as u8,
                ),
            };
            if has_alpha {
                raw.extend_from_slice(&[r, g, b, a]);
            } else {
                raw.extend_from_slice(&[r, g, b]);
            }
        }
    }
    encode_stored_png(width, height, if has_alpha { 6 } else { 2 }, &raw)
}

/// Encodes pre-filtered top-down PNG scanlines into a valid PNG using RFC 1950
/// zlib and RFC 1951 stored blocks (`BTYPE = 00`), Adler-32 and IEEE CRC-32.
fn encode_stored_png(width: u32, height: u32, color_type: u8, raw: &[u8]) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || raw.is_empty() {
        return None;
    }
    let blocks = raw.len().div_ceil(u16::MAX as usize);
    let idat_len = 2usize
        .checked_add(raw.len())?
        .checked_add(blocks * 5)?
        .checked_add(4)?;
    let total_len = 8usize
        .checked_add(12 + 13)?
        .checked_add(12 + idat_len)?
        .checked_add(12)?;
    if total_len > MAX_CLIPBOARD_IMAGE_LEN {
        return None;
    }
    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(&PNG_SIGNATURE);

    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&width.to_be_bytes());
    ihdr[4..8].copy_from_slice(&height.to_be_bytes());
    ihdr[8] = 8;
    ihdr[9] = color_type;
    write_png_chunk(&mut out, b"IHDR", &ihdr);

    let mut zlib = Vec::with_capacity(idat_len);
    zlib.extend_from_slice(&[0x78, 0x01]);
    let chunks: Vec<&[u8]> = raw.chunks(u16::MAX as usize).collect();
    for (index, block) in chunks.iter().enumerate() {
        let last = index + 1 == chunks.len();
        zlib.push(u8::from(last));
        let len = block.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(raw).to_be_bytes());
    write_png_chunk(&mut out, b"IDAT", &zlib);
    write_png_chunk(&mut out, b"IEND", &[]);
    Some(out)
}

fn write_png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(kind, data).to_be_bytes());
}

fn crc32(kind: &[u8; 4], data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in kind.iter().chain(data.iter()) {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let (mut s1, mut s2) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &byte in chunk {
            s1 += u32::from(byte);
            s2 += s1;
        }
        s1 %= MOD;
        s2 %= MOD;
    }
    (s2 << 16) | s1
}

/// Maximum uncompressed `CF_DIB` pixel buffer produced from a PNG (64 MiB,
/// enough for 5K displays while bounding decompression bombs).
const MAX_DIB_PIXELS_LEN: usize = 64 * 1024 * 1024;

#[repr(C)]
struct IUnknownVtbl {
    query_interface:
        unsafe extern "system" fn(*mut IStream, *const u8, *mut *mut core::ffi::c_void) -> i32,
    add_ref: unsafe extern "system" fn(*mut IStream) -> u32,
    release: unsafe extern "system" fn(*mut IStream) -> u32,
}

#[repr(C)]
struct IStream {
    vtbl: *const IUnknownVtbl,
}

enum GpBitmap {}

#[repr(C)]
struct GdiplusStartupInput {
    gdiplus_version: u32,
    debug_event_callback: *const core::ffi::c_void,
    suppress_background_thread: i32,
    suppress_external_codecs: i32,
}

#[repr(C)]
struct GpRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[repr(C)]
struct BitmapData {
    width: u32,
    height: u32,
    stride: i32,
    pixel_format: i32,
    scan0: *mut u8,
    reserved: usize,
}

const IMAGE_LOCK_MODE_READ: u32 = 1;
const PIXEL_FORMAT_32BPP_ARGB: i32 = 0x0026_200A;

#[link(name = "ole32")]
unsafe extern "system" {
    fn CreateStreamOnHGlobal(
        hglobal: HANDLE,
        f_delete_on_release: i32,
        ppstm: *mut *mut IStream,
    ) -> i32;
}

#[link(name = "gdiplus")]
unsafe extern "system" {
    fn GdiplusStartup(
        token: *mut usize,
        input: *const GdiplusStartupInput,
        output: *mut core::ffi::c_void,
    ) -> i32;
    fn GdipCreateBitmapFromStream(stream: *mut IStream, bitmap: *mut *mut GpBitmap) -> i32;
    fn GdipGetImageWidth(image: *mut GpBitmap, width: *mut u32) -> i32;
    fn GdipGetImageHeight(image: *mut GpBitmap, height: *mut u32) -> i32;
    fn GdipBitmapLockBits(
        bitmap: *mut GpBitmap,
        rect: *const GpRect,
        flags: u32,
        format: i32,
        locked_data: *mut BitmapData,
    ) -> i32;
    fn GdipBitmapUnlockBits(bitmap: *mut GpBitmap, locked_data: *mut BitmapData) -> i32;
    fn GdipDisposeImage(image: *mut GpBitmap) -> i32;
}

fn ensure_gdiplus() -> bool {
    static STARTED: OnceLock<bool> = OnceLock::new();
    *STARTED.get_or_init(|| {
        let mut token = 0usize;
        let input = GdiplusStartupInput {
            gdiplus_version: 1,
            debug_event_callback: std::ptr::null(),
            suppress_background_thread: 0,
            suppress_external_codecs: 0,
        };
        // SAFETY: `input` and `token` are valid stack pointers; `output` is
        // optional when `suppress_background_thread` is 0. The token lives for
        // the lifetime of the process.
        unsafe { GdiplusStartup(&mut token, &input, std::ptr::null_mut()) == 0 }
    })
}

struct StreamGuard(*mut IStream);

impl Drop for StreamGuard {
    fn drop(&mut self) {
        // SAFETY: `self.0` was returned by `CreateStreamOnHGlobal` with
        // `fDeleteOnRelease = 1`, so `Release` frees both stream and `HGLOBAL`.
        unsafe {
            ((*(*self.0).vtbl).release)(self.0);
        }
    }
}

struct BitmapGuard(*mut GpBitmap);

impl Drop for BitmapGuard {
    fn drop(&mut self) {
        // SAFETY: `self.0` was returned by `GdipCreateBitmapFromStream`.
        unsafe {
            GdipDisposeImage(self.0);
        }
    }
}

/// Decodes `png` into a 32-bit bottom-up `CF_DIB` (`BITMAPINFOHEADER` + BGRA
/// scanlines) via Win32 GDI+.
pub(crate) fn png_to_dib(png: &[u8]) -> Option<Vec<u8>> {
    decode_image_to_dib(valid_png_slice(png)?)
}

fn decode_image_to_dib(png: &[u8]) -> Option<Vec<u8>> {
    if !ensure_gdiplus() {
        return None;
    }
    let block = global_bytes(png)?;
    let mut stream: *mut IStream = std::ptr::null_mut();
    // SAFETY: `block` is a valid movable `HGLOBAL`; `1` transfers ownership to
    // the stream when `CreateStreamOnHGlobal` succeeds.
    let hr = unsafe { CreateStreamOnHGlobal(block, 1, &mut stream) };
    if hr < 0 || stream.is_null() {
        // SAFETY: `CreateStreamOnHGlobal` failed, so `block` is still ours.
        unsafe { GlobalFree(block) };
        return None;
    }
    let _stream = StreamGuard(stream);

    let mut bitmap: *mut GpBitmap = std::ptr::null_mut();
    // SAFETY: `stream` is a valid COM `IStream` holding the PNG bytes.
    if unsafe { GdipCreateBitmapFromStream(stream, &mut bitmap) } != 0 || bitmap.is_null() {
        return None;
    }
    let _bitmap = BitmapGuard(bitmap);

    let (mut width, mut height) = (0u32, 0u32);
    // SAFETY: `bitmap` is alive until `_bitmap` drops.
    unsafe {
        if GdipGetImageWidth(bitmap, &mut width) != 0
            || GdipGetImageHeight(bitmap, &mut height) != 0
        {
            return None;
        }
    }
    if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return None;
    }
    let row_bytes = (width as usize).checked_mul(4)?;
    let pixel_bytes = row_bytes.checked_mul(height as usize)?;
    if pixel_bytes > MAX_DIB_PIXELS_LEN {
        return None;
    }
    let rect = GpRect {
        x: 0,
        y: 0,
        width: width as i32,
        height: height as i32,
    };
    let mut locked = BitmapData {
        width: 0,
        height: 0,
        stride: 0,
        pixel_format: 0,
        scan0: std::ptr::null_mut(),
        reserved: 0,
    };
    // SAFETY: `rect` lies inside `bitmap`, and `locked` is unlocked before
    // `_bitmap` disposes the image.
    if unsafe {
        GdipBitmapLockBits(
            bitmap,
            &rect,
            IMAGE_LOCK_MODE_READ,
            PIXEL_FORMAT_32BPP_ARGB,
            &mut locked,
        )
    } != 0
        || locked.scan0.is_null()
    {
        return None;
    }

    let mut dib = Vec::with_capacity(BITMAPINFOHEADER_LEN + pixel_bytes);
    dib.extend_from_slice(&(BITMAPINFOHEADER_LEN as u32).to_le_bytes());
    dib.extend_from_slice(&(width as i32).to_le_bytes());
    dib.extend_from_slice(&(height as i32).to_le_bytes());
    dib.extend_from_slice(&1u16.to_le_bytes());
    dib.extend_from_slice(&32u16.to_le_bytes());
    dib.extend_from_slice(&BI_RGB.to_le_bytes());
    dib.extend_from_slice(&(pixel_bytes as u32).to_le_bytes());
    dib.extend_from_slice(&[0u8; 16]);

    // SAFETY: GDI+ guarantees `scan0 + y * stride` has `width * 4` readable
    // bytes for every `0 <= y < height` until `GdipBitmapUnlockBits`.
    unsafe {
        for src_y in (0..height as usize).rev() {
            let row_ptr = locked.scan0.offset(src_y as isize * locked.stride as isize);
            let row = std::slice::from_raw_parts(row_ptr, row_bytes);
            dib.extend_from_slice(row);
        }
        GdipBitmapUnlockBits(bitmap, &mut locked);
    }
    Some(dib)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dib_header(width: i32, height: i32, bit_count: u16, compression: u32) -> Vec<u8> {
        let mut header = Vec::with_capacity(BITMAPINFOHEADER_LEN);
        header.extend_from_slice(&(BITMAPINFOHEADER_LEN as u32).to_le_bytes());
        header.extend_from_slice(&width.to_le_bytes());
        header.extend_from_slice(&height.to_le_bytes());
        header.extend_from_slice(&1u16.to_le_bytes());
        header.extend_from_slice(&bit_count.to_le_bytes());
        header.extend_from_slice(&compression.to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes());
        header.extend_from_slice(&[0u8; 16]);
        header
    }

    #[test]
    fn checksums_match_standard_vectors() {
        assert_eq!(crc32(b"IEND", &[]), 0xAE42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn exclusion_rules_honour_presence_and_zero_dword() {
        let present = [100, 101];
        let zero = [200, 201];
        assert!(!is_excluded_by(present, zero, |_| false, |_| None));
        assert!(is_excluded_by(present, zero, |f| f == 100, |_| None));
        assert!(is_excluded_by(present, zero, |f| f == 101, |_| None));
        assert!(is_excluded_by(present, zero, |f| f == 200, |_| Some(0)));
        assert!(is_excluded_by(present, zero, |f| f == 201, |_| None));
        assert!(!is_excluded_by(present, zero, |f| f == 200, |_| Some(1)));
    }

    #[test]
    fn dib_24bit_padded_rows_roundtrip_through_png_and_gdiplus() {
        // 3x2 bottom-up 24-bit DIB: each row is 9 pixel bytes + 3 pad bytes = 12.
        let mut dib = make_dib_header(3, 2, 24, BI_RGB);
        // Bottom row (y = 1): blue, green, red + 3 pad bytes.
        dib.extend_from_slice(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 0xAA, 0xBB, 0xCC]);
        // Top row (y = 0): white, black, orange + 3 pad bytes.
        dib.extend_from_slice(&[255, 255, 255, 0, 0, 0, 0x21, 0x6A, 0xF2, 1, 2, 3]);

        let png = dib_to_png(&dib).expect("24-bit DIB should encode to PNG");
        assert!(png.starts_with(&PNG_SIGNATURE));
        let mut padded_png = png.clone();
        padded_png.extend_from_slice(&[0, 0, 0, 0]);
        assert_eq!(valid_png_slice(&padded_png), Some(png.as_slice()));

        let decoded = png_to_dib(&png).expect("GDI+ should decode pure-Rust PNG to CF_DIB");
        assert_eq!(u32::from_le_bytes(decoded[0..4].try_into().unwrap()), 40);
        assert_eq!(i32::from_le_bytes(decoded[4..8].try_into().unwrap()), 3);
        assert_eq!(i32::from_le_bytes(decoded[8..12].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(decoded[14..16].try_into().unwrap()), 32);
        let px = &decoded[40..];
        // Bottom row first (BGRA): blue, green, red.
        assert_eq!(
            &px[0..12],
            &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]
        );
        // Top row second (BGRA): white, black, orange (R=0xF2, G=0x6A, B=0x21).
        assert_eq!(
            &px[12..24],
            &[255, 255, 255, 255, 0, 0, 0, 255, 0x21, 0x6A, 0xF2, 255]
        );
    }

    #[test]
    fn dib_32bit_handles_gdi_zero_alpha_and_real_transparency() {
        // 1) GDI screenshot style: all alpha = 0 -> treated as opaque RGB.
        let mut gdi_dib = make_dib_header(2, 1, 32, BI_RGB);
        gdi_dib.extend_from_slice(&[10, 20, 30, 0, 40, 50, 60, 0]);
        let gdi_png = dib_to_png(&gdi_dib).unwrap();
        // Color type offset in PNG: 8 (sig) + 8 (len+IHDR) + 9 = 25 -> 2 (RGB).
        assert_eq!(gdi_png[25], 2);
        let gdi_back = png_to_dib(&gdi_png).unwrap();
        assert_eq!(&gdi_back[40..48], &[10, 20, 30, 255, 40, 50, 60, 255]);

        // 2) Top-down 32-bit DIB with real alpha -> preserved as RGBA (color type 6).
        let mut alpha_dib = make_dib_header(1, -2, 32, BI_RGB);
        // Top row: semi-transparent (alpha = 128); bottom row: fully transparent.
        alpha_dib.extend_from_slice(&[100, 150, 200, 128, 0, 0, 0, 0]);
        let alpha_png = dib_to_png(&alpha_dib).unwrap();
        assert_eq!(alpha_png[25], 6);
        let alpha_back = png_to_dib(&alpha_png).unwrap();
        // Bottom-up in output CF_DIB: bottom row first, then top row.
        assert_eq!(&alpha_back[40..44], &[0, 0, 0, 0]);
        assert_eq!(&alpha_back[44..48], &[100, 150, 200, 128]);
    }

    #[test]
    fn dib_bitfields_and_invalid_inputs_are_checked() {
        let mut bf = make_dib_header(1, 1, 32, BI_BITFIELDS);
        // R, G, B masks after 40-byte BITMAPINFOHEADER.
        bf.extend_from_slice(&0x00FF_0000u32.to_le_bytes());
        bf.extend_from_slice(&0x0000_FF00u32.to_le_bytes());
        bf.extend_from_slice(&0x0000_00FFu32.to_le_bytes());
        bf.extend_from_slice(&0x0011_2233u32.to_le_bytes());
        let png = dib_to_png(&bf).unwrap();
        let back = png_to_dib(&png).unwrap();
        assert_eq!(&back[40..44], &[0x33, 0x22, 0x11, 0xFF]);

        assert!(dib_to_png(&[0u8; 20]).is_none());
        assert!(valid_png_slice(b"not a png").is_none());
        assert!(valid_png_slice(&PNG_SIGNATURE).is_none());
    }

    #[test]
    fn png_rejects_corrupt_chunks_missing_header_and_oversized_dimensions() {
        let mut dib = make_dib_header(1, 1, 24, BI_RGB);
        dib.extend_from_slice(&[0, 0, 255, 0]);
        let mut png = dib_to_png(&dib).unwrap();
        assert!(valid_png_slice(&png).is_some());
        png[20] ^= 1;
        assert!(valid_png_slice(&png).is_none());

        let mut invalid = PNG_SIGNATURE.to_vec();
        write_png_chunk(&mut invalid, b"IEND", &[]);
        assert!(valid_png_slice(&invalid).is_none());

        let mut header = [0u8; 13];
        header[..4].copy_from_slice(&u32::MAX.to_be_bytes());
        header[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
        let mut oversized = PNG_SIGNATURE.to_vec();
        write_png_chunk(&mut oversized, b"IHDR", &header);
        write_png_chunk(&mut oversized, b"IDAT", &[0]);
        write_png_chunk(&mut oversized, b"IEND", &[]);
        assert!(valid_png_slice(&oversized).is_none());
    }

    #[test]
    fn hdrop_paths_extracts_unicode_paths_from_dropfiles_block() {
        // Win32 DROPFILES header: pFiles=20, pt=(0,0), fNC=0, fWide=1 (UTF-16).
        let mut raw = Vec::new();
        raw.extend_from_slice(&20u32.to_le_bytes());
        raw.extend_from_slice(&[0u8; 12]);
        raw.extend_from_slice(&1i32.to_le_bytes());
        for path in [r"C:\Temp\rapport été.pdf", r"D:\Photos\image_01.png"] {
            for unit in wide(path) {
                raw.extend_from_slice(&unit.to_le_bytes());
            }
        }
        raw.extend_from_slice(&0u16.to_le_bytes());

        let block = global_bytes(&raw).expect("allocating DROPFILES HGLOBAL");
        // SAFETY: `block` holds a valid wide `DROPFILES` structure and is freed right after.
        let extracted = unsafe {
            let paths = hdrop_paths(block as HDROP);
            GlobalFree(block);
            paths
        };
        assert_eq!(
            extracted,
            vec![
                PathBuf::from(r"C:\Temp\rapport été.pdf"),
                PathBuf::from(r"D:\Photos\image_01.png"),
            ]
        );
    }
}
