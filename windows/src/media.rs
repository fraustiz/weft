//! What the players of a `media` packet say over time: a `position` is the
//! one at the moment of sending, and moves on while the player plays.

use std::time::Duration;

use crate::packets::Player;

/// A position this far from where a player should be is a seek, worth a
/// new packet; closer, it is the clock drifting.
pub const SEEK_TOLERANCE: Duration = Duration::from_secs(2);

/// Where `player` is, `elapsed` after its position was read.
pub fn position(player: &Player, elapsed: Duration) -> Option<u64> {
    let position = player.position?;
    if !player.playing {
        return Some(position);
    }
    let moved = position.saturating_add(elapsed.as_millis().min(u64::MAX as u128) as u64);
    Some(match player.length {
        Some(length) if length > 0 => moved.min(length),
        _ => moved,
    })
}

/// Whether `new` tells the other devices something `old`, read `elapsed`
/// earlier, did not: anything but a position that only moved with time.
pub fn differs(old: &[Player], elapsed: Duration, new: &[Player]) -> bool {
    if old.len() != new.len() {
        return true;
    }
    old.iter().zip(new).any(|(old, new)| {
        let same_but_position = Player {
            position: new.position,
            ..old.clone()
        } == *new;
        if !same_but_position {
            return true;
        }
        match (position(old, elapsed), new.position) {
            (Some(expected), Some(actual)) => {
                expected.abs_diff(actual) > SEEK_TOLERANCE.as_millis() as u64
            }
            (expected, actual) => expected.is_some() != actual.is_some(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(playing: bool, position: u64) -> Player {
        Player {
            id: "a".into(),
            title: "Song".into(),
            playing,
            position: Some(position),
            length: Some(10_000),
            ..Player::default()
        }
    }

    #[test]
    fn positions_move_while_playing_up_to_the_length() {
        let second = Duration::from_secs(1);
        assert_eq!(position(&song(true, 1000), second), Some(2000));
        assert_eq!(position(&song(false, 1000), second), Some(1000));
        assert_eq!(position(&song(true, 9500), second), Some(10_000));
        let unknown = Player {
            position: None,
            ..song(true, 0)
        };
        assert_eq!(position(&unknown, second), None);
    }

    #[test]
    fn only_real_changes_differ() {
        let old = [song(true, 1000)];
        let three = Duration::from_secs(3);
        // Time passing is no news; a seek, a pause or a new title is.
        assert!(!differs(&old, three, &[song(true, 4000)]));
        assert!(!differs(&old, three, &[song(true, 5500)]));
        assert!(differs(&old, three, &[song(true, 8000)]));
        assert!(differs(&old, three, &[song(false, 4000)]));
        let other = Player {
            title: "Other".into(),
            ..song(true, 4000)
        };
        assert!(differs(&old, three, &[other]));
        assert!(differs(&old, three, &[]));
        assert!(!differs(&[], three, &[]));
    }
}
