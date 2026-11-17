//! What the players of a `media` packet say over time: a `position` is the
//! one at the moment of sending, and moves on while the player plays; and
//! their covers, the `media.art` packets (`docs/features.md` section 6).

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use crate::packets::Player;

/// The largest cover, in bytes once decoded.
pub const MAX_ART_LEN: usize = 128 * 1024;

/// The longest cover id.
pub const MAX_ART_ID_LEN: usize = 64;

/// The image types a cover may have.
pub const ART_MIMES: &[&str] = &["image/jpeg", "image/png"];

/// The first bytes of every image of each type in [`ART_MIMES`].
const ART_SIGNATURES: &[(&str, &[u8])] = &[
    ("image/jpeg", &[0xff, 0xd8, 0xff]),
    (
        "image/png",
        &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
    ),
];

/// Whether `data` starts as an image of type `mime` must: a cheap check
/// that drops what is not an image without decoding it.
pub fn looks_like(mime: &str, data: &[u8]) -> bool {
    ART_SIGNATURES
        .iter()
        .any(|(known, signature)| *known == mime && data.starts_with(signature))
}

/// The covers kept per device.
pub const COVERS_KEPT: usize = 8;

/// A cover image. Its bytes are shared, never copied, between snapshots.
#[derive(Clone, PartialEq, Eq)]
pub struct Cover {
    /// One of [`ART_MIMES`].
    pub mime: String,
    pub data: Arc<[u8]>,
}

impl Cover {
    /// Whether it may be sent or kept: a known image type whose first
    /// bytes it starts with (see [`looks_like`]), and at most
    /// [`MAX_ART_LEN`] bytes. It is not decoded.
    pub fn is_valid(&self) -> bool {
        looks_like(&self.mime, &self.data) && self.data.len() <= MAX_ART_LEN
    }

    /// The id this computer gives it: the first 128 bits of its BLAKE2b
    /// hash, in hex, so the same image keeps the same id.
    pub fn id(&self) -> String {
        weft_cli::hex(&weft_protocol::content_hash(&self.data))
    }
}

/// Without the bytes, which would fill the logs.
impl std::fmt::Debug for Cover {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cover")
            .field("mime", &self.mime)
            .field("len", &self.data.len())
            .finish()
    }
}

/// Whether `id` may name a cover: 1 to [`MAX_ART_ID_LEN`] ASCII letters,
/// digits, `-` or `_`.
pub fn is_art_id(id: &str) -> bool {
    (1..=MAX_ART_ID_LEN).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// The last [`COVERS_KEPT`] covers a device sent, by id, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Covers(VecDeque<(String, Cover)>);

impl Covers {
    pub fn get(&self, id: &str) -> Option<&Cover> {
        self.0
            .iter()
            .find(|(known, _)| known == id)
            .map(|(_, cover)| cover)
    }

    /// Keeps `cover` as the newest, forgetting the oldest beyond
    /// [`COVERS_KEPT`]. A cover sent again under its id replaces the old
    /// one.
    pub fn insert(&mut self, id: String, cover: Cover) {
        self.0.retain(|(known, _)| *known != id);
        self.0.push_back((id, cover));
        while self.0.len() > COVERS_KEPT {
            self.0.pop_front();
        }
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One of this computer's players, as the platform reads it, with its
/// cover if it has one. The worker names the cover in the player's `art`;
/// whatever `art` the platform set is replaced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalPlayer {
    pub player: Player,
    /// Scaled by the platform, its longer side at most 512 px (300 px is
    /// enough). One over [`MAX_ART_LEN`] or of another type is dropped.
    pub cover: Option<Cover>,
}

impl From<Player> for LocalPlayer {
    fn from(player: Player) -> LocalPlayer {
        LocalPlayer {
            player,
            cover: None,
        }
    }
}

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

    const JPEG: &[u8] = &[0xff, 0xd8, 0xff];
    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

    /// `len` bytes in all, starting with `signature`.
    fn image(signature: &[u8], byte: u8, len: usize) -> Arc<[u8]> {
        let mut data = signature.to_vec();
        data.resize(len, byte);
        data.into()
    }

    fn cover(byte: u8) -> Cover {
        Cover {
            mime: "image/jpeg".into(),
            data: image(JPEG, byte, 13),
        }
    }

    #[test]
    fn art_ids() {
        assert!(is_art_id("a") && is_art_id("AZ09-_") && is_art_id(&"x".repeat(64)));
        for refused in ["", "a b", "a/b", "é", "a.b", "a+b"] {
            assert!(!is_art_id(refused), "{refused}");
        }
        assert!(!is_art_id(&"x".repeat(65)));
        // This computer's ids are valid, and stable for the same image.
        let id = cover(1).id();
        assert!(is_art_id(&id) && id.len() == 32);
        assert_eq!(id, cover(1).id());
        assert_ne!(id, cover(2).id());
    }

    #[test]
    fn covers_have_a_type_and_a_size_limit() {
        assert!(cover(1).is_valid());
        let png = Cover {
            mime: "image/png".into(),
            data: image(PNG, 1, 20),
        };
        assert!(png.is_valid());
        // The type must match the first bytes, and something else is no
        // image.
        let mislabelled = Cover {
            mime: "image/png".into(),
            ..cover(1)
        };
        assert!(!mislabelled.is_valid());
        let text = Cover {
            data: b"not an image".to_vec().into(),
            ..cover(1)
        };
        assert!(!text.is_valid());
        let gif = Cover {
            mime: "image/gif".into(),
            ..cover(1)
        };
        assert!(!gif.is_valid());
        let empty = Cover {
            data: Vec::new().into(),
            ..cover(1)
        };
        assert!(!empty.is_valid());
        let largest = Cover {
            data: image(JPEG, 0, MAX_ART_LEN),
            ..cover(1)
        };
        assert!(largest.is_valid());
        let over = Cover {
            data: image(JPEG, 0, MAX_ART_LEN + 1),
            ..cover(1)
        };
        assert!(!over.is_valid());
    }

    #[test]
    fn the_last_eight_covers_are_kept() {
        let mut covers = Covers::default();
        for index in 0..10u8 {
            covers.insert(format!("c{index}"), cover(index));
        }
        assert_eq!(covers.len(), COVERS_KEPT);
        assert!(covers.get("c0").is_none() && covers.get("c1").is_none());
        assert_eq!(covers.get("c9"), Some(&cover(9)));
        // Sent again, a cover becomes the newest and keeps one entry.
        covers.insert("c2".into(), cover(42));
        covers.insert("c10".into(), cover(10));
        assert_eq!(covers.len(), COVERS_KEPT);
        assert_eq!(covers.get("c2"), Some(&cover(42)));
        assert!(covers.get("c3").is_none());
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
