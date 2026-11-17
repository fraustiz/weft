//! Remote pointer, scroll, click, Unicode text, modified special keys, and
//! presenter shortcuts via `ydotool` / `wtype` / `xdotool`, plus detached
//! execution of user-configured shortcut commands.
//!
//! Typed text is always piped via `stdin`, never interpolated into a shell or
//! exposed in `argv`.

use std::process::Stdio;
use std::sync::Arc;

use weft_client::input::{Button, Input, Key, Modifiers, Press, Remainder};

use crate::clipboard::DisplayEnv;
use crate::runner::{Runner, SystemRunner};

/// Detected pointer tool on Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerTool {
    Ydotool,
    Xdotool,
    Unavailable,
}

/// Detected keyboard/text tool on Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardTool {
    Wtype,
    Ydotool,
    Xdotool,
    Unavailable,
}

pub fn detect_tools(env: &DisplayEnv, runner: &dyn Runner) -> (PointerTool, KeyboardTool) {
    if env.wayland_display.is_some() {
        let pointer = if runner.has_program("ydotool") {
            PointerTool::Ydotool
        } else if env.display.is_some() && runner.has_program("xdotool") {
            PointerTool::Xdotool
        } else {
            PointerTool::Unavailable
        };
        let keyboard = if runner.has_program("wtype") {
            KeyboardTool::Wtype
        } else if runner.has_program("ydotool") {
            KeyboardTool::Ydotool
        } else if env.display.is_some() && runner.has_program("xdotool") {
            KeyboardTool::Xdotool
        } else {
            KeyboardTool::Unavailable
        };
        return (pointer, keyboard);
    }

    if env.display.is_some() {
        let pointer = if runner.has_program("xdotool") {
            PointerTool::Xdotool
        } else if runner.has_program("ydotool") {
            PointerTool::Ydotool
        } else {
            PointerTool::Unavailable
        };
        let keyboard = if runner.has_program("xdotool") {
            KeyboardTool::Xdotool
        } else if runner.has_program("ydotool") {
            KeyboardTool::Ydotool
        } else {
            KeyboardTool::Unavailable
        };
        return (pointer, keyboard);
    }

    (PointerTool::Unavailable, KeyboardTool::Unavailable)
}

fn x11_keysym(key: Key) -> &'static str {
    match key {
        Key::Enter => "Return",
        Key::Backspace => "BackSpace",
        Key::Tab => "Tab",
        Key::Escape => "Escape",
        Key::Left => "Left",
        Key::Right => "Right",
        Key::Up => "Up",
        Key::Down => "Down",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "Prior",
        Key::PageDown => "Next",
        Key::Delete => "Delete",
        Key::F5 => "F5",
    }
}

/// Linux evdev keycodes (`linux/input-event-codes.h`) for `ydotool key`.
fn evdev_keycode(key: Key) -> u16 {
    match key {
        Key::Escape => 1,
        Key::Backspace => 14,
        Key::Tab => 15,
        Key::Enter => 28,
        Key::F5 => 63,
        Key::Home => 102,
        Key::Up => 103,
        Key::PageUp => 104,
        Key::Left => 105,
        Key::Right => 106,
        Key::End => 107,
        Key::Down => 108,
        Key::PageDown => 109,
        Key::Delete => 111,
    }
}

fn evdev_ascii_keycode(ch: char) -> Option<u16> {
    Some(match ch.to_ascii_lowercase() {
        '1' => 2,
        '2' => 3,
        '3' => 4,
        '4' => 5,
        '5' => 6,
        '6' => 7,
        '7' => 8,
        '8' => 9,
        '9' => 10,
        '0' => 11,
        'q' => 16,
        'w' => 17,
        'e' => 18,
        'r' => 19,
        't' => 20,
        'y' => 21,
        'u' => 22,
        'i' => 23,
        'o' => 24,
        'p' => 25,
        'a' => 30,
        's' => 31,
        'd' => 32,
        'f' => 33,
        'g' => 34,
        'h' => 35,
        'j' => 36,
        'k' => 37,
        'l' => 38,
        'z' => 44,
        'x' => 45,
        'c' => 46,
        'v' => 47,
        'b' => 48,
        'n' => 49,
        'm' => 50,
        _ => return None,
    })
}

/// Stateful input injector keeping fractional pointer and scroll remainders.
pub struct InputInjector {
    pointer: PointerTool,
    keyboard: KeyboardTool,
    runner: Arc<dyn Runner>,
    motion: Remainder,
    wheel: Remainder,
}

impl InputInjector {
    pub fn detect() -> Self {
        Self::with_runner(&DisplayEnv::from_env(), Arc::new(SystemRunner))
    }

    pub fn with_runner(env: &DisplayEnv, runner: Arc<dyn Runner>) -> Self {
        let (pointer, keyboard) = detect_tools(env, runner.as_ref());
        Self {
            pointer,
            keyboard,
            runner,
            motion: Remainder::default(),
            wheel: Remainder::default(),
        }
    }

    pub fn tools(&self) -> (PointerTool, KeyboardTool) {
        (self.pointer, self.keyboard)
    }

    pub fn apply(&mut self, input: Input) {
        match input {
            Input::Move { dx, dy } => {
                let (x, y) = self.motion.take(dx, dy);
                if x != 0 || y != 0 {
                    self.move_pointer(x, y);
                }
            }
            Input::Scroll { x, y } => {
                let (hx, vy) = self.wheel.take(x, y);
                if hx != 0 || vy != 0 {
                    self.scroll(hx, vy);
                }
            }
            Input::Click { button, press } => self.click(button, press),
            Input::Key { key, modifiers } => {
                self.press_keysym(x11_keysym(key), evdev_keycode(key), modifiers);
            }
            Input::Text { text, modifiers } => self.type_text(&text, modifiers),
            Input::Spot(_) => {}
        }
    }

    fn move_pointer(&self, dx: i32, dy: i32) {
        let (xs, ys) = (dx.to_string(), dy.to_string());
        match self.pointer {
            PointerTool::Ydotool => {
                let _ =
                    self.runner
                        .run_quiet("ydotool", &["mousemove", "-x", &xs, "-y", &ys], None);
            }
            PointerTool::Xdotool => {
                let _ =
                    self.runner
                        .run_quiet("xdotool", &["mousemove_relative", "--", &xs, &ys], None);
            }
            PointerTool::Unavailable => {}
        }
    }

    fn scroll(&self, hx: i32, vy: i32) {
        match self.pointer {
            PointerTool::Ydotool => {
                let (xs, ys) = (hx.to_string(), vy.to_string());
                let _ = self.runner.run_quiet(
                    "ydotool",
                    &["mousemove", "-w", "-x", &xs, "-y", &ys],
                    None,
                );
            }
            PointerTool::Xdotool => {
                // Positive `vy` scrolls up (button 4), negative down (button 5).
                if vy != 0 {
                    let btn = if vy > 0 { "4" } else { "5" };
                    let count = vy.unsigned_abs().to_string();
                    let _ =
                        self.runner
                            .run_quiet("xdotool", &["click", "--repeat", &count, btn], None);
                }
                // Positive `hx` scrolls right (button 7), negative left (button 6).
                if hx != 0 {
                    let btn = if hx > 0 { "7" } else { "6" };
                    let count = hx.unsigned_abs().to_string();
                    let _ =
                        self.runner
                            .run_quiet("xdotool", &["click", "--repeat", &count, btn], None);
                }
            }
            PointerTool::Unavailable => {}
        }
    }

    fn click(&self, button: Button, press: Press) {
        match self.pointer {
            PointerTool::Ydotool => {
                let low = match button {
                    Button::Left => 0x00u8,
                    Button::Right => 0x01u8,
                    Button::Middle => 0x02u8,
                };
                let mask = match press {
                    Press::Click | Press::Double => 0xC0u8 | low,
                    Press::Down => 0x40u8 | low,
                    Press::Up => 0x80u8 | low,
                };
                let code = format!("0x{mask:02X}");
                if press == Press::Double {
                    let _ =
                        self.runner
                            .run_quiet("ydotool", &["click", "--repeat", "2", &code], None);
                } else {
                    let _ = self.runner.run_quiet("ydotool", &["click", &code], None);
                }
            }
            PointerTool::Xdotool => {
                let btn = match button {
                    Button::Left => "1",
                    Button::Middle => "2",
                    Button::Right => "3",
                };
                match press {
                    Press::Click => {
                        let _ = self.runner.run_quiet("xdotool", &["click", btn], None);
                    }
                    Press::Double => {
                        let _ = self.runner.run_quiet(
                            "xdotool",
                            &["click", "--repeat", "2", "--delay", "50", btn],
                            None,
                        );
                    }
                    Press::Down => {
                        let _ = self.runner.run_quiet("xdotool", &["mousedown", btn], None);
                    }
                    Press::Up => {
                        let _ = self.runner.run_quiet("xdotool", &["mouseup", btn], None);
                    }
                }
            }
            PointerTool::Unavailable => {}
        }
    }

    fn press_keysym(&self, keysym: &str, evdev: u16, modifiers: Modifiers) {
        match self.keyboard {
            KeyboardTool::Wtype => {
                let mut args: Vec<&str> = Vec::new();
                let mods = [
                    (modifiers.ctrl, "ctrl"),
                    (modifiers.alt, "alt"),
                    (modifiers.shift, "shift"),
                    (modifiers.meta, "logo"),
                ];
                for &(held, name) in &mods {
                    if held {
                        args.extend(["-M", name]);
                    }
                }
                args.extend(["-k", keysym]);
                for &(held, name) in mods.iter().rev() {
                    if held {
                        args.extend(["-m", name]);
                    }
                }
                let _ = self.runner.run_quiet("wtype", &args, None);
            }
            KeyboardTool::Ydotool => {
                let mut seq: Vec<String> = Vec::new();
                let mods = [
                    (modifiers.ctrl, 29u16),
                    (modifiers.alt, 56u16),
                    (modifiers.shift, 42u16),
                    (modifiers.meta, 125u16),
                ];
                for &(held, code) in &mods {
                    if held {
                        seq.push(format!("{code}:1"));
                    }
                }
                seq.push(format!("{evdev}:1"));
                seq.push(format!("{evdev}:0"));
                for &(held, code) in mods.iter().rev() {
                    if held {
                        seq.push(format!("{code}:0"));
                    }
                }
                let mut args = vec!["key"];
                args.extend(seq.iter().map(String::as_str));
                let _ = self.runner.run_quiet("ydotool", &args, None);
            }
            KeyboardTool::Xdotool => {
                let mut chord = Vec::new();
                if modifiers.ctrl {
                    chord.push("ctrl");
                }
                if modifiers.alt {
                    chord.push("alt");
                }
                if modifiers.shift {
                    chord.push("shift");
                }
                if modifiers.meta {
                    chord.push("super");
                }
                chord.push(keysym);
                let combo = chord.join("+");
                let _ = self.runner.run_quiet(
                    "xdotool",
                    &["key", "--clearmodifiers", "--", &combo],
                    None,
                );
            }
            KeyboardTool::Unavailable => {}
        }
    }

    fn type_text(&self, text: &str, modifiers: Modifiers) {
        if text.is_empty() {
            return;
        }
        // Single ASCII letter/digit with Ctrl/Alt/Meta is a keyboard shortcut (e.g. Ctrl+A).
        let shortcut = modifiers.ctrl || modifiers.alt || modifiers.meta;
        let mut chars = text.chars();
        if let (Some(ch), None) = (chars.next(), chars.next())
            && shortcut
            && ch.is_ascii_alphanumeric()
        {
            let lower = ch.to_ascii_lowercase().to_string();
            let evdev = evdev_ascii_keycode(ch).unwrap_or(0);
            if evdev != 0 || self.keyboard != KeyboardTool::Ydotool {
                self.press_keysym(&lower, evdev, modifiers);
                return;
            }
        }

        let bytes = text.as_bytes();
        match self.keyboard {
            KeyboardTool::Wtype => {
                let _ = self.runner.run_quiet("wtype", &["-"], Some(bytes));
            }
            KeyboardTool::Ydotool => {
                let _ = self
                    .runner
                    .run_quiet("ydotool", &["type", "--file", "-"], Some(bytes));
            }
            KeyboardTool::Xdotool => {
                let _ = self.runner.run_quiet(
                    "xdotool",
                    &["type", "--clearmodifiers", "--delay", "0", "--file", "-"],
                    Some(bytes),
                );
            }
            KeyboardTool::Unavailable => {}
        }
    }
}

/// Runs a user-configured shortcut command asynchronously without logging the
/// command line itself.
pub fn run_shortcut_command(line: &str) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    #[cfg(unix)]
    let spawned = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(line)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    #[cfg(windows)]
    let spawned = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let shell = std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into());
        std::process::Command::new(shell)
            .arg("/C")
            .raw_arg(line)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    };

    #[cfg(not(any(unix, windows)))]
    let spawned = std::process::Command::new("sh")
        .arg("-c")
        .arg(line)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    match spawned {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(err) => tracing::warn!(error = %err, "could not run user shortcut command"),
    }
}

#[cfg(test)]
mod tests {
    use weft_client::input::presenter_key;

    use super::*;
    use crate::runner::FakeRunner;

    #[test]
    fn wayland_uses_ydotool_and_wtype_with_stdin_for_text() {
        let runner = Arc::new(FakeRunner::new(&["ydotool", "wtype"]));
        let mut injector = InputInjector::with_runner(&DisplayEnv::wayland(), runner.clone());
        assert_eq!(
            injector.tools(),
            (PointerTool::Ydotool, KeyboardTool::Wtype)
        );

        // Fractional steps accumulate before dispatching.
        injector.apply(Input::Move { dx: 0.6, dy: -0.6 });
        assert!(runner.calls().is_empty());
        injector.apply(Input::Move { dx: 0.6, dy: -0.6 });

        injector.apply(Input::Click {
            button: Button::Left,
            press: Press::Double,
        });
        injector.apply(Input::Key {
            key: Key::Left,
            modifiers: Modifiers {
                ctrl: true,
                shift: true,
                ..Modifiers::default()
            },
        });
        injector.apply(Input::Text {
            text: "Bonjour émoji 🎉".into(),
            modifiers: Modifiers::default(),
        });
        // Presenter actions map to F5, Right, Left, Escape.
        for action in ["start", "next", "previous", "stop"] {
            injector.apply(Input::Key {
                key: presenter_key(action).unwrap(),
                modifiers: Modifiers::default(),
            });
        }

        let calls = runner.calls();
        assert_eq!(calls[0].program, "ydotool");
        assert_eq!(calls[0].args, ["mousemove", "-x", "1", "-y", "-1"]);
        assert_eq!(calls[1].args, ["click", "--repeat", "2", "0xC0"]);
        assert_eq!(calls[2].program, "wtype");
        assert_eq!(
            calls[2].args,
            [
                "-M", "ctrl", "-M", "shift", "-k", "Left", "-m", "shift", "-m", "ctrl"
            ]
        );
        // Unicode text goes via stdin (`wtype -`), never in argv!
        assert_eq!(calls[3].program, "wtype");
        assert_eq!(calls[3].args, ["-"]);
        assert_eq!(
            calls[3].stdin.as_deref(),
            Some("Bonjour émoji 🎉".as_bytes())
        );
        assert_eq!(calls[4].args, ["-k", "F5"]);
        assert_eq!(calls[5].args, ["-k", "Right"]);
        assert_eq!(calls[6].args, ["-k", "Left"]);
        assert_eq!(calls[7].args, ["-k", "Escape"]);
    }

    #[test]
    fn x11_uses_xdotool_for_pointer_scroll_shortcuts_and_stdin_text() {
        let runner = Arc::new(FakeRunner::new(&["xdotool"]));
        let mut injector = InputInjector::with_runner(&DisplayEnv::x11(), runner.clone());
        assert_eq!(
            injector.tools(),
            (PointerTool::Xdotool, KeyboardTool::Xdotool)
        );

        injector.apply(Input::Move { dx: -12.0, dy: 8.0 });
        injector.apply(Input::Scroll { x: -1.0, y: 2.0 });
        injector.apply(Input::Text {
            text: "c".into(),
            modifiers: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        });
        injector.apply(Input::Text {
            text: "--secret-looking-text".into(),
            modifiers: Modifiers::default(),
        });

        let calls = runner.calls();
        assert_eq!(calls[0].args, ["mousemove_relative", "--", "-12", "8"]);
        assert_eq!(calls[1].args, ["click", "--repeat", "2", "4"]);
        assert_eq!(calls[2].args, ["click", "--repeat", "1", "6"]);
        assert_eq!(calls[3].args, ["key", "--clearmodifiers", "--", "ctrl+c"]);
        assert_eq!(
            calls[4].args,
            ["type", "--clearmodifiers", "--delay", "0", "--file", "-"]
        );
        assert_eq!(
            calls[4].stdin.as_deref(),
            Some("--secret-looking-text".as_bytes())
        );
    }
}
