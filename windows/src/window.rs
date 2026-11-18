//! The main window as Win32 sees it.
//!
//! windows-reactor has no hidden windows and no way to keep a window from
//! closing: closing the last one ends the app. So once WinUI has created
//! the window, [`adopt`] subclasses it, on its own thread, and a close
//! request hides it instead; the app keeps running in the notification
//! area, which shows the window again with [`show`]. [`quit`] lets one
//! close through, which ends the UI loop.

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::Shell::{
    DefSubclassProc, DragAcceptFiles, DragFinish, HDROP, SetWindowSubclass,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, EnumThreadWindows, GetClassNameW, GetForegroundWindow, GetSystemMetrics, HHOOK,
    ICON_BIG, ICON_SMALL, IMAGE_ICON, IsChild, IsIconic, LR_DEFAULTCOLOR, LoadImageW, MSG,
    PM_REMOVE, PostMessageW, SM_CXICON, SM_CXSMICON, SW_HIDE, SW_RESTORE, SW_SHOW, SendMessageW,
    SetForegroundWindow, SetWindowsHookExW, ShowWindow, UnhookWindowsHookEx, WH_GETMESSAGE,
    WM_CLOSE, WM_DROPFILES, WM_KEYDOWN, WM_NCDESTROY, WM_SETICON,
};

use windows_sys::Win32::UI::WindowsAndMessaging::{EVENT_OBJECT_FOCUS, WINEVENT_OUTOFCONTEXT};

/// The class WinUI gives its top-level windows.
const WINUI_CLASS: &str = "WinUIDesktopWin32WindowClass";
const SUBCLASS_ID: usize = 1;
/// The icon resource of the executable, set by the build script.
const ICON_RESOURCE: usize = 1;

/// The main window once adopted, or 0.
static WINDOW: AtomicIsize = AtomicIsize::new(0);
/// Set by [`quit`]: the next close request goes through.
static QUITTING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DropEvent {
    Paths(Vec<PathBuf>),
    Paste { focus_epoch: u64 },
}

thread_local! {
    // This hook observes only the WinUI thread and lives no longer than its window.
    static PASTE_HOOK: Cell<HHOOK> = const { Cell::new(std::ptr::null_mut()) };
    static FOCUS_HOOK: Cell<HWINEVENTHOOK> = const { Cell::new(std::ptr::null_mut()) };
}

static FOCUS_EPOCH: AtomicU64 = AtomicU64::new(0);

struct DropQueue {
    state: Mutex<(u64, Vec<DropEvent>, bool)>,
    changed: Condvar,
}

impl DropQueue {
    fn new() -> DropQueue {
        DropQueue {
            state: Mutex::new((0, Vec::new(), false)),
            changed: Condvar::new(),
        }
    }

    fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        state.1.clear();
        state.2 = false;
    }

    fn stop(&self) {
        let mut state = self.state.lock().unwrap();
        state.2 = true;
        self.changed.notify_all();
    }

    fn push(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        self.push_event(DropEvent::Paths(paths));
    }

    fn push_event(&self, event: DropEvent) {
        let mut state = self.state.lock().unwrap();
        if state.2 {
            return;
        }
        state.0 += 1;
        state.1.push(event);
        self.changed.notify_all();
    }

    fn wait(&self, _generation: u64) -> Option<(u64, DropEvent)> {
        let mut state = self.state.lock().unwrap();
        loop {
            if !state.1.is_empty() {
                let paths = state.1.remove(0);
                return Some((state.0, paths));
            }
            if state.2 {
                return None;
            }
            state = self.changed.wait(state).unwrap();
        }
    }
}

static DROP_QUEUE: OnceLock<DropQueue> = OnceLock::new();

fn drop_queue() -> &'static DropQueue {
    DROP_QUEUE.get_or_init(DropQueue::new)
}

/// Clears any stale dropped paths before the UI loop starts.
pub(crate) fn reset_drops() {
    drop_queue().reset();
}

/// Wakes any background watcher waiting for dropped files when the UI exits.
pub(crate) fn stop_drops() {
    stop_paste_hook();
    drop_queue().stop();
}

/// Blocks until a batch of files is dropped on the window, or `None` once
/// the UI exits.
pub(crate) fn wait_drops(generation: u64) -> Option<(u64, DropEvent)> {
    drop_queue().wait(generation)
}

unsafe extern "system" fn focus_changed(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _window: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    FOCUS_EPOCH.fetch_add(1, Ordering::AcqRel);
}

fn stop_paste_hook() {
    FOCUS_EPOCH.fetch_add(1, Ordering::AcqRel);
    FOCUS_HOOK.with(|slot| {
        let hook = slot.replace(std::ptr::null_mut());
        if !hook.is_null() {
            // SAFETY: this UI thread installed the focus-event observer.
            unsafe { UnhookWinEvent(hook) };
        }
    });
    PASTE_HOOK.with(|slot| {
        let hook = slot.replace(std::ptr::null_mut());
        if !hook.is_null() {
            // SAFETY: this handle was installed on, and is removed from, the UI thread.
            unsafe { UnhookWindowsHookEx(hook) };
        }
    });
}

fn is_file_paste_key(
    key: usize,
    repeated: bool,
    ctrl: bool,
    alt: bool,
    shift: bool,
    win: bool,
) -> bool {
    key == b'V' as usize && !repeated && ctrl && !alt && !shift && !win
}

extern "system" fn paste_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam == PM_REMOVE as usize {
        // SAFETY: WH_GETMESSAGE supplies a valid MSG for nonnegative hook codes.
        let message = unsafe { &*(lparam as *const MSG) };
        let window = WINDOW.load(Ordering::Acquire) as HWND;
        // SAFETY: handle queries only; this hook runs on the window's UI thread.
        let belongs = unsafe {
            !window.is_null()
                && GetForegroundWindow() == window
                && (message.hwnd == window || IsChild(window, message.hwnd) != 0)
        };
        if message.message == WM_KEYDOWN && belongs {
            // SAFETY: reading modifier states on the thread receiving the key message.
            let down = |key: u16| unsafe { GetKeyState(key as i32) < 0 };
            if is_file_paste_key(
                message.wParam,
                message.lParam & (1 << 30) != 0,
                down(VK_CONTROL),
                down(VK_MENU),
                down(VK_SHIFT),
                down(VK_LWIN) || down(VK_RWIN),
            ) {
                drop_queue().push_event(DropEvent::Paste {
                    focus_epoch: FOCUS_EPOCH.load(Ordering::Acquire),
                });
            }
        }
    }
    // Never swallow or rewrite the key: in particular, native text paste stays intact.
    // SAFETY: forwarding the unchanged hook arguments to the remaining hook chain.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

/// Rejects a queued shortcut when focus changes or the window loses foreground.
pub(crate) fn paste_focus_unchanged(epoch: u64) -> bool {
    let window = WINDOW.load(Ordering::Acquire) as HWND;
    // SAFETY: querying the foreground window only.
    !window.is_null()
        && unsafe { GetForegroundWindow() } == window
        && FOCUS_EPOCH.load(Ordering::Acquire) == epoch
}

/// Checks focus off the UI thread (UI Automation can synchronously call back into it).
/// Only known noneditable XAML controls accept file paste. Unknown controls fail closed.
pub(crate) fn can_paste_files(epoch: u64) -> bool {
    use crate::bindings::{
        CUIAutomation, CoCreateInstance, IUIAutomation, PROPERTYID, UIA_HWND, VARIANT,
        VARIANT_BOOL, VARTYPE,
    };
    use windows_sys::Win32::System::Com::CLSCTX_INPROC_SERVER;

    let window = WINDOW.load(Ordering::Acquire) as HWND;
    // SAFETY: querying the current foreground window has no side effects.
    if !paste_focus_unchanged(epoch) {
        return false;
    }
    if windows_core::init_mta().is_err() {
        return false;
    }
    let focused_class = || -> windows_core::Result<String> {
        // SAFETY: COM interfaces belong to this background thread's MTA.
        unsafe {
            let automation: IUIAutomation = CoCreateInstance(
                &CUIAutomation,
                None::<&windows_core::IUnknown>,
                CLSCTX_INPROC_SERVER,
            )?;
            let focused = automation.GetFocusedElement()?;
            if focused.CurrentProcessId()? != std::process::id() as i32 {
                return Ok(String::new());
            }
            // HasKeyboardFocus within this window's subtree. Process identity alone
            // is insufficient when a file dialog or a second window owns the focus.
            let root = automation.ElementFromHandle(UIA_HWND(window.cast()))?;
            let mut value = VARIANT::default();
            {
                let inner = &mut *value.Anonymous.Anonymous;
                inner.vt = VARTYPE(11); // VT_BOOL
                inner.Anonymous.boolVal = VARIANT_BOOL(-1);
            }
            let condition = automation.CreatePropertyCondition(PROPERTYID(30008), &value)?;
            let descendant = root.FindFirst(4, &condition)?; // TreeScope_Descendants
            if !automation.CompareElements(&focused, &descendant)?.as_bool() {
                return Ok(String::new());
            }
            Ok(String::from_utf16_lossy(&focused.CurrentClassName()?))
        }
    };
    let allowed = focused_class().is_ok_and(|class| accepts_file_paste_focus(&class));
    // Focus may have moved while UI Automation was querying the provider.
    // SAFETY: querying the foreground window only.
    allowed && paste_focus_unchanged(epoch)
}

fn accepts_file_paste_focus(class: &str) -> bool {
    matches!(
        class,
        "Button"
            | "ListViewItem"
            | "GridViewItem"
            | "NavigationViewItem"
            | "Grid"
            | "StackPanel"
            | "Border"
            | "ScrollViewer"
    )
}

extern "system" fn find_winui_window(window: HWND, found: LPARAM) -> i32 {
    let mut class = [0u16; 64];
    // SAFETY: the buffer is as long as announced; `found` points to the
    // `isize` of `adopt`, alive for the whole enumeration.
    unsafe {
        let len = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
        if String::from_utf16_lossy(&class[..len.max(0) as usize]) == WINUI_CLASS {
            *(found as *mut isize) = window as isize;
            return 0;
        }
    }
    1
}

extern "system" fn subclass_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if message == WM_CLOSE && !QUITTING.load(Ordering::Acquire) {
        // SAFETY: hiding the window this procedure belongs to.
        unsafe { ShowWindow(window, SW_HIDE) };
        return 0;
    }
    if message == WM_NCDESTROY {
        stop_paste_hook();
        WINDOW.store(0, Ordering::Release);
    }
    if message == WM_DROPFILES {
        let hdrop = wparam as HDROP;
        // SAFETY: `WM_DROPFILES` hands `hdrop` to this procedure until
        // `DragFinish` releases it.
        let paths = unsafe {
            let paths = crate::clipboard::hdrop_paths(hdrop);
            DragFinish(hdrop);
            paths
        };
        show_hwnd(window);
        drop_queue().push(paths);
        return 0;
    }
    // SAFETY: the arguments are the ones this procedure received.
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}

/// Finds the WinUI window of the calling thread and makes a close request
/// hide it; enables shell file drops; gives it the app's icon, which WinUI
/// only takes from a file; hides it at once when `hidden`. Call it on the UI
/// thread once the window exists. Returns whether the window was found.
pub fn adopt(hidden: bool) -> bool {
    let mut found: isize = 0;
    // SAFETY: the callback only writes to `found`, which outlives the call.
    unsafe {
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(find_winui_window),
            &mut found as *mut isize as LPARAM,
        )
    };
    if found == 0 {
        tracing::warn!("main window not found: closing it will quit the app");
        return false;
    }
    let window = found as HWND;
    // SAFETY: `window` belongs to this thread, as `SetWindowSubclass`
    // requires, and `subclass_proc` matches the signature it expects.
    if unsafe { SetWindowSubclass(window, Some(subclass_proc), SUBCLASS_ID, 0) } == 0 {
        tracing::warn!("could not take over the main window: closing it will quit the app");
        return false;
    }
    // SAFETY: `window` belongs to this thread; `1` enables `WM_DROPFILES`.
    unsafe { DragAcceptFiles(window, 1) };
    WINDOW.store(found, Ordering::Release);
    PASTE_HOOK.with(|slot| {
        if slot.get().is_null() {
            // This focus observer is also process/thread scoped. If it cannot be
            // installed, file paste stays available only through the button.
            // SAFETY: callback only updates an atomic counter; both IDs restrict
            // notifications to this process's UI thread.
            let focus = unsafe {
                SetWinEventHook(
                    EVENT_OBJECT_FOCUS,
                    EVENT_OBJECT_FOCUS,
                    std::ptr::null_mut(),
                    Some(focus_changed),
                    std::process::id(),
                    GetCurrentThreadId(),
                    WINEVENT_OUTOFCONTEXT,
                )
            };
            if focus.is_null() {
                tracing::warn!("could not observe focus for the file paste shortcut");
                return;
            }
            FOCUS_HOOK.with(|slot| slot.set(focus));
            // SAFETY: an in-process callback limited to this UI thread, never a global hook.
            let hook = unsafe {
                SetWindowsHookExW(
                    WH_GETMESSAGE,
                    Some(paste_hook),
                    std::ptr::null_mut(),
                    GetCurrentThreadId(),
                )
            };
            if hook.is_null() {
                tracing::warn!("could not enable the file paste shortcut");
                stop_paste_hook();
            }
            slot.set(hook);
        }
    });
    set_icon(window);
    if hidden {
        // SAFETY: a window of this thread.
        unsafe { ShowWindow(window, SW_HIDE) };
    }
    true
}

/// The main window once adopted, as a raw handle, or 0: the owner of
/// dialogs opened from other threads.
pub fn handle() -> isize {
    WINDOW.load(Ordering::Acquire)
}

fn show_hwnd(window: HWND) {
    if window.is_null() {
        return;
    }
    // SAFETY: these calls only change how the window shows, and fail
    // harmlessly if it is gone.
    unsafe {
        ShowWindow(window, SW_SHOW);
        if IsIconic(window) != 0 {
            ShowWindow(window, SW_RESTORE);
        }
        SetForegroundWindow(window);
    }
}

/// Shows the main window, hidden or minimised, in front. Any thread.
pub fn show() {
    show_hwnd(WINDOW.load(Ordering::Acquire) as HWND);
}

/// Closes the main window for good, which ends the UI loop. Any thread.
pub fn quit() {
    QUITTING.store(true, Ordering::Release);
    let window = WINDOW.load(Ordering::Acquire) as HWND;
    if !window.is_null() {
        // SAFETY: posting fails harmlessly if the window is gone.
        unsafe { PostMessageW(window, WM_CLOSE, 0, 0) };
    }
}

/// Gives `window` the executable's icon, at the sizes the taskbar and the
/// window switcher ask for.
fn set_icon(window: HWND) {
    for (kind, metric) in [(ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)] {
        // SAFETY: the resource id is passed as a pointer, as Win32 expects;
        // the icons are loaded once and kept as long as the window uses
        // them; a failed load sets nothing.
        unsafe {
            let size = GetSystemMetrics(metric);
            let icon = LoadImageW(
                GetModuleHandleW(std::ptr::null()),
                ICON_RESOURCE as *const u16,
                IMAGE_ICON,
                size,
                size,
                LR_DEFAULTCOLOR,
            );
            if !icon.is_null() {
                SendMessageW(window, WM_SETICON, kind as usize, icon as isize);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn paste_shortcut_requires_exact_initial_ctrl_v() {
        assert!(is_file_paste_key(
            b'V' as usize,
            false,
            true,
            false,
            false,
            false
        ));
        for flags in [
            (true, true, false, false, false),
            (false, false, false, false, false),
            (false, true, true, false, false),
            (false, true, false, true, false),
            (false, true, false, false, true),
        ] {
            assert!(!is_file_paste_key(
                b'V' as usize,
                flags.0,
                flags.1,
                flags.2,
                flags.3,
                flags.4
            ));
        }
        assert!(!is_file_paste_key(
            b'C' as usize,
            false,
            true,
            false,
            false,
            false
        ));
    }

    #[test]
    fn file_paste_focus_excludes_editable_and_unknown_controls() {
        for class in [
            "Button",
            "ListViewItem",
            "NavigationViewItem",
            "Grid",
            "Border",
        ] {
            assert!(accepts_file_paste_focus(class));
        }
        for class in [
            "TextBox",
            "RichEditBox",
            "PasswordBox",
            "ComboBox",
            "Unknown",
            "",
        ] {
            assert!(!accepts_file_paste_focus(class));
        }
    }

    #[test]
    fn drop_queue_delivers_batches_in_order_and_wakes_on_stop() {
        let queue = Arc::new(DropQueue::new());
        queue.push(Vec::new());
        queue.push(vec![PathBuf::from(r"C:\a.txt")]);
        queue.push(vec![PathBuf::from(r"C:\b.txt")]);

        let (generation, first) = queue.wait(0).unwrap();
        assert_eq!(generation, 2);
        assert_eq!(first, DropEvent::Paths(vec![PathBuf::from(r"C:\a.txt")]));
        let (_, second) = queue.wait(generation).unwrap();
        assert_eq!(second, DropEvent::Paths(vec![PathBuf::from(r"C:\b.txt")]));

        let waiter_queue = queue.clone();
        let waiter = std::thread::spawn(move || waiter_queue.wait(generation));
        std::thread::sleep(std::time::Duration::from_millis(20));
        queue.stop();
        assert!(waiter.join().unwrap().is_none());
    }
}
