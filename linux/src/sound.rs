//! Ring alarm sound generator and Linux audio player loop (`pw-play`, `paplay`,
//! or `aplay`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use weft_client::worker::RING_DURATION;

use crate::runner::Runner;

const RATE: u32 = 22_050;

/// Detected system audio player binary for WAV playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioPlayer {
    PwPlay,
    PaPlay,
    APlay,
}

impl AudioPlayer {
    pub fn program(self) -> &'static str {
        match self {
            AudioPlayer::PwPlay => "pw-play",
            AudioPlayer::PaPlay => "paplay",
            AudioPlayer::APlay => "aplay",
        }
    }

    pub fn args(self, wav_path: &str) -> Vec<String> {
        match self {
            AudioPlayer::PwPlay | AudioPlayer::PaPlay => vec![wav_path.to_owned()],
            AudioPlayer::APlay => vec!["-q".to_owned(), wav_path.to_owned()],
        }
    }
}

/// Selects `pw-play`, `paplay`, or `aplay` in order of preference.
pub fn detect_audio_player(runner: &dyn Runner) -> Option<AudioPlayer> {
    [AudioPlayer::PwPlay, AudioPlayer::PaPlay, AudioPlayer::APlay]
        .into_iter()
        .find(|player| runner.has_program(player.program()))
}

/// Generates a 1-second two-tone alarm WAV in memory (mono, 16-bit PCM,
/// 22,050 Hz: 880 Hz and 1320 Hz soft-edged square waves with 5 ms fades).
pub fn alarm_wav() -> Vec<u8> {
    let tone = |frequency: f64, seconds: f64| -> Vec<i16> {
        let samples = (f64::from(RATE) * seconds) as usize;
        (0..samples)
            .map(|n| {
                let phase = (n as f64 * frequency / f64::from(RATE)).fract();
                let wave = ((phase * std::f64::consts::TAU).sin() * 3.0).clamp(-1.0, 1.0);
                let edge = (n.min(samples - n) as f64 / (f64::from(RATE) * 0.005)).min(1.0);
                (wave * edge * 0.9 * f64::from(i16::MAX)) as i16
            })
            .collect()
    };
    let silence = |seconds: f64| vec![0i16; (f64::from(RATE) * seconds) as usize];
    let samples: Vec<i16> = [
        tone(880.0, 0.15),
        silence(0.05),
        tone(1320.0, 0.15),
        silence(0.65),
    ]
    .concat();
    encode_wav(&samples)
}

fn encode_wav(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

/// Plays the ring alarm in a loop until [`Ringer::stop`] or `duration` elapses.
pub struct Ringer {
    state_dir: PathBuf,
    runner: Arc<dyn Runner>,
    use_os_spawn: bool,
    generation: AtomicU64,
    active_child: Mutex<Option<Child>>,
    wake: Condvar,
}

impl Ringer {
    pub fn new(state_dir: PathBuf, runner: Arc<dyn Runner>) -> Arc<Self> {
        Arc::new(Self {
            state_dir,
            runner,
            use_os_spawn: true,
            generation: AtomicU64::new(0),
            active_child: Mutex::new(None),
            wake: Condvar::new(),
        })
    }

    /// Test constructor that routes playback through `Runner::run_quiet` instead
    /// of spawning OS processes directly.
    pub fn with_fake_runner(state_dir: PathBuf, runner: Arc<dyn Runner>) -> Arc<Self> {
        Arc::new(Self {
            state_dir,
            runner,
            use_os_spawn: false,
            generation: AtomicU64::new(0),
            active_child: Mutex::new(None),
            wake: Condvar::new(),
        })
    }

    pub fn start(self: &Arc<Self>) {
        self.start_for(RING_DURATION);
    }

    pub fn start_for(self: &Arc<Self>, duration: Duration) {
        let Some(player) = detect_audio_player(self.runner.as_ref()) else {
            tracing::debug!("no audio player (pw-play, paplay, aplay) found for ring");
            return;
        };
        let wav_path = match ensure_wav_file(&self.state_dir) {
            Ok(path) => path,
            Err(err) => {
                tracing::warn!(error = %err, "could not write ring alarm wav");
                return;
            }
        };
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.kill_active();

        let ringer = Arc::clone(self);
        std::thread::Builder::new()
            .name("weft-ring".into())
            .spawn(move || ringer.play_loop(generation, player, &wav_path, duration))
            .ok();
    }

    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.kill_active();
        self.wake.notify_all();
    }

    pub fn is_ringing(&self) -> bool {
        self.active_child.lock().unwrap().is_some()
    }

    fn kill_active(&self) {
        if let Some(mut child) = self.active_child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn play_loop(&self, generation: u64, player: AudioPlayer, wav_path: &Path, duration: Duration) {
        let deadline = Instant::now() + duration;
        let wav_str = wav_path.to_string_lossy().into_owned();
        let args = player.args(&wav_str);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

        while Instant::now() < deadline && self.generation.load(Ordering::Acquire) == generation {
            let step_started = Instant::now();
            if self.use_os_spawn {
                let spawned = Command::new(player.program())
                    .args(&arg_refs)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn();
                match spawned {
                    Ok(child) => {
                        *self.active_child.lock().unwrap() = Some(child);
                        while Instant::now() < deadline
                            && self.generation.load(Ordering::Acquire) == generation
                        {
                            let mut slot = self.active_child.lock().unwrap();
                            let Some(child) = slot.as_mut() else {
                                break;
                            };
                            if let Ok(Some(_)) = child.try_wait() {
                                *slot = None;
                                break;
                            }
                            let _ = self.wake.wait_timeout(slot, Duration::from_millis(40));
                        }
                        self.kill_active();
                    }
                    Err(err) => {
                        tracing::debug!(error = %err, "ring audio playback failed");
                        break;
                    }
                }
            } else {
                let ok = self
                    .runner
                    .run_quiet(player.program(), &arg_refs, None)
                    .unwrap_or(false);
                if !ok {
                    break;
                }
            }
            // Prevent a tight spin if the player exits immediately without audio hardware.
            if self.generation.load(Ordering::Acquire) == generation {
                let elapsed = step_started.elapsed();
                if elapsed < Duration::from_millis(200) {
                    let pause = (Duration::from_millis(200) - elapsed)
                        .min(deadline.saturating_duration_since(Instant::now()));
                    let guard = self.active_child.lock().unwrap();
                    let _ = self.wake.wait_timeout(guard, pause);
                }
            }
        }
    }
}

fn ensure_wav_file(state_dir: &Path) -> std::io::Result<PathBuf> {
    fs::create_dir_all(state_dir)?;
    let path = state_dir.join("ring.wav");
    if !path.is_file() {
        fs::write(&path, alarm_wav())?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::runner::FakeRunner;

    #[test]
    fn alarm_is_valid_one_second_loud_wav() {
        let wav = alarm_wav();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        let data_len = u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize;
        assert_eq!(wav.len(), 44 + data_len);
        assert!(data_len.abs_diff(RATE as usize * 2) < 16, "{data_len}");
        let peak = wav[44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]).unsigned_abs())
            .max()
            .unwrap();
        assert!(peak > 28_000, "{peak}");
    }

    #[test]
    fn prefers_pipewire_then_pulseaudio_then_alsa_and_stops_cleanly() {
        let all = FakeRunner::new(&["pw-play", "paplay", "aplay"]);
        assert_eq!(detect_audio_player(&all), Some(AudioPlayer::PwPlay));
        let pulse = FakeRunner::new(&["paplay", "aplay"]);
        assert_eq!(detect_audio_player(&pulse), Some(AudioPlayer::PaPlay));
        let alsa = FakeRunner::new(&["aplay"]);
        assert_eq!(detect_audio_player(&alsa), Some(AudioPlayer::APlay));
        let none = FakeRunner::new(&[]);
        assert_eq!(detect_audio_player(&none), None);

        let dir = TempDir::new().unwrap();
        let runner = Arc::new(FakeRunner::new(&["pw-play"]));
        let ringer = Ringer::with_fake_runner(dir.path().to_path_buf(), runner.clone());
        ringer.start_for(Duration::from_millis(350));
        std::thread::sleep(Duration::from_millis(100));
        ringer.stop();
        std::thread::sleep(Duration::from_millis(80));

        let calls = runner.calls();
        assert!(!calls.is_empty());
        assert_eq!(calls[0].program, "pw-play");
        assert!(dir.path().join("ring.wav").is_file());
    }
}
