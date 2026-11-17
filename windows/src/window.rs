//! The main window as Win32 sees it.
//!
//! windows-reactor has no hidden windows and no way to keep a window from
//! closing: closing the last one ends the app. So once WinUI has created
//! the window, [`adopt`] subclasses it, on its own thread, and a close
//! request hides it instead; the app keeps running in the notification
//! area, which shows the window again with [`show`]. [`quit`] lets one
//! close through, which ends the UI loop.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Shell::{
    DefSubclassProc, DragAcceptFiles, DragFinish, HDROP, SetWindowSubclass,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumThreadWindows, GetClassNameW, GetSystemMetrics, ICON_BIG, ICON_SMALL, IMAGE_ICON, IsIconic,
    LR_DEFAULTCOLOR, LoadImageW, PostMessageW, SM_CXICON, SM_CXSMICON, SW_HIDE, SW_RESTORE,
    SW_SHOW, SendMessageW, SetForegroundWindow, ShowWindow, WM_CLOSE, WM_DROPFILES, WM_SETICON,
};

/// The class WinUI gives its top-level windows.
const WINUI_CLASS: &str = "WinUIDesktopWin32WindowClass";
const SUBCLASS_ID: usize = 1;
/// The icon resource of the executable, set by the build script.
const ICON_RESOURCE: usize = 1;

/// The main window once adopted, or 0.
static WINDOW: AtomicIsize = AtomicIsize::new(0);
/// Set by [`quit`]: the next close request goes through.
static QUITTING: AtomicBool = AtomicBool::new(false);

struct DropQueue {
    state: Mutex<(u64, Vec<Vec<PathBuf>>, bool)>,
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
        let mut state = self.state.lock().unwrap();
        state.0 += 1;
        state.1.push(paths);
        self.changed.notify_all();
    }

    fn wait(&self, _generation: u64) -> Option<(u64, Vec<PathBuf>)> {
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
    drop_queue().stop();
}

/// Blocks until a batch of files is dropped on the window, or `None` once
/// the UI exits.
pub(crate) fn wait_drops(generation: u64) -> Option<(u64, Vec<PathBuf>)> {
    drop_queue().wait(generation)
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
    fn drop_queue_delivers_batches_in_order_and_wakes_on_stop() {
        let queue = Arc::new(DropQueue::new());
        queue.push(Vec::new());
        queue.push(vec![PathBuf::from(r"C:\a.txt")]);
        queue.push(vec![PathBuf::from(r"C:\b.txt")]);

        let (generation, first) = queue.wait(0).unwrap();
        assert_eq!(generation, 2);
        assert_eq!(first, vec![PathBuf::from(r"C:\a.txt")]);
        let (_, second) = queue.wait(generation).unwrap();
        assert_eq!(second, vec![PathBuf::from(r"C:\b.txt")]);

        let waiter_queue = queue.clone();
        let waiter = std::thread::spawn(move || waiter_queue.wait(generation));
        std::thread::sleep(std::time::Duration::from_millis(20));
        queue.stop();
        assert!(waiter.join().unwrap().is_none());
    }
}
