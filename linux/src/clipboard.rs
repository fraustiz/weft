//! Linux clipboard adapter for Wayland (`wl-clipboard`) and X11 (`xclip` / `xsel`).
//!
//! Implements [`weft_client::sync::Clipboard`], honours password manager
//! exclusions (`x-kde-passwordManagerHint` from Bitwarden, KeePassXC, 1Password,
//! etc.), never passes clipboard payloads via command-line arguments, and
//! provides `image/png` helpers ready for Stage 2 image clipboard sync.

use std::sync::{Arc, Mutex};

use weft_client::sync::{Clipboard, Content};
use weft_protocol::{MAX_CLIPBOARD_TEXT_LEN, content_hash};

use crate::runner::{Runner, SystemRunner};

const PNG_SIGNATURE: &[u8; 8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
const PASSWORD_HINT_MIME: &str = "x-kde-passwordManagerHint";

/// Display session environment variables used to select the clipboard tool.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DisplayEnv {
    pub wayland_display: Option<String>,
    pub display: Option<String>,
}

impl DisplayEnv {
    pub fn from_env() -> Self {
        let non_empty = |key: &str| {
            std::env::var(key)
                .ok()
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        Self {
            wayland_display: non_empty("WAYLAND_DISPLAY"),
            display: non_empty("DISPLAY"),
        }
    }

    pub fn wayland() -> Self {
        Self {
            wayland_display: Some("wayland-0".into()),
            display: None,
        }
    }

    pub fn x11() -> Self {
        Self {
            wayland_display: None,
            display: Some(":0".into()),
        }
    }
}

/// Detected clipboard backend on Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Wayland via `wl-paste` and `wl-copy`.
    Wayland,
    /// X11 via `xclip`.
    Xclip,
    /// X11 fallback via `xsel`.
    Xsel,
    /// Headless or missing clipboard utilities.
    Unavailable,
}

/// Selects the clipboard backend from display environment variables and
/// installed helper binaries.
pub fn detect_backend(env: &DisplayEnv, runner: &dyn Runner) -> Backend {
    if env.wayland_display.is_some()
        && runner.has_program("wl-paste")
        && runner.has_program("wl-copy")
    {
        return Backend::Wayland;
    }
    if env.display.is_some() {
        if runner.has_program("xclip") {
            return Backend::Xclip;
        }
        if runner.has_program("xsel") {
            return Backend::Xsel;
        }
    }
    Backend::Unavailable
}

/// Whether a newline-separated list of clipboard targets/MIME types marks the
/// selection as secret (password managers on Linux set `x-kde-passwordManagerHint`).
pub fn has_password_manager_hint(mime_lines: &str) -> bool {
    mime_lines.lines().any(|line| {
        let mime = line.trim();
        mime.eq_ignore_ascii_case(PASSWORD_HINT_MIME)
            || mime.eq_ignore_ascii_case("org.nspasteboard.ConcealedType")
    })
}

/// Whether the offered MIME/target list includes UTF-8 or plain text.
pub fn has_text_mime(mime_lines: &str) -> bool {
    mime_lines.lines().any(|line| {
        let mime = line.trim();
        let lower = mime.to_ascii_lowercase();
        lower == "text/plain"
            || lower.starts_with("text/plain;")
            || matches!(mime, "UTF8_STRING" | "STRING" | "TEXT")
    })
}

/// Whether the offered MIME/target list includes `image/png`.
pub fn has_png_mime(mime_lines: &str) -> bool {
    mime_lines.lines().any(|line| {
        let lower = line.trim().to_ascii_lowercase();
        lower == "image/png" || lower.starts_with("image/png;")
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fingerprint {
    Empty,
    Excluded,
    TooLarge([u8; 16]),
    Text([u8; 16]),
    Binary([u8; 16]),
}

struct State {
    sequence: u32,
    fingerprint: Fingerprint,
    probed: Option<Content>,
}

/// Shareable Linux clipboard handle implementing [`Clipboard`].
#[derive(Clone)]
pub struct LinuxClipboard {
    backend: Backend,
    runner: Arc<dyn Runner>,
    state: Arc<Mutex<State>>,
}

impl LinuxClipboard {
    /// Detects Wayland vs X11 from the current process environment.
    pub fn detect() -> Self {
        Self::with_runner(&DisplayEnv::from_env(), Arc::new(SystemRunner))
    }

    /// Creates a clipboard adapter using explicit display variables and `runner`.
    pub fn with_runner(env: &DisplayEnv, runner: Arc<dyn Runner>) -> Self {
        let backend = detect_backend(env, runner.as_ref());
        let initial_state = State {
            sequence: 1,
            fingerprint: Fingerprint::Empty,
            probed: None,
        };
        let clip = Self {
            backend,
            runner,
            state: Arc::new(Mutex::new(initial_state)),
        };
        // Record whatever is already on the clipboard at startup so Sync::new
        // sees its initial sequence and never broadcasts pre-existing content.
        let (_, fp) = clip.probe_once();
        let mut guard = clip.state.lock().unwrap();
        guard.fingerprint = fp;
        guard.probed = None;
        drop(guard);
        clip
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    /// Reads the current clipboard content immediately without altering `Sync`
    /// sequence tracking (used by `weft-desktop clip`).
    pub fn read_direct(&self) -> Content {
        self.probe_once().0
    }

    /// Reads `image/png` bytes from the clipboard when present and not marked
    /// by a password manager (ready for Stage 2 image clipboard wiring).
    pub fn read_png(&self) -> Option<Vec<u8>> {
        let out = match self.backend {
            Backend::Wayland => {
                let types = self.list_mimes()?;
                if has_password_manager_hint(&types) || !has_png_mime(&types) {
                    return None;
                }
                self.runner
                    .run("wl-paste", &["--type", "image/png"], None)
                    .ok()?
            }
            Backend::Xclip => {
                let types = self.list_mimes()?;
                if has_password_manager_hint(&types) || !has_png_mime(&types) {
                    return None;
                }
                self.runner
                    .run(
                        "xclip",
                        &["-selection", "clipboard", "-o", "-t", "image/png"],
                        None,
                    )
                    .ok()?
            }
            Backend::Xsel | Backend::Unavailable => return None,
        };
        if !out.success || !out.stdout.starts_with(PNG_SIGNATURE) {
            return None;
        }
        Some(out.stdout)
    }

    /// Places `image/png` bytes onto the clipboard via stdin and advances the
    /// local sequence number so it is treated as Weft's own write.
    pub fn write_png(&mut self, png: &[u8]) -> bool {
        if !png.starts_with(PNG_SIGNATURE) {
            return false;
        }
        let ok = match self.backend {
            Backend::Wayland => self
                .runner
                .run_quiet("wl-copy", &["--type", "image/png"], Some(png))
                .unwrap_or(false),
            Backend::Xclip => self
                .runner
                .run_quiet(
                    "xclip",
                    &["-selection", "clipboard", "-i", "-t", "image/png"],
                    Some(png),
                )
                .unwrap_or(false),
            Backend::Xsel | Backend::Unavailable => false,
        };
        if ok {
            let mut guard = self.state.lock().unwrap();
            guard.sequence = guard.sequence.wrapping_add(1);
            guard.fingerprint = Fingerprint::Binary(content_hash(png));
            guard.probed = None;
        }
        ok
    }

    fn list_mimes(&self) -> Option<String> {
        let out = match self.backend {
            Backend::Wayland => self.runner.run("wl-paste", &["--list-types"], None).ok()?,
            Backend::Xclip => self
                .runner
                .run(
                    "xclip",
                    &["-selection", "clipboard", "-o", "-t", "TARGETS"],
                    None,
                )
                .ok()?,
            Backend::Xsel | Backend::Unavailable => return None,
        };
        out.success.then(|| out.stdout_lossy())
    }

    fn read_raw_text(&self) -> Option<Vec<u8>> {
        match self.backend {
            Backend::Wayland => {
                for mime in ["text/plain;charset=utf-8", "text/plain"] {
                    if let Ok(out) =
                        self.runner
                            .run("wl-paste", &["--no-newline", "--type", mime], None)
                        && out.success
                    {
                        return Some(out.stdout);
                    }
                }
                None
            }
            Backend::Xclip => {
                let utf8 = self.runner.run(
                    "xclip",
                    &["-selection", "clipboard", "-o", "-t", "UTF8_STRING"],
                    None,
                );
                if let Ok(out) = utf8
                    && out.success
                {
                    return Some(out.stdout);
                }
                let plain = self
                    .runner
                    .run("xclip", &["-selection", "clipboard", "-o"], None)
                    .ok()?;
                plain.success.then_some(plain.stdout)
            }
            Backend::Xsel => {
                let out = self
                    .runner
                    .run("xsel", &["--clipboard", "--output"], None)
                    .ok()?;
                out.success.then_some(out.stdout)
            }
            Backend::Unavailable => None,
        }
    }

    fn probe_once(&self) -> (Content, Fingerprint) {
        if self.backend == Backend::Unavailable {
            return (Content::NoText, Fingerprint::Empty);
        }
        if matches!(self.backend, Backend::Wayland | Backend::Xclip) {
            let Some(mimes) = self.list_mimes() else {
                return (Content::NoText, Fingerprint::Empty);
            };
            if has_password_manager_hint(&mimes) {
                return (Content::Excluded, Fingerprint::Excluded);
            }
            if has_png_mime(&mimes)
                && let Some(bytes) = self.read_png()
            {
                let hash = content_hash(&bytes);
                if bytes.len() > weft_client::sync::MAX_CLIPBOARD_IMAGE_BYTES {
                    return (Content::TooLarge, Fingerprint::TooLarge(hash));
                }
                if let Some(image) =
                    weft_client::sync::ClipboardImage::new(weft_client::sync::ImageMime::Png, bytes)
                {
                    return (Content::Image(image), Fingerprint::Binary(hash));
                }
            }
            if !has_text_mime(&mimes) {
                return (Content::NoText, Fingerprint::Empty);
            }
        }
        let Some(bytes) = self.read_raw_text() else {
            return (Content::NoText, Fingerprint::Empty);
        };
        if bytes.is_empty() {
            return (Content::Text(String::new()), Fingerprint::Empty);
        }
        if bytes.len() > MAX_CLIPBOARD_TEXT_LEN {
            return (
                Content::TooLarge,
                Fingerprint::TooLarge(content_hash(&bytes)),
            );
        }
        match String::from_utf8(bytes) {
            Ok(text) => {
                let fp = Fingerprint::Text(content_hash(text.as_bytes()));
                (Content::Text(text), fp)
            }
            Err(_) => (Content::NoText, Fingerprint::Empty),
        }
    }

    fn write_bytes(&self, text: &str, private: bool) -> bool {
        let bytes = text.as_bytes();
        match self.backend {
            Backend::Wayland => {
                if private
                    && self
                        .runner
                        .run_quiet(
                            "wl-copy",
                            &["--sensitive", "--type", "text/plain;charset=utf-8"],
                            Some(bytes),
                        )
                        .unwrap_or(false)
                {
                    return true;
                }
                self.runner
                    .run_quiet(
                        "wl-copy",
                        &["--type", "text/plain;charset=utf-8"],
                        Some(bytes),
                    )
                    .unwrap_or(false)
            }
            Backend::Xclip => self
                .runner
                .run_quiet(
                    "xclip",
                    &["-selection", "clipboard", "-i", "-t", "UTF8_STRING"],
                    Some(bytes),
                )
                .unwrap_or(false),
            Backend::Xsel => self
                .runner
                .run_quiet("xsel", &["--clipboard", "--input"], Some(bytes))
                .unwrap_or(false),
            Backend::Unavailable => false,
        }
    }
}

impl Clipboard for LinuxClipboard {
    fn write_image(&mut self, image: &weft_client::sync::ClipboardImage) -> Option<u32> {
        if image.mime != weft_client::sync::ImageMime::Png || !image.is_valid() {
            return None;
        }
        self.write_png(&image.bytes)
            .then(|| self.state.lock().unwrap().sequence)
    }
    fn sequence(&self) -> u32 {
        let (content, fp) = self.probe_once();
        let mut guard = self.state.lock().unwrap();
        if fp != guard.fingerprint {
            guard.sequence = guard.sequence.wrapping_add(1);
            guard.fingerprint = fp;
        }
        guard.probed = Some(content);
        guard.sequence
    }

    fn read(&mut self) -> Content {
        if let Some(probed) = self.state.lock().unwrap().probed.take() {
            return probed;
        }
        self.probe_once().0
    }

    fn write(&mut self, text: &str) -> Option<u32> {
        if !self.write_bytes(text, false) {
            return None;
        }
        let mut guard = self.state.lock().unwrap();
        guard.sequence = guard.sequence.wrapping_add(1);
        guard.fingerprint = if text.is_empty() {
            Fingerprint::Empty
        } else {
            Fingerprint::Text(content_hash(text.as_bytes()))
        };
        guard.probed = None;
        Some(guard.sequence)
    }

    fn write_private(&mut self, text: &str) -> Option<u32> {
        if !self.write_bytes(text, true) {
            return None;
        }
        let mut guard = self.state.lock().unwrap();
        guard.sequence = guard.sequence.wrapping_add(1);
        guard.fingerprint = Fingerprint::Excluded;
        guard.probed = None;
        Some(guard.sequence)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use weft_client::sync::{Incoming, Outgoing, Pause, Sync};

    use super::*;
    use crate::runner::{CmdOutput, FakeRunner};

    #[test]
    fn detects_wayland_vs_x11_backends() {
        let both = FakeRunner::new(&["wl-paste", "wl-copy", "xclip", "xsel"]);
        assert_eq!(
            detect_backend(&DisplayEnv::wayland(), &both),
            Backend::Wayland
        );
        assert_eq!(detect_backend(&DisplayEnv::x11(), &both), Backend::Xclip);

        let xsel_only = FakeRunner::new(&["xsel"]);
        assert_eq!(
            detect_backend(&DisplayEnv::x11(), &xsel_only),
            Backend::Xsel
        );

        let none = FakeRunner::new(&[]);
        assert_eq!(
            detect_backend(&DisplayEnv::wayland(), &none),
            Backend::Unavailable
        );
        assert_eq!(
            detect_backend(&DisplayEnv::default(), &both),
            Backend::Unavailable
        );
    }

    #[test]
    fn excludes_password_manager_hint_on_wayland_without_reading_secret() {
        let runner = Arc::new(FakeRunner::with_handler(
            &["wl-paste", "wl-copy"],
            |prog, args, _| {
                if prog == "wl-paste" && args == ["--list-types"] {
                    return Ok(CmdOutput::ok_str(
                        "x-kde-passwordManagerHint\ntext/plain;charset=utf-8\ntext/plain\n",
                    ));
                }
                panic!("must not read secret clipboard payload: {prog} {args:?}");
            },
        ));
        let mut clip = LinuxClipboard::with_runner(&DisplayEnv::wayland(), runner.clone());
        assert!(clip.read() == Content::Excluded);
        assert!(clip.read_png().is_none());
    }

    #[test]
    fn excludes_password_manager_hint_on_x11_xclip() {
        let runner = Arc::new(FakeRunner::with_handler(&["xclip"], |prog, args, _| {
            if prog == "xclip" && args == ["-selection", "clipboard", "-o", "-t", "TARGETS"] {
                return Ok(CmdOutput::ok_str(
                    "TARGETS\nTIMESTAMP\nx-kde-passwordManagerHint\nUTF8_STRING\n",
                ));
            }
            panic!("must not read secret clipboard payload: {prog} {args:?}");
        }));
        let mut clip = LinuxClipboard::with_runner(&DisplayEnv::x11(), runner);
        assert!(clip.read() == Content::Excluded);
    }

    #[test]
    fn syncs_wayland_text_and_ignores_own_writes() {
        let store: Arc<Mutex<(String, String)>> = Arc::new(Mutex::new((
            "text/plain;charset=utf-8\n".into(),
            "startup-value".into(),
        )));
        let store_bg = store.clone();
        let runner = Arc::new(FakeRunner::with_handler(
            &["wl-paste", "wl-copy"],
            move |prog, args, stdin| {
                let mut guard = store_bg.lock().unwrap();
                match prog {
                    "wl-paste" if args == ["--list-types"] => Ok(CmdOutput::ok_str(&guard.0)),
                    "wl-paste" => Ok(CmdOutput::ok_str(&guard.1)),
                    "wl-copy" => {
                        let written =
                            String::from_utf8(stdin.unwrap_or_default().to_vec()).unwrap();
                        if args.contains(&"--sensitive") {
                            guard.0 =
                                "x-kde-passwordManagerHint\ntext/plain;charset=utf-8\n".into();
                        } else {
                            guard.0 = "text/plain;charset=utf-8\n".into();
                        }
                        guard.1 = written;
                        Ok(CmdOutput::ok(Vec::new()))
                    }
                    _ => Err(std::io::ErrorKind::NotFound.into()),
                }
            },
        ));

        let clip = LinuxClipboard::with_runner(&DisplayEnv::wayland(), runner.clone());
        let mut sync = Sync::new(clip);

        // Pre-existing clipboard content at startup is not broadcast.
        assert_eq!(
            sync.local_change(Pause::default(), true, Instant::now()),
            Outgoing::Unchanged
        );

        // Remote text written via wl-copy stdin is remembered as Weft's own write.
        assert_eq!(
            sync.remote_text("from phone", Pause::default()),
            Incoming::Written
        );
        assert_eq!(store.lock().unwrap().1, "from phone");
        assert_eq!(
            sync.local_change(Pause::default(), true, Instant::now()),
            Outgoing::OwnWrite
        );

        // User copies new text locally -> sent once.
        store.lock().unwrap().1 = "copied on linux".into();
        assert_eq!(
            sync.local_change(Pause::default(), true, Instant::now()),
            Outgoing::Send("copied on linux".into())
        );
        assert_eq!(
            sync.local_change(Pause::default(), true, Instant::now()),
            Outgoing::Unchanged
        );

        // Verify wl-copy received text on stdin, never in argv.
        for call in runner.calls() {
            if call.program == "wl-copy" {
                assert!(!call.args.iter().any(|a| a.contains("from phone")));
            }
        }
    }

    #[test]
    fn reads_and_writes_png_images() {
        let mut valid_png = PNG_SIGNATURE.to_vec();
        valid_png.extend_from_slice(&[1, 2, 3, 4]);
        let png_store = Arc::new(Mutex::new(Vec::<u8>::new()));
        let png_bg = png_store.clone();

        let runner = Arc::new(FakeRunner::with_handler(
            &["wl-paste", "wl-copy"],
            move |prog, args, stdin| match (prog, args) {
                ("wl-paste", ["--list-types"]) => Ok(CmdOutput::ok_str("image/png\n")),
                ("wl-paste", ["--type", "image/png"]) => {
                    Ok(CmdOutput::ok(png_bg.lock().unwrap().clone()))
                }
                ("wl-copy", ["--type", "image/png"]) => {
                    *png_bg.lock().unwrap() = stdin.unwrap_or_default().to_vec();
                    Ok(CmdOutput::ok(Vec::new()))
                }
                _ => Ok(CmdOutput::err(1, "")),
            },
        ));

        let mut clip = LinuxClipboard::with_runner(&DisplayEnv::wayland(), runner);
        assert!(!clip.write_png(b"not-a-png"));
        assert!(clip.write_png(&valid_png));
        assert_eq!(clip.read_png().unwrap(), valid_png);
        assert!(matches!(clip.read_direct(), Content::Image(image) if image.bytes == valid_png));
        let mut sync = Sync::new(clip);
        let image =
            weft_client::sync::ClipboardImage::new(weft_client::sync::ImageMime::Png, valid_png)
                .unwrap();
        assert_eq!(
            sync.remote_image(&image, Pause::default()),
            Incoming::Written
        );
        assert_eq!(
            sync.local_change(Pause::default(), true, Instant::now()),
            Outgoing::OwnWrite
        );
    }
}
