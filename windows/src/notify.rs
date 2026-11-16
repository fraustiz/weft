//! What the user sees and hears of what the other devices do: the worker's
//! [`Notice`]s become a ring, an opened link and Windows notifications.
//!
//! Notifications are the tray icon's balloons, which Windows shows as
//! notifications without the app identity toasts need. Replacing them with
//! real toasts only changes [`show`].

use std::path::Path;

use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::clipboard::wide;
use crate::i18n::{is_french, t};
use crate::tray::Link;
use crate::ui::words;
use crate::worker::Notice;
use crate::{sound, window};

/// Shows a notification.
fn show(link: &Link, title: String, text: String) {
    link.balloon(title, text);
}

fn named(device: Option<String>) -> String {
    device.unwrap_or_else(|| t("a device", "un appareil").to_owned())
}

/// Opens a web link in the default browser. The worker checked it is an
/// http or https link.
fn open_url(url: String) {
    // The shell wants COM on the calling thread: a short-lived one of its
    // own, so the worker never waits on the browser.
    std::thread::spawn(move || {
        let (verb, url) = (wide("open"), wide(&url));
        // SAFETY: the strings are NUL-terminated and outlive the calls; COM
        // is initialised for this thread only, which ends right after.
        unsafe {
            CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                url.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            );
        }
    });
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Acts on a notice. Called on the worker thread or a pool thread.
pub fn handle(link: &Link, notice: Notice) {
    let french = is_french();
    match notice {
        Notice::Ring { device, .. } => {
            sound::start();
            window::show();
            let device = named(device);
            let title = if french {
                format!("{device} fait sonner cet ordinateur")
            } else {
                format!("{device} is ringing this computer")
            };
            let text = t(
                "Select Found it in Weft to stop",
                "Choisissez Trouvé dans Weft pour l'arrêter",
            );
            show(link, title, text.into());
        }
        Notice::RingEnded => sound::stop(),
        Notice::Ping {
            device, message, ..
        } => {
            let device = named(device);
            let title = if french {
                format!("Ping de {device}")
            } else {
                format!("Ping from {device}")
            };
            show(link, title, message.unwrap_or_default());
        }
        Notice::Link { device, url, .. } => {
            let device = named(device);
            let title = if french {
                format!("Lien de {device}")
            } else {
                format!("Link from {device}")
            };
            open_url(url.clone());
            show(link, title, url);
        }
        Notice::FileReceived { device, path, .. } => {
            let name = file_name(&path);
            let device = named(device);
            let (title, text) = if french {
                (
                    format!("{name} reçu"),
                    format!("De {device}, dans Téléchargements"),
                )
            } else {
                (
                    format!("Received {name}"),
                    format!("From {device}, in Downloads"),
                )
            };
            show(link, title, text);
        }
        Notice::TransferFailed {
            name,
            outgoing,
            failure,
            ..
        } => {
            let title = match (outgoing, french) {
                (true, false) => format!("{name} was not sent"),
                (true, true) => format!("{name} n'a pas été envoyé"),
                (false, false) => format!("{name} was not received"),
                (false, true) => format!("{name} n'a pas été reçu"),
            };
            show(link, title, words::transfer_failure(failure).into());
        }
    }
}
