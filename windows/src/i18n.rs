//! Interface language: English, or French when the Windows display language
//! is French. `WEFT_LANG=en|fr` forces one.

use std::sync::OnceLock;

fn detect() -> bool {
    if let Ok(forced) = std::env::var("WEFT_LANG") {
        return forced.eq_ignore_ascii_case("fr");
    }
    system_is_french()
}

#[cfg(windows)]
fn system_is_french() -> bool {
    const PRIMARY_LANGUAGE_MASK: u16 = 0x03ff;
    const LANG_FRENCH: u16 = 0x0c;
    // SAFETY: no argument, no side effect.
    let language = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    language & PRIMARY_LANGUAGE_MASK == LANG_FRENCH
}

#[cfg(not(windows))]
fn system_is_french() -> bool {
    false
}

/// Whether the interface is in French. Decided once per process.
pub fn is_french() -> bool {
    static FRENCH: OnceLock<bool> = OnceLock::new();
    *FRENCH.get_or_init(detect)
}

/// Picks the English or the French version of a text.
pub fn t(english: &'static str, french: &'static str) -> &'static str {
    if is_french() { french } else { english }
}
