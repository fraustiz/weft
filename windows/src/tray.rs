//! The notification-area icon and the clipboard listener.
//!
//! Both live on one thread, around a hidden window: Windows reports
//! clipboard changes to a window, and every clipboard access happens on the
//! thread that owns it. The other threads reach it through a [`Link`],
//! which posts messages to that window; a second launch of the app finds
//! that window by its title and asks it to show the main window.

use std::cell::RefCell;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use windows_sys::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows_sys::Win32::System::DataExchange::{
    AddClipboardFormatListener, RemoveClipboardFormatListener,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFY_ICON_MESSAGE,
    NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyMenu, DestroyWindow, DispatchMessageW, FindWindowW, GetCursorPos, GetMessageW,
    GetSystemMetrics, GetWindowThreadProcessId, HICON, IDI_APPLICATION, IMAGE_ICON,
    LR_DEFAULTCOLOR, LoadIconW, LoadImageW, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, PostMessageW,
    PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SM_CXSMICON, SM_CYSMICON,
    SetForegroundWindow, SetMenuDefaultItem, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenu, TranslateMessage, WM_APP, WM_CLIPBOARDUPDATE, WM_CLOSE, WM_CONTEXTMENU,
    WM_DESTROY, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSW,
};

use crate::clipboard::{SystemClipboard, wide};
use crate::i18n::t;
use crate::sync::{Outgoing, Sync};
use crate::window;
use crate::worker::{Command, Handle, Snapshot, Status};

/// Sent by the shell for mouse and keyboard actions on the icon.
const WM_ICON: u32 = WM_APP + 1;
/// A received text waits in the [`Link`].
const WM_REMOTE_TEXT: u32 = WM_APP + 2;
/// The worker published a new snapshot.
const WM_SNAPSHOT: u32 = WM_APP + 3;
/// A text from the main window waits in the [`Link`].
const WM_LOCAL_TEXT: u32 = WM_APP + 4;
/// Another launch of the app asks for the main window.
const WM_SHOW_MAIN: u32 = WM_APP + 5;

const CLASS_NAME: &str = "WeftTray";
const ICON_ID: u32 = 1;
/// The icon resource of the executable, set by the build script.
const ICON_RESOURCE: usize = 1;
const MENU_OPEN: usize = 1;
const MENU_PAUSE: usize = 2;
const MENU_QUIT: usize = 3;

/// The message Explorer broadcasts when it restarts, registered at startup.
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

thread_local! {
    /// The state of the tray thread, for its window procedure.
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

/// How the other threads reach the tray thread.
#[derive(Default)]
pub struct Link {
    /// The tray window, or 0 while there is none.
    window: AtomicIsize,
    /// The received text waiting to be written. Only the latest counts.
    text: Mutex<Option<String>>,
    /// The text from the main window waiting to be written.
    local: Mutex<Option<String>>,
}

impl Link {
    /// Hands over a text to put on the clipboard.
    pub fn remote_text(&self, text: String) {
        *self.text.lock().unwrap() = Some(text);
        self.post(WM_REMOTE_TEXT);
    }

    /// Hands over a text from the main window to put on the clipboard for
    /// this computer only: it is not sent, and stays out of the clipboard
    /// history ([`Sync::local_text`]).
    pub fn local_text(&self, text: String) {
        *self.local.lock().unwrap() = Some(text);
        self.post(WM_LOCAL_TEXT);
    }

    /// Tells the tray that the worker's snapshot changed.
    pub fn snapshot_changed(&self) {
        self.post(WM_SNAPSHOT);
    }

    /// Removes the icon and ends the tray thread, as Quit does.
    pub fn quit(&self) {
        self.post(WM_CLOSE);
    }

    fn post(&self, message: u32) {
        let window = self.window.load(Ordering::Acquire);
        if window != 0 {
            // SAFETY: posting to a window from another thread is allowed,
            // and fails harmlessly if the window is gone.
            unsafe { PostMessageW(window as HWND, message, 0, 0) };
        }
    }
}

/// Name of the mutex that makes the app single-instance: one per state
/// directory, since two instances with different identities have no reason
/// to exclude each other.
fn instance_name(state_dir: &Path) -> String {
    let path = std::path::absolute(state_dir).unwrap_or_else(|_| state_dir.to_owned());
    let path = path.to_string_lossy().replace('/', "\\").to_lowercase();
    // FNV-1a, 64 bits: a short, stable name.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.trim_end_matches('\\').bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("Local\\Weft.SingleInstance.{hash:016x}")
}

/// Claims the state directory for this process. Returns `false` if another
/// instance already runs on it.
pub fn single_instance(state_dir: &Path) -> bool {
    let name = wide(&instance_name(state_dir));
    // SAFETY: `name` is NUL-terminated and outlives the call. The mutex is
    // never closed: it lasts as long as the process.
    unsafe {
        let mutex = CreateMutexW(null(), 0, name.as_ptr());
        mutex.is_null() || GetLastError() != ERROR_ALREADY_EXISTS
    }
}

/// From a second launch on the same state directory: asks the instance
/// already running to show its main window. Returns whether it was found.
pub fn show_running_instance(state_dir: &Path) -> bool {
    let (class, title) = (wide(CLASS_NAME), wide(&instance_name(state_dir)));
    // SAFETY: both strings are NUL-terminated and outlive the calls; the
    // others take plain values and fail harmlessly on a stale window.
    unsafe {
        let window = FindWindowW(class.as_ptr(), title.as_ptr());
        if window.is_null() {
            return false;
        }
        // This process was just started by the user, so it may bring a
        // window to the front: it hands that right to the running one.
        let mut process = 0;
        GetWindowThreadProcessId(window, &mut process);
        AllowSetForegroundWindow(process);
        PostMessageW(window, WM_SHOW_MAIN, 0, 0) != 0
    }
}

/// How many other devices there are, in words.
fn device_count(count: usize) -> String {
    match count {
        0 => t("no other device", "aucun autre appareil").to_owned(),
        1 => t("1 device", "1 appareil").to_owned(),
        count => format!("{count} {}", t("devices", "appareils")),
    }
}

/// One line saying where the synchronisation stands.
fn status_text(snapshot: &Snapshot) -> String {
    if snapshot.pause.all {
        return t("paused", "en pause").to_owned();
    }
    match &snapshot.status {
        Status::Connecting => t("connecting", "connexion").to_owned(),
        Status::Connected => format!(
            "{}, {}",
            t("connected", "connecté"),
            device_count(snapshot.devices.len())
        ),
        Status::Offline { .. } => t("offline", "hors ligne").to_owned(),
        Status::Fatal(reason) => format!("{}: {reason}", t("stopped", "arrêté")),
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

struct Tray {
    window: HWND,
    worker: Handle,
    link: Arc<Link>,
    sync: Sync<SystemClipboard>,
    icon: HICON,
    /// The tooltip as last given to the shell.
    tip: String,
}

impl Tray {
    fn icon_data(&self) -> NOTIFYICONDATAW {
        // SAFETY: all-zero is a valid NOTIFYICONDATAW.
        let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.window;
        data.uID = ICON_ID;
        data
    }

    fn notify(&self, message: NOTIFY_ICON_MESSAGE, data: &NOTIFYICONDATAW) {
        // SAFETY: `data` is a fully initialised structure of the size it
        // announces.
        unsafe { Shell_NotifyIconW(message, data) };
    }

    /// Shows the icon, or updates its tooltip if it is already there.
    fn show_icon(&mut self, message: NOTIFY_ICON_MESSAGE) {
        self.tip = format!("Weft — {}", status_text(&self.worker.snapshot()));
        let mut data = self.icon_data();
        data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        data.uCallbackMessage = WM_ICON;
        data.hIcon = self.icon;
        // The last slot stays 0: the tooltip is cut to fit, with its NUL.
        let room = data.szTip.len() - 1;
        for (slot, unit) in data.szTip[..room].iter_mut().zip(self.tip.encode_utf16()) {
            *slot = unit;
        }
        self.notify(message, &data);
    }

    fn refresh_icon(&mut self) {
        let tip = format!("Weft — {}", status_text(&self.worker.snapshot()));
        if tip != self.tip {
            self.show_icon(NIM_MODIFY);
        }
    }

    fn remove_icon(&self) {
        self.notify(NIM_DELETE, &self.icon_data());
    }

    /// The system reported a clipboard change: sends the text if the
    /// policy says so.
    fn clipboard_changed(&mut self) {
        let snapshot = self.worker.snapshot();
        let online = snapshot.is_connected() && !snapshot.devices.is_empty();
        match self
            .sync
            .local_change(snapshot.pause, online, std::time::Instant::now())
        {
            Outgoing::Send(text) => self.worker.send(Command::SendText(text)),
            Outgoing::TooLarge => {
                tracing::info!("the copied text is over the size limit; not sent");
            }
            outcome => tracing::debug!(?outcome, "clipboard change not sent"),
        }
    }

    /// Writes the text the worker handed over, if any.
    fn apply_remote_text(&mut self) {
        let text = self.link.text.lock().unwrap().take();
        if let Some(text) = text {
            let outcome = self.sync.remote_text(&text, self.worker.snapshot().pause);
            tracing::debug!(?outcome, "received text");
        }
    }

    /// Writes the text the main window handed over, if any.
    fn apply_local_text(&mut self) {
        let text = self.link.local.lock().unwrap().take();
        if let Some(text) = text
            && !self.sync.local_text(&text)
        {
            tracing::warn!("could not put the text on the clipboard");
        }
    }
}

/// Runs `action` on the tray state. Does nothing outside the tray thread,
/// before the state exists, or if it is already in use further up the
/// stack (which no code path does: nothing dispatches messages while
/// holding it).
fn with_tray<T>(action: impl FnOnce(&mut Tray) -> T) -> Option<T> {
    TRAY.with(|tray| tray.try_borrow_mut().ok()?.as_mut().map(action))
}

/// The context menu: Open (the default, as a click does), a status line,
/// the pause switch and Quit.
fn show_menu(window: HWND) {
    let Some((status, paused)) = with_tray(|tray| {
        let snapshot = tray.worker.snapshot();
        (capitalised(&status_text(&snapshot)), snapshot.pause.all)
    }) else {
        return;
    };
    let pause = if paused {
        t("Resume sync", "Reprendre la synchronisation")
    } else {
        t("Pause sync", "Mettre la synchronisation en pause")
    };
    // The tray state is not held here: the menu runs its own message loop,
    // which keeps delivering clipboard changes to the window procedure.
    // SAFETY: the strings are NUL-terminated and outlive the calls that
    // copy them; the menu is destroyed before returning.
    let chosen = unsafe {
        let menu = CreatePopupMenu();
        AppendMenuW(
            menu,
            MF_STRING,
            MENU_OPEN,
            wide(t("Open Weft", "Ouvrir Weft")).as_ptr(),
        );
        SetMenuDefaultItem(menu, MENU_OPEN as u32, 0);
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, wide(&status).as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, MENU_PAUSE, wide(pause).as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(
            menu,
            MF_STRING,
            MENU_QUIT,
            wide(t("Quit", "Quitter")).as_ptr(),
        );
        let mut cursor = POINT { x: 0, y: 0 };
        GetCursorPos(&mut cursor);
        // Without this the menu stays open when the user clicks elsewhere.
        SetForegroundWindow(window);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
            cursor.x,
            cursor.y,
            0,
            window,
            null(),
        );
        PostMessageW(window, WM_NULL, 0, 0);
        DestroyMenu(menu);
        chosen as usize
    };
    match chosen {
        MENU_OPEN => window::show(),
        MENU_PAUSE => {
            with_tray(|tray| tray.worker.send(Command::SetPaused(!paused)));
        }
        // SAFETY: `window` is this thread's window. Destroying it removes
        // the icon and ends the message loop (WM_DESTROY).
        MENU_QUIT => unsafe {
            DestroyWindow(window);
        },
        _ => {}
    }
}

extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CLIPBOARDUPDATE => {
            with_tray(Tray::clipboard_changed);
        }
        WM_REMOTE_TEXT => {
            with_tray(Tray::apply_remote_text);
        }
        WM_SNAPSHOT => {
            with_tray(Tray::refresh_icon);
        }
        WM_LOCAL_TEXT => {
            with_tray(Tray::apply_local_text);
        }
        WM_SHOW_MAIN => window::show(),
        WM_ICON => match (lparam as u32) & 0xffff {
            WM_LBUTTONUP => window::show(),
            WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(window),
            _ => {}
        },
        WM_DESTROY => {
            with_tray(|tray| tray.remove_icon());
            // SAFETY: `window` is the window being destroyed, on its thread.
            unsafe {
                RemoveClipboardFormatListener(window);
                PostQuitMessage(0);
            }
        }
        // Explorer restarted and lost the icon: put it back.
        _ if message != 0 && message == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            with_tray(|tray| tray.show_icon(NIM_ADD));
        }
        // SAFETY: the arguments are the ones this procedure received.
        _ => return unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
    0
}

/// Creates the hidden window, the icon and the clipboard listener, then
/// runs the message loop until the user quits. The window is named after
/// `state_dir`, for [`show_running_instance`]. Call it on a thread of its
/// own.
pub fn run(worker: Handle, link: Arc<Link>, state_dir: &Path) {
    let (class_name, title) = (wide(CLASS_NAME), wide(&instance_name(state_dir)));
    // SAFETY: the class and window names are NUL-terminated and outlive
    // the calls; `window_proc` matches the signature Windows expects.
    let (window, icon) = unsafe {
        let instance = GetModuleHandleW(null());
        let mut class: WNDCLASSW = std::mem::zeroed();
        class.lpfnWndProc = Some(window_proc);
        class.hInstance = instance;
        class.lpszClassName = class_name.as_ptr();
        RegisterClassW(&class);
        TASKBAR_CREATED.store(
            RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            Ordering::Relaxed,
        );
        // A hidden top-level window rather than a message-only one, which
        // would not receive the TaskbarCreated broadcast.
        let window = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        // The size the notification area uses, so that the matching image
        // of the icon is picked instead of a scaled one.
        let mut icon = LoadImageW(
            instance,
            ICON_RESOURCE as *const u16,
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            LR_DEFAULTCOLOR,
        );
        if icon.is_null() {
            icon = LoadIconW(null_mut(), IDI_APPLICATION);
        }
        (window, icon)
    };
    if window.is_null() {
        tracing::error!("could not create the tray window");
        return;
    }
    let mut tray = Tray {
        window,
        worker,
        link: link.clone(),
        // From here on, only changes are sent: not what is there now.
        sync: Sync::new(SystemClipboard::new(window)),
        icon,
        tip: String::new(),
    };
    // SAFETY: `window` was just created by this thread.
    if unsafe { AddClipboardFormatListener(window) } == 0 {
        tracing::error!("could not listen to clipboard changes");
    }
    tray.show_icon(NIM_ADD);
    TRAY.set(Some(tray));
    link.window.store(window as isize, Ordering::Release);
    // What arrived before the window existed.
    link.post(WM_REMOTE_TEXT);
    link.post(WM_LOCAL_TEXT);
    link.post(WM_SNAPSHOT);
    tracing::info!("tray icon ready");

    // SAFETY: a standard message loop; `message` is written by GetMessageW
    // before it is read.
    unsafe {
        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    link.window.store(0, Ordering::Release);
    TRAY.set(None);
    tracing::info!("tray icon removed");
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::sync::Pause;
    use crate::worker::{Device, PairingState};

    fn snapshot(status: Status, devices: usize) -> Snapshot {
        Snapshot {
            status,
            name: "Test PC".into(),
            device_id: [1; 16],
            relay_url: "wss://relay.example/v1".into(),
            devices: (0..devices)
                .map(|n| Device {
                    id: [n as u8; 16],
                    name: None,
                })
                .collect(),
            pause: Pause::default(),
            pairing: PairingState::Idle,
            last_activity: None,
        }
    }

    #[test]
    fn status_line() {
        let connected = |devices| status_text(&snapshot(Status::Connected, devices));
        assert_eq!(
            connected(0),
            t(
                "connected, no other device",
                "connecté, aucun autre appareil"
            )
        );
        assert_eq!(
            connected(1),
            t("connected, 1 device", "connecté, 1 appareil")
        );
        assert_eq!(
            connected(3),
            t("connected, 3 devices", "connecté, 3 appareils")
        );
        let offline = Status::Offline {
            reason: "the relay closed the connection".into(),
            retry_at: Instant::now(),
        };
        assert_eq!(
            status_text(&snapshot(offline, 2)),
            t("offline", "hors ligne")
        );
        assert_eq!(
            status_text(&snapshot(Status::Connecting, 2)),
            t("connecting", "connexion")
        );

        // The pause is what the user asked for: it shows whatever the
        // connection does.
        let mut paused = snapshot(Status::Connected, 2);
        paused.pause.all = true;
        assert_eq!(status_text(&paused), t("paused", "en pause"));
        assert_eq!(capitalised(&status_text(&paused)), t("Paused", "En pause"));
    }

    #[test]
    fn one_instance_name_per_state_directory() {
        let name = instance_name(Path::new(r"C:\data\weft"));
        assert!(name.starts_with(r"Local\Weft.SingleInstance."));
        assert_eq!(name, instance_name(Path::new(r"c:\DATA\Weft\")));
        assert_eq!(name, instance_name(Path::new("C:/data/weft")));
        assert_ne!(name, instance_name(Path::new(r"C:\data\weft-test")));
    }
}
