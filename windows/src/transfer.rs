//! File transfers (`docs/features.md` section 4), as pure state: what to
//! send next, what to accept, when to retry or give up. The worker does the
//! reading, writing and sending around it.

use std::time::{Duration, Instant};

use crate::packets::{FileOffer, MAX_FILE_SIZE, Packet, RejectReason, transfer_id};

/// The size of every chunk but the last one.
pub const CHUNK: u64 = weft_protocol::MAX_FILE_CHUNK_DATA_LEN as u64;
/// Chunks a sender keeps in flight without acknowledgement.
pub const WINDOW: u64 = 8;
/// Transfers at once, per direction and per device.
pub const MAX_ACTIVE: usize = 4;
/// A sender gives up after this long without progress.
pub const GIVE_UP: Duration = Duration::from_secs(10 * 60);
/// A sender offers again after this long without progress: chunks or
/// answers may have been lost while a device was away.
pub const RETRY: Duration = Duration::from_secs(15);
/// How long a receiver keeps a partial file for a resume.
pub const PARTIAL_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);
/// The longest name a received file gets, in bytes.
pub const MAX_NAME_LEN: usize = 200;

/// Where a transfer stands, in either direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Waiting for a free slot (sender), or for the receiver's answer.
    Waiting,
    Running,
    Done,
    Cancelled,
    Failed(Failure),
}

impl Stage {
    pub fn is_active(self) -> bool {
        matches!(self, Stage::Waiting | Stage::Running)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The receiver said no; `None` for a reason this app does not know.
    Rejected(Option<RejectReason>),
    /// Nothing moved for [`GIVE_UP`].
    NoProgress,
    /// The file could not be read (sender) or written (receiver).
    Disk,
    /// Over 4 GiB, which a transfer cannot carry.
    TooLarge,
}

/// A file this device sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    pub id: [u8; 16],
    pub to: [u8; 16],
    pub name: String,
    pub size: u64,
    /// Not yet offered: [`Outgoing::start`] offers it.
    pub queued: bool,
    pub stage: Stage,
    /// Bytes the receiver confirmed.
    pub acked: u64,
    /// Bytes sent so far; at most [`WINDOW`] chunks past `acked`.
    pub sent: u64,
    /// The last progress, or the last offer, for retries and giving up.
    progress_at: Instant,
    offered_at: Instant,
}

/// What a sender's clock says to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    Nothing,
    /// Send the offer again.
    Offer,
    /// Too long without progress: the transfer failed.
    GiveUp,
}

impl Outgoing {
    pub fn new(id: [u8; 16], to: [u8; 16], name: String, size: u64, now: Instant) -> Outgoing {
        let stage = if size > MAX_FILE_SIZE {
            Stage::Failed(Failure::TooLarge)
        } else {
            Stage::Waiting
        };
        Outgoing {
            id,
            to,
            name,
            size,
            queued: true,
            stage,
            acked: 0,
            sent: 0,
            progress_at: now,
            offered_at: now,
        }
    }

    pub fn offer(&self) -> Packet {
        Packet::FileOffer(FileOffer {
            id: transfer_id(&self.id),
            name: self.name.clone(),
            size: self.size,
            mime: None,
        })
    }

    /// Offers it, first or again: what was in flight is taken as lost
    /// and the receiver says where to start. Returns the offer to send.
    pub fn start(&mut self, now: Instant) -> Packet {
        if self.queued {
            self.queued = false;
            self.progress_at = now;
        }
        self.stage = Stage::Waiting;
        self.sent = self.acked;
        self.offered_at = now;
        self.offer()
    }

    /// The receiver accepted from `offset`. Returns `false`, and changes
    /// nothing, for an answer that does not fit.
    pub fn accept(&mut self, offset: u64, now: Instant) -> bool {
        if !self.stage.is_active() || self.queued || offset > self.size {
            return false;
        }
        self.stage = Stage::Running;
        self.acked = offset;
        self.sent = offset;
        self.progress_at = now;
        true
    }

    /// The receiver has written `offset` bytes. Acknowledgements of chunks
    /// from before a new offer still count.
    pub fn ack(&mut self, offset: u64, now: Instant) {
        if self.stage == Stage::Running && offset > self.acked && offset <= self.size {
            self.acked = offset;
            self.sent = self.sent.max(offset);
            self.progress_at = now;
        }
    }

    /// The next chunk to send, as offset and length, if the window allows.
    pub fn next_chunk(&self) -> Option<(u64, usize)> {
        if self.stage != Stage::Running || self.sent >= self.size {
            return None;
        }
        if self.sent - self.acked >= WINDOW * CHUNK {
            return None;
        }
        let len = (self.size - self.sent).min(CHUNK);
        Some((self.sent, len as usize))
    }

    pub fn chunk_sent(&mut self, len: usize) {
        self.sent += len as u64;
    }

    pub fn done(&mut self) {
        if self.stage.is_active() {
            self.acked = self.size;
            self.sent = self.size;
            self.stage = Stage::Done;
        }
    }

    pub fn end(&mut self, stage: Stage) {
        if self.stage.is_active() {
            self.stage = stage;
        }
    }

    pub fn tick(&mut self, now: Instant) -> Tick {
        if !self.stage.is_active() || self.queued {
            return Tick::Nothing;
        }
        if now.duration_since(self.progress_at) >= GIVE_UP {
            self.stage = Stage::Failed(Failure::NoProgress);
            return Tick::GiveUp;
        }
        let last = self.progress_at.max(self.offered_at);
        if now.duration_since(last) >= RETRY {
            return Tick::Offer;
        }
        Tick::Nothing
    }
}

/// A file this device receives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incoming {
    pub id: [u8; 16],
    pub from: [u8; 16],
    /// Already sanitised.
    pub name: String,
    pub size: u64,
    pub written: u64,
    pub stage: Stage,
    progress_at: Instant,
}

impl Incoming {
    pub fn new(
        id: [u8; 16],
        from: [u8; 16],
        name: String,
        size: u64,
        written: u64,
        now: Instant,
    ) -> Incoming {
        Incoming {
            id,
            from,
            name,
            size,
            written,
            stage: Stage::Running,
            progress_at: now,
        }
    }

    /// Whether a chunk at `offset` of `len` bytes is the next one: any other
    /// is ignored.
    pub fn wants(&self, offset: u64, len: usize) -> bool {
        self.stage == Stage::Running
            && offset == self.written
            && len > 0
            && self.written + len as u64 <= self.size
    }

    /// Records a written chunk. Returns whether the file is complete.
    pub fn wrote(&mut self, len: usize, now: Instant) -> bool {
        self.written += len as u64;
        self.progress_at = now;
        self.is_complete()
    }

    pub fn is_complete(&self) -> bool {
        self.written == self.size
    }

    /// Resumes after a new offer of the same transfer.
    pub fn resume(&mut self, now: Instant) {
        self.stage = Stage::Running;
        self.progress_at = now;
    }

    /// Marks it failed once the sender has been silent for [`GIVE_UP`].
    /// The partial file stays for a resume. Returns whether it just failed.
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.stage == Stage::Running && now.duration_since(self.progress_at) >= GIVE_UP {
            self.stage = Stage::Failed(Failure::NoProgress);
            return true;
        }
        false
    }
}

/// What a receiver answers to an offer: accept from an offset, or reject.
///
/// `active` counts the transfers already running from that device, `have`
/// the bytes of a partial file kept from an earlier attempt, `free` the
/// space left where files are written, if known.
pub fn decide_offer(
    enabled: bool,
    size: u64,
    active: usize,
    have: u64,
    free: Option<u64>,
) -> Result<u64, RejectReason> {
    if !enabled {
        return Err(RejectReason::Disabled);
    }
    if size > MAX_FILE_SIZE {
        return Err(RejectReason::TooLarge);
    }
    if active >= MAX_ACTIVE {
        return Err(RejectReason::Busy);
    }
    // A partial file longer than the offer is not this file: start over.
    let offset = if have <= size { have } else { 0 };
    if free.is_some_and(|free| free < size - offset) {
        return Err(RejectReason::NoSpace);
    }
    Ok(offset)
}

/// Characters Windows does not allow in a file name.
const FORBIDDEN: &[char] = &['<', '>', ':', '"', '|', '?', '*'];

fn is_reserved(stem: &str) -> bool {
    let stem = stem.trim_end_matches([' ', '.']).to_ascii_uppercase();
    match stem.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" => true,
        _ => {
            let bytes = stem.as_bytes();
            bytes.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && (b'1'..=b'9').contains(&bytes[3])
        }
    }
}

/// Splits off the extension, dot included; none for a leading dot only.
fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(0) | None => (name, ""),
        Some(dot) => name.split_at(dot),
    }
}

/// Cuts `text` to at most `max` bytes, on a character boundary.
fn cut(text: &str, max: usize) -> &str {
    let mut end = max.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// A name safe to create in the Downloads folder from what a sender
/// offered: the last path component only, without control or forbidden
/// characters, not `.` or `..`, not a reserved device name, without
/// trailing dots or spaces, at most [`MAX_NAME_LEN`] bytes with the
/// extension kept.
pub fn sanitize_name(offered: &str) -> String {
    const FALLBACK: &str = "file";
    let last = offered.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = last
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| if FORBIDDEN.contains(&c) { '_' } else { c })
        .collect();
    let mut name = cleaned
        .trim_start_matches(' ')
        .trim_end_matches(['.', ' '])
        .to_owned();
    if name.is_empty() {
        name = FALLBACK.into();
    }
    // Windows reserves the device names whatever follows the first dot.
    if is_reserved(name.split('.').next().unwrap_or("")) {
        name = format!("_{name}");
    }
    if name.len() > MAX_NAME_LEN {
        let (mut stem, mut extension) = split_extension(&name);
        // An extension that long is not one: cut the whole name.
        if extension.len() > 16 {
            (stem, extension) = (&name, "");
        }
        let stem = cut(stem, MAX_NAME_LEN - extension.len()).trim_end_matches(['.', ' ']);
        let stem = if stem.is_empty() { FALLBACK } else { stem };
        name = cut(&format!("{stem}{extension}"), MAX_NAME_LEN).to_owned();
    }
    name
}

/// The `attempt`-th name to try for `name` so nothing is overwritten:
/// `name`, then `stem (1).ext`, `stem (2).ext`…
pub fn numbered(name: &str, attempt: u32) -> String {
    if attempt == 0 {
        return name.to_owned();
    }
    let (stem, extension) = split_extension(name);
    format!("{stem} ({attempt}){extension}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: u64 = 20 * CHUNK + 5;

    fn sending(now: Instant) -> Outgoing {
        let mut outgoing = Outgoing::new([1; 16], [2; 16], "a.bin".into(), SIZE, now);
        outgoing.start(now);
        assert!(outgoing.accept(0, now));
        outgoing
    }

    #[test]
    fn a_sender_keeps_at_most_eight_chunks_in_flight() {
        let now = Instant::now();
        let mut outgoing = sending(now);
        let mut sent = 0;
        while let Some((offset, len)) = outgoing.next_chunk() {
            assert_eq!(offset, sent);
            assert_eq!(len as u64, CHUNK);
            outgoing.chunk_sent(len);
            sent += len as u64;
        }
        assert_eq!(sent, WINDOW * CHUNK);
        outgoing.ack(CHUNK, now);
        assert_eq!(
            outgoing.next_chunk(),
            Some((WINDOW * CHUNK, CHUNK as usize))
        );
        // An old or impossible acknowledgement changes nothing.
        outgoing.ack(CHUNK / 2, now);
        outgoing.ack(SIZE + 1, now);
        assert_eq!(outgoing.acked, CHUNK);
    }

    #[test]
    fn the_last_chunk_is_shorter_and_done_ends_it() {
        let now = Instant::now();
        let mut outgoing = sending(now);
        outgoing.start(now);
        assert!(outgoing.accept(20 * CHUNK, now));
        assert_eq!(outgoing.next_chunk(), Some((20 * CHUNK, 5)));
        outgoing.chunk_sent(5);
        assert_eq!(outgoing.next_chunk(), None);
        outgoing.done();
        assert_eq!(outgoing.stage, Stage::Done);
        assert_eq!(outgoing.tick(now + GIVE_UP), Tick::Nothing);
    }

    #[test]
    fn a_new_offer_resumes_from_what_the_receiver_has() {
        let now = Instant::now();
        let mut outgoing = sending(now);
        for _ in 0..3 {
            let (_, len) = outgoing.next_chunk().unwrap();
            outgoing.chunk_sent(len);
        }
        outgoing.ack(CHUNK, now);
        // The connection dropped: offer again, the receiver has 2 chunks.
        assert!(matches!(outgoing.start(now), Packet::FileOffer(_)));
        assert_eq!(outgoing.stage, Stage::Waiting);
        assert_eq!(outgoing.next_chunk(), None);
        assert!(outgoing.accept(2 * CHUNK, now));
        assert_eq!(outgoing.next_chunk(), Some((2 * CHUNK, CHUNK as usize)));
        assert!(!outgoing.accept(SIZE + 1, now));
    }

    #[test]
    fn a_silent_receiver_gets_offers_again_then_the_sender_gives_up() {
        let now = Instant::now();
        let mut outgoing = Outgoing::new([1; 16], [2; 16], "a".into(), 10, now);
        // Queued transfers wait for a slot without a clock.
        assert_eq!(outgoing.tick(now + GIVE_UP), Tick::Nothing);
        outgoing.start(now);
        assert_eq!(outgoing.tick(now + RETRY / 2), Tick::Nothing);
        assert_eq!(outgoing.tick(now + RETRY), Tick::Offer);
        outgoing.start(now + RETRY);
        assert_eq!(outgoing.tick(now + RETRY + RETRY / 2), Tick::Nothing);
        assert_eq!(outgoing.tick(now + GIVE_UP), Tick::GiveUp);
        assert_eq!(outgoing.stage, Stage::Failed(Failure::NoProgress));
    }

    #[test]
    fn files_over_4_gib_are_refused_before_offering() {
        let outgoing = Outgoing::new(
            [1; 16],
            [2; 16],
            "a".into(),
            MAX_FILE_SIZE + 1,
            Instant::now(),
        );
        assert_eq!(outgoing.stage, Stage::Failed(Failure::TooLarge));
        assert!(
            Outgoing::new([1; 16], [2; 16], "a".into(), MAX_FILE_SIZE, Instant::now())
                .stage
                .is_active()
        );
    }

    #[test]
    fn a_receiver_takes_only_the_next_chunk() {
        let now = Instant::now();
        let mut incoming = Incoming::new([1; 16], [2; 16], "a".into(), CHUNK + 3, 0, now);
        assert!(!incoming.wants(CHUNK, 3));
        assert!(!incoming.wants(0, 0));
        assert!(!incoming.wants(0, CHUNK as usize + 4));
        assert!(incoming.wants(0, CHUNK as usize));
        assert!(!incoming.wrote(CHUNK as usize, now));
        assert!(!incoming.wants(0, CHUNK as usize));
        assert!(incoming.wants(CHUNK, 3));
        assert!(incoming.wrote(3, now));
    }

    #[test]
    fn a_silent_sender_fails_the_receiving_side_until_it_offers_again() {
        let now = Instant::now();
        let mut incoming = Incoming::new([1; 16], [2; 16], "a".into(), 10, 4, now);
        assert!(!incoming.tick(now + RETRY));
        assert!(incoming.tick(now + GIVE_UP));
        assert!(!incoming.wants(4, 1));
        incoming.resume(now + GIVE_UP);
        assert!(incoming.wants(4, 1));
    }

    #[test]
    fn offers_are_accepted_or_rejected_with_a_reason() {
        assert_eq!(decide_offer(true, 100, 0, 0, None), Ok(0));
        assert_eq!(decide_offer(true, 100, 3, 40, Some(60)), Ok(40));
        assert_eq!(decide_offer(true, 100, 0, 140, Some(100)), Ok(0));
        assert_eq!(decide_offer(true, 0, 0, 0, Some(0)), Ok(0));
        assert_eq!(
            decide_offer(false, 100, 0, 0, None),
            Err(RejectReason::Disabled)
        );
        assert_eq!(
            decide_offer(true, MAX_FILE_SIZE + 1, 0, 0, None),
            Err(RejectReason::TooLarge)
        );
        assert_eq!(
            decide_offer(true, 100, MAX_ACTIVE, 0, None),
            Err(RejectReason::Busy)
        );
        assert_eq!(
            decide_offer(true, 100, 0, 40, Some(59)),
            Err(RejectReason::NoSpace)
        );
    }

    #[test]
    fn names_are_sanitised() {
        let cases = [
            ("photo.jpg", "photo.jpg"),
            ("../../Windows/system32/evil.dll", "evil.dll"),
            (r"C:\Users\x\notes.txt", "notes.txt"),
            ("..", "file"),
            (".", "file"),
            ("", "file"),
            ("dir/", "file"),
            ("a\u{0}b\u{1f}c\u{7f}.txt", "abc.txt"),
            ("what?<>:\"|*.txt", "what_______.txt"),
            ("trailing. . ", "trailing"),
            ("  leading.txt", "leading.txt"),
            ("CON", "_CON"),
            ("con.txt", "_con.txt"),
            ("Nul.tar.gz", "_Nul.tar.gz"),
            ("COM1.log", "_COM1.log"),
            ("lpt9", "_lpt9"),
            ("COM0.log", "COM0.log"),
            ("COMMA.txt", "COMMA.txt"),
            ("console.txt", "console.txt"),
            (".gitignore", ".gitignore"),
            ("résumé.pdf", "résumé.pdf"),
        ];
        for (offered, expected) in cases {
            assert_eq!(sanitize_name(offered), expected, "{offered:?}");
        }
    }

    #[test]
    fn long_names_are_cut_keeping_the_extension() {
        let long = format!("{}.pdf", "a".repeat(300));
        let name = sanitize_name(&long);
        assert_eq!(name.len(), MAX_NAME_LEN);
        assert!(name.ends_with("a.pdf"));

        // Cut on a character boundary.
        let wide = format!("{}.txt", "é".repeat(150));
        let name = sanitize_name(&wide);
        assert!(
            name.len() <= MAX_NAME_LEN && name.ends_with(".txt"),
            "{name}"
        );

        // No extension worth keeping.
        let name = sanitize_name(&format!("x.{}", "b".repeat(300)));
        assert_eq!(name.len(), MAX_NAME_LEN);
    }

    #[test]
    fn numbered_names_never_overwrite() {
        assert_eq!(numbered("photo.jpg", 0), "photo.jpg");
        assert_eq!(numbered("photo.jpg", 1), "photo (1).jpg");
        assert_eq!(numbered("photo.jpg", 2), "photo (2).jpg");
        assert_eq!(numbered("README", 1), "README (1)");
        assert_eq!(numbered(".gitignore", 1), ".gitignore (1)");
    }
}
