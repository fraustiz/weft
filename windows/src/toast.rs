//! Windows toast notifications for an app that is not packaged.
//!
//! Such an app has no identity of its own: it gives itself one, an
//! application user model id ([`APP_ID`]), registered under
//! `HKCU\Software\Classes\AppUserModelId\<id>` with the name and icon the
//! toasts show. The key is the app's identity and is kept from one run to
//! the next; an uninstaller should delete it, with the icon file next to
//! the state (see [`register`]).
//!
//! Clicks reach the app through the toast's `Activated` event while it
//! runs. There is no COM activator: a toast clicked after the app quit
//! does nothing.

use std::io;
use std::path::{Path, PathBuf};

use windows_core::{HSTRING, IInspectable, Interface};
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

use crate::bindings::{
    IPropertyValue, ToastActivatedEventArgs, ToastNotification, ToastNotificationManager,
    ToastNotifier, XmlDocument,
};
use crate::clipboard::wide;

/// The identity of the app's toasts.
pub const APP_ID: &str = "Weft.Windows";

/// The icon toasts show, rendered by the build script.
const ICON: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/weft.png"));

/// The id of the reply box, and the name of its text in the activation.
const REPLY_INPUT: &str = "reply";

/// Windows shows five buttons at most.
const MAX_BUTTONS: usize = 5;

fn key_path() -> String {
    format!(r"Software\Classes\AppUserModelId\{APP_ID}")
}

fn set_value(name: &str, value: &str) -> io::Result<()> {
    let (key, name, value) = (wide(&key_path()), wide(name), wide(value));
    // SAFETY: the strings are NUL-terminated and outlive the call; the data
    // is the value with its NUL, of the size given.
    let status: WIN32_ERROR = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            REG_SZ,
            value.as_ptr().cast(),
            size_of_val(value.as_slice()) as u32,
        )
    };
    match status {
        ERROR_SUCCESS => Ok(()),
        status => Err(io::Error::from_raw_os_error(status as i32)),
    }
}

/// Gives the process its identity: writes the icon to `icon_dir` (where it
/// stays, for the notification centre), registers the id with the name
/// "Weft" and that icon, and makes it the id of this process. Call it
/// before the first window.
///
/// Uninstalling: delete `HKCU\Software\Classes\AppUserModelId\Weft.Windows`
/// and `toast-icon.png` in `icon_dir`.
pub fn register(icon_dir: &Path) -> io::Result<PathBuf> {
    std::fs::create_dir_all(icon_dir)?;
    let icon = icon_dir.join("toast-icon.png");
    if std::fs::read(&icon).ok().as_deref() != Some(ICON) {
        std::fs::write(&icon, ICON)?;
    }
    set_value("DisplayName", "Weft")?;
    set_value("IconUri", &icon.to_string_lossy())?;
    let id = wide(APP_ID);
    // SAFETY: the id is NUL-terminated and outlives the call.
    let result = unsafe { SetCurrentProcessExplicitAppUserModelID(id.as_ptr()) };
    if result < 0 {
        return Err(io::Error::other(format!("app id refused: {result:#x}")));
    }
    Ok(icon)
}

/// Escapes text for an XML attribute or element.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            // Not allowed in XML 1.0.
            c if c.is_control() && !matches!(c, '\n' | '\t') => {}
            c => escaped.push(c),
        }
    }
    escaped
}

/// How a toast behaves on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    /// Shown a few seconds, then kept in the notification centre.
    Default,
    /// Stays on screen until acted on, like a phone ringing.
    IncomingCall,
    /// Stays on screen until dismissed.
    Reminder,
}

/// A button: its text and the argument its click gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Button {
    pub label: String,
    pub argument: String,
}

impl Button {
    pub fn new(label: impl Into<String>, argument: impl Into<String>) -> Button {
        Button {
            label: label.into(),
            argument: argument.into(),
        }
    }
}

/// What a toast shows. The first line is its title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Content {
    pub lines: Vec<String>,
    /// The placeholder of a reply box, and the text of its send button,
    /// whose argument is `reply`.
    pub reply: Option<(String, String)>,
    pub buttons: Vec<Button>,
    pub scenario: Scenario,
    /// No sound: the phone already makes one.
    pub quiet: bool,
}

impl Content {
    pub fn new(lines: Vec<String>) -> Content {
        Content {
            lines,
            reply: None,
            buttons: Vec::new(),
            scenario: Scenario::Default,
            quiet: false,
        }
    }

    /// The toast's XML. Empty lines are left out; buttons past what
    /// Windows shows, the reply's send button included, are dropped.
    pub fn xml(&self) -> String {
        let scenario = match self.scenario {
            Scenario::Default => "",
            Scenario::IncomingCall => r#" scenario="incomingCall""#,
            Scenario::Reminder => r#" scenario="reminder""#,
        };
        let mut xml =
            format!(r#"<toast launch="open"{scenario}><visual><binding template="ToastGeneric">"#);
        for line in self.lines.iter().filter(|line| !line.trim().is_empty()) {
            xml.push_str(&format!("<text>{}</text>", escape(line)));
        }
        xml.push_str("</binding></visual>");
        let mut actions = String::new();
        let mut room = MAX_BUTTONS;
        if let Some((placeholder, send)) = &self.reply {
            actions.push_str(&format!(
                r#"<input id="{REPLY_INPUT}" type="text" placeHolderContent="{}"/>"#,
                escape(placeholder)
            ));
            actions.push_str(&format!(
                r#"<action content="{}" arguments="reply" hint-inputId="{REPLY_INPUT}"/>"#,
                escape(send)
            ));
            room -= 1;
        }
        for button in self.buttons.iter().take(room) {
            actions.push_str(&format!(
                r#"<action content="{}" arguments="{}"/>"#,
                escape(&button.label),
                escape(&button.argument)
            ));
        }
        if !actions.is_empty() {
            xml.push_str(&format!("<actions>{actions}</actions>"));
        }
        if self.quiet {
            xml.push_str(r#"<audio silent="true"/>"#);
        }
        xml.push_str("</toast>");
        xml
    }
}

/// A click on a toast: the argument of its button (`open` for the toast
/// itself) and the text of its reply box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activation {
    pub argument: String,
    pub reply: Option<String>,
}

fn activation(args: &IInspectable) -> windows_core::Result<Activation> {
    let args: ToastActivatedEventArgs = args.cast()?;
    let argument = args.Arguments()?.to_string_lossy();
    // The reply box only exists on some toasts.
    let reply = args
        .UserInput()
        .and_then(|input| input.Lookup(&HSTRING::from(REPLY_INPUT)))
        .and_then(|value| value.cast::<IPropertyValue>()?.GetString())
        .ok()
        .map(|text| text.to_string_lossy());
    Ok(Activation { argument, reply })
}

/// Shows toasts as [`APP_ID`].
pub struct Toaster {
    notifier: ToastNotifier,
}

/// A toast on screen or in the notification centre. Its click handler
/// lives as long as this value.
pub struct Shown {
    _toast: ToastNotification,
    _activated: windows_core::EventRevoker,
}

impl Toaster {
    pub fn new() -> windows_core::Result<Toaster> {
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))?;
        Ok(Toaster { notifier })
    }

    /// Shows `content`, replacing the toast of the same `group` and `tag`.
    /// `on_click` runs on a thread of the system's.
    pub fn show(
        &self,
        group: &str,
        tag: &str,
        content: &Content,
        suppress_popup: bool,
        on_click: impl Fn(Activation) + Send + 'static,
    ) -> windows_core::Result<Shown> {
        let document = XmlDocument::new()?;
        document.LoadXml(&HSTRING::from(content.xml()))?;
        let toast = ToastNotification::CreateToastNotification(&document)?;
        toast.SetTag(&HSTRING::from(tag))?;
        toast.SetGroup(&HSTRING::from(group))?;
        toast.SetSuppressPopup(suppress_popup)?;
        let activated = toast.Activated(move |_, args| match args.as_ref().map(activation) {
            Some(Ok(activation)) => on_click(activation),
            Some(Err(err)) => tracing::warn!(error = %err, "unreadable toast activation"),
            None => {}
        })?;
        self.notifier.Show(&toast)?;
        Ok(Shown {
            _toast: toast,
            _activated: activated,
        })
    }

    /// Removes the toast of `group` and `tag`, from the screen and the
    /// notification centre.
    pub fn remove(&self, group: &str, tag: &str) -> windows_core::Result<()> {
        ToastNotificationManager::History()?.RemoveGroupedTagWithId(
            &HSTRING::from(tag),
            &HSTRING::from(group),
            &HSTRING::from(APP_ID),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_is_escaped_and_buttons_capped() {
        let mut content = Content::new(vec![
            "Chat · Phone".into(),
            "<Ann & \"Bob\">".into(),
            " ".into(),
        ]);
        content.reply = Some(("Reply".into(), "Send".into()));
        content.buttons = (0..6)
            .map(|index| Button::new(format!("B{index}"), format!("action:{index}")))
            .collect();
        content.quiet = true;
        let xml = content.xml();
        assert!(xml.starts_with(r#"<toast launch="open"><visual>"#), "{xml}");
        assert!(
            xml.contains("<text>&lt;Ann &amp; &quot;Bob&quot;&gt;</text>"),
            "{xml}"
        );
        // The blank line is left out.
        assert_eq!(xml.matches("<text>").count(), 2);
        assert!(xml.contains(r#"<input id="reply" type="text" placeHolderContent="Reply"/>"#));
        assert!(xml.contains(r#"arguments="reply" hint-inputId="reply""#));
        // The send button and four others.
        assert_eq!(xml.matches("<action ").count(), 5);
        assert!(xml.contains(r#"arguments="action:3""#) && !xml.contains("action:4"));
        assert!(xml.ends_with(r#"<audio silent="true"/></toast>"#));

        let mut call = Content::new(vec!["Incoming call".into()]);
        call.scenario = Scenario::IncomingCall;
        let xml = call.xml();
        assert!(xml.starts_with(r#"<toast launch="open" scenario="incomingCall">"#));
        assert!(!xml.contains("<actions>"));
        assert_eq!(escape("a\u{1}b\nc"), "ab\nc");
    }
}
