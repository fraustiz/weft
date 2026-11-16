//! What happens to clipboard text: which local changes go to the group, and
//! which received texts reach the local clipboard.
//!
//! The rules live here, away from Win32, so they can be tested anywhere. The
//! platform layer provides a [`Clipboard`]; sequence numbers (a counter the
//! system increments on every clipboard change) are how a change is told
//! apart from content that was already there, or from Weft's own write.

use std::time::{Duration, Instant};

use weft_protocol::MAX_CLIPBOARD_TEXT_LEN;

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
    /// The application that copied it asked clipboard monitors to leave it
    /// alone (password managers do).
    Excluded,
    /// Text that is over the limit whatever its exact size; not converted.
    TooLarge,
    /// No text, text that is not valid Unicode, or a clipboard that could
    /// not be read.
    NoText,
}

/// The system clipboard, as much of it as the policy needs.
pub trait Clipboard {
    /// The current sequence number.
    fn sequence(&self) -> u32;
    /// The current content. Must not return excluded content as text.
    fn read(&mut self) -> Content;
    /// Replaces the content with `text`. Returns the sequence number right
    /// after the write, or `None` if it failed.
    fn write(&mut self, text: &str) -> Option<u32>;
    /// [`write`](Clipboard::write), with the content marked for clipboard
    /// monitors and the clipboard history to leave alone.
    fn write_private(&mut self, text: &str) -> Option<u32>;
}

/// What to do about a local clipboard change.
#[derive(Clone, PartialEq, Eq)]
pub enum Outgoing {
    /// Send this text to the group.
    Send(String),
    /// Nothing changed since the last look (several notifications for one
    /// change, or the content present at startup).
    Unchanged,
    /// The change is Weft writing a received text.
    OwnWrite,
    Paused,
    /// Not connected, or no other device to send to.
    Offline,
    Excluded,
    NoText,
    Empty,
    /// Over [`MAX_CLIPBOARD_TEXT_LEN`]: not sent at all rather than cut.
    TooLarge,
    /// The text this device just received, written again by something
    /// other than Weft.
    Echo,
    /// The text sent a moment ago: the same copy, reported again.
    SameCopy,
}

/// Never shows the text: outcomes end up in logs.
impl std::fmt::Debug for Outgoing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Outgoing::Send(_) => "Send(..)",
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

/// What happened to a text received from the group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incoming {
    /// It is now the clipboard content.
    Written,
    Paused,
    /// Empty texts are ignored: they would only clear the clipboard.
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
        }
    }

    /// Decides what to do when the system reports a clipboard change, at
    /// `now`. `online` is whether there is a connection and a device to
    /// send to.
    ///
    /// A change that is not sent (paused, offline, too large) is not sent
    /// later either: after a pause or a reconnection, only new changes go
    /// out. The clipboard is only read when its text would be sent.
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
        let text = match self.clipboard.read() {
            Content::Text(text) => text,
            Content::Excluded => return Outgoing::Excluded,
            Content::TooLarge => return Outgoing::TooLarge,
            Content::NoText => return Outgoing::NoText,
        };
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
        self.sent = Some((text.clone(), now));
        Outgoing::Send(text)
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
        let later = sync.sent.as_ref().map(|(_, at)| *at + 2 * SAME_COPY);
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
    }
}
