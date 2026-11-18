//! The main window, in WinUI 3 through windows-reactor.
//!
//! It runs on the main thread and only talks to the rest of the app through
//! the worker's [`Handle`] (snapshots in, commands out) and the tray's
//! [`Link`] (text for the clipboard). [`words`] decides what the window says
//! and is tested; the other modules build views.

mod app;
mod detail;
mod devices;
mod messages;
mod pairing;
mod picker;
mod qr;
mod settings;
mod widgets;
pub(crate) mod words;

use std::sync::{Arc, Condvar, Mutex, OnceLock};

use crate::tray::Link;
use crate::update::Updater;
use crate::worker::Handle;

struct SettingsNav {
    state: Mutex<(u64, bool)>,
    changed: Condvar,
}

static SETTINGS_NAV: OnceLock<SettingsNav> = OnceLock::new();

fn settings_nav() -> &'static SettingsNav {
    SETTINGS_NAV.get_or_init(|| SettingsNav {
        state: Mutex::new((0, false)),
        changed: Condvar::new(),
    })
}

/// Asks the main window to open the Settings page. Callable from any thread.
pub fn open_settings() {
    let nav = settings_nav();
    let mut state = nav.state.lock().unwrap();
    state.0 += 1;
    nav.changed.notify_all();
}

pub(crate) fn wait_open_settings(generation: u64) -> Option<u64> {
    let nav = settings_nav();
    let mut state = nav.state.lock().unwrap();
    loop {
        if state.0 > generation {
            return Some(state.0);
        }
        if state.1 {
            return None;
        }
        state = nav.changed.wait(state).unwrap();
    }
}

/// What the window needs from the rest of the app.
#[derive(Clone)]
pub struct Shell {
    pub worker: Handle,
    pub link: Arc<Link>,
    pub updater: Arc<Updater>,
    /// Start with the window hidden, as at sign-in.
    pub hidden: bool,
}

/// Given once, never changed.
impl PartialEq for Shell {
    fn eq(&self, other: &Shell) -> bool {
        Arc::ptr_eq(&self.link, &other.link)
            && Arc::ptr_eq(&self.updater, &other.updater)
            && self.hidden == other.hidden
    }
}

/// Runs the window until it is closed for good ([`crate::window::quit`]).
/// Fails if WinUI cannot start, for example without the Windows App
/// Runtime.
pub fn run(shell: Shell) -> Result<(), String> {
    settings_nav().state.lock().unwrap().1 = false;
    crate::window::reset_drops();
    let result =
        windows_reactor::App::run_component::<app::Weft>(shell).map_err(|err| err.to_string());
    crate::window::stop_drops();
    let nav = settings_nav();
    nav.state.lock().unwrap().1 = true;
    nav.changed.notify_all();
    result
}
