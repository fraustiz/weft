//! What the window says, decided apart from the views so it can be tested.
//!
//! Texts that change with time come with how long they stay true, so the
//! window redraws when they change and not more often.

use std::time::{Duration, Instant, SystemTime};

use weft_cli::short_id;
use weft_cli::state::sanitize_device_name;

use crate::i18n::{is_french, t};
use crate::packets::{Battery, Connectivity, RejectReason};
use crate::transfer::{Failure, Stage};
use crate::worker::{Activity, Device, Direction, PairingFailure, Snapshot, Status, TransferView};

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;

/// Where the synchronisation stands, in words, and whether it is live: the
/// status mark is lit only then.
pub fn status(snapshot: &Snapshot) -> (String, bool) {
    if snapshot.pause.all {
        return (t("Paused", "En pause").into(), false);
    }
    match &snapshot.status {
        Status::Connected => (t("Connected", "Connecté").into(), true),
        Status::Connecting => (t("Connecting", "Connexion").into(), false),
        Status::Offline { .. } => (
            t("Offline — trying again", "Hors ligne — nouvelle tentative").into(),
            false,
        ),
        Status::Fatal(reason) => (format!("{}{reason}", t("Stopped: ", "Arrêté : ")), false),
    }
}

/// How long ago, in the largest unit that fits, and how long until it
/// reads differently.
fn ago(elapsed: Duration) -> (String, Duration) {
    let seconds = elapsed.as_secs();
    let (text, unit) = match seconds {
        0..MINUTE => (t("just now", "à l'instant").to_owned(), MINUTE),
        MINUTE..HOUR => {
            let minutes = seconds / MINUTE;
            let text = if is_french() {
                format!("il y a {minutes} min")
            } else {
                format!("{minutes} min ago")
            };
            (text, MINUTE)
        }
        HOUR..DAY => {
            let hours = seconds / HOUR;
            let text = if is_french() {
                format!("il y a {hours} h")
            } else {
                format!("{hours} h ago")
            };
            (text, HOUR)
        }
        _ => {
            let days = seconds / DAY;
            let text = match (is_french(), days) {
                (false, 1) => "1 day ago".to_owned(),
                (false, days) => format!("{days} days ago"),
                (true, 1) => "il y a 1 jour".to_owned(),
                (true, days) => format!("il y a {days} jours"),
            };
            (text, DAY)
        }
    };
    let next = Duration::from_secs(unit - seconds % unit)
        - Duration::from_nanos(elapsed.subsec_nanos().into());
    (text, next)
}

/// The last text that went through, without the text: "Last text sent
/// 2 min ago". `None` before the first one.
pub fn activity(activity: Option<Activity>, now: SystemTime) -> Option<(String, Duration)> {
    let activity = activity?;
    // A clock set back makes it "just now" rather than in the future.
    let (when, next) = ago(now.duration_since(activity.at).unwrap_or_default());
    let what = match activity.direction {
        Direction::Sent => t("Last text sent", "Dernier texte envoyé"),
        Direction::Received => t("Last text received", "Dernier texte reçu"),
    };
    Some((format!("{what} {when}"), next))
}

/// When the pairing code stops working, in whole minutes rounded up, and
/// how long until that changes.
pub fn expiry(expires_at: Instant, now: Instant) -> (String, Duration) {
    let left = expires_at.saturating_duration_since(now);
    let minutes = left.as_secs().div_ceil(MINUTE).max(1);
    let text = if is_french() {
        format!("Expire dans {minutes} min")
    } else {
        format!("Expires in {minutes} min")
    };
    let next = left.saturating_sub(Duration::from_secs((minutes - 1) * MINUTE));
    (text, next.max(Duration::from_secs(1)))
}

/// A device as the list names it.
pub fn device_name(device: &Device) -> &str {
    device
        .name
        .as_deref()
        .unwrap_or(t("Unnamed device", "Appareil sans nom"))
}

/// The short id shown under a device's name.
pub fn device_id(device: &Device) -> String {
    short_id(&device.id)
}

/// Why a pairing failed, as a sentence the user can act on.
pub fn failure(failure: &PairingFailure) -> String {
    let text = match failure {
        PairingFailure::InvalidCode => t(
            "This is not a Weft pairing code. Check that it was copied whole.",
            "Ce n'est pas un code d'association Weft. Vérifiez qu'il a été copié en entier.",
        ),
        PairingFailure::Expired => t(
            "This code has expired: a code lasts 5 minutes.",
            "Ce code a expiré : un code dure 5 minutes.",
        ),
        PairingFailure::OwnCode => t(
            "This code was made on this computer. Enter it on the other device instead.",
            "Ce code a été créé sur cet ordinateur. Saisissez-le plutôt sur l'autre appareil.",
        ),
        PairingFailure::OtherRelay => t(
            "This code is for a device on another relay. Both devices must use the same relay.",
            "Ce code vient d'un appareil sur un autre relais. Les deux appareils doivent utiliser le même relais.",
        ),
        PairingFailure::GroupFull => t(
            "There are already as many devices as Weft allows. Remove one to add another.",
            "Weft a déjà autant d'appareils qu'il en accepte. Retirez-en un pour en ajouter un autre.",
        ),
        PairingFailure::NotConnected => t(
            "Weft is not connected to the relay. Check the network, then try again.",
            "Weft n'est pas connecté au relais. Vérifiez le réseau, puis réessayez.",
        ),
        PairingFailure::ConnectionLost => t(
            "The connection to the relay was lost during pairing.",
            "La connexion au relais a été perdue pendant l'association.",
        ),
        PairingFailure::NoAnswer => t(
            "The other device did not answer in time. Check that it still shows the code.",
            "L'autre appareil n'a pas répondu à temps. Vérifiez qu'il affiche toujours le code.",
        ),
        PairingFailure::Other(raw) => return raw.clone(),
    };
    text.to_owned()
}

/// Why a transfer failed, as a short sentence.
pub fn transfer_failure(failure: Failure) -> &'static str {
    match failure {
        Failure::Rejected(Some(RejectReason::Disabled)) => t(
            "The other device does not accept files",
            "L'autre appareil n'accepte pas les fichiers",
        ),
        Failure::Rejected(Some(RejectReason::TooLarge)) => t(
            "Too large for the other device",
            "Trop grand pour l'autre appareil",
        ),
        Failure::Rejected(Some(RejectReason::NoSpace)) => t(
            "Not enough space on the other device",
            "Pas assez de place sur l'autre appareil",
        ),
        Failure::Rejected(Some(RejectReason::Busy)) => t(
            "The other device is busy with other files",
            "L'autre appareil reçoit déjà d'autres fichiers",
        ),
        Failure::Rejected(None) => t("Refused by the other device", "Refusé par l'autre appareil"),
        Failure::NoProgress => t(
            "Stopped: nothing moved for 10 minutes",
            "Arrêté : rien n'a avancé pendant 10 minutes",
        ),
        Failure::Disk => t(
            "The file could not be read or written",
            "Le fichier n'a pas pu être lu ou écrit",
        ),
        Failure::TooLarge => t(
            "Over 4 GB, the most Weft sends",
            "Plus de 4 Go, le maximum de Weft",
        ),
    }
}

/// A size in the units Explorer uses: 1 KB is 1024 bytes.
pub fn size(bytes: u64) -> String {
    const UNITS: [(&str, &str); 4] = [("KB", "Ko"), ("MB", "Mo"), ("GB", "Go"), ("TB", "To")];
    if bytes < 1024 {
        return if is_french() {
            format!("{bytes} o")
        } else {
            format!("{bytes} B")
        };
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let (english, french) = UNITS[unit];
    let number = if value < 10.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.0}")
    };
    if is_french() {
        format!("{} {french}", number.replace('.', ","))
    } else {
        format!("{number} {english}")
    }
}

/// The line under a transfer's name: how far it is, or how it ended.
pub fn transfer_status(transfer: &TransferView) -> String {
    match transfer.stage {
        Stage::Waiting => t(
            "Waiting for the other device",
            "En attente de l'autre appareil",
        )
        .into(),
        Stage::Running => {
            let (done, total) = (size(transfer.done), size(transfer.size));
            if is_french() {
                format!("{done} sur {total}")
            } else {
                format!("{done} of {total}")
            }
        }
        Stage::Done if transfer.outgoing => t("Sent", "Envoyé").into(),
        Stage::Done => t("Saved in Downloads", "Enregistré dans Téléchargements").into(),
        Stage::Cancelled => t("Cancelled", "Annulé").into(),
        Stage::Failed(failure) => transfer_failure(failure).into(),
    }
}

/// The battery level as a figure, and its state in words, if known.
pub fn battery(battery: &Battery) -> Option<(String, Option<&'static str>)> {
    let level = battery.level?;
    let state = if battery.charging == Some(true) {
        Some(t("Charging", "En charge"))
    } else if battery.low == Some(true) {
        Some(t("Low battery", "Batterie faible"))
    } else {
        None
    };
    // French puts a no-break space before the sign.
    let figure = if is_french() {
        format!("{level}\u{a0}%")
    } else {
        format!("{level}%")
    };
    Some((figure, state))
}

/// The network a device is on, in words, if known.
pub fn network(connectivity: &Connectivity) -> Option<String> {
    let network = connectivity.network.as_deref()?;
    Some(match network {
        "wifi" => "Wi-Fi".into(),
        "none" => t("No network", "Pas de réseau").into(),
        other => other.to_owned(),
    })
}

/// Why a device name is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameProblem {
    Empty,
    ControlCharacters,
    TooLong,
}

impl NameProblem {
    pub fn text(self) -> &'static str {
        match self {
            NameProblem::Empty => t("Enter a name", "Saisissez un nom"),
            NameProblem::ControlCharacters => t(
                "Remove the tabs and other control characters",
                "Retirez les tabulations et autres caractères de contrôle",
            ),
            NameProblem::TooLong => t(
                "Too long: 64 bytes at most, and accented letters or emoji take more than one",
                "Trop long : 64 octets au plus, et une lettre accentuée ou un emoji en prend plusieurs",
            ),
        }
    }
}

/// The name to save for what was typed, surrounding spaces aside, or why
/// it cannot be: the rule is the one names on the wire follow.
pub fn check_name(typed: &str) -> Result<String, NameProblem> {
    let name = typed.trim();
    match sanitize_device_name(name) {
        // Valid as typed: nothing had to be removed or cut.
        Some(valid) if valid == name => Ok(valid),
        _ if name.is_empty() => Err(NameProblem::Empty),
        _ if name.chars().any(char::is_control) => Err(NameProblem::ControlCharacters),
        _ => Err(NameProblem::TooLong),
    }
}

/// A position or length in a track, as players show them: `3:07`, or
/// `1:02:45` past an hour.
pub fn track_time(ms: u64) -> String {
    let seconds = ms / 1000;
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// A date and time on this computer's clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
}

/// A time in ms since the epoch, as this computer's time zone has it.
pub fn local_time(ms: i64) -> Option<LocalTime> {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

    // A FILETIME counts 100 ns from 1601.
    const EPOCH: u64 = 116_444_736_000_000_000;
    let ticks = u64::try_from(ms)
        .ok()?
        .checked_mul(10_000)?
        .checked_add(EPOCH)?;
    let file = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    // SAFETY: plain structures, valid for the calls; a null zone is the
    // current one.
    let local: SYSTEMTIME = unsafe {
        let mut utc: SYSTEMTIME = std::mem::zeroed();
        let mut local: SYSTEMTIME = std::mem::zeroed();
        if FileTimeToSystemTime(&file, &mut utc) == 0
            || SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) == 0
        {
            return None;
        }
        local
    };
    Some(LocalTime {
        year: local.wYear,
        month: local.wMonth,
        day: local.wDay,
        hour: local.wHour,
        minute: local.wMinute,
    })
}

/// When a message came: the time today, the day this year, the full date
/// before; with the time on any day when `always_time`, as a thread shows
/// it.
pub fn message_time(then: LocalTime, now: LocalTime, always_time: bool) -> String {
    const EN: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    const FR: [&str; 12] = [
        "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
        "déc.",
    ];
    if (then.year, then.month, then.day) == (now.year, now.month, now.day) {
        return format!("{:02}:{:02}", then.hour, then.minute);
    }
    let months = if is_french() { FR } else { EN };
    let month = months[usize::from(then.month.clamp(1, 12) - 1)];
    let date = if then.year == now.year {
        format!("{} {month}", then.day)
    } else {
        format!("{} {month} {}", then.day, then.year)
    };
    if always_time {
        format!("{date} {:02}:{:02}", then.hour, then.minute)
    } else {
        date
    }
}

/// [`message_time`] of a time in ms since the epoch; empty if unknown.
pub fn sms_time(ms: i64, always_time: bool) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0);
    match (local_time(ms), local_time(now)) {
        (Some(then), Some(now)) if ms > 0 => message_time(then, now, always_time),
        _ => String::new(),
    }
}

/// Why the phone could not send an SMS, from its reason.
pub fn sms_failure(reason: Option<&str>) -> &'static str {
    match reason {
        Some("permission") => t(
            "Weft may not send SMS on the phone",
            "Weft n'a pas le droit d'envoyer des SMS sur le téléphone",
        ),
        Some("noSms") => t(
            "This phone cannot send SMS",
            "Ce téléphone ne peut pas envoyer de SMS",
        ),
        Some("noService") => t("No mobile service", "Pas de réseau mobile"),
        Some("radioOff") => t(
            "The phone's mobile network is off",
            "Le réseau mobile du téléphone est coupé",
        ),
        _ => t(
            "The phone could not send it",
            "Le téléphone n'a pas pu l'envoyer",
        ),
    }
}

/// Ready-made shortcuts: a name and the line `cmd /C` runs. Each only
/// fills a new shortcut, which the user can still change.
pub fn command_presets() -> [(&'static str, &'static str); 6] {
    [
        (
            t("Lock this PC", "Verrouiller le PC"),
            "rundll32.exe user32.dll,LockWorkStation",
        ),
        // Not `rundll32 powrprof.dll,SetSuspendState`: rundll32 passes its
        // own arguments, so that one hibernates where hibernation is on.
        (
            t("Sleep", "Mettre en veille"),
            "powershell -NoProfile -WindowStyle Hidden -Command \"Add-Type -AssemblyName System.Windows.Forms; [void][System.Windows.Forms.Application]::SetSuspendState('Suspend', $false, $false)\"",
        ),
        // SC_MONITORPOWER, 2 for off, posted to every window: posted, not
        // sent, so a window that does not answer cannot hold it up.
        (
            t("Turn off the screen", "Éteindre l'écran"),
            "powershell -NoProfile -WindowStyle Hidden -Command \"(Add-Type -Name Screen -Namespace Weft -PassThru -MemberDefinition '[DllImport(\\\"user32.dll\\\")] public static extern bool PostMessage(IntPtr w, uint m, IntPtr p, IntPtr l);')::PostMessage([IntPtr]0xFFFF, 0x0112, [IntPtr]0xF170, [IntPtr]2)\"",
        ),
        // The mute key, as the keyboard's.
        (
            t("Mute or unmute", "Couper ou remettre le son"),
            "powershell -NoProfile -WindowStyle Hidden -Command \"(New-Object -ComObject WScript.Shell).SendKeys([char]173)\"",
        ),
        (
            t("Shut down in 1 minute", "Éteindre dans 1 minute"),
            "shutdown /s /t 60",
        ),
        (
            t("Cancel the shutdown", "Annuler l'extinction"),
            "shutdown /a",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_presets_are_complete_lines() {
        let presets = command_presets();
        for (name, line) in presets {
            assert!(!name.is_empty() && !line.trim().is_empty());
            // cmd /C keeps its quotes in pairs.
            assert_eq!(line.matches('"').count() % 2, 0, "{line}");
        }
        let lines: Vec<&str> = presets.iter().map(|(_, line)| *line).collect();
        assert!(lines.contains(&"rundll32.exe user32.dll,LockWorkStation"));
        assert!(lines.contains(&"shutdown /s /t 60") && lines.contains(&"shutdown /a"));
        assert!(!lines.iter().any(|line| line.contains("powrprof")));
        assert!(lines[1].contains("SetSuspendState('Suspend'"));
        assert!(lines[2].contains(r#"[DllImport(\"user32.dll\")]"#));
    }
    use crate::sync::Pause;
    use crate::worker::PairingState;

    #[test]
    fn message_times_shorten_with_age() {
        let at = |year, month, day, hour, minute| LocalTime {
            year,
            month,
            day,
            hour,
            minute,
        };
        let now = at(2026, 11, 16, 18, 0);
        assert_eq!(message_time(at(2026, 11, 16, 9, 5), now, false), "09:05");
        let day = message_time(at(2026, 11, 2, 9, 5), now, false);
        assert!(day == "2 Nov" || day == "2 nov.", "{day}");
        let old = message_time(at(2025, 8, 30, 9, 5), now, false);
        assert!(old == "30 Aug 2025" || old == "30 août 2025", "{old}");
        // A thread keeps the time on other days.
        let stamp = message_time(at(2026, 11, 15, 23, 2), now, true);
        assert!(
            stamp == "15 Nov 23:02" || stamp == "15 nov. 23:02",
            "{stamp}"
        );
        assert_eq!(message_time(at(2026, 11, 16, 9, 5), now, true), "09:05");
        // The epoch is a date like any other; before it there is none.
        assert!(local_time(0).is_some());
        assert_eq!(local_time(-1), None);
    }

    fn snapshot(status: Status) -> Snapshot {
        Snapshot {
            status,
            name: "Test PC".into(),
            device_id: [1; 16],
            relay_url: "wss://relay.example/v1".into(),
            devices: Vec::new(),
            peers: Default::default(),
            pause: Pause::default(),
            features: Default::default(),
            control: Default::default(),
            commands: Vec::new(),
            pairing: PairingState::Idle,
            last_activity: None,
            ringing: None,
            transfers: Vec::new(),
        }
    }

    #[test]
    fn the_status_is_always_in_words_and_lit_only_when_live() {
        assert_eq!(
            status(&snapshot(Status::Connected)),
            (t("Connected", "Connecté").into(), true)
        );
        let offline = Status::Offline {
            reason: "the relay closed the connection".into(),
            retry_at: Instant::now(),
        };
        let (text, live) = status(&snapshot(offline));
        assert!(!live);
        assert!(text.starts_with(t("Offline", "Hors ligne")));
        let (text, live) = status(&snapshot(Status::Fatal("invalid relay url".into())));
        assert!(!live);
        assert!(text.ends_with("invalid relay url"));

        let mut paused = snapshot(Status::Connected);
        paused.pause.all = true;
        assert_eq!(status(&paused), (t("Paused", "En pause").into(), false));
    }

    #[test]
    fn activity_reads_in_the_largest_unit_and_says_when_it_changes() {
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let sent = Some(Activity {
            direction: Direction::Sent,
            at,
        });
        let after = |seconds| activity(sent, at + Duration::from_secs(seconds)).unwrap();

        assert_eq!(activity(None, at), None);
        assert_eq!(
            after(0),
            (
                t(
                    "Last text sent just now",
                    "Dernier texte envoyé à l'instant"
                )
                .into(),
                Duration::from_secs(60)
            )
        );
        assert_eq!(
            after(150),
            (
                t(
                    "Last text sent 2 min ago",
                    "Dernier texte envoyé il y a 2 min"
                )
                .into(),
                Duration::from_secs(30)
            )
        );
        assert_eq!(after(2 * HOUR + 5).1, Duration::from_secs(HOUR - 5));
        assert!(after(2 * HOUR).0.contains('2'));
        assert!(after(3 * DAY).0.contains('3'));

        let received = Activity {
            direction: Direction::Received,
            at,
        };
        // A clock set back.
        let (text, _) = activity(Some(received), at - Duration::from_secs(5)).unwrap();
        assert_eq!(
            text,
            t(
                "Last text received just now",
                "Dernier texte reçu à l'instant"
            )
        );
    }

    #[test]
    fn expiry_counts_whole_minutes_rounded_up() {
        let now = Instant::now();
        let in_ = |seconds| expiry(now + Duration::from_secs(seconds), now);
        assert_eq!(
            in_(300),
            (
                t("Expires in 5 min", "Expire dans 5 min").into(),
                Duration::from_secs(60)
            )
        );
        assert_eq!(in_(241).1, Duration::from_secs(1));
        assert!(in_(240).0.contains('4'));
        assert!(in_(30).0.contains('1'));
        assert!(expiry(now, now + Duration::from_secs(5)).0.contains('1'));
    }

    #[test]
    fn devices_without_a_name_are_still_named() {
        let device = Device {
            id: [0xab; 16],
            name: None,
        };
        assert_eq!(
            device_name(&device),
            t("Unnamed device", "Appareil sans nom")
        );
        assert_eq!(device_id(&device), "abababab");
        let named = Device {
            name: Some("Test phone".into()),
            ..device
        };
        assert_eq!(device_name(&named), "Test phone");
    }

    #[test]
    fn known_failures_have_their_own_sentence_and_others_keep_their_text() {
        let known = [
            PairingFailure::InvalidCode,
            PairingFailure::Expired,
            PairingFailure::OwnCode,
            PairingFailure::OtherRelay,
            PairingFailure::GroupFull,
            PairingFailure::NotConnected,
            PairingFailure::ConnectionLost,
            PairingFailure::NoAnswer,
        ];
        let sentences: Vec<String> = known.iter().map(failure).collect();
        for (index, sentence) in sentences.iter().enumerate() {
            assert!(sentence.ends_with('.'), "{sentence}");
            assert!(!sentence.contains('!'), "{sentence}");
            assert!(!sentences[..index].contains(sentence), "{sentence}");
        }
        assert_eq!(
            failure(&PairingFailure::Other("relay error: rate limited".into())),
            "relay error: rate limited"
        );
    }

    #[test]
    fn names_are_checked_as_typed() {
        assert_eq!(check_name("  Test PC "), Ok("Test PC".into()));
        assert_eq!(check_name("é".repeat(32).as_str()), Ok("é".repeat(32)));
        assert_eq!(check_name("   "), Err(NameProblem::Empty));
        assert_eq!(check_name("Test\tPC"), Err(NameProblem::ControlCharacters));
        assert_eq!(check_name(&"a".repeat(65)), Err(NameProblem::TooLong));
        // 33 characters, but 66 bytes.
        assert_eq!(check_name(&"é".repeat(33)), Err(NameProblem::TooLong));
    }

    #[test]
    fn sizes_read_as_explorer_shows_them() {
        assert_eq!(size(0), t("0 B", "0 o"));
        assert_eq!(size(1023), t("1023 B", "1023 o"));
        assert_eq!(size(1536), t("1.5 KB", "1,5 Ko"));
        assert_eq!(size(5 * 1024 * 1024), t("5.0 MB", "5,0 Mo"));
        assert_eq!(size(250 * 1024 * 1024), t("250 MB", "250 Mo"));
        assert_eq!(size(4 << 30), t("4.0 GB", "4,0 Go"));
    }

    #[test]
    fn transfers_say_where_they_stand() {
        let mut transfer = TransferView {
            id: [1; 16],
            device: [2; 16],
            outgoing: true,
            name: "photo.jpg".into(),
            size: 2048,
            done: 1024,
            stage: Stage::Running,
        };
        assert_eq!(
            transfer_status(&transfer),
            t("1.0 KB of 2.0 KB", "1,0 Ko sur 2,0 Ko")
        );
        transfer.stage = Stage::Done;
        assert_eq!(transfer_status(&transfer), t("Sent", "Envoyé"));
        let busy = Failure::Rejected(Some(RejectReason::Busy));
        transfer.stage = Stage::Failed(busy);
        assert_eq!(transfer_status(&transfer), transfer_failure(busy));
        // Every failure has its own sentence, without a final period.
        let failures = [
            Failure::Rejected(Some(RejectReason::Disabled)),
            Failure::Rejected(Some(RejectReason::TooLarge)),
            Failure::Rejected(Some(RejectReason::NoSpace)),
            busy,
            Failure::Rejected(None),
            Failure::NoProgress,
            Failure::Disk,
            Failure::TooLarge,
        ];
        let sentences: Vec<&str> = failures.iter().map(|f| transfer_failure(*f)).collect();
        for (index, sentence) in sentences.iter().enumerate() {
            assert!(!sentence.ends_with('.'), "{sentence}");
            assert!(!sentences[..index].contains(sentence), "{sentence}");
        }
    }

    #[test]
    fn battery_and_network_only_say_what_is_known() {
        assert_eq!(battery(&Battery::default()), None);
        let charging = Battery {
            level: Some(82),
            charging: Some(true),
            low: Some(false),
        };
        let (figure, state) = battery(&charging).unwrap();
        assert!(figure.starts_with("82"));
        assert_eq!(state, Some(t("Charging", "En charge")));
        let low = Battery {
            level: Some(9),
            charging: None,
            low: Some(true),
        };
        assert_eq!(
            battery(&low).unwrap().1,
            Some(t("Low battery", "Batterie faible"))
        );

        assert_eq!(network(&Connectivity::default()), None);
        let wifi = Connectivity {
            network: Some("wifi".into()),
            signal: Some(3),
        };
        assert_eq!(network(&wifi).as_deref(), Some("Wi-Fi"));
        let lte = Connectivity {
            network: Some("LTE".into()),
            signal: None,
        };
        assert_eq!(network(&lte).as_deref(), Some("LTE"));
    }

    #[test]
    fn track_times() {
        assert_eq!(track_time(0), "0:00");
        assert_eq!(track_time(187_400), "3:07");
        assert_eq!(track_time(3_765_000), "1:02:45");
    }
}
