//! MPRIS2 media integration (`playerctl` with `gdbus` fallback) including
//! metadata, playback status, microsecond-to-millisecond position conversion,
//! `file://` cover art loading, and [`MediaAction`] execution.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use weft_client::media::{Cover, LocalPlayer, MAX_ART_LEN, looks_like};
use weft_client::packets::{MediaAction, Player};

use crate::runner::{Runner, SystemRunner};

const UNIT_SEP: char = '\x1f';
const PLAYERCTL_FORMAT: &str = "{{status}}\x1f{{playerName}}\x1f{{xesam:title}}\x1f{{xesam:artist}}\x1f{{xesam:album}}\x1f{{mpris:length}}\x1f{{position}}\x1f{{volume}}\x1f{{mpris:artUrl}}";
const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";

/// Checks that `id` is a safe MPRIS player identifier (never a CLI flag).
pub fn is_valid_player_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && !id.starts_with('-')
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Decodes a `file://` URL into a local filesystem [`PathBuf`], handling
/// percent-encoded UTF-8 sequences such as `%20`.
pub fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let rest = url.trim().strip_prefix("file://")?;
    let path_part = rest.strip_prefix("localhost").unwrap_or(rest);
    if path_part.is_empty() {
        return None;
    }
    let decoded = percent_decode(path_part)?;
    // On Windows test hosts, `file:///C:/...` yields `/C:/...`; strip leading `/` before drive.
    #[cfg(windows)]
    if decoded.len() >= 3
        && decoded.as_bytes()[0] == b'/'
        && decoded.as_bytes()[1].is_ascii_alphabetic()
        && decoded.as_bytes()[2] == b':'
    {
        return Some(PathBuf::from(&decoded[1..]));
    }
    Some(PathBuf::from(decoded))
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = (bytes[i + 1] as char).to_digit(16)? as u8;
            let lo = (bytes[i + 2] as char).to_digit(16)? as u8;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Loads a local cover image (`image/jpeg` or `image/png`) up to [`MAX_ART_LEN`].
pub fn load_cover_from_url(art_url: &str) -> Option<Cover> {
    let path = file_url_to_path(art_url)?;
    load_cover_from_path(&path)
}

pub fn load_cover_from_path(path: &Path) -> Option<Cover> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_ART_LEN as u64 {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    if bytes.len() > MAX_ART_LEN {
        return None;
    }
    let mime = if looks_like("image/png", &bytes) {
        "image/png"
    } else if looks_like("image/jpeg", &bytes) {
        "image/jpeg"
    } else {
        return None;
    };
    Some(Cover {
        mime: mime.to_owned(),
        data: bytes.into(),
    })
}

/// Parses one formatted `playerctl metadata --format ...` line for `id`.
pub fn parse_playerctl_line(id: &str, line: &str) -> Option<(Player, Option<String>)> {
    let line = line.trim_end_matches(['\r', '\n']);
    let fields: Vec<&str> = line.split(UNIT_SEP).collect();
    if fields.len() < 9 {
        return None;
    }
    let status = fields[0].trim();
    let app_raw = fields[1].trim();
    let title = fields[2].trim();
    let artist = fields[3].trim();
    let album = fields[4].trim();
    let length_us: Option<u64> = fields[5]
        .trim()
        .parse::<f64>()
        .ok()
        .and_then(|v| (v.is_finite() && v > 0.0).then_some(v as u64));
    let pos_us: Option<u64> = fields[6]
        .trim()
        .parse::<f64>()
        .ok()
        .and_then(|v| (v.is_finite() && v >= 0.0).then_some(v as u64));
    let volume: Option<u8> = fields[7].trim().parse::<f64>().ok().and_then(|v| {
        (v.is_finite() && v >= 0.0).then_some((v * 100.0).round().clamp(0.0, 100.0) as u8)
    });
    let art_url = fields[8].trim();
    let art = (!art_url.is_empty()).then(|| art_url.to_owned());

    let length = length_us.map(|us| us / 1000);
    let position = pos_us.map(|us| us / 1000);
    let app = if app_raw.is_empty() {
        id.split('.').next().unwrap_or(id).to_owned()
    } else {
        app_raw.to_owned()
    };
    let playing = status.eq_ignore_ascii_case("playing");

    Some((
        Player {
            id: id.to_owned(),
            app,
            title: title.to_owned(),
            artist: artist.to_owned(),
            album: album.to_owned(),
            playing,
            position,
            length,
            can_play: true,
            can_pause: true,
            can_next: true,
            can_previous: true,
            can_seek: length.is_some(),
            volume,
            art: None,
        },
        art,
    ))
}

/// Extracts `org.mpris.MediaPlayer2.<suffix>` player suffixes from `gdbus`
/// `org.freedesktop.DBus.ListNames` output.
pub fn parse_gdbus_mpris_names(output: &str) -> Vec<String> {
    let mut names = Vec::new();
    for segment in output.split('\'') {
        if let Some(suffix) = segment.strip_prefix(MPRIS_PREFIX)
            && is_valid_player_id(suffix)
            && !names.iter().any(|existing| existing == suffix)
        {
            names.push(suffix.to_owned());
        }
    }
    names
}

fn extract_gvariant_str(dict: &str, key: &str) -> Option<String> {
    let needle = format!("'{key}':");
    let rest = dict.split_once(&needle)?.1.trim_start();
    let rest = rest.strip_prefix('<').map(str::trim_start).unwrap_or(rest);
    if let Some(after_quote) = rest.strip_prefix('\'') {
        let end = after_quote.find('\'')?;
        return Some(after_quote[..end].to_owned());
    }
    if let Some(after_bracket) = rest.strip_prefix('[') {
        let inner = after_bracket.split(']').next()?;
        let items: Vec<&str> = inner
            .split('\'')
            .enumerate()
            .filter_map(|(idx, part)| (idx % 2 == 1).then_some(part))
            .collect();
        if !items.is_empty() {
            return Some(items.join(", "));
        }
    }
    None
}

fn extract_gvariant_number(dict: &str, key: &str) -> Option<f64> {
    let needle = format!("'{key}':");
    let rest = dict.split_once(&needle)?.1.trim_start();
    let rest = rest.strip_prefix('<')?.trim_start();
    let inner = rest.split('>').next()?.trim();
    let token = inner.split_whitespace().last()?;
    token.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn extract_gvariant_bool(dict: &str, key: &str) -> Option<bool> {
    let needle = format!("'{key}':");
    let rest = dict.split_once(&needle)?.1.trim_start();
    let rest = rest.strip_prefix('<')?.trim_start();
    let inner = rest.split('>').next()?.trim();
    match inner {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Parses `gdbus ... Properties.GetAll org.mpris.MediaPlayer2.Player` output.
pub fn parse_gdbus_player_properties(id: &str, props: &str) -> Option<(Player, Option<String>)> {
    let status = extract_gvariant_str(props, "PlaybackStatus")?;
    let title = extract_gvariant_str(props, "xesam:title").unwrap_or_default();
    let artist = extract_gvariant_str(props, "xesam:artist").unwrap_or_default();
    let album = extract_gvariant_str(props, "xesam:album").unwrap_or_default();
    let art = extract_gvariant_str(props, "mpris:artUrl").filter(|s| !s.is_empty());
    let length = extract_gvariant_number(props, "mpris:length")
        .filter(|&v| v > 0.0)
        .map(|v| (v as u64) / 1000);
    let position = extract_gvariant_number(props, "Position")
        .filter(|&v| v >= 0.0)
        .map(|v| (v as u64) / 1000);
    let volume = extract_gvariant_number(props, "Volume")
        .filter(|&v| v >= 0.0)
        .map(|v| (v * 100.0).round().clamp(0.0, 100.0) as u8);

    Some((
        Player {
            id: id.to_owned(),
            app: id.split('.').next().unwrap_or(id).to_owned(),
            title,
            artist,
            album,
            playing: status.eq_ignore_ascii_case("Playing"),
            position,
            length,
            can_play: extract_gvariant_bool(props, "CanPlay").unwrap_or(true),
            can_pause: extract_gvariant_bool(props, "CanPause").unwrap_or(true),
            can_next: extract_gvariant_bool(props, "CanGoNext").unwrap_or(true),
            can_previous: extract_gvariant_bool(props, "CanGoPrevious").unwrap_or(true),
            can_seek: extract_gvariant_bool(props, "CanSeek").unwrap_or(length.is_some()),
            volume,
            art: None,
        },
        art,
    ))
}

/// Reads active MPRIS2 media players and executes [`MediaAction`] commands.
pub struct MediaAdapter {
    runner: Arc<dyn Runner>,
}

impl MediaAdapter {
    pub fn new(runner: Arc<dyn Runner>) -> Self {
        Self { runner }
    }

    pub fn system() -> Self {
        Self::new(Arc::new(SystemRunner))
    }

    pub fn list_players(&self) -> Vec<LocalPlayer> {
        if self.runner.has_program("playerctl") {
            return self.list_via_playerctl();
        }
        if self.runner.has_program("gdbus") {
            return self.list_via_gdbus();
        }
        Vec::new()
    }

    pub fn execute(&self, player: &str, action: MediaAction) -> bool {
        if !is_valid_player_id(player) {
            return false;
        }
        if self.runner.has_program("playerctl") {
            return self.execute_playerctl(player, action);
        }
        if self.runner.has_program("gdbus") {
            return self.execute_gdbus(player, action);
        }
        false
    }

    fn list_via_playerctl(&self) -> Vec<LocalPlayer> {
        let Ok(list_out) = self.runner.run("playerctl", &["--list-all"], None) else {
            return Vec::new();
        };
        if !list_out.success {
            return Vec::new();
        }
        let mut players = Vec::new();
        for raw_id in list_out.stdout_lossy().lines() {
            let id = raw_id.trim();
            if !is_valid_player_id(id) {
                continue;
            }
            let player_arg = format!("--player={id}");
            let Ok(meta_out) = self.runner.run(
                "playerctl",
                &[&player_arg, "metadata", "--format", PLAYERCTL_FORMAT],
                None,
            ) else {
                continue;
            };
            if !meta_out.success {
                continue;
            }
            if let Some((player, art_url)) = parse_playerctl_line(id, &meta_out.stdout_lossy()) {
                let cover = art_url.as_deref().and_then(load_cover_from_url);
                players.push(LocalPlayer { player, cover });
            }
        }
        players
    }

    fn list_via_gdbus(&self) -> Vec<LocalPlayer> {
        let Ok(names_out) = self.runner.run(
            "gdbus",
            &[
                "call",
                "--session",
                "--dest",
                "org.freedesktop.DBus",
                "--object-path",
                "/org/freedesktop/DBus",
                "--method",
                "org.freedesktop.DBus.ListNames",
            ],
            None,
        ) else {
            return Vec::new();
        };
        if !names_out.success {
            return Vec::new();
        }
        let mut players = Vec::new();
        for id in parse_gdbus_mpris_names(&names_out.stdout_lossy()) {
            let dest = format!("{MPRIS_PREFIX}{id}");
            let Ok(props_out) = self.runner.run(
                "gdbus",
                &[
                    "call",
                    "--session",
                    "--dest",
                    &dest,
                    "--object-path",
                    MPRIS_PATH,
                    "--method",
                    "org.freedesktop.DBus.Properties.GetAll",
                    "org.mpris.MediaPlayer2.Player",
                ],
                None,
            ) else {
                continue;
            };
            if !props_out.success {
                continue;
            }
            if let Some((player, art_url)) =
                parse_gdbus_player_properties(&id, &props_out.stdout_lossy())
            {
                let cover = art_url.as_deref().and_then(load_cover_from_url);
                players.push(LocalPlayer { player, cover });
            }
        }
        players
    }

    fn execute_playerctl(&self, player: &str, action: MediaAction) -> bool {
        let player_arg = format!("--player={player}");
        let val_str;
        let args: Vec<&str> = match action {
            MediaAction::Play => vec![&player_arg, "play"],
            MediaAction::Pause => vec![&player_arg, "pause"],
            MediaAction::PlayPause => vec![&player_arg, "play-pause"],
            MediaAction::Next => vec![&player_arg, "next"],
            MediaAction::Previous => vec![&player_arg, "previous"],
            MediaAction::Seek(pos_ms) => {
                val_str = format!("{:.3}", pos_ms as f64 / 1000.0);
                vec![&player_arg, "position", &val_str]
            }
            MediaAction::SetVolume(vol) => {
                val_str = format!("{:.2}", f64::from(vol.min(100)) / 100.0);
                vec![&player_arg, "volume", &val_str]
            }
        };
        self.runner
            .run("playerctl", &args, None)
            .is_ok_and(|o| o.success)
    }

    fn execute_gdbus(&self, player: &str, action: MediaAction) -> bool {
        let dest = format!("{MPRIS_PREFIX}{player}");
        match action {
            MediaAction::Play
            | MediaAction::Pause
            | MediaAction::PlayPause
            | MediaAction::Next
            | MediaAction::Previous => {
                let method = match action {
                    MediaAction::Play => "org.mpris.MediaPlayer2.Player.Play",
                    MediaAction::Pause => "org.mpris.MediaPlayer2.Player.Pause",
                    MediaAction::PlayPause => "org.mpris.MediaPlayer2.Player.PlayPause",
                    MediaAction::Next => "org.mpris.MediaPlayer2.Player.Next",
                    MediaAction::Previous => "org.mpris.MediaPlayer2.Player.Previous",
                    _ => unreachable!(),
                };
                self.runner
                    .run(
                        "gdbus",
                        &[
                            "call",
                            "--session",
                            "--dest",
                            &dest,
                            "--object-path",
                            MPRIS_PATH,
                            "--method",
                            method,
                        ],
                        None,
                    )
                    .is_ok_and(|o| o.success)
            }
            MediaAction::SetVolume(vol) => {
                let variant = format!("<double {:.2}>", f64::from(vol.min(100)) / 100.0);
                self.runner
                    .run(
                        "gdbus",
                        &[
                            "call",
                            "--session",
                            "--dest",
                            &dest,
                            "--object-path",
                            MPRIS_PATH,
                            "--method",
                            "org.freedesktop.DBus.Properties.Set",
                            "org.mpris.MediaPlayer2.Player",
                            "Volume",
                            &variant,
                        ],
                        None,
                    )
                    .is_ok_and(|o| o.success)
            }
            MediaAction::Seek(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::runner::{CmdOutput, FakeRunner};

    #[test]
    fn parses_playerctl_metadata_cover_art_and_runs_actions() {
        let temp = TempDir::new().unwrap();
        let art_file = temp.path().join("cover art.png");
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        png.extend_from_slice(&[0u8; 32]);
        fs::write(&art_file, &png).unwrap();

        let slash_path = art_file.to_string_lossy().replace('\\', "/");
        let encoded_path = slash_path.replace(' ', "%20");
        let art_url = if encoded_path.starts_with('/') {
            format!("file://{encoded_path}")
        } else {
            format!("file:///{encoded_path}")
        };

        let meta_line = format!(
            "Playing\x1fspotify\x1fVeridis Quo\x1fDaft Punk\x1fDiscovery\x1f345000000\x1f120500000\x1f0.85\x1f{art_url}\n"
        );
        let runner = Arc::new(FakeRunner::with_handler(
            &["playerctl"],
            move |prog, args, _| {
                assert_eq!(prog, "playerctl");
                if args == ["--list-all"] {
                    Ok(CmdOutput::ok_str("spotify\n"))
                } else if args.get(1) == Some(&"metadata") {
                    Ok(CmdOutput::ok_str(&meta_line))
                } else {
                    Ok(CmdOutput::ok(Vec::new()))
                }
            },
        ));

        let adapter = MediaAdapter::new(runner.clone());
        let players = adapter.list_players();
        assert_eq!(players.len(), 1);
        let lp = &players[0];
        assert_eq!(lp.player.id, "spotify");
        assert_eq!(lp.player.title, "Veridis Quo");
        assert_eq!(lp.player.artist, "Daft Punk");
        assert_eq!(lp.player.album, "Discovery");
        assert!(lp.player.playing);
        assert_eq!(lp.player.length, Some(345_000));
        assert_eq!(lp.player.position, Some(120_500));
        assert_eq!(lp.player.volume, Some(85));
        assert_eq!(lp.cover.as_ref().unwrap().mime, "image/png");

        assert!(adapter.execute("spotify", MediaAction::PlayPause));
        assert!(adapter.execute("spotify", MediaAction::Seek(42_500)));
        assert!(adapter.execute("spotify", MediaAction::SetVolume(60)));
        assert!(!adapter.execute("--bad-flag", MediaAction::Play));

        let calls = runner.calls();
        assert_eq!(calls[2].args, ["--player=spotify", "play-pause"]);
        assert_eq!(calls[3].args, ["--player=spotify", "position", "42.500"]);
        assert_eq!(calls[4].args, ["--player=spotify", "volume", "0.60"]);
    }

    #[test]
    fn parses_gdbus_mpris_fallback() {
        let names = "([':1.1', 'org.mpris.MediaPlayer2.vlc', 'org.freedesktop.DBus'],)\n";
        assert_eq!(parse_gdbus_mpris_names(names), vec!["vlc"]);

        let props = "({'PlaybackStatus': <'Paused'>, 'Volume': <0.5>, 'Position': <int64 15000000>, 'Metadata': <{'xesam:title': <'Track 1'>, 'xesam:artist': <['Artist A', 'Artist B']>, 'mpris:length': <int64 180000000>}>},)\n";
        let (player, _) = parse_gdbus_player_properties("vlc", props).unwrap();
        assert_eq!(player.id, "vlc");
        assert!(!player.playing);
        assert_eq!(player.title, "Track 1");
        assert_eq!(player.artist, "Artist A, Artist B");
        assert_eq!(player.position, Some(15_000));
        assert_eq!(player.length, Some(180_000));
        assert_eq!(player.volume, Some(50));
    }
}
