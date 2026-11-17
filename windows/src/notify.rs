//! What the user sees and hears of what the other devices do: the worker's
//! [`Notice`]s become a ring, an opened link, Windows toasts, and the
//! players and volume of this computer doing what a device asked.
//!
//! Toasts go through the [`System`] thread, which falls back on the tray
//! icon's balloons when Windows refuses them. A click on a toast becomes a
//! command for the worker: the packet the phone expects for that button.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use weft_cli::hex;
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::clipboard::wide;
use crate::i18n::{is_french, t};
use crate::packets::{Call, CallEvent, Notification, Packet};
use crate::system::{OnClick, System, Toast};
use crate::toast::{Activation, Button, Content, Scenario};
use crate::ui::words;
use crate::worker::{Command, Handle, Notice};
use crate::{sound, window};

/// The group of the toasts about this computer itself.
const OWN_GROUP: &str = "weft";
/// The tag of the ring toast, and of a device's ringing call.
const RING_TAG: &str = "ring";
const CALL_TAG: &str = "call";
const UPDATE_TAG: &str = "update";

/// Turns notices into what the user sees; any thread.
pub struct Notifier {
    pub system: System,
    pub clipboard: Arc<crate::tray::Link>,
}

fn named(device: Option<String>) -> String {
    device.unwrap_or_else(|| t("a device", "un appareil").to_owned())
}

/// A tag no other toast has, for toasts that never replace one another.
fn fresh_tag() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!("t{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// The group of a device's toasts. At most 64 characters, as Windows asks.
fn group_of(device: &[u8; 16]) -> String {
    hex(device)
}

/// The tag of a phone notification: its id can be longer than the 64
/// characters Windows takes, so it is hashed (FNV-1a), the same way in
/// every run so a removal finds a toast of an earlier one.
pub fn notification_tag(id: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in id.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("n{hash:016x}")
}

/// Opens a web link in the default browser. The worker checked it is an
/// http or https link.
pub(crate) fn open_url(url: String) {
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

/// A click on the toast itself shows the window; buttons do what
/// `on_button` says.
fn clicks(on_button: impl Fn(&Activation, &Handle) + Send + Sync + 'static) -> OnClick {
    Arc::new(move |activation: Activation, worker: &Handle| {
        if activation.argument == "open" {
            window::show();
        } else {
            on_button(&activation, worker);
        }
    })
}

/// The toast of a phone notification, and what its buttons send.
pub fn notification_toast(from: [u8; 16], device: &str, notification: Notification) -> Toast {
    let heading = if notification.app.trim().is_empty() {
        device.to_owned()
    } else {
        format!("{} · {device}", notification.app)
    };
    let mut content = Content::new(vec![
        heading,
        notification.title.clone(),
        notification.text.clone(),
    ]);
    if notification.reply {
        content.reply = Some((
            t("Reply", "Répondre").to_owned(),
            t("Send", "Envoyer").to_owned(),
        ));
    }
    // Three of the phone's actions, then the reply and the dismiss: five
    // buttons, as many as Windows shows.
    content.buttons = notification
        .actions
        .iter()
        .take(3)
        .enumerate()
        .map(|(index, label)| Button::new(label.clone(), format!("action:{index}")))
        .collect();
    if notification.dismissable {
        content.buttons.push(Button::new(
            t("Dismiss on phone", "Effacer sur le téléphone"),
            "dismiss",
        ));
    }
    let (id, actions) = (notification.id.clone(), notification.actions.clone());
    let on_click = clicks(move |activation, worker| {
        let id = id.clone();
        let packet = match activation.argument.as_str() {
            "reply" => match activation.reply.as_deref().map(str::trim) {
                Some(text) if !text.is_empty() => Packet::NotificationReply {
                    id,
                    text: text.to_owned(),
                },
                _ => return,
            },
            "dismiss" => Packet::NotificationDismiss { id },
            argument => {
                let label = argument
                    .strip_prefix("action:")
                    .and_then(|index| index.parse::<usize>().ok())
                    .and_then(|index| actions.get(index));
                match label {
                    Some(action) => Packet::NotificationAction {
                        id,
                        action: action.clone(),
                    },
                    None => return,
                }
            }
        };
        worker.send(Command::SendPacket(from, packet));
    });
    Toast {
        group: group_of(&from),
        tag: notification_tag(&notification.id),
        content,
        suppress_popup: notification.silent,
        on_click: Some(on_click),
    }
}

/// The toast of a ringing or missed call; `None` for the other events,
/// which only remove the ringing one.
pub fn call_toast(from: [u8; 16], device: &str, call: &Call) -> Option<Toast> {
    let french = is_french();
    let caller = call.caller();
    let (heading, scenario, tag, button) = match call.event()? {
        CallEvent::Ringing => {
            let heading = match (caller, french) {
                (Some(caller), false) => format!("Incoming call: {caller}"),
                (Some(caller), true) => format!("Appel entrant : {caller}"),
                (None, _) => t("Incoming call", "Appel entrant").to_owned(),
            };
            let mute = Button::new(t("Mute", "Silence"), "mute");
            (heading, Scenario::IncomingCall, CALL_TAG.to_owned(), mute)
        }
        CallEvent::Missed => {
            let heading = match (caller, french) {
                (Some(caller), false) => format!("Missed call: {caller}"),
                (Some(caller), true) => format!("Appel manqué : {caller}"),
                (None, _) => t("Missed call", "Appel manqué").to_owned(),
            };
            // A reminder stays on screen, and needs a button to go.
            let close = Button::new(t("OK", "OK"), "close");
            (heading, Scenario::Reminder, fresh_tag(), close)
        }
        CallEvent::Talking | CallEvent::Ended => return None,
    };
    let mut content = Content::new(vec![heading, device.to_owned()]);
    content.scenario = scenario;
    content.buttons = vec![button];
    // The phone rings already.
    content.quiet = true;
    let on_click = clicks(move |activation, worker| {
        if activation.argument == "mute" {
            worker.send(Command::SendPacket(from, Packet::CallMute {}));
        }
    });
    Some(Toast {
        group: group_of(&from),
        tag,
        content,
        suppress_popup: false,
        on_click: Some(on_click),
    })
}

/// The toast of a new SMS, with a reply box that answers in its thread.
pub fn sms_toast(
    from: [u8; 16],
    device: &str,
    thread: i64,
    sender: &str,
    text: String,
    addresses: Vec<String>,
) -> Toast {
    let mut content = Content::new(vec![format!("{sender} · {device}"), text]);
    content.reply = Some((
        t("Reply", "Répondre").to_owned(),
        t("Send", "Envoyer").to_owned(),
    ));
    let on_click = clicks(move |activation, worker| {
        if activation.argument != "reply" {
            return;
        }
        if let Some(text) = activation.reply.as_deref().map(str::trim)
            && !text.is_empty()
        {
            worker.send(Command::SendSms {
                device: from,
                addresses: addresses.clone(),
                text: text.to_owned(),
                thread: Some(thread),
            });
        }
    });
    Toast {
        group: group_of(&from),
        // A newer message of the thread replaces the older one.
        tag: format!("s{thread}"),
        content,
        suppress_popup: false,
        on_click: Some(on_click),
    }
}

/// The toast of an available update; clicking it shows the window on the
/// Settings page.
pub fn update_toast(version: &str) -> Toast {
    let title = if is_french() {
        format!("Version {version} disponible")
    } else {
        format!("Version {version} available")
    };
    let text = t(
        "Open Weft's settings to download and install it",
        "Ouvrez les réglages de Weft pour la télécharger et l'installer",
    )
    .to_owned();
    Toast {
        group: OWN_GROUP.into(),
        tag: UPDATE_TAG.into(),
        content: Content::new(vec![title, text]),
        suppress_popup: false,
        on_click: Some(Arc::new(|_, _| {
            crate::ui::open_settings();
            window::show();
        })),
    }
}

impl Notifier {
    /// Shows a plain toast: a title and a line.
    fn show(&self, title: String, text: String) {
        self.system.toast(Toast {
            group: OWN_GROUP.into(),
            tag: fresh_tag(),
            content: Content::new(vec![title, text]),
            suppress_popup: false,
            on_click: Some(clicks(|_, _| {})),
        });
    }

    /// Posts a toast when an automatic check finds `version`.
    pub fn update_available(&self, version: &str) {
        self.system.toast(update_toast(version));
    }

    /// Acts on a notice. Called on the worker thread or a pool thread.
    pub fn handle(&self, notice: Notice) {
        let french = is_french();
        match notice {
            Notice::ClipboardImage { image, .. } => self.clipboard.remote_image(image),
            Notice::Ring { device, .. } => {
                sound::start();
                window::show();
                let device = named(device);
                let title = if french {
                    format!("{device} fait sonner cet ordinateur")
                } else {
                    format!("{device} is ringing this computer")
                };
                let mut content = Content::new(vec![
                    title,
                    t(
                        "It stops by itself after a minute",
                        "Elle s'arrête d'elle-même au bout d'une minute",
                    )
                    .into(),
                ]);
                content.buttons = vec![Button::new(t("Found it", "Trouvé"), "found")];
                content.quiet = true;
                self.system.toast(Toast {
                    group: OWN_GROUP.into(),
                    tag: RING_TAG.into(),
                    content,
                    suppress_popup: false,
                    on_click: Some(clicks(|activation, worker| {
                        if activation.argument == "found" {
                            worker.send(Command::FoundIt);
                        }
                    })),
                });
            }
            Notice::RingEnded => {
                sound::stop();
                self.system.remove_toast(OWN_GROUP, RING_TAG);
            }
            Notice::Ping {
                device, message, ..
            } => {
                let device = named(device);
                let title = if french {
                    format!("Ping de {device}")
                } else {
                    format!("Ping from {device}")
                };
                self.show(title, message.unwrap_or_default());
            }
            Notice::Link { device, url, .. } => {
                let device = named(device);
                let title = if french {
                    format!("Lien de {device}")
                } else {
                    format!("Link from {device}")
                };
                open_url(url.clone());
                self.show(title, url);
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
                self.show(title, text);
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
                self.show(title, words::transfer_failure(failure).into());
            }
            Notice::Notification {
                from,
                device,
                notification,
            } => {
                let toast = notification_toast(from, &named(device), notification);
                self.system.toast(toast);
            }
            Notice::NotificationRemoved { from, id } => {
                self.system
                    .remove_toast(&group_of(&from), &notification_tag(&id));
            }
            Notice::Call { from, device, call } => {
                // Whatever happened, the phone no longer rings.
                self.system.remove_toast(&group_of(&from), CALL_TAG);
                if let Some(toast) = call_toast(from, &named(device), &call) {
                    self.system.toast(toast);
                }
            }
            Notice::MediaCommand { player, action, .. } => self.system.media(player, action),
            Notice::SetVolume { set, .. } => self.system.volume(set),
            Notice::SmsReceived {
                from,
                device,
                thread,
                sender,
                text,
                addresses,
            } => {
                let toast = sms_toast(from, &named(device), thread, &sender, text, addresses);
                self.system.toast(toast);
            }
            Notice::CommandRan { device, name, .. } => {
                let device = named(device);
                let title = if french {
                    format!("Commande « {name} » lancée pour {device}")
                } else {
                    format!("Ran {name} for {device}")
                };
                self.show(title, String::new());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_tags_are_short_and_stable() {
        let long = "0|com.example.chat|42|tag|10123".repeat(4);
        let tag = notification_tag(&long);
        assert_eq!(tag.len(), 17);
        assert_eq!(tag, notification_tag(&long));
        assert_ne!(tag, notification_tag("other"));
        assert_eq!(group_of(&[0xab; 16]).len(), 32);
    }

    #[test]
    fn notification_toasts_carry_the_phone_s_buttons() {
        let notification = Notification {
            id: "a".into(),
            app: "Chat".into(),
            title: "Ann".into(),
            text: "Hi".into(),
            dismissable: true,
            actions: vec!["One".into(), "Two".into(), "Three".into(), "Four".into()],
            reply: true,
            silent: true,
            ..Notification::default()
        };
        let toast = notification_toast([1; 16], "Phone", notification);
        assert_eq!(toast.content.lines, ["Chat · Phone", "Ann", "Hi"]);
        assert!(toast.suppress_popup);
        let arguments: Vec<&str> = toast
            .content
            .buttons
            .iter()
            .map(|button| button.argument.as_str())
            .collect();
        assert_eq!(arguments, ["action:0", "action:1", "action:2", "dismiss"]);
        let xml = toast.content.xml();
        assert_eq!(xml.matches("<action ").count(), 5, "{xml}");
    }

    #[test]
    fn calls_ring_and_go_missed() {
        let call = |event: &str| Call {
            event: event.into(),
            number: Some("+33 600".into()),
            name: None,
        };
        let ringing = call_toast([1; 16], "Phone", &call("ringing")).unwrap();
        assert_eq!(ringing.tag, CALL_TAG);
        assert_eq!(ringing.content.scenario, Scenario::IncomingCall);
        assert_eq!(ringing.content.buttons[0].argument, "mute");
        let missed = call_toast([1; 16], "Phone", &call("missed")).unwrap();
        assert_eq!(missed.content.scenario, Scenario::Reminder);
        assert_ne!(missed.tag, CALL_TAG);
        assert!(call_toast([1; 16], "Phone", &call("ended")).is_none());
        assert!(call_toast([1; 16], "Phone", &call("talking")).is_none());
    }

    #[test]
    fn update_toast_uses_own_group_and_update_tag() {
        let toast = update_toast("0.2.0");
        assert_eq!(toast.group, OWN_GROUP);
        assert_eq!(toast.tag, UPDATE_TAG);
        assert!(!toast.suppress_popup);
        assert_eq!(toast.content.lines.len(), 2);
        assert!(toast.content.lines[0].contains("0.2.0"));
        assert!(toast.on_click.is_some());
    }
}
