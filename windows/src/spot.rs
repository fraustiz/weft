//! The presenter's spot: a round red dot over everything, that clicks go
//! through, on the screen of the window in front.
//!
//! It is a layered window on a thread of its own, with its message loop,
//! made the first time the spot shows. [`show`] only records where the spot
//! should be and posts a message; the window's thread moves it.

use std::sync::mpsc::channel;
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, GetDC, GetMonitorInfoW,
    MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow, ReleaseDC, SelectObject,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetMessageW,
    HWND_TOPMOST, MSG, PostMessageW, RegisterClassW, SW_HIDE, SWP_NOACTIVATE, SWP_NOSIZE,
    SWP_SHOWWINDOW, SetWindowPos, ShowWindow, ULW_ALPHA, UpdateLayeredWindow, WM_APP, WNDCLASSW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::clipboard::wide;

/// The spot's diameter at 100 % scale, in pixels.
const DIAMETER: f64 = 28.0;
/// Its colour, and how opaque its middle is.
const RED: (u8, u8, u8) = (0xE5, 0x2B, 0x22);
const OPACITY: f64 = 0.85;
/// The message that tells the window to go where [`TARGET`] says.
const MOVE: u32 = WM_APP + 1;

/// Where the spot should be, -1 to 1 on each axis, or hidden.
static TARGET: Mutex<Option<(f64, f64)>> = Mutex::new(None);
/// The window, once made; 0 if it could not be.
static WINDOW: OnceLock<isize> = OnceLock::new();

/// Shows the spot at `at` on the screen of the window in front, or hides
/// it. Any thread.
pub fn show(at: Option<(f64, f64)>) {
    *TARGET.lock().unwrap() = at;
    if at.is_none() && WINDOW.get().is_none() {
        return;
    }
    let window = *WINDOW.get_or_init(start);
    if window != 0 {
        // SAFETY: posting fails harmlessly if the window is gone.
        unsafe { PostMessageW(window as HWND, MOVE, 0, 0) };
    }
}

/// Starts the window's thread and waits for the window.
fn start() -> isize {
    let (made, window) = channel();
    let spawned = std::thread::Builder::new()
        .name("weft-spot".into())
        .spawn(move || {
            let window = create();
            let _ = made.send(window as isize);
            if window.is_null() {
                return;
            }
            let mut message: MSG = unsafe { std::mem::zeroed() };
            // SAFETY: a plain message loop for this thread's window.
            unsafe {
                while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                    DispatchMessageW(&message);
                }
            }
        });
    match spawned {
        Ok(_) => window.recv().unwrap_or(0),
        Err(err) => {
            tracing::warn!(error = %err, "could not start the presenter spot");
            0
        }
    }
}

/// The spot's diameter on this display, in pixels.
fn diameter() -> i32 {
    // SAFETY: no argument.
    let dpi = unsafe { GetDpiForSystem() }.max(96);
    (DIAMETER * f64::from(dpi) / 96.0).round() as i32
}

/// The spot's pixels, premultiplied BGRA, top row first: a disc with a
/// smooth edge.
fn pixels(size: i32) -> Vec<u8> {
    let centre = f64::from(size) / 2.0;
    let radius = centre - 1.0;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (dx, dy) = (f64::from(x) + 0.5 - centre, f64::from(y) + 0.5 - centre);
            let cover = (radius - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
            let alpha = cover * OPACITY;
            let channel = |value: u8| (f64::from(value) * alpha).round() as u8;
            pixels.extend([
                channel(RED.2),
                channel(RED.1),
                channel(RED.0),
                (alpha * 255.0).round() as u8,
            ]);
        }
    }
    pixels
}

extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == MOVE {
        place(window);
        return 0;
    }
    // SAFETY: the arguments are the ones this procedure received.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

/// Moves the window to [`TARGET`], or hides it.
fn place(window: HWND) {
    let target = *TARGET.lock().unwrap();
    // SAFETY: calls on this thread's window and on plain structures.
    unsafe {
        let Some((x, y)) = target else {
            ShowWindow(window, SW_HIDE);
            return;
        };
        let monitor = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return;
        }
        let screen = info.rcMonitor;
        let (left, top) = (f64::from(screen.left), f64::from(screen.top));
        let width = f64::from(screen.right - screen.left);
        let height = f64::from(screen.bottom - screen.top);
        let size = diameter();
        let px = left + (x + 1.0) / 2.0 * width - f64::from(size) / 2.0;
        let py = top + (y + 1.0) / 2.0 * height - f64::from(size) / 2.0;
        SetWindowPos(
            window,
            HWND_TOPMOST,
            px.round() as i32,
            py.round() as i32,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

/// Makes the window, hidden, with the spot drawn into it. Null on failure.
fn create() -> HWND {
    let class = wide("WeftPresenterSpot");
    let size = diameter();
    // SAFETY: the class name outlives the calls; the bitmap is as large as
    // announced and stays selected into its DC for the window's life,
    // which is the process's.
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let mut class_info: WNDCLASSW = std::mem::zeroed();
        class_info.lpfnWndProc = Some(procedure);
        class_info.hInstance = instance;
        class_info.lpszClassName = class.as_ptr();
        RegisterClassW(&class_info);
        let window = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0,
            0,
            size,
            size,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if window.is_null() {
            tracing::warn!("could not make the presenter spot");
            return window;
        }
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            // Negative: top row first.
            biHeight: -size,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let screen = GetDC(std::ptr::null_mut());
        let memory = CreateCompatibleDC(screen);
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(
            memory,
            &info,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        );
        if !bitmap.is_null() && !bits.is_null() {
            let pixels = pixels(size);
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), pixels.len());
            SelectObject(memory, bitmap);
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let extent = SIZE { cx: size, cy: size };
            let origin = POINT { x: 0, y: 0 };
            UpdateLayeredWindow(
                window,
                screen,
                std::ptr::null(),
                &extent,
                memory,
                &origin,
                0,
                &blend,
                ULW_ALPHA,
            );
        }
        ReleaseDC(std::ptr::null_mut(), screen);
        window
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spot_is_a_disc_with_a_soft_edge() {
        let size = 28;
        let pixels = pixels(size);
        let alpha = |x: i32, y: i32| pixels[((y * size + x) * 4 + 3) as usize];
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(14, 14), (OPACITY * 255.0).round() as u8);
        let edge = alpha(4, 4);
        assert!(edge > 0 && edge < alpha(14, 14), "{edge}");
        // Premultiplied: no channel above its alpha.
        assert!(
            pixels
                .chunks(4)
                .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3])
        );
    }
}
