//! What the other devices make this computer do: [`Injector`] turns the
//! [`Input`]s the worker lets through into `SendInput` calls and the
//! presenter's spot, and [`run_command`] runs one of the user's commands.
//!
//! Windows keeps input from reaching a window that runs elevated (as
//! administrator) when this app does not: such a window just ignores it.

use std::process::Stdio;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEINPUT, MapVirtualKeyW,
    SendInput,
};

use crate::input::{Button, Input, Press, Remainder, Stroke, WHEEL_DELTA, strokes};
use crate::spot;

/// Hides the console `cmd` would open.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Sends input to this computer, keeping the fractions of pixels and
/// notches between calls.
#[derive(Default)]
pub struct Injector {
    motion: Remainder,
    wheel: Remainder,
}

fn mouse(dx: i32, dy: i32, data: i32, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                // A signed amount in an unsigned field, as Windows has it.
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn key(stroke: Stroke) -> INPUT {
    let (vk, scan, mut flags) = match stroke {
        Stroke::Key { key, extended, .. } => {
            // SAFETY: a pure lookup.
            let scan = unsafe { MapVirtualKeyW(key.into(), MAPVK_VK_TO_VSC) } as u16;
            let flags = if extended { KEYEVENTF_EXTENDEDKEY } else { 0 };
            (key, scan, flags)
        }
        Stroke::Unit { unit, .. } => (0, unit, KEYEVENTF_UNICODE),
    };
    if matches!(
        stroke,
        Stroke::Key { up: true, .. } | Stroke::Unit { up: true, .. }
    ) {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(inputs: &[INPUT]) {
    if inputs.is_empty() {
        return;
    }
    // SAFETY: the slice is as long as announced, of `INPUT`s of the size
    // given.
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
    if sent as usize != inputs.len() {
        tracing::debug!("input blocked: the window in front may run as administrator");
    }
}

impl Injector {
    pub fn apply(&mut self, input: Input) {
        match input {
            Input::Move { dx, dy } => {
                let (x, y) = self.motion.take(dx, dy);
                if x != 0 || y != 0 {
                    send(&[mouse(x, y, 0, MOUSEEVENTF_MOVE)]);
                }
            }
            Input::Scroll { x, y } => {
                // As the contract has it: positive `y` turns the wheel up
                // (away from the user), positive `x` scrolls right.
                let (horizontal, vertical) = self.wheel.take(x * WHEEL_DELTA, y * WHEEL_DELTA);
                let mut inputs = Vec::new();
                if vertical != 0 {
                    inputs.push(mouse(0, 0, vertical, MOUSEEVENTF_WHEEL));
                }
                if horizontal != 0 {
                    inputs.push(mouse(0, 0, horizontal, MOUSEEVENTF_HWHEEL));
                }
                send(&inputs);
            }
            Input::Click { button, press } => {
                let (down, up) = match button {
                    Button::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
                    Button::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
                    Button::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
                };
                let flags: &[u32] = match press {
                    Press::Click => &[down, up],
                    Press::Double => &[down, up, down, up],
                    Press::Down => &[down],
                    Press::Up => &[up],
                };
                let inputs: Vec<INPUT> = flags.iter().map(|&flag| mouse(0, 0, 0, flag)).collect();
                send(&inputs);
            }
            Input::Key { .. } | Input::Text { .. } => {
                let inputs: Vec<INPUT> = strokes(&input).into_iter().map(key).collect();
                send(&inputs);
            }
            Input::Spot(at) => spot::show(at),
        }
    }
}

/// Runs `line` as `cmd /C` would from the Run box, without a console
/// window, and does not wait for it.
pub fn run_command(line: &str) {
    use std::os::windows::process::CommandExt;

    let shell = std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into());
    let spawned = std::process::Command::new(shell)
        .arg("/C")
        // As typed: cmd parses its own command line.
        .raw_arg(line)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Err(err) = spawned {
        tracing::warn!(error = %err, "could not run a command");
    }
}
