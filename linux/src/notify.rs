//! Desktop notifications via `gdbus` (`org.freedesktop.Notifications`) with
//! automatic fallback to `notify-send`, plus [`Notice`] routing for rings,
//! links, calls, media commands, and volume changes.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use weft_client::packets::{CallEvent, RejectReason, is_shareable_url};
use weft_client::transfer::Failure;
use weft_client::worker::Notice;

use crate::media::MediaAdapter;
use crate::runner::{Runner, SystemRunner};
use crate::sound::Ringer;
use crate::volume::VolumeAdapter;

const DBUS_DEST: &str = "org.freedesktop.Notifications";
const DBUS_PATH: &str = "/org/freedesktop/Notifications";
const METHOD_NOTIFY: &str = "org.freedesktop.Notifications.Notify";
const METHOD_CLOSE: &str = "org.freedesktop.Notifications.CloseNotification";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum TrackedKey {
    Ring,
    Update,
    PhoneNotification { from: [u8; 16], id: String },
    Call([u8; 16]),
    Sms { from: [u8; 16], thread: i64 },
}

/// Parses the `uint32` notification ID returned by `gdbus call ... Notify`, e.g.
/// `(uint32 42,)` or `(42,)`.
pub fn parse_gdbus_notification_id(stdout: &str) -> Option<u32> {
    let trimmed = stdout.trim();
    let inner = trimmed.strip_prefix('(')?.strip_suffix(')')?.trim();
    let first = inner.split(',').next()?.trim();
    let digits = first.strip_prefix("uint32").map(str::trim).unwrap_or(first);
    digits.parse::<u32>().ok().filter(|&id| id > 0)
}

/// Formats `text` as a GVariant string literal (`"..."`) so `gdbus call` parses
/// arbitrary text (including quotes, backslashes, leading dashes, and newlines)
/// unambiguously without ever going through a shell.
pub fn gvariant_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn named_device(device: Option<String>) -> String {
    device.unwrap_or_else(|| "a device".to_owned())
}

fn transfer_failure_text(failure: Failure) -> &'static str {
    match failure {
        Failure::Rejected(Some(RejectReason::Disabled)) => {
            "File transfers are turned off on the other device"
        }
        Failure::Rejected(Some(RejectReason::TooLarge)) => "The file is too large",
        Failure::Rejected(Some(RejectReason::NoSpace)) => {
            "Not enough free space on the other device"
        }
        Failure::Rejected(Some(RejectReason::Busy)) => "Too many transfers already running",
        Failure::Rejected(None) => "Refused by the other device",
        Failure::NoProgress => "Timed out with no progress",
        Failure::Disk => "Could not read or write the file on disk",
        Failure::TooLarge => "Over 4 GiB, the maximum transfer size",
    }
}

/// Routes worker [`Notice`] events to desktop notifications, `xdg-open`,
/// ring audio, MPRIS2 players, and PipeWire/PulseAudio volume.
pub struct Notifier {
    runner: Arc<dyn Runner>,
    ringer: Arc<Ringer>,
    media: Arc<MediaAdapter>,
    volume: Arc<VolumeAdapter>,
    active_ids: Mutex<HashMap<TrackedKey, u32>>,
}

impl Notifier {
    pub fn new(
        runner: Arc<dyn Runner>,
        ringer: Arc<Ringer>,
        media: Arc<MediaAdapter>,
        volume: Arc<VolumeAdapter>,
    ) -> Arc<Self> {
        Arc::new(Self {
            runner,
            ringer,
            media,
            volume,
            active_ids: Mutex::new(HashMap::new()),
        })
    }

    pub fn system(state_dir: &Path) -> Arc<Self> {
        let runner: Arc<dyn Runner> = Arc::new(SystemRunner);
        let ringer = Ringer::new(state_dir.to_path_buf(), runner.clone());
        let media = Arc::new(MediaAdapter::new(runner.clone()));
        let volume = Arc::new(VolumeAdapter::new(runner.clone()));
        Self::new(runner, ringer, media, volume)
    }

    pub fn update_available(&self, version: &str) {
        self.post(
            Some(TrackedKey::Update),
            &format!("Weft {version} available"),
            "Run `weft-desktop update --install` to update.",
            false,
            false,
            -1,
        );
    }

    pub fn stop_ring(&self) {
        self.ringer.stop();
        self.close_tracked(&TrackedKey::Ring);
    }

    pub fn handle(&self, notice: Notice) {
        match notice {
            // The daemon routes images to the clipboard sync loop.
            Notice::ClipboardImage { .. } => {}
            Notice::Ring { device, .. } => {
                self.ringer.start();
                let who = named_device(device);
                self.post(
                    Some(TrackedKey::Ring),
                    &format!("{who} is ringing this computer"),
                    "Run `weft-desktop found-it` to stop ringing.",
                    true,
                    true,
                    60_000,
                );
            }
            Notice::RingEnded => {
                self.stop_ring();
            }
            Notice::Ping {
                device, message, ..
            } => {
                let who = named_device(device);
                self.post(
                    None,
                    &format!("Ping from {who}"),
                    message.as_deref().unwrap_or(""),
                    false,
                    false,
                    -1,
                );
            }
            Notice::Link { device, url, .. } => {
                if is_shareable_url(&url) {
                    self.open_url(&url);
                    let who = named_device(device);
                    self.post(None, &format!("Link from {who}"), &url, false, false, -1);
                }
            }
            Notice::FileReceived { device, path, .. } => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "file".into());
                let who = named_device(device);
                self.post(
                    None,
                    &format!("Received {name}"),
                    &format!("Saved from {who} in Downloads"),
                    false,
                    false,
                    -1,
                );
            }
            Notice::TransferFailed {
                name,
                outgoing,
                failure,
                ..
            } => {
                let summary = if outgoing {
                    format!("{name} was not sent")
                } else {
                    format!("{name} was not received")
                };
                self.post(
                    None,
                    &summary,
                    transfer_failure_text(failure),
                    false,
                    false,
                    -1,
                );
            }
            Notice::Notification {
                from,
                device,
                notification,
            } => {
                let who = named_device(device);
                let heading = if notification.app.trim().is_empty() {
                    who
                } else {
                    format!("{} · {who}", notification.app)
                };
                let body = match (
                    notification.title.trim().is_empty(),
                    notification.text.trim().is_empty(),
                ) {
                    (false, false) => format!("{}\n{}", notification.title, notification.text),
                    (false, true) => notification.title,
                    (true, false) => notification.text,
                    (true, true) => String::new(),
                };
                self.post(
                    Some(TrackedKey::PhoneNotification {
                        from,
                        id: notification.id,
                    }),
                    &heading,
                    &body,
                    false,
                    notification.silent,
                    -1,
                );
            }
            Notice::NotificationRemoved { from, id } => {
                self.close_tracked(&TrackedKey::PhoneNotification { from, id });
            }
            Notice::Call { from, device, call } => {
                let key = TrackedKey::Call(from);
                let who = named_device(device);
                match call.event() {
                    Some(CallEvent::Ringing) => {
                        let summary = match call.caller() {
                            Some(caller) => format!("Incoming call: {caller}"),
                            None => "Incoming call".to_owned(),
                        };
                        self.post(Some(key), &summary, &who, true, true, 30_000);
                    }
                    Some(CallEvent::Missed) => {
                        self.close_tracked(&key);
                        let summary = match call.caller() {
                            Some(caller) => format!("Missed call: {caller}"),
                            None => "Missed call".to_owned(),
                        };
                        self.post(None, &summary, &who, false, false, -1);
                    }
                    Some(CallEvent::Talking | CallEvent::Ended) | None => {
                        self.close_tracked(&key);
                    }
                }
            }
            Notice::MediaCommand { player, action, .. } => {
                let _ = self.media.execute(&player, action);
            }
            Notice::SetVolume { set, .. } => {
                let _ = self.volume.set_volume(set);
            }
            Notice::SmsReceived {
                from,
                device,
                thread,
                sender,
                text,
                ..
            } => {
                let who = named_device(device);
                self.post(
                    Some(TrackedKey::Sms { from, thread }),
                    &format!("{sender} · {who}"),
                    &text,
                    false,
                    false,
                    -1,
                );
            }
            Notice::CommandRan { device, name, .. } => {
                let who = named_device(device);
                self.post(None, &format!("Ran {name} for {who}"), "", false, false, -1);
            }
        }
    }

    fn open_url(&self, url: &str) {
        if self.runner.has_program("xdg-open") {
            let _ = self.runner.spawn_detached("xdg-open", &[url]);
        }
    }

    fn post(
        &self,
        tracked: Option<TrackedKey>,
        summary: &str,
        body: &str,
        critical: bool,
        silent: bool,
        timeout_ms: i32,
    ) {
        let replaces = tracked
            .as_ref()
            .and_then(|key| self.active_ids.lock().unwrap().get(key).copied())
            .unwrap_or(0);

        if self.runner.has_program("gdbus")
            && let Some(id) =
                self.notify_via_gdbus(replaces, summary, body, critical, silent, timeout_ms)
        {
            if let Some(key) = tracked {
                self.active_ids.lock().unwrap().insert(key, id);
            }
            return;
        }

        if self.runner.has_program("notify-send") {
            let urgency = if critical { "critical" } else { "normal" };
            let mut args = vec!["--app-name=Weft", "--urgency", urgency];
            let timeout_str;
            if timeout_ms > 0 {
                timeout_str = timeout_ms.to_string();
                args.extend(["--expire-time", &timeout_str]);
            }
            args.push("--");
            args.push(summary);
            if !body.is_empty() {
                args.push(body);
            }
            let _ = self.runner.run("notify-send", &args, None);
        }
    }

    fn notify_via_gdbus(
        &self,
        replaces_id: u32,
        summary: &str,
        body: &str,
        critical: bool,
        silent: bool,
        timeout_ms: i32,
    ) -> Option<u32> {
        let app = gvariant_string("Weft");
        let replaces = format!("uint32 {replaces_id}");
        let icon = gvariant_string("dialog-information");
        let sum = gvariant_string(summary);
        let bdy = gvariant_string(body);
        let mut hints = Vec::new();
        if critical {
            hints.push("'urgency': <byte 2>");
        }
        if silent {
            hints.push("'suppress-sound': <true>");
        }
        let hints_arg = format!("{{{}}}", hints.join(", "));
        let timeout_arg = timeout_ms.to_string();
        let args = [
            "call",
            "--session",
            "--dest",
            DBUS_DEST,
            "--object-path",
            DBUS_PATH,
            "--method",
            METHOD_NOTIFY,
            "--",
            &app,
            &replaces,
            &icon,
            &sum,
            &bdy,
            "[]",
            &hints_arg,
            &timeout_arg,
        ];
        let out = self.runner.run("gdbus", &args, None).ok()?;
        if !out.success {
            return None;
        }
        parse_gdbus_notification_id(&out.stdout_lossy())
    }

    fn close_tracked(&self, key: &TrackedKey) {
        let Some(id) = self.active_ids.lock().unwrap().remove(key) else {
            return;
        };
        if !self.runner.has_program("gdbus") {
            return;
        }
        let id_arg = format!("uint32 {id}");
        let args = [
            "call",
            "--session",
            "--dest",
            DBUS_DEST,
            "--object-path",
            DBUS_PATH,
            "--method",
            METHOD_CLOSE,
            "--",
            &id_arg,
        ];
        let _ = self.runner.run("gdbus", &args, None);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use tempfile::TempDir;
    use weft_client::packets::Notification;

    use super::*;
    use crate::runner::{CmdOutput, FakeRunner};

    #[test]
    fn parses_gdbus_notification_ids() {
        assert_eq!(parse_gdbus_notification_id("(uint32 42,)\n"), Some(42));
        assert_eq!(parse_gdbus_notification_id("(7,)"), Some(7));
        assert_eq!(parse_gdbus_notification_id("(uint32 0,)\n"), None);
        assert_eq!(parse_gdbus_notification_id("error"), None);
    }

    #[test]
    fn posts_replaces_and_closes_notifications_via_gdbus() {
        let next_id = Arc::new(AtomicU32::new(100));
        let next_bg = next_id.clone();
        let runner = Arc::new(FakeRunner::with_handler(
            &["gdbus"],
            move |prog, args, _| {
                assert_eq!(prog, "gdbus");
                if args.contains(&METHOD_NOTIFY) {
                    let id = next_bg.fetch_add(1, Ordering::SeqCst);
                    Ok(CmdOutput::ok_str(&format!("(uint32 {id},)\n")))
                } else {
                    Ok(CmdOutput::ok_str("()\n"))
                }
            },
        ));
        let temp = TempDir::new().unwrap();
        let ringer = Ringer::with_fake_runner(temp.path().to_path_buf(), runner.clone());
        let media = Arc::new(MediaAdapter::new(runner.clone()));
        let volume = Arc::new(VolumeAdapter::new(runner.clone()));
        let notifier = Notifier::new(runner.clone(), ringer, media, volume);

        let from = [0xaa; 16];
        notifier.handle(Notice::Notification {
            from,
            device: Some("Pixel".into()),
            notification: Notification {
                id: "notif-1".into(),
                app: "Signal".into(),
                title: "Alice \"A\"".into(),
                text: "-hello".into(),
                silent: true,
                ..Notification::default()
            },
        });
        // Update same notification -> reuses uint32 100 as replaces_id!
        notifier.handle(Notice::Notification {
            from,
            device: Some("Pixel".into()),
            notification: Notification {
                id: "notif-1".into(),
                app: "Signal".into(),
                title: "Alice".into(),
                text: "updated".into(),
                ..Notification::default()
            },
        });
        // Removal -> calls CloseNotification with uint32 101!
        notifier.handle(Notice::NotificationRemoved {
            from,
            id: "notif-1".into(),
        });

        let calls = runner.calls();
        assert_eq!(calls.len(), 3);
        assert!(calls[0].args.contains(&"uint32 0".to_owned()));
        assert!(calls[0].args.iter().any(|a| a.contains("suppress-sound")));
        assert!(calls[1].args.contains(&"uint32 100".to_owned()));
        assert!(calls[2].args.contains(&METHOD_CLOSE.to_owned()));
        assert!(calls[2].args.contains(&"uint32 101".to_owned()));
    }

    #[test]
    fn falls_back_to_notify_send_when_gdbus_fails_or_is_absent() {
        let runner = Arc::new(FakeRunner::new(&["notify-send", "xdg-open"]));
        let temp = TempDir::new().unwrap();
        let ringer = Ringer::with_fake_runner(temp.path().to_path_buf(), runner.clone());
        let media = Arc::new(MediaAdapter::new(runner.clone()));
        let volume = Arc::new(VolumeAdapter::new(runner.clone()));
        let notifier = Notifier::new(runner.clone(), ringer, media, volume);

        notifier.handle(Notice::Ping {
            from: [1; 16],
            device: Some("Phone".into()),
            message: Some("-dash message".into()),
        });
        notifier.handle(Notice::Link {
            from: [1; 16],
            device: Some("Phone".into()),
            url: "https://example.org/test".into(),
        });

        let calls = runner.calls();
        assert_eq!(calls[0].program, "notify-send");
        assert!(calls[0].args.contains(&"--".to_owned()));
        assert_eq!(calls[1].program, "xdg-open");
        assert_eq!(calls[1].args, ["https://example.org/test"]);
    }
}
