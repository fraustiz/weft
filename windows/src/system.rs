//! This computer's side of the toasts, the players and the volume, on a
//! thread of its own in the multithreaded apartment: WinRT and COM calls
//! there may block, and never hold up the window or the worker.
//!
//! Other threads talk to it through a [`System`]; it tells the worker what
//! the players and the volume are with [`Command::LocalMedia`] and
//! [`Command::LocalVolume`].
//!
//! - Toasts: shown and removed by group and tag; a failed toast becomes a
//!   balloon of the tray icon.
//! - Players: the global system media transport controls. Their events
//!   mark the list as changed; it is read and sent [`MEDIA_DEBOUNCE`] after
//!   the first change, so a burst of events gives one packet.
//! - Volume: the default output device, read every [`VOLUME_POLL`]. The
//!   device itself is looked up again every [`DEVICE_REFRESH`], in case the
//!   default changed.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use windows_core::EventRevoker;
use windows_sys::Win32::Media::Audio::{eMultimedia, eRender};
use windows_sys::Win32::System::Com::CLSCTX_ALL;

use crate::bindings::{
    CoCreateInstance, GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status, IAudioEndpointVolume,
    IMMDeviceEnumerator, MMDeviceEnumerator,
};
use crate::packets::{MediaAction, Player, Volume, VolumeSet};
use crate::toast::{Activation, Content, Shown, Toaster};
use crate::tray::Link;
use crate::worker::{Command, Handle};

/// How long after the first change of the players they are read.
pub const MEDIA_DEBOUNCE: Duration = Duration::from_millis(300);
/// How often the volume is read.
pub const VOLUME_POLL: Duration = Duration::from_millis(500);
/// How often the default output device is looked up again.
const DEVICE_REFRESH: Duration = Duration::from_secs(5);
/// Toasts whose click handlers are kept; older ones no longer answer.
const TOASTS_KEPT: usize = 64;

/// What a click on a toast does, given the worker.
pub type OnClick = Arc<dyn Fn(Activation, &Handle) + Send + Sync>;

/// A toast to show.
pub struct Toast {
    pub group: String,
    pub tag: String,
    pub content: Content,
    /// Straight to the notification centre, without popping up.
    pub suppress_popup: bool,
    pub on_click: Option<OnClick>,
}

enum Request {
    Toast(Toast),
    Remove {
        group: String,
        tag: String,
    },
    Media {
        player: String,
        action: MediaAction,
    },
    Volume(VolumeSet),
    /// A player changed, or the list of players when `sessions`: read
    /// them all soon.
    MediaChanged {
        sessions: bool,
    },
}

/// The other threads' side of the system thread. Requests sent before it
/// starts wait for it.
#[derive(Clone)]
pub struct System {
    requests: Sender<Request>,
}

/// The requests for the thread [`start`] runs.
pub struct Inbox(Receiver<Request>);

impl System {
    pub fn new() -> (System, Inbox) {
        let (requests, inbox) = channel();
        (System { requests }, Inbox(inbox))
    }

    pub fn toast(&self, toast: Toast) {
        let _ = self.requests.send(Request::Toast(toast));
    }

    pub fn remove_toast(&self, group: &str, tag: &str) {
        let _ = self.requests.send(Request::Remove {
            group: group.into(),
            tag: tag.into(),
        });
    }

    pub fn media(&self, player: String, action: MediaAction) {
        let _ = self.requests.send(Request::Media { player, action });
    }

    pub fn volume(&self, set: VolumeSet) {
        let _ = self.requests.send(Request::Volume(set));
    }
}

/// Starts the system thread for `system`. Toasts fall back on `link`'s
/// balloons.
pub fn start(system: &System, inbox: Inbox, worker: Handle, link: Arc<Link>) {
    // The events of the players come back through the same queue.
    let changed = system.requests.clone();
    let spawned = std::thread::Builder::new()
        .name("weft-system".into())
        .spawn(move || run(inbox.0, changed, worker, link));
    if let Err(err) = spawned {
        tracing::error!(error = %err, "could not start the system thread");
    }
}

/// A short name for the app of a media session, from its id: `Spotify.exe`
/// gives `Spotify`, `Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic`
/// gives `ZuneMusic`.
pub fn app_name(app_id: &str) -> String {
    let name = app_id.rsplit('!').next().unwrap_or(app_id);
    let name = name.split('_').next().unwrap_or(name);
    let name = name.strip_suffix(".exe").unwrap_or(name);
    let name = name.rsplit('.').next().unwrap_or(name);
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => app_id.to_owned(),
    }
}

/// 100-nanosecond ticks to milliseconds, never below zero.
fn ms(ticks: i64) -> u64 {
    (ticks / 10_000).max(0) as u64
}

/// The players of this computer, and the sessions behind their ids.
struct Players {
    manager: Manager,
    _sessions_changed: EventRevoker,
    /// The events of the sessions, dropped (unsubscribed) on a new list.
    watched: Vec<EventRevoker>,
    sessions: Vec<(String, Session)>,
}

impl Players {
    fn new(changed: Sender<Request>) -> windows_core::Result<Players> {
        let manager = Manager::RequestAsync()?.join()?;
        let notify = changed.clone();
        let sessions_changed = manager.SessionsChanged(move |_, _| {
            let _ = notify.send(Request::MediaChanged { sessions: true });
        })?;
        let mut players = Players {
            manager,
            _sessions_changed: sessions_changed,
            watched: Vec::new(),
            sessions: Vec::new(),
        };
        players.watch(&changed);
        Ok(players)
    }

    /// Lists the sessions, and subscribes to the changes of each.
    fn watch(&mut self, changed: &Sender<Request>) {
        self.watched.clear();
        self.sessions.clear();
        let Ok(list) = self.manager.GetSessions() else {
            return;
        };
        let count = list.Size().unwrap_or(0);
        for index in 0..count {
            let Ok(session) = list.GetAt(index) else {
                continue;
            };
            let app_id = session
                .SourceAppUserModelId()
                .map(|id| id.to_string_lossy())
                .unwrap_or_default();
            // Two sessions of one app get ids of their own.
            let same = self
                .sessions
                .iter()
                .filter(|(id, _)| id.split('#').next() == Some(&app_id))
                .count();
            let id = if same == 0 {
                app_id
            } else {
                format!("{app_id}#{}", same + 1)
            };
            let (properties, playback, timeline) =
                (changed.clone(), changed.clone(), changed.clone());
            let subscribed = [
                session.MediaPropertiesChanged(move |_, _| {
                    let _ = properties.send(Request::MediaChanged { sessions: false });
                }),
                session.PlaybackInfoChanged(move |_, _| {
                    let _ = playback.send(Request::MediaChanged { sessions: false });
                }),
                session.TimelinePropertiesChanged(move |_, _| {
                    let _ = timeline.send(Request::MediaChanged { sessions: false });
                }),
            ];
            self.watched.extend(subscribed.into_iter().flatten());
            self.sessions.push((id, session));
        }
    }

    fn read(&self) -> Vec<Player> {
        self.sessions
            .iter()
            .filter_map(|(id, session)| read_player(id, session).ok())
            .collect()
    }

    fn command(&self, player: &str, action: MediaAction) {
        let Some((_, session)) = self.sessions.iter().find(|(id, _)| id == player) else {
            tracing::debug!("media command for a player that is gone");
            return;
        };
        let done = match action {
            MediaAction::Play => session.TryPlayAsync().and_then(|op| op.join()),
            MediaAction::Pause => session.TryPauseAsync().and_then(|op| op.join()),
            MediaAction::PlayPause => session.TryTogglePlayPauseAsync().and_then(|op| op.join()),
            MediaAction::Next => session.TrySkipNextAsync().and_then(|op| op.join()),
            MediaAction::Previous => session.TrySkipPreviousAsync().and_then(|op| op.join()),
            MediaAction::Seek(position) => {
                let start = session
                    .GetTimelineProperties()
                    .and_then(|timeline| timeline.StartTime())
                    .map(|start| start.duration)
                    .unwrap_or(0);
                let ticks = start.saturating_add((position as i64).saturating_mul(10_000));
                session
                    .TryChangePlaybackPositionAsync(ticks)
                    .and_then(|op| op.join())
            }
            // Windows has no volume per player.
            MediaAction::SetVolume(_) => Ok(false),
        };
        match done {
            Ok(true) => {}
            Ok(false) => tracing::debug!(?action, "the player refused a media command"),
            Err(err) => tracing::warn!(error = %err, "media command failed"),
        }
    }
}

fn read_player(id: &str, session: &Session) -> windows_core::Result<Player> {
    let app_id = session.SourceAppUserModelId()?.to_string_lossy();
    let properties = session.TryGetMediaPropertiesAsync()?.join()?;
    let info = session.GetPlaybackInfo()?;
    let controls = info.Controls()?;
    let playing = info.PlaybackStatus()? == Status::Playing;
    let timeline = session.GetTimelineProperties()?;
    let start = timeline.StartTime()?.duration;
    let length = ms(timeline.EndTime()?.duration - start);
    let mut position = ms(timeline.Position()?.duration - start);
    // Players update the timeline now and then; it moved on since.
    let updated = timeline.LastUpdatedTime()?.universal_time;
    if playing && updated > 0 {
        let now = windows_time::DateTime::now().universal_time;
        position = position.saturating_add(ms(now - updated));
    }
    let (position, length) = if length > 0 {
        (Some(position.min(length)), Some(length))
    } else {
        (None, None)
    };
    Ok(Player {
        id: id.to_owned(),
        app: app_name(&app_id),
        title: properties.Title()?.to_string_lossy(),
        artist: properties.Artist()?.to_string_lossy(),
        album: properties.AlbumTitle()?.to_string_lossy(),
        playing,
        position,
        length,
        can_play: controls.IsPlayEnabled()?,
        can_pause: controls.IsPauseEnabled()?,
        can_next: controls.IsNextEnabled()?,
        can_previous: controls.IsPreviousEnabled()?,
        can_seek: controls.IsPlaybackPositionEnabled()? && length.is_some(),
        volume: None,
    })
}

/// The default output device's volume.
struct Endpoint {
    enumerator: IMMDeviceEnumerator,
    volume: Option<(IAudioEndpointVolume, Instant)>,
}

impl Endpoint {
    fn new() -> windows_core::Result<Endpoint> {
        // SAFETY: a plain COM creation on a thread in the multithreaded
        // apartment.
        let enumerator = unsafe {
            CoCreateInstance(
                &MMDeviceEnumerator,
                None::<&windows_core::IUnknown>,
                CLSCTX_ALL,
            )?
        };
        Ok(Endpoint {
            enumerator,
            volume: None,
        })
    }

    /// The volume interface of the current default device.
    fn device(&mut self) -> Option<&IAudioEndpointVolume> {
        let stale = self
            .volume
            .as_ref()
            .is_none_or(|(_, at)| at.elapsed() >= DEVICE_REFRESH);
        if stale {
            // SAFETY: COM calls on interfaces this thread owns; no
            // activation parameters.
            let found = unsafe {
                self.enumerator
                    .GetDefaultAudioEndpoint(eRender, eMultimedia)
                    .and_then(|device| device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None))
            };
            // No output device: nothing to read until one comes.
            self.volume = found.ok().map(|volume| (volume, Instant::now()));
        }
        self.volume.as_ref().map(|(volume, _)| volume)
    }

    fn read(&mut self) -> Option<Volume> {
        let device = self.device()?;
        // SAFETY: COM calls on an interface this thread owns.
        let (level, muted) = unsafe {
            (
                device.GetMasterVolumeLevelScalar().ok()?,
                device.GetMute().ok()?,
            )
        };
        Some(Volume {
            level: Some((level * 100.0).round().clamp(0.0, 100.0) as u8),
            muted: Some(muted.as_bool()),
        })
    }

    fn set(&mut self, set: VolumeSet) {
        let Some(device) = self.device() else {
            return;
        };
        // SAFETY: COM calls on an interface this thread owns; no event
        // context.
        unsafe {
            if let Some(level) = set.level {
                let scalar = f32::from(level.min(100)) / 100.0;
                let _ = device.SetMasterVolumeLevelScalar(scalar, std::ptr::null());
            }
            if let Some(muted) = set.muted {
                let _ = device.SetMute(muted, std::ptr::null());
            }
        }
    }
}

fn run(inbox: Receiver<Request>, changed: Sender<Request>, worker: Handle, link: Arc<Link>) {
    if let Err(err) = windows_core::init_mta() {
        tracing::error!(error = %err, "no COM on the system thread: no toasts, players or volume");
    }
    let toaster = Toaster::new()
        .inspect_err(|err| tracing::warn!(error = %err, "no toasts: balloons instead"))
        .ok();
    let mut players = Players::new(changed.clone())
        .inspect_err(|err| tracing::warn!(error = %err, "the players cannot be read"))
        .ok();
    let mut endpoint = Endpoint::new()
        .inspect_err(|err| tracing::warn!(error = %err, "the volume cannot be read"))
        .ok();
    let mut shown: VecDeque<((String, String), Shown)> = VecDeque::new();
    // Read once at the start.
    let mut media_due = Some(Instant::now());
    let mut sessions_changed = false;
    let mut volume_due = Instant::now();
    loop {
        let next = media_due.map_or(volume_due, |due| due.min(volume_due));
        let request = match inbox.recv_timeout(next.saturating_duration_since(Instant::now())) {
            Ok(request) => Some(request),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        match request {
            Some(Request::Toast(toast)) => {
                let key = (toast.group.clone(), toast.tag.clone());
                shown.retain(|(shown_key, _)| *shown_key != key);
                let result = match &toaster {
                    Some(toaster) => {
                        let (on_click, worker) = (toast.on_click.clone(), worker.clone());
                        toaster.show(
                            &toast.group,
                            &toast.tag,
                            &toast.content,
                            toast.suppress_popup,
                            move |activation| {
                                if let Some(on_click) = &on_click {
                                    on_click(activation, &worker);
                                }
                            },
                        )
                    }
                    None => Err(windows_core::Error::empty()),
                };
                match result {
                    Ok(handle) => {
                        shown.push_back((key, handle));
                        while shown.len() > TOASTS_KEPT {
                            shown.pop_front();
                        }
                    }
                    Err(err) => {
                        if toaster.is_some() {
                            tracing::warn!(error = %err, "toast failed: balloon instead");
                        }
                        if !toast.suppress_popup {
                            let mut lines = toast.content.lines.into_iter();
                            let title = lines.next().unwrap_or_default();
                            let text: Vec<String> = lines.collect();
                            link.balloon(title, text.join("\n"));
                        }
                    }
                }
            }
            Some(Request::Remove { group, tag }) => {
                shown.retain(|(key, _)| *key != (group.clone(), tag.clone()));
                if let Some(toaster) = &toaster
                    && let Err(err) = toaster.remove(&group, &tag)
                {
                    tracing::debug!(error = %err, "toast not removed");
                }
            }
            Some(Request::Media { player, action }) => {
                if let Some(players) = &players {
                    players.command(&player, action);
                }
                media_due.get_or_insert(Instant::now() + MEDIA_DEBOUNCE);
            }
            Some(Request::Volume(set)) => {
                if let Some(endpoint) = &mut endpoint {
                    endpoint.set(set);
                }
                volume_due = Instant::now();
            }
            Some(Request::MediaChanged { sessions }) => {
                sessions_changed |= sessions;
                media_due.get_or_insert(Instant::now() + MEDIA_DEBOUNCE);
            }
            None => {}
        }
        let now = Instant::now();
        if media_due.is_some_and(|due| now >= due) {
            media_due = None;
            if let Some(players) = &mut players {
                if std::mem::take(&mut sessions_changed) {
                    players.watch(&changed);
                }
                worker.send(Command::LocalMedia(players.read()));
            }
        }
        if now >= volume_due {
            volume_due = now + VOLUME_POLL;
            if let Some(volume) = endpoint.as_mut().and_then(Endpoint::read) {
                worker.send(Command::LocalVolume(volume));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_names_from_session_ids() {
        assert_eq!(app_name("Spotify.exe"), "Spotify");
        assert_eq!(app_name("chrome"), "Chrome");
        assert_eq!(
            app_name("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"),
            "ZuneMusic"
        );
        assert_eq!(app_name(""), "");
    }
}
