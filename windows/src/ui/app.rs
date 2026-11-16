//! The window's component: its state, the messages that change it, and the
//! frame (title bar and navigation) around the pages.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use windows_reactor::*;

use super::words::{self, NameProblem};
use super::{Shell, detail, devices, pairing, picker, qr, settings};
use crate::features::Features;
use crate::i18n::t;
use crate::packets::{MediaAction, MediaCommand, Packet, VolumeSet};
use crate::worker::{Command, Handle, PairingState, Published, RING_DURATION, Snapshot};
use crate::{autostart, window};

/// The default client size in effective pixels: a small utility window
/// that shows the whole pairing code.
const SIZE: (f64, f64) = (520.0, 640.0);
/// Below this the pairing code would not fit across.
const MIN_SIZE: (f64, f64) = (400.0, 480.0);
/// The longest wait between two redraws for texts that change with time.
const MAX_TICK: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Devices,
    Settings,
}

impl Page {
    const fn tag(self) -> &'static str {
        match self {
            Page::Devices => "devices",
            Page::Settings => "settings",
        }
    }
}

#[derive(Clone)]
pub enum Message {
    /// The worker published a snapshot; `None` once it has stopped.
    Published(Option<Published>),
    /// A text that depends on the time may have changed.
    Tick,
    Navigate(Option<String>),
    /// Opens the pairing view with a new code, or asks for another one.
    StartPairing,
    /// Leaves the pairing view.
    ClosePairing,
    CopyCode,
    JoinCodeEdited(String),
    Join,
    DismissAdded,
    AskRemove([u8; 16]),
    /// Remove was chosen in the confirmation.
    ConfirmRemove,
    CancelRemove,
    /// The confirmation finished hiding.
    RemovalHidden,
    NameEdited(String),
    SaveName,
    SetPaused(bool),
    SetAutostart(bool),
    SetFeatures(Features),
    /// Shows a device's details in place of the list.
    OpenDevice([u8; 16]),
    CloseDevice,
    /// Rings the device shown, or stops the ring.
    Ring,
    StopRinging,
    /// The user found this computer.
    FoundIt,
    /// Opens the file picker for the device shown.
    PickFiles,
    /// The picker closed with these files, maybe none.
    FilesPicked(Vec<PathBuf>),
    /// Files were dropped on the device shown.
    Dropped(DroppedData),
    CancelTransfer([u8; 16]),
    /// Play, pause, next or previous on the player shown.
    Media(MediaAction),
    /// The seek bar of the player shown moved, to this many seconds.
    Seek(f64),
    /// The volume slider of the device shown moved, to 0 to 100.
    PhoneVolume(f64),
    PhoneMute(bool),
}

/// How long a value the user just set is shown, while the device's own
/// report of it is on its way.
pub const DRAFT_HOLD: Duration = Duration::from_secs(2);

/// A value the user just set on a device, and when.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Draft<T> {
    pub device: [u8; 16],
    pub value: T,
    pub at: Instant,
}

impl<T: Copy> Draft<T> {
    /// The value, while it is fresh and for `device`.
    pub fn for_device(draft: &Option<Draft<T>>, device: [u8; 16]) -> Option<(T, Instant)> {
        draft
            .filter(|draft| draft.device == device && draft.at.elapsed() < DRAFT_HOLD)
            .map(|draft| (draft.value, draft.at))
    }
}

pub struct Weft {
    pub shell: Shell,
    pub snapshot: Arc<Snapshot>,
    pub page: Page,
    /// The pairing view is shown in place of the device list.
    pub pairing: bool,
    /// The QR card of the code being shown, and that code.
    pub qr: Option<(String, View)>,
    /// "Copy code" was used for the code being shown.
    pub copied: bool,
    pub join_code: String,
    /// The device the last pairing added, until the notice is dismissed.
    pub added: Option<[u8; 16]>,
    /// The device whose removal waits for a confirmation.
    pub removing: Option<[u8; 16]>,
    /// The confirmation is hiding after a choice; `true` if it was Remove.
    pub removal_hiding: Option<bool>,
    /// The device name being edited, if it differs from the saved one.
    pub name_draft: Option<String>,
    pub name_problem: Option<NameProblem>,
    /// Read from the registry, never assumed.
    pub autostart: bool,
    pub autostart_failed: bool,
    /// The device whose details are shown.
    pub selected: Option<[u8; 16]>,
    /// The device this one rang, and when that ring ends by itself.
    pub rang: Option<([u8; 16], Instant)>,
    /// The file picker is open.
    pub picking: bool,
    /// The position just asked of the player shown, in milliseconds.
    pub seek_draft: Option<Draft<u64>>,
    /// The volume just asked of the device shown.
    pub volume_draft: Option<Draft<u8>>,
    /// A tick is on its way.
    ticking: bool,
}

/// Waits on a pool thread for a snapshot newer than `generation`: the
/// reactor has no other way to hear from another thread. Dropping the
/// task does not cancel it.
fn watch(context: &ComponentContext<Weft>, worker: Handle, generation: u64) {
    context.spawn_background(move |_| Message::Published(worker.wait_newer(generation)));
}

impl Weft {
    /// What follows a new snapshot: the QR card for a new code, and the
    /// end of the pairing view once a device was added.
    fn absorb(&mut self, published: Published) {
        self.snapshot = published.snapshot;
        if self.name_draft.as_deref() == Some(&self.snapshot.name) {
            self.name_draft = None;
        }
        match &self.snapshot.pairing {
            PairingState::ShowingCode { code, .. } => {
                if self.qr.as_ref().is_none_or(|(shown, _)| shown != code) {
                    self.qr = qr::view(code).map(|view| (code.clone(), view));
                    self.copied = false;
                }
            }
            PairingState::Success { device } => {
                self.added = Some(*device);
                self.pairing = false;
                self.join_code.clear();
                self.qr = None;
                // Back to idle, so the next pairing starts afresh.
                self.shell.worker.send(Command::CancelPairing);
            }
            _ => self.qr = None,
        }
    }

    /// How long until a text on screen changes with time, if one does.
    fn next_change(&self) -> Option<Duration> {
        let activity = words::activity(self.snapshot.last_activity, SystemTime::now());
        let expiry = match (&self.snapshot.pairing, self.pairing) {
            (PairingState::ShowingCode { expires_at, .. }, true) => {
                Some(words::expiry(*expires_at, Instant::now()).1)
            }
            _ => None,
        };
        let rang = self
            .rang
            .map(|(_, until)| until.saturating_duration_since(Instant::now()));
        // The player shown moves on every second.
        let playing = self
            .selected
            .and_then(|device| self.snapshot.peer(&device).media)
            .is_some_and(|media| media.current().is_some_and(|player| player.playing))
            .then_some(Duration::from_secs(1));
        let drafts = [
            self.seek_draft.map(|draft| draft.at),
            self.volume_draft.map(|draft| draft.at),
        ]
        .into_iter()
        .flatten()
        .map(|at| (at + DRAFT_HOLD).saturating_duration_since(Instant::now()))
        .min();
        activity
            .map(|(_, next)| next)
            .into_iter()
            .chain(expiry)
            .chain(rang)
            .chain(playing)
            .chain(drafts)
            .min()
    }

    /// Makes sure a tick comes when a text changes with time. Ticks are
    /// never cancelled, only spaced: one is on its way at most.
    fn schedule_tick(&mut self, context: &ComponentContext<Self>) {
        if self.ticking {
            return;
        }
        if let Some(next) = self.next_change() {
            self.ticking = true;
            let wait = next.min(MAX_TICK);
            context.spawn_background(move |_| {
                std::thread::sleep(wait);
                Message::Tick
            });
        }
    }

    /// Sends files to the device shown.
    fn send_files(&mut self, paths: Vec<PathBuf>) {
        if let Some(device) = self.selected
            && !paths.is_empty()
        {
            self.shell.worker.send(Command::SendFiles(device, paths));
        }
    }

    /// The device shown and the id of its player shown.
    fn player_shown(&self) -> Option<([u8; 16], String)> {
        let device = self.selected?;
        let media = self.snapshot.peer(&device).media?;
        Some((device, media.current()?.id.clone()))
    }

    fn start_pairing(&mut self) {
        let worker = &self.shell.worker;
        // Idle first, so no earlier outcome shows while the code comes.
        worker.send(Command::CancelPairing);
        worker.send(Command::StartPairing);
        self.pairing = true;
        self.added = None;
        self.copied = false;
    }
}

impl Component for Weft {
    type Input = Shell;
    type Message = Message;

    fn create(shell: &Shell, context: &ComponentContext<Self>) -> Self {
        let published = shell.worker.published();
        watch(context, shell.worker.clone(), published.generation);
        let mut weft = Weft {
            shell: shell.clone(),
            snapshot: published.snapshot.clone(),
            page: Page::Devices,
            pairing: false,
            qr: None,
            copied: false,
            join_code: String::new(),
            added: None,
            removing: None,
            removal_hiding: None,
            name_draft: None,
            name_problem: None,
            autostart: autostart::is_enabled(),
            autostart_failed: false,
            selected: None,
            rang: None,
            picking: false,
            seek_draft: None,
            volume_draft: None,
            ticking: false,
        };
        weft.absorb(published);
        weft.schedule_tick(context);
        weft
    }

    fn update(&mut self, message: Message, context: &ComponentContext<Self>) {
        let worker = self.shell.worker.clone();
        match message {
            Message::Published(Some(published)) => {
                watch(context, worker, published.generation);
                self.absorb(published);
            }
            // The worker stopped: nothing more to wait for.
            Message::Published(None) => {}
            Message::Tick => {
                self.ticking = false;
                if self.rang.is_some_and(|(_, until)| Instant::now() >= until) {
                    self.rang = None;
                }
            }
            Message::Navigate(tag) => {
                if tag.as_deref() == Some(Page::Settings.tag()) {
                    self.page = Page::Settings;
                } else if tag.as_deref() == Some(Page::Devices.tag()) {
                    self.page = Page::Devices;
                    self.selected = None;
                }
            }
            Message::StartPairing => self.start_pairing(),
            Message::ClosePairing => {
                worker.send(Command::CancelPairing);
                self.pairing = false;
                self.qr = None;
            }
            Message::CopyCode => {
                if let PairingState::ShowingCode { code, .. } = &self.snapshot.pairing {
                    self.shell.link.local_text(code.clone());
                    self.copied = true;
                }
            }
            Message::JoinCodeEdited(code) => self.join_code = code,
            Message::Join => {
                if !self.join_code.trim().is_empty() {
                    worker.send(Command::Join(self.join_code.clone()));
                }
            }
            Message::DismissAdded => self.added = None,
            Message::AskRemove(device) => self.removing = Some(device),
            Message::ConfirmRemove | Message::CancelRemove => {
                if self.removing.is_some() && self.removal_hiding.is_none() {
                    let confirmed = matches!(message, Message::ConfirmRemove);
                    // Closing the dialog from code does not report it
                    // closed: drop it from the tree once it has hidden.
                    self.removal_hiding = Some(confirmed);
                    context.spawn_background(|_| {
                        std::thread::sleep(std::time::Duration::from_millis(300));
                        Message::RemovalHidden
                    });
                }
            }
            Message::RemovalHidden => {
                if let (Some(device), Some(true)) = (self.removing, self.removal_hiding) {
                    worker.send(Command::RemoveDevice(device));
                }
                self.removing = None;
                self.removal_hiding = None;
            }
            Message::NameEdited(name) => {
                // Setting the field's text reports a change too: only a
                // name other than the saved one is a draft.
                self.name_draft = (name != self.snapshot.name).then_some(name);
                self.name_problem = None;
            }
            Message::SaveName => {
                if let Some(draft) = &self.name_draft {
                    match words::check_name(draft) {
                        Ok(name) => {
                            worker.send(Command::Rename(name.clone()));
                            // Shown until the worker has it.
                            self.name_draft = Some(name);
                        }
                        Err(problem) => self.name_problem = Some(problem),
                    }
                }
            }
            Message::SetPaused(paused) => worker.send(Command::SetPaused(paused)),
            Message::SetAutostart(enabled) => {
                self.autostart_failed = match autostart::set_enabled(enabled) {
                    Ok(()) => false,
                    Err(err) => {
                        tracing::warn!(error = %err, "could not change the startup entry");
                        true
                    }
                };
                self.autostart = autostart::is_enabled();
            }
            Message::SetFeatures(features) => worker.send(Command::SetFeatures(features)),
            Message::OpenDevice(device) => self.selected = Some(device),
            Message::CloseDevice => self.selected = None,
            Message::Ring => {
                if let Some(device) = self.selected {
                    worker.send(Command::Ring(device));
                    self.rang = Some((device, Instant::now() + RING_DURATION));
                }
            }
            Message::StopRinging => {
                if let Some((device, _)) = self.rang.take() {
                    worker.send(Command::StopRinging(device));
                }
            }
            Message::FoundIt => worker.send(Command::FoundIt),
            Message::PickFiles => {
                if !self.picking {
                    self.picking = true;
                    context.spawn_background(|_| Message::FilesPicked(picker::pick_files()));
                }
            }
            Message::FilesPicked(paths) => {
                self.picking = false;
                self.send_files(paths);
            }
            Message::Dropped(DroppedData::StorageItems(items)) => {
                // Folders are not sent: only files.
                let paths = items
                    .into_iter()
                    .map(|item| PathBuf::from(item.path))
                    .filter(|path| path.is_file())
                    .collect();
                self.send_files(paths);
            }
            Message::Dropped(_) => {}
            Message::CancelTransfer(id) => worker.send(Command::CancelTransfer(id)),
            Message::Media(action) => {
                if let Some((device, player)) = self.player_shown() {
                    let command = MediaCommand::new(&player, action);
                    worker.send(Command::SendPacket(device, Packet::MediaCommand(command)));
                }
            }
            Message::Seek(seconds) => {
                let position = (seconds.max(0.0) * 1000.0) as u64;
                let shown = self
                    .selected
                    .and_then(|device| detail::position_shown(self, device));
                // Setting the bar's value reports a move too: only one away
                // from where it was put is the user's.
                let moved = shown.is_none_or(|shown| shown.abs_diff(position) > 1500);
                if moved && let Some((device, player)) = self.player_shown() {
                    let command = MediaCommand::new(&player, MediaAction::Seek(position));
                    worker.send(Command::SendPacket(device, Packet::MediaCommand(command)));
                    self.seek_draft = Some(Draft {
                        device,
                        value: position,
                        at: Instant::now(),
                    });
                }
            }
            Message::PhoneVolume(level) => {
                if let Some(device) = self.selected {
                    let level = level.round().clamp(0.0, 100.0) as u8;
                    if detail::volume_shown(self, device) != Some(level) {
                        let set = VolumeSet {
                            level: Some(level),
                            muted: None,
                        };
                        worker.send(Command::SendPacket(device, Packet::VolumeSet(set)));
                        self.volume_draft = Some(Draft {
                            device,
                            value: level,
                            at: Instant::now(),
                        });
                    }
                }
            }
            Message::PhoneMute(muted) => {
                if let Some(device) = self.selected {
                    let set = VolumeSet {
                        level: None,
                        muted: Some(muted),
                    };
                    worker.send(Command::SendPacket(device, Packet::VolumeSet(set)));
                }
            }
        }
        self.schedule_tick(context);
    }

    fn view(&self, shell: &Shell, context: &mut ViewContext<Self>) -> View {
        context.window_title("Weft");
        context.window_visuals(
            WindowVisuals::new()
                .backdrop(WindowBackdrop::Mica)
                .client_size(SIZE.0, SIZE.1)
                .constraints(WindowConstraints {
                    min_width: Some(MIN_SIZE.0),
                    min_height: Some(MIN_SIZE.1),
                    ..Default::default()
                }),
        );
        // Once WinUI has made the window: closing it hides it, and at
        // sign-in it starts hidden.
        let hidden = shell.hidden;
        context.use_effect("adopt-window", (), move || {
            window::adopt(hidden);
            None
        });

        let page = match self.page {
            Page::Devices if self.pairing => pairing::view(self, context),
            Page::Devices => match self.selected {
                Some(device) if self.snapshot.devices.iter().any(|known| known.id == device) => {
                    detail::view(self, device, context)
                }
                _ => devices::view(self, context),
            },
            Page::Settings => settings::view(self, context),
        };
        let item = |page: Page, label: &str| {
            KeyedView::new(
                page.tag(),
                NavigationViewItem::new()
                    .tag(page.tag())
                    .is_selected(self.page == page)
                    .selects_on_invoked(true)
                    .slots([SlotView::new(
                        NavigationViewItemSlot::Content,
                        TextBlock::new().text(label),
                    )]),
            )
        };
        let navigation = NavigationView::new()
            .pane_display_mode(NavigationViewPaneDisplayMode::Top)
            .is_settings_visible(false)
            // A top bar has no pane: left open, it would cover the page.
            .is_pane_open(false)
            .is_pane_toggle_button_visible(false)
            .is_back_button_visible(NavigationViewBackButtonVisible::Collapsed)
            .always_show_header(false)
            .on_selected_tag_changed(context.callback(Message::Navigate))
            .grid_row(1)
            .slots([
                SlotView::collection(
                    NavigationViewSlot::MenuItems,
                    [
                        item(Page::Devices, t("Devices", "Appareils")),
                        item(Page::Settings, t("Settings", "Réglages")),
                    ],
                ),
                SlotView::new(
                    NavigationViewSlot::Content,
                    ScrollViewer::new().content(
                        Border::new()
                            .padding(Thickness::new(24.0, 16.0, 24.0, 24.0))
                            .content(match detail::ringing(self, context) {
                                Some(ringing) => super::widgets::column(24.0, vec![ringing, page]),
                                None => page,
                            }),
                    ),
                ),
            ]);
        Grid::new()
            .rows([GridLength::Auto, GridLength::STAR])
            .children((TitleBar::new().title("Weft").grid_row(0), navigation))
    }
}
