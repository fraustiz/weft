//! Weft for Windows.
//!
//! A device of a Weft group (`docs/protocol.md`) that lives in the
//! notification area: it sends the text copied on this computer to the
//! user's other devices and puts on the clipboard the text they send.
//!
//! - [`sync`] decides what happens to clipboard text;
//! - [`worker`] holds the connection to the relay, on its own thread;
//! - [`ui`] is the main window, on the main thread;
//! - [`clipboard`], [`tray`], [`window`] and [`autostart`] are the Win32
//!   side: the system clipboard, the icon with its menu, the main window's
//!   close and show, and the sign-in entry.
//!
//! The session logic itself is the one of the command-line client, used
//! as a library.

pub mod i18n;
pub mod sync;
pub mod worker;

#[cfg(windows)]
pub mod autostart;
#[cfg(windows)]
pub mod clipboard;
#[cfg(windows)]
pub mod tray;
#[cfg(windows)]
pub mod ui;
#[cfg(windows)]
pub mod window;
