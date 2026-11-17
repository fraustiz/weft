//! Ready-made shortcut command presets for Linux desktops.

use weft_client::features::UserCommand;
use weft_protocol::random_message_id;

/// A ready-made command preset offered by `weft-desktop commands presets`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandPreset {
    pub key: &'static str,
    pub name: &'static str,
    pub line: &'static str,
}

/// Standard Linux desktop shortcuts using freedesktop / systemd / PipeWire tools.
pub const COMMAND_PRESETS: &[CommandPreset] = &[
    CommandPreset {
        key: "lock",
        name: "Lock screen",
        line: "loginctl lock-session",
    },
    CommandPreset {
        key: "suspend",
        name: "Suspend",
        line: "systemctl suspend",
    },
    CommandPreset {
        key: "mute",
        name: "Mute / unmute audio",
        line: "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle",
    },
    CommandPreset {
        key: "play-pause",
        name: "Play / pause media",
        line: "playerctl play-pause",
    },
];

/// Finds a preset by its short `key` (e.g. `"lock"`), 1-based index, or case-insensitive `name`.
pub fn find_preset(query: &str) -> Option<&'static CommandPreset> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(index) = trimmed.parse::<usize>()
        && index >= 1
        && index <= COMMAND_PRESETS.len()
    {
        return Some(&COMMAND_PRESETS[index - 1]);
    }
    COMMAND_PRESETS.iter().find(|preset| {
        preset.key.eq_ignore_ascii_case(trimmed) || preset.name.eq_ignore_ascii_case(trimmed)
    })
}

impl CommandPreset {
    /// Creates a [`UserCommand`] with a fresh random 8-byte hex ID.
    pub fn to_user_command(&self) -> UserCommand {
        UserCommand {
            id: weft_cli::hex(&random_message_id()[..8]),
            name: self.name.to_owned(),
            line: self.line.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_presets_cover_lock_suspend_and_pipewire_mute() {
        let lines: Vec<&str> = COMMAND_PRESETS.iter().map(|p| p.line).collect();
        assert!(lines.contains(&"loginctl lock-session"));
        assert!(lines.contains(&"systemctl suspend"));
        assert!(lines.contains(&"wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle"));

        assert_eq!(find_preset("lock").unwrap().line, "loginctl lock-session");
        assert_eq!(find_preset("2").unwrap().line, "systemctl suspend");
        assert_eq!(
            find_preset("Mute / unmute audio").unwrap().line,
            "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle"
        );
        assert!(find_preset("0").is_none());
        assert!(find_preset("unknown").is_none());

        let cmd = find_preset("lock").unwrap().to_user_command();
        assert_eq!(cmd.id.len(), 16);
        assert_eq!(cmd.name, "Lock screen");
        assert_eq!(cmd.line, "loginctl lock-session");
    }
}
