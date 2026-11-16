//! The ring of this computer: a two-tone alarm, made in memory and played
//! in a loop by the system until [`stop`].

use std::sync::OnceLock;

use windows_sys::Win32::Media::Audio::{
    PlaySoundW, SND_ASYNC, SND_LOOP, SND_MEMORY, SND_NODEFAULT,
};

const RATE: u32 = 22_050;

/// One second of the alarm: two short beeps a fifth apart, then silence.
/// Square-ish waves, near full scale, so it is heard across a room.
fn alarm() -> Vec<u8> {
    let tone = |frequency: f64, seconds: f64| -> Vec<i16> {
        let samples = (f64::from(RATE) * seconds) as usize;
        (0..samples)
            .map(|n| {
                let phase = (n as f64 * frequency / f64::from(RATE)).fract();
                // A soft-edged square: loud without the click of a pure one.
                let wave = ((phase * std::f64::consts::TAU).sin() * 3.0).clamp(-1.0, 1.0);
                // 5 ms fades at both ends.
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
    wav(&samples)
}

/// A mono, 16-bit PCM WAV file of `samples`.
fn wav(samples: &[i16]) -> Vec<u8> {
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

/// Starts the alarm, or restarts it.
pub fn start() {
    // Played from memory, so it must outlive the sound: made once, kept.
    static ALARM: OnceLock<Vec<u8>> = OnceLock::new();
    let alarm = ALARM.get_or_init(alarm);
    // SAFETY: with SND_MEMORY the first argument points to a whole WAV
    // file, which lives as long as the process.
    unsafe {
        PlaySoundW(
            alarm.as_ptr().cast(),
            std::ptr::null_mut(),
            SND_MEMORY | SND_ASYNC | SND_LOOP | SND_NODEFAULT,
        )
    };
}

/// Stops the alarm, if it plays.
pub fn stop() {
    // SAFETY: a null sound stops the one playing.
    unsafe { PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_alarm_is_a_one_second_wav() {
        let alarm = alarm();
        assert_eq!(&alarm[..4], b"RIFF");
        assert_eq!(&alarm[8..16], b"WAVEfmt ");
        let data_len = u32::from_le_bytes(alarm[40..44].try_into().unwrap()) as usize;
        assert_eq!(alarm.len(), 44 + data_len);
        // A second, give or take the rounding of each part.
        assert!(data_len.abs_diff(RATE as usize * 2) < 16, "{data_len}");
        // Loud: some samples come close to full scale.
        let peak = alarm[44..]
            .chunks(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]).unsigned_abs())
            .max()
            .unwrap();
        assert!(peak > 28_000, "{peak}");
    }
}
