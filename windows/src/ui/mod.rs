//! The main window, in WinUI 3 through windows-reactor.
//!
//! It runs on the main thread and only talks to the rest of the app through
//! the worker's [`Handle`] (snapshots in, commands out) and the tray's
//! [`Link`] (text for the clipboard). [`words`] decides what the window says
//! and is tested; the other modules build views.

mod app;
mod detail;
mod devices;
mod pairing;
mod picker;
mod qr;
mod settings;
mod widgets;
pub(crate) mod words;

use std::sync::Arc;

use crate::tray::Link;
use crate::worker::Handle;

/// What the window needs from the rest of the app.
#[derive(Clone)]
pub struct Shell {
    pub worker: Handle,
    pub link: Arc<Link>,
    /// Start with the window hidden, as at sign-in.
    pub hidden: bool,
}

/// Given once, never changed.
impl PartialEq for Shell {
    fn eq(&self, other: &Shell) -> bool {
        Arc::ptr_eq(&self.link, &other.link) && self.hidden == other.hidden
    }
}

/// Runs the window until it is closed for good ([`crate::window::quit`]).
/// Fails if WinUI cannot start, for example without the Windows App
/// Runtime.
pub fn run(shell: Shell) -> Result<(), String> {
    windows_reactor::App::run_component::<app::Weft>(shell).map_err(|err| err.to_string())
}
