//! What happens to clipboard text: which local changes go to the group, and
//! which received texts reach the local clipboard.
//!
//! The rules live here, away from Win32, so they can be tested anywhere. The
//! platform layer provides a [`Clipboard`]; sequence numbers (a counter the
//! system increments on every clipboard change) are how a change is told
//! apart from content that was already there, or from Weft's own write.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use weft_protocol::MAX_CLIPBOARD_TEXT_LEN;

/// Largest clipboard image payload, in bytes (16 MiB).
pub const MAX_CLIPBOARD_IMAGE_BYTES: usize = 16 * 1024 * 1024;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const JPEG_SIGNATURE: &[u8] = &[0xff, 0xd8, 0xff];

/// Supported clipboard image formats on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageMime {
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/jpeg")]
    Jpeg,
}

impl ImageMime {
    pub fn as_str(self) -> &'static str {
        match self {
            ImageMime::Png => "image/png",
            ImageMime::Jpeg => "image/jpeg",
        }
    }

    pub fn parse(mime: &str) -> Option<ImageMime> {
        match mime {
            "image/png" => Some(ImageMime::Png),
            "image/jpeg" => Some(ImageMime::Jpeg),
            _ => None,
        }
    }

    pub fn signature(self) -> &'static [u8] {
        match self {
            ImageMime::Png => PNG_SIGNATURE,
            ImageMime::Jpeg => JPEG_SIGNATURE,
        }
    }

    /// Whether `bytes` begins with the magic bytes of this format.
    pub fn matches(self, bytes: &[u8]) -> bool {
        bytes.starts_with(self.signature())
    }
}

/// A PNG or JPEG clipboard image (1 byte to [`MAX_CLIPBOARD_IMAGE_BYTES`]).
#[derive(Clone, PartialEq, Eq)]
pub struct ClipboardImage {
    pub mime: ImageMime,
    pub bytes: Vec<u8>,
}

impl ClipboardImage {
    /// Builds a validated [`ClipboardImage`], checking size (`1..=16 MiB`)
    /// and magic bytes (`PNG` or `JPEG`).
    pub fn new(mime: ImageMime, bytes: Vec<u8>) -> Option<ClipboardImage> {
        let image = ClipboardImage { mime, bytes };
        image.is_valid().then_some(image)
    }

    /// Whether `bytes` is within `1..=MAX_CLIPBOARD_IMAGE_BYTES` and starts
    /// with `mime`'s magic signature.
    pub fn is_valid(&self) -> bool {
        (1..=MAX_CLIPBOARD_IMAGE_BYTES).contains(&self.bytes.len())
            && self.mime.matches(&self.bytes)
    }

    /// 16-byte BLAKE2b content hash (`weft_protocol::content_hash`).
    pub fn hash(&self) -> [u8; 16] {
        weft_protocol::content_hash(&self.bytes)
    }
}

/// Never shows the image bytes: outcomes and hooks must never log clipboard content.
impl std::fmt::Debug for ClipboardImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipboardImage")
            .field("mime", &self.mime.as_str())
            .field("len", &self.bytes.len())
            .finish()
    }
}

/// How long after sending a text the same text is taken for the same copy.
/// Many applications write the clipboard in several steps, and each step is
/// reported as a change.
const SAME_COPY: Duration = Duration::from_secs(1);

/// The pause switches. `all` is the one the tray offers; the two others
/// pause one direction only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pause {
    pub all: bool,
    pub send: bool,
    pub receive: bool,
}

impl Pause {
    /// Whether local changes stay on this device.
    pub fn blocks_send(self) -> bool {
        self.all || self.send
    }

    /// Whether received text stays out of the local clipboard.
    pub fn blocks_receive(self) -> bool {
        self.all || self.receive
    }
}

/// What the platform layer found on the clipboard.
#[derive(Clone, PartialEq, Eq)]
pub enum Content {
    Text(String),
    Image(ClipboardImage),
    /// The application that copied it asked clipboard monitors to leave it
    /// alone (password managers do).
    Excluded,
    /// Content that is over the limit whatever its exact size; not converted.
    TooLarge,
    /// No text or supported image, text that is not valid Unicode, or a
    /// clipboard that could not be read.
    NoText,
}

/// The system clipboard, as much of it as the policy needs.
pub trait Clipboard {
    /// The current sequence number.
    fn sequence(&self) -> u32;
    /// The current content. Must not return excluded content as text or image.
    fn read(&mut self) -> Content;
    /// Replaces the content with `text`. Returns the sequence number right
    /// after the write, or `None` if it failed.
    fn write(&mut self, text: &str) -> Option<u32>;
    /// [`write`](Clipboard::write), with the content marked for clipboard
    /// monitors and the clipboard history to leave alone.
    fn write_private(&mut self, text: &str) -> Option<u32>;
    /// Replaces the content with `image`. Returns the sequence number right
    /// after the write, or `None` if unsupported or failed.
    fn write_image(&mut self, _image: &ClipboardImage) -> Option<u32> {
        None
    }
}

/// What to do about a local clipboard change.
#[derive(Clone, PartialEq, Eq)]
pub enum Outgoing {
    /// Send this text to the group.
    Send(String),
    /// Send this image to the group.
    SendImage(ClipboardImage),
    /// Nothing changed since the last look (several notifications for one
    /// change, or the content present at startup).
    Unchanged,
    /// The change is Weft writing a received text or image.
    OwnWrite,
    Paused,
    /// Not connected, or no other device to send to.
    Offline,
    Excluded,
    NoText,
    Empty,
    /// Over [`MAX_CLIPBOARD_TEXT_LEN`] or [`MAX_CLIPBOARD_IMAGE_BYTES`]:
    /// not sent at all rather than cut.
    TooLarge,
    /// The text or image this device just received, written again by
    /// something other than Weft.
    Echo,
    /// The text or image sent a moment ago: the same copy, reported again.
    SameCopy,
}

/// Never shows the text or image: outcomes end up in logs.
impl std::fmt::Debug for Outgoing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Outgoing::Send(_) => "Send(..)",
            Outgoing::SendImage(_) => "SendImage(..)",
            Outgoing::Unchanged => "Unchanged",
            Outgoing::OwnWrite => "OwnWrite",
            Outgoing::Paused => "Paused",
            Outgoing::Offline => "Offline",
            Outgoing::Excluded => "Excluded",
            Outgoing::NoText => "NoText",
            Outgoing::Empty => "Empty",
            Outgoing::TooLarge => "TooLarge",
            Outgoing::Echo => "Echo",
            Outgoing::SameCopy => "SameCopy",
        })
    }
}

/// What happened to a text or image received from the group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incoming {
    /// It is now the clipboard content.
    Written,
    Paused,
    /// Empty payloads are ignored: they would only clear the clipboard.
    Empty,
    /// The clipboard refused the write.
    Failed,
}

/// The policy, with the little it remembers between two changes.
pub struct Sync<C> {
    clipboard: C,
    /// Sequence number of the last change looked at.
    seen: u32,
    /// Sequence number right after Weft's last write.
    own: Option<u32>,
    /// The last text received, until this device sends another one. Some
    /// clipboard tools write the content again after every change: without
    /// this, a received text would go straight back, forever between two
    /// such machines.
    received: Option<String>,
    /// The last text sent, and when.
    sent: Option<(String, Instant)>,
    /// Content hash of the last image received, until this device sends
    /// new content, preventing clipboard tools from echoing it back.
    received_image_hash: Option<[u8; 16]>,
    /// Content hash of the last image sent, and when.
    sent_image: Option<([u8; 16], Instant)>,
}

impl<C: Clipboard> Sync<C> {
    /// Starts from the clipboard as it is: what it holds now is never sent,
    /// only later changes are.
    pub fn new(clipboard: C) -> Sync<C> {
        let seen = clipboard.sequence();
        Sync {
            clipboard,
            seen,
            own: None,
            received: None,
            sent: None,
            received_image_hash: None,
            sent_image: None,
        }
    }

    /// Decides what to do when the system reports a clipboard change, at
    /// `now`. `online` is whether there is a connection and a device to
    /// send to.
    ///
    /// A change that is not sent (paused, offline, too large) is not sent
    /// later either: after a pause or a reconnection, only new changes go
    /// out. The clipboard is only read when its content would be sent.
    pub fn local_change(&mut self, pause: Pause, online: bool, now: Instant) -> Outgoing {
        let sequence = self.clipboard.sequence();
        if self.own == Some(sequence) {
            self.seen = sequence;
            return Outgoing::OwnWrite;
        }
        if sequence == self.seen {
            return Outgoing::Unchanged;
        }
        self.seen = sequence;
        if pause.blocks_send() {
            return Outgoing::Paused;
        }
        if !online {
            return Outgoing::Offline;
        }
        match self.clipboard.read() {
            Content::Text(text) => {
                if text.is_empty() {
                    return Outgoing::Empty;
                }
                if text.len() > MAX_CLIPBOARD_TEXT_LEN {
                    return Outgoing::TooLarge;
                }
                if self.received.as_deref() == Some(&text) {
                    return Outgoing::Echo;
                }
                if let Some((sent, at)) = &self.sent
                    && *sent == text
                    && now.saturating_duration_since(*at) < SAME_COPY
                {
                    return Outgoing::SameCopy;
                }
                self.received = None;
                self.received_image_hash = None;
                self.sent = Some((text.clone(), now));
                Outgoing::Send(text)
            }
            Content::Image(image) => {
                if image.bytes.is_empty() {
                    return Outgoing::Empty;
                }
                if image.bytes.len() > MAX_CLIPBOARD_IMAGE_BYTES {
                    return Outgoing::TooLarge;
                }
                if !image.mime.matches(&image.bytes) {
                    return Outgoing::NoText;
                }
                let hash = image.hash();
                if self.received_image_hash == Some(hash) {
                    return Outgoing::Echo;
                }
                if let Some((sent_hash, at)) = &self.sent_image
                    && *sent_hash == hash
                    && now.saturating_duration_since(*at) < SAME_COPY
                {
                    return Outgoing::SameCopy;
                }
                self.received = None;
                self.received_image_hash = None;
                self.sent_image = Some((hash, now));
                Outgoing::SendImage(image)
            }
            Content::Excluded => Outgoing::Excluded,
            Content::TooLarge => Outgoing::TooLarge,
            Content::NoText => Outgoing::NoText,
        }
    }

    /// Puts a text received from the group on the clipboard. The write is
    /// remembered, so the change it causes is not sent back.
    pub fn remote_text(&mut self, text: &str, pause: Pause) -> Incoming {
        if pause.blocks_receive() {
            return Incoming::Paused;
        }
        if text.is_empty() {
            return Incoming::Empty;
        }
        match self.clipboard.write(text) {
            Some(sequence) => {
                self.own = Some(sequence);
                self.received = Some(text.to_owned());
                self.received_image_hash = None;
                Incoming::Written
            }
            None => Incoming::Failed,
        }
    }

    /// Puts an image received from the group on the clipboard. Both the
    /// write's sequence number (`own`) and `content_hash(&image.bytes)` are
    /// recorded so received images never echo back.
    pub fn remote_image(&mut self, image: &ClipboardImage, pause: Pause) -> Incoming {
        if pause.blocks_receive() {
            return Incoming::Paused;
        }
        if image.bytes.is_empty() {
            return Incoming::Empty;
        }
        if !image.is_valid() {
            return Incoming::Failed;
        }
        match self.clipboard.write_image(image) {
            Some(sequence) => {
                self.own = Some(sequence);
                self.received_image_hash = Some(image.hash());
                self.received = None;
                Incoming::Written
            }
            None => Incoming::Failed,
        }
    }

    /// Puts a text from Weft's own window on the clipboard, such as a
    /// pairing code to paste elsewhere on this computer. It stays on this
    /// device whatever the pause says: the change it causes is Weft's own
    /// and is never sent, and the content is marked private. Returns
    /// whether the clipboard took it.
    pub fn local_text(&mut self, text: &str) -> bool {
        match self.clipboard.write_private(text) {
            Some(sequence) => {
                self.own = Some(sequence);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clipboard in memory. `copy` plays the user or another application.
    struct Fake {
        sequence: u32,
        content: Content,
        reads: u32,
        fail_writes: bool,
    }

    impl Fake {
        fn holding(text: &str) -> Fake {
            Fake {
                sequence: 7,
                content: Content::Text(text.into()),
                reads: 0,
                fail_writes: false,
            }
        }
    }

    impl Clipboard for Fake {
        fn sequence(&self) -> u32 {
            self.sequence
        }

        fn read(&mut self) -> Content {
            self.reads += 1;
            self.content.clone()
        }

        fn write(&mut self, text: &str) -> Option<u32> {
            if self.fail_writes {
                return None;
            }
            // Emptying then setting: two increments, as on Windows.
            self.sequence += 2;
            self.content = Content::Text(text.into());
            Some(self.sequence)
        }

        fn write_private(&mut self, text: &str) -> Option<u32> {
            let sequence = self.write(text)?;
            // What a monitor sees of content marked private.
            self.content = Content::Excluded;
            Some(sequence)
        }

        fn write_image(&mut self, image: &ClipboardImage) -> Option<u32> {
            if self.fail_writes {
                return None;
            }
            self.sequence += 2;
            self.content = Content::Image(image.clone());
            Some(self.sequence)
        }
    }

    const RUNNING: Pause = Pause {
        all: false,
        send: false,
        receive: false,
    };

    fn started(text: &str) -> Sync<Fake> {
        Sync::new(Fake::holding(text))
    }

    fn copy(sync: &mut Sync<Fake>, content: Content) {
        sync.clipboard.sequence += 1;
        sync.clipboard.content = content;
    }

    fn copy_text(sync: &mut Sync<Fake>, text: &str) {
        copy(sync, Content::Text(text.into()));
    }

    /// A local change well after the previous one.
    fn change(sync: &mut Sync<Fake>, pause: Pause, online: bool) -> Outgoing {
        let last_text = sync.sent.as_ref().map(|(_, at)| *at);
        let last_img = sync.sent_image.as_ref().map(|(_, at)| *at);
        let later = last_text.max(last_img).map(|at| at + 2 * SAME_COPY);
        sync.local_change(pause, online, later.unwrap_or_else(Instant::now))
    }

    fn send(text: &str) -> Outgoing {
        Outgoing::Send(text.into())
    }

    #[test]
    fn a_local_change_is_sent_once() {
        let mut sync = started("old");
        copy_text(&mut sync, "new");
        assert_eq!(change(&mut sync, RUNNING, true), send("new"));
        // A second notification for the same change.
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::Unchanged);
        assert_eq!(sync.clipboard.reads, 1);
    }

    #[test]
    fn content_present_at_startup_is_not_sent() {
        let mut sync = started("copied before Weft started");
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::Unchanged);
        assert_eq!(sync.clipboard.reads, 0);
    }

    #[test]
    fn nothing_is_sent_or_read_while_paused() {
        for pause in [
            Pause {
                all: true,
                ..RUNNING
            },
            Pause {
                send: true,
                ..RUNNING
            },
        ] {
            let mut sync = started("old");
            copy_text(&mut sync, "while paused");
            assert_eq!(change(&mut sync, pause, true), Outgoing::Paused);
            assert_eq!(sync.clipboard.reads, 0);
            // Resuming does not send what was copied during the pause.
            assert_eq!(change(&mut sync, RUNNING, true), Outgoing::Unchanged);
            copy_text(&mut sync, "after the pause");
            assert_eq!(change(&mut sync, RUNNING, true), send("after the pause"));
        }
        // Pausing reception alone does not stop sending.
        let mut sync = started("old");
        copy_text(&mut sync, "new");
        let receive_paused = Pause {
            receive: true,
            ..RUNNING
        };
        assert_eq!(change(&mut sync, receive_paused, true), send("new"));
    }

    #[test]
    fn a_change_made_offline_is_not_sent_after_reconnecting() {
        let mut sync = started("old");
        copy_text(&mut sync, "while offline");
        assert_eq!(change(&mut sync, RUNNING, false), Outgoing::Offline);
        assert_eq!(sync.clipboard.reads, 0);
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::Unchanged);
        copy_text(&mut sync, "back online");
        assert_eq!(change(&mut sync, RUNNING, true), send("back online"));
    }

    #[test]
    fn excluded_content_is_not_sent() {
        let mut sync = started("old");
        copy(&mut sync, Content::Excluded);
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::Excluded);
    }

    #[test]
    fn empty_or_missing_text_is_not_sent() {
        let mut sync = started("old");
        copy_text(&mut sync, "");
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::Empty);
        copy(&mut sync, Content::NoText);
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::NoText);
    }

    #[test]
    fn text_over_the_limit_is_not_sent_nor_cut() {
        let mut sync = started("old");
        let limit = "é".repeat(MAX_CLIPBOARD_TEXT_LEN / 2);
        assert_eq!(limit.len(), MAX_CLIPBOARD_TEXT_LEN);
        copy_text(&mut sync, &limit);
        assert_eq!(change(&mut sync, RUNNING, true), send(&limit));

        // One byte more, counted in UTF-8.
        copy_text(&mut sync, &format!("{limit}a"));
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::TooLarge);
        copy(&mut sync, Content::TooLarge);
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::TooLarge);
        // Nothing of it goes out later.
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::Unchanged);
    }

    #[test]
    fn received_text_is_written_and_never_sent_back() {
        let mut sync = started("old");
        assert_eq!(sync.remote_text("from afar", RUNNING), Incoming::Written);
        assert!(sync.clipboard.content == Content::Text("from afar".into()));
        // The notification caused by the write, possibly more than once.
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::OwnWrite);
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::OwnWrite);
        assert_eq!(sync.clipboard.reads, 0);

        // A real change afterwards is sent.
        copy_text(&mut sync, "typed here");
        assert_eq!(change(&mut sync, RUNNING, true), send("typed here"));
    }

    #[test]
    fn the_same_text_does_not_ping_pong() {
        let mut sync = started("old");
        assert_eq!(sync.remote_text("echo", RUNNING), Incoming::Written);
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::OwnWrite);
        // A clipboard tool writes the same text again: a new change, not
        // made by Weft, with the text that was just received.
        copy_text(&mut sync, "echo");
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::Echo);

        // The same text arriving again is written again, still not resent.
        assert_eq!(sync.remote_text("echo", RUNNING), Incoming::Written);
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::OwnWrite);
    }

    #[test]
    fn one_copy_written_in_several_steps_is_sent_once() {
        let mut sync = started("old");
        let start = Instant::now();
        copy_text(&mut sync, "copied");
        assert_eq!(sync.local_change(RUNNING, true, start), send("copied"));
        // The application writes it again right away: a new change.
        copy_text(&mut sync, "copied");
        let soon = start + SAME_COPY / 2;
        assert_eq!(sync.local_change(RUNNING, true, soon), Outgoing::SameCopy);
        // Another text in the same moment is a real copy.
        copy_text(&mut sync, "something else");
        assert_eq!(
            sync.local_change(RUNNING, true, soon),
            send("something else")
        );
    }

    #[test]
    fn a_text_copied_again_on_purpose_is_sent_again() {
        let mut sync = started("old");
        // The other devices may hold something else by now: every copy made
        // here goes out, even of the same text.
        copy_text(&mut sync, "mine");
        assert_eq!(change(&mut sync, RUNNING, true), send("mine"));
        copy_text(&mut sync, "mine");
        assert_eq!(change(&mut sync, RUNNING, true), send("mine"));

        // And a received text can be sent back once this device has sent
        // something else: it is no longer what the others hold.
        assert_eq!(sync.remote_text("theirs", RUNNING), Incoming::Written);
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::OwnWrite);
        copy_text(&mut sync, "mine");
        assert_eq!(change(&mut sync, RUNNING, true), send("mine"));
        copy_text(&mut sync, "theirs");
        assert_eq!(change(&mut sync, RUNNING, true), send("theirs"));
    }

    #[test]
    fn received_text_is_not_written_while_paused() {
        for pause in [
            Pause {
                all: true,
                ..RUNNING
            },
            Pause {
                receive: true,
                ..RUNNING
            },
        ] {
            let mut sync = started("old");
            assert_eq!(sync.remote_text("from afar", pause), Incoming::Paused);
            assert!(sync.clipboard.content == Content::Text("old".into()));
        }
        // Pausing sending alone does not stop reception.
        let mut sync = started("old");
        let send_paused = Pause {
            send: true,
            ..RUNNING
        };
        assert_eq!(
            sync.remote_text("from afar", send_paused),
            Incoming::Written
        );
    }

    #[test]
    fn empty_received_text_and_failed_writes_change_nothing() {
        let mut sync = started("old");
        assert_eq!(sync.remote_text("", RUNNING), Incoming::Empty);
        sync.clipboard.fail_writes = true;
        assert_eq!(sync.remote_text("from afar", RUNNING), Incoming::Failed);
        assert!(sync.clipboard.content == Content::Text("old".into()));
        // The failed text was not recorded: copying it locally sends it.
        copy_text(&mut sync, "from afar");
        assert_eq!(change(&mut sync, RUNNING, true), send("from afar"));
    }

    #[test]
    fn a_local_text_stays_on_this_device() {
        let mut sync = started("old");
        assert!(sync.local_text("WEFT1:code"));
        assert!(sync.clipboard.content == Content::Excluded);
        // The change it causes is Weft's own: not read, not sent.
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::OwnWrite);
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::OwnWrite);
        assert_eq!(sync.clipboard.reads, 0);
        // The next real copy goes out as usual.
        copy_text(&mut sync, "typed here");
        assert_eq!(change(&mut sync, RUNNING, true), send("typed here"));

        let mut sync = started("old");
        sync.clipboard.fail_writes = true;
        assert!(!sync.local_text("WEFT1:code"));
        assert!(sync.clipboard.content == Content::Text("old".into()));
    }

    #[test]
    fn outcomes_never_show_the_text() {
        assert_eq!(format!("{:?}", send("secret")), "Send(..)");
        let png = png_image(1, 24);
        assert_eq!(
            format!("{:?}", Outgoing::SendImage(png.clone())),
            "SendImage(..)"
        );
        let dbg = format!("{png:?}");
        assert!(dbg.contains("image/png") && dbg.contains("24"));
    }

    fn png_image(fill: u8, len: usize) -> ClipboardImage {
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.resize(len, fill);
        ClipboardImage::new(ImageMime::Png, bytes).unwrap()
    }

    #[test]
    fn clipboard_images_validate_magic_bytes_and_size() {
        let png = png_image(0xaa, 32);
        assert!(png.is_valid());

        let mut jpg = JPEG_SIGNATURE.to_vec();
        jpg.extend_from_slice(&[1, 2, 3, 4]);
        let jpg = ClipboardImage::new(ImageMime::Jpeg, jpg).unwrap();
        assert!(jpg.is_valid());

        // Mismatched magic bytes, empty, or over 16 MiB are rejected.
        assert!(ClipboardImage::new(ImageMime::Png, jpg.bytes.clone()).is_none());
        assert!(ClipboardImage::new(ImageMime::Png, Vec::new()).is_none());
        let mut huge = PNG_SIGNATURE.to_vec();
        huge.resize(MAX_CLIPBOARD_IMAGE_BYTES + 1, 0);
        assert!(ClipboardImage::new(ImageMime::Png, huge.clone()).is_none());

        let mut sync = started("old");
        copy(
            &mut sync,
            Content::Image(ClipboardImage {
                mime: ImageMime::Png,
                bytes: huge,
            }),
        );
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::TooLarge);
    }

    #[test]
    fn received_images_never_echo_back() {
        let mut sync = started("old");
        let img = png_image(7, 64);
        assert_eq!(sync.remote_image(&img, RUNNING), Incoming::Written);
        assert!(sync.clipboard.content == Content::Image(img.clone()));

        // Weft's own write is suppressed by sequence number (`own`).
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::OwnWrite);
        assert_eq!(sync.clipboard.reads, 0);

        // Another clipboard manager re-writing the same image is suppressed by `content_hash`.
        copy(&mut sync, Content::Image(img.clone()));
        assert_eq!(change(&mut sync, RUNNING, true), Outgoing::Echo);

        // A multi-step local copy within SAME_COPY is sent once.
        let other = png_image(9, 64);
        let t0 = Instant::now();
        copy(&mut sync, Content::Image(other.clone()));
        assert_eq!(
            sync.local_change(RUNNING, true, t0),
            Outgoing::SendImage(other.clone())
        );
        copy(&mut sync, Content::Image(other.clone()));
        assert_eq!(
            sync.local_change(RUNNING, true, t0 + SAME_COPY / 2),
            Outgoing::SameCopy
        );

        // Once another image was sent, copying the earlier received image sends it.
        copy(&mut sync, Content::Image(img.clone()));
        assert_eq!(
            sync.local_change(RUNNING, true, t0 + 2 * SAME_COPY),
            Outgoing::SendImage(img)
        );
    }
}
