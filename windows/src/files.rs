//! The user's folders the Windows app puts files in. The disk side of
//! transfers is in [`weft_client::files`], re-exported here.

pub use weft_client::files::*;

/// The user's Downloads folder.
#[cfg(windows)]
pub fn downloads_dir() -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_Downloads, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
    };
    let mut path: windows_sys::core::PWSTR = std::ptr::null_mut();
    // SAFETY: on success `path` is a NUL-terminated string the shell
    // allocated, read once and freed; on failure it is freed too, as the
    // documentation asks.
    unsafe {
        let found = SHGetKnownFolderPath(
            &FOLDERID_Downloads,
            KF_FLAG_DEFAULT as u32,
            std::ptr::null_mut(),
            &mut path,
        );
        let result = if found == 0 && !path.is_null() {
            let len = (0..).take_while(|&i| *path.add(i) != 0).count();
            let units = std::slice::from_raw_parts(path, len);
            Some(PathBuf::from(std::ffi::OsString::from_wide(units)))
        } else {
            None
        };
        CoTaskMemFree(path as *const _);
        result
    }
}
