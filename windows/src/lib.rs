//! Weft for Windows.
//!
//! A device of a Weft group (`docs/protocol.md`) that lives in the
//! notification area: it sends the text copied on this computer to the
//! user's other devices and puts on the clipboard the text they send. It
//! also rings, shows the status of, and exchanges files and links with
//! those devices (`docs/features.md`).
//!
//! - [`sync`] decides what happens to clipboard text;
//! - [`packets`], [`features`] and [`transfer`] are the feature packets,
//!   the switches, and the file transfer rules; [`files`] their disk side;
//! - [`worker`] holds the connection to the relay, on its own thread;
//! - [`ui`] is the main window, on the main thread;
//! - [`clipboard`], [`tray`], [`window`] and [`autostart`] are the Win32
//!   side: the system clipboard, the icon with its menu, the main window's
//!   close and show, and the sign-in entry; [`notify`] and [`sound`] turn
//!   what other devices do into notifications, opened links and a ring.
//!
//! The session logic itself is the one of the command-line client, used
//! as a library.

pub mod features;
pub mod files;
pub mod i18n;
pub mod packets;
pub mod sync;
pub mod transfer;
pub mod worker;

#[cfg(windows)]
pub mod autostart;
#[cfg(windows)]
pub mod clipboard;
#[cfg(windows)]
pub mod notify;
#[cfg(windows)]
pub mod sound;
#[cfg(windows)]
pub mod tray;
#[cfg(windows)]
pub mod ui;
#[cfg(windows)]
pub mod window;
