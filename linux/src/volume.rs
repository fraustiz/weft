//! System audio output volume control via PipeWire (`wpctl`) with fallback to
//! PulseAudio (`pactl`).

use std::sync::Arc;

use weft_client::packets::{Volume, VolumeSet};

use crate::runner::{Runner, SystemRunner};

const PIPEWIRE_SINK: &str = "@DEFAULT_AUDIO_SINK@";
const PULSE_SINK: &str = "@DEFAULT_SINK@";

/// Audio mixer backend on Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeBackend {
    PipeWire,
    PulseAudio,
    Unavailable,
}

pub fn detect_volume_backend(runner: &dyn Runner) -> VolumeBackend {
    if runner.has_program("wpctl") {
        VolumeBackend::PipeWire
    } else if runner.has_program("pactl") {
        VolumeBackend::PulseAudio
    } else {
        VolumeBackend::Unavailable
    }
}

/// Parses `wpctl get-volume @DEFAULT_AUDIO_SINK@` output, e.g.:
/// - `Volume: 0.45` -> `level: Some(45), muted: Some(false)`
/// - `Volume: 0.45 [MUTED]` -> `level: Some(45), muted: Some(true)`
/// - `Volume: 1,25` -> `level: Some(100), muted: Some(false)`
pub fn parse_wpctl_volume(output: &str) -> Option<Volume> {
    let line = output.lines().find(|l| l.contains("Volume:"))?;
    let after = line.split_once("Volume:")?.1.trim();
    let muted = after.contains("[MUTED]");
    let number_token = after.split_whitespace().next()?.replace(',', ".");
    let fraction: f64 = number_token.parse().ok()?;
    if !fraction.is_finite() || fraction < 0.0 {
        return None;
    }
    let level = (fraction * 100.0).round().clamp(0.0, 100.0) as u8;
    Some(Volume {
        level: Some(level),
        muted: Some(muted),
    })
}

/// Parses `pactl get-sink-volume @DEFAULT_SINK@` and `pactl get-sink-mute` outputs.
pub fn parse_pactl_volume(volume_out: &str, mute_out: Option<&str>) -> Option<Volume> {
    let level = volume_out.split('/').find_map(|segment| {
        let token = segment.trim().strip_suffix('%')?.trim();
        let pct: u16 = token.parse().ok()?;
        Some(pct.min(100) as u8)
    });
    let muted = mute_out.and_then(|raw| {
        let lower = raw.to_ascii_lowercase();
        if lower.contains("yes") || lower.contains("true") || lower.contains("oui") {
            Some(true)
        } else if lower.contains("no") || lower.contains("false") || lower.contains("non") {
            Some(false)
        } else {
            None
        }
    });
    if level.is_none() && muted.is_none() {
        return None;
    }
    Some(Volume { level, muted })
}

/// Reads and updates the default audio sink volume.
pub struct VolumeAdapter {
    runner: Arc<dyn Runner>,
}

impl VolumeAdapter {
    pub fn new(runner: Arc<dyn Runner>) -> Self {
        Self { runner }
    }

    pub fn system() -> Self {
        Self::new(Arc::new(SystemRunner))
    }

    pub fn backend(&self) -> VolumeBackend {
        detect_volume_backend(self.runner.as_ref())
    }

    pub fn get_volume(&self) -> Option<Volume> {
        match self.backend() {
            VolumeBackend::PipeWire => {
                let out = self
                    .runner
                    .run("wpctl", &["get-volume", PIPEWIRE_SINK], None)
                    .ok()?;
                if out.success
                    && let Some(vol) = parse_wpctl_volume(&out.stdout_lossy())
                {
                    return Some(vol);
                }
                if self.runner.has_program("pactl") {
                    return self.get_pactl_volume();
                }
                None
            }
            VolumeBackend::PulseAudio => self.get_pactl_volume(),
            VolumeBackend::Unavailable => None,
        }
    }

    pub fn set_volume(&self, set: VolumeSet) -> bool {
        match self.backend() {
            VolumeBackend::PipeWire => {
                let mut ok = true;
                if let Some(level) = set.level {
                    let arg = format!("{}%", level.min(100));
                    ok &= self
                        .runner
                        .run("wpctl", &["set-volume", PIPEWIRE_SINK, &arg], None)
                        .is_ok_and(|o| o.success);
                }
                if let Some(muted) = set.muted {
                    let flag = if muted { "1" } else { "0" };
                    ok &= self
                        .runner
                        .run("wpctl", &["set-mute", PIPEWIRE_SINK, flag], None)
                        .is_ok_and(|o| o.success);
                }
                ok
            }
            VolumeBackend::PulseAudio => {
                let mut ok = true;
                if let Some(level) = set.level {
                    let arg = format!("{}%", level.min(100));
                    ok &= self
                        .runner
                        .run("pactl", &["set-sink-volume", PULSE_SINK, &arg], None)
                        .is_ok_and(|o| o.success);
                }
                if let Some(muted) = set.muted {
                    let flag = if muted { "1" } else { "0" };
                    ok &= self
                        .runner
                        .run("pactl", &["set-sink-mute", PULSE_SINK, flag], None)
                        .is_ok_and(|o| o.success);
                }
                ok
            }
            VolumeBackend::Unavailable => false,
        }
    }

    fn get_pactl_volume(&self) -> Option<Volume> {
        let vol_out = self
            .runner
            .run("pactl", &["get-sink-volume", PULSE_SINK], None)
            .ok()
            .filter(|o| o.success)?;
        let mute_out = self
            .runner
            .run("pactl", &["get-sink-mute", PULSE_SINK], None)
            .ok()
            .filter(|o| o.success)
            .map(|o| o.stdout_lossy());
        parse_pactl_volume(&vol_out.stdout_lossy(), mute_out.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::{CmdOutput, FakeRunner};

    #[test]
    fn parses_wpctl_volume_and_mute_states() {
        assert_eq!(
            parse_wpctl_volume("Volume: 0.45\n"),
            Some(Volume {
                level: Some(45),
                muted: Some(false)
            })
        );
        assert_eq!(
            parse_wpctl_volume("Volume: 0.80 [MUTED]\n"),
            Some(Volume {
                level: Some(80),
                muted: Some(true)
            })
        );
        assert_eq!(
            parse_wpctl_volume("Volume: 1,35\n"),
            Some(Volume {
                level: Some(100),
                muted: Some(false)
            })
        );
        assert_eq!(parse_wpctl_volume("nothing"), None);
    }

    #[test]
    fn parses_pactl_volume_and_mute_states() {
        let raw = "Volume: front-left: 32768 /  50% / -18.06 dB,   front-right: 32768 /  50% / -18.06 dB\n";
        assert_eq!(
            parse_pactl_volume(raw, Some("Mute: no\n")),
            Some(Volume {
                level: Some(50),
                muted: Some(false)
            })
        );
        assert_eq!(
            parse_pactl_volume(raw, Some("Mute: yes\n")),
            Some(Volume {
                level: Some(50),
                muted: Some(true)
            })
        );
    }

    #[test]
    fn sets_and_reads_via_pipewire_and_pulseaudio_fallback() {
        let pw = Arc::new(FakeRunner::with_handler(&["wpctl"], |prog, args, _| {
            assert_eq!(prog, "wpctl");
            if args[0] == "get-volume" {
                Ok(CmdOutput::ok_str("Volume: 0.62 [MUTED]\n"))
            } else {
                Ok(CmdOutput::ok(Vec::new()))
            }
        }));
        let adapter = VolumeAdapter::new(pw.clone());
        assert_eq!(
            adapter.get_volume(),
            Some(Volume {
                level: Some(62),
                muted: Some(true)
            })
        );
        assert!(adapter.set_volume(VolumeSet {
            level: Some(75),
            muted: Some(false)
        }));
        let calls = pw.calls();
        assert_eq!(calls[1].args, ["set-volume", "@DEFAULT_AUDIO_SINK@", "75%"]);
        assert_eq!(calls[2].args, ["set-mute", "@DEFAULT_AUDIO_SINK@", "0"]);

        let pa = Arc::new(FakeRunner::with_handler(&["pactl"], |prog, args, _| {
            assert_eq!(prog, "pactl");
            match args[0] {
                "get-sink-volume" => Ok(CmdOutput::ok_str(
                    "Volume: front-left: 19660 /  30% / -31.37 dB\n",
                )),
                "get-sink-mute" => Ok(CmdOutput::ok_str("Mute: no\n")),
                _ => Ok(CmdOutput::ok(Vec::new())),
            }
        }));
        let pa_adapter = VolumeAdapter::new(pa.clone());
        assert_eq!(
            pa_adapter.get_volume(),
            Some(Volume {
                level: Some(30),
                muted: Some(false)
            })
        );
        assert!(pa_adapter.set_volume(VolumeSet {
            level: Some(40),
            muted: Some(true)
        }));
        let pa_calls = pa.calls();
        assert_eq!(
            pa_calls[2].args,
            ["set-sink-volume", "@DEFAULT_SINK@", "40%"]
        );
        assert_eq!(pa_calls[3].args, ["set-sink-mute", "@DEFAULT_SINK@", "1"]);
    }
}
