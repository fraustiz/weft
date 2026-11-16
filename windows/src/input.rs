//! What the `input.*` and `presenter` packets of `docs/features.md`
//! section 8 ask this computer to do, decided apart from Windows so it can
//! be tested: [`Input`] is what a packet means, [`strokes`] the key presses
//! that type it, [`Remainder`] the fractions of pixels and notches kept
//! between packets. [`typed`] goes the other way: the packets that type on
//! the phone what changed in a text box.

use crate::packets::{InputClick, InputKey, InputPointer, Packet, Presenter};

/// The largest move or scroll one packet may ask for; more is noise.
const MAX_STEP: f64 = 10_000.0;
/// The longest text one packet may type, in characters.
const MAX_TEXT: usize = 1000;
/// One wheel notch, in the units Windows counts.
pub const WHEEL_DELTA: f64 = 120.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Click,
    Double,
    /// Held, for a drag, until [`Press::Up`].
    Down,
    Up,
}

/// The keys `input.key` names, and F5 for the presenter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Enter,
    Backspace,
    Tab,
    Escape,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    F5,
}

impl Key {
    /// The key of an `input.key` name; F5 has none.
    pub fn parse(name: &str) -> Option<Key> {
        Some(match name {
            "enter" => Key::Enter,
            "backspace" => Key::Backspace,
            "tab" => Key::Tab,
            "escape" => Key::Escape,
            "left" => Key::Left,
            "right" => Key::Right,
            "up" => Key::Up,
            "down" => Key::Down,
            "home" => Key::Home,
            "end" => Key::End,
            "pageUp" => Key::PageUp,
            "pageDown" => Key::PageDown,
            "delete" => Key::Delete,
            _ => return None,
        })
    }

    /// The Windows virtual key, and whether it is an extended key (the
    /// navigation block, which has twins on the numeric keypad).
    pub fn virtual_key(self) -> (u16, bool) {
        match self {
            Key::Enter => (VK_RETURN, false),
            Key::Backspace => (0x08, false),
            Key::Tab => (VK_TAB, false),
            Key::Escape => (0x1B, false),
            Key::Left => (0x25, true),
            Key::Right => (0x27, true),
            Key::Up => (0x26, true),
            Key::Down => (0x28, true),
            Key::Home => (0x24, true),
            Key::End => (0x23, true),
            Key::PageUp => (0x21, true),
            Key::PageDown => (0x22, true),
            Key::Delete => (0x2E, true),
            Key::F5 => (0x74, false),
        }
    }
}

const VK_RETURN: u16 = 0x0D;
const VK_TAB: u16 = 0x09;
const VK_SHIFT: u16 = 0x10;
const VK_CONTROL: u16 = 0x11;
const VK_MENU: u16 = 0x12;
const VK_LWIN: u16 = 0x5B;

/// The modifiers held around a key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl Modifiers {
    fn of(key: &InputKey) -> Modifiers {
        Modifiers {
            ctrl: key.ctrl,
            alt: key.alt,
            shift: key.shift,
            meta: key.meta,
        }
    }

    /// The modifiers' virtual keys, in the order they go down.
    fn keys(self) -> Vec<(u16, bool)> {
        [
            (self.ctrl, (VK_CONTROL, false)),
            (self.alt, (VK_MENU, false)),
            (self.shift, (VK_SHIFT, false)),
            (self.meta, (VK_LWIN, true)),
        ]
        .into_iter()
        .filter_map(|(held, key)| held.then_some(key))
        .collect()
    }
}

/// What a device asks this computer to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// Move the pointer by this many pixels.
    Move {
        dx: f64,
        dy: f64,
    },
    /// Turn the wheels by this many notches: `y` positive turns the wheel
    /// up (away from the user), `x` positive scrolls right.
    Scroll {
        x: f64,
        y: f64,
    },
    Click {
        button: Button,
        press: Press,
    },
    Key {
        key: Key,
        modifiers: Modifiers,
    },
    Text {
        text: String,
        modifiers: Modifiers,
    },
    /// Show the presenter's spot at this place, -1 to 1 on each axis of
    /// the screen, or hide it.
    Spot(Option<(f64, f64)>),
}

/// A step that is a number this side of [`MAX_STEP`], or nothing.
fn step(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && value.abs() <= MAX_STEP)
}

fn pointer(pointer: &InputPointer) -> Vec<Input> {
    let mut inputs = Vec::new();
    let (dx, dy) = (step(pointer.dx), step(pointer.dy));
    if dx.is_some() || dy.is_some() {
        inputs.push(Input::Move {
            dx: dx.unwrap_or(0.0),
            dy: dy.unwrap_or(0.0),
        });
    }
    let (x, y) = (step(pointer.scroll_x), step(pointer.scroll_y));
    if x.is_some() || y.is_some() {
        inputs.push(Input::Scroll {
            x: x.unwrap_or(0.0),
            y: y.unwrap_or(0.0),
        });
    }
    inputs
}

fn click(click: &InputClick) -> Option<Input> {
    let button = match click.button.as_str() {
        "left" => Button::Left,
        "right" => Button::Right,
        "middle" => Button::Middle,
        _ => return None,
    };
    let press = if click.down {
        Press::Down
    } else if click.up {
        Press::Up
    } else if click.double {
        Press::Double
    } else {
        Press::Click
    };
    Some(Input::Click { button, press })
}

fn key(packet: &InputKey) -> Option<Input> {
    let modifiers = Modifiers::of(packet);
    // A named key wins over text, as on the phone.
    if let Some(key) = packet.key.as_deref().and_then(Key::parse) {
        return Some(Input::Key { key, modifiers });
    }
    let text: String = packet.text.as_deref()?.chars().take(MAX_TEXT).collect();
    (!text.is_empty()).then_some(Input::Text { text, modifiers })
}

/// The key a presenter action stands for: the arrows move between
/// slides, F5 starts the show and Escape ends it, in PowerPoint, Impress
/// and the browsers' slide decks alike.
pub fn presenter_key(action: &str) -> Option<Key> {
    Some(match action {
        "next" => Key::Right,
        "previous" => Key::Left,
        "start" => Key::F5,
        "stop" => Key::Escape,
        _ => return None,
    })
}

fn presenter(presenter: &Presenter) -> Option<Input> {
    if let Some(action) = &presenter.action {
        return presenter_key(action).map(|key| Input::Key {
            key,
            modifiers: Modifiers::default(),
        });
    }
    match presenter.pointer? {
        None => Some(Input::Spot(None)),
        Some(spot) if spot.x.is_finite() && spot.y.is_finite() => Some(Input::Spot(Some((
            spot.x.clamp(-1.0, 1.0),
            spot.y.clamp(-1.0, 1.0),
        )))),
        Some(_) => None,
    }
}

/// What an `input.*` or `presenter` packet asks for; nothing for any other
/// packet or one that makes no sense.
pub fn of_packet(packet: &Packet) -> Vec<Input> {
    match packet {
        Packet::InputPointer(packet) => pointer(packet),
        Packet::InputClick(packet) => click(packet).into_iter().collect(),
        Packet::InputKey(packet) => key(packet).into_iter().collect(),
        Packet::Presenter(packet) => presenter(packet).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// One key going down or up: a virtual key, or a UTF-16 unit typed as it
/// is (`KEYEVENTF_UNICODE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stroke {
    Key { key: u16, extended: bool, up: bool },
    Unit { unit: u16, up: bool },
}

fn press(key: (u16, bool), strokes: &mut Vec<Stroke>) {
    let (key, extended) = key;
    strokes.push(Stroke::Key {
        key,
        extended,
        up: false,
    });
    strokes.push(Stroke::Key {
        key,
        extended,
        up: true,
    });
}

/// `inner` with the modifiers pressed before it and released after it.
fn held(modifiers: Modifiers, inner: Vec<Stroke>) -> Vec<Stroke> {
    let keys = modifiers.keys();
    let down = keys.iter().map(|&(key, extended)| Stroke::Key {
        key,
        extended,
        up: false,
    });
    let up = keys.iter().rev().map(|&(key, extended)| Stroke::Key {
        key,
        extended,
        up: true,
    });
    down.chain(inner).chain(up).collect()
}

/// Text as key strokes: line breaks and tabs as their keys, everything
/// else as UTF-16 units, the two halves of a surrogate pair down together
/// and then up together so the pair reaches the window as one character.
fn text_strokes(text: &str) -> Vec<Stroke> {
    let mut strokes = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                // A Windows line break is one Enter.
                chars.next_if_eq(&'\n');
                press((VK_RETURN, false), &mut strokes);
            }
            '\n' => press((VK_RETURN, false), &mut strokes),
            '\t' => press((VK_TAB, false), &mut strokes),
            c if c.is_control() => {}
            c => {
                let mut units = [0; 2];
                let units = c.encode_utf16(&mut units);
                for up in [false, true] {
                    for unit in units.iter() {
                        strokes.push(Stroke::Unit { unit: *unit, up });
                    }
                }
            }
        }
    }
    strokes
}

/// The strokes that type a [`Input::Key`] or [`Input::Text`]; nothing for
/// the other inputs.
///
/// Typed text ignores the modifiers, except one letter or digit with Ctrl,
/// Alt or the Windows key: that is a shortcut, sent as its key so the
/// application sees Ctrl+A rather than a character.
pub fn strokes(input: &Input) -> Vec<Stroke> {
    match input {
        Input::Key { key, modifiers } => {
            let mut inner = Vec::new();
            press(key.virtual_key(), &mut inner);
            held(*modifiers, inner)
        }
        Input::Text { text, modifiers } => {
            let shortcut = modifiers.ctrl || modifiers.alt || modifiers.meta;
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if shortcut && c.is_ascii_alphanumeric() => {
                    let mut inner = Vec::new();
                    press((c.to_ascii_uppercase() as u16, false), &mut inner);
                    held(*modifiers, inner)
                }
                _ => text_strokes(text),
            }
        }
        _ => Vec::new(),
    }
}

/// The fractions left over from moves or scrolls, so that many small
/// steps add up to whole pixels or wheel units instead of being lost.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Remainder {
    x: f64,
    y: f64,
}

impl Remainder {
    /// Adds a step, and takes out the whole part of what is owed.
    pub fn take(&mut self, x: f64, y: f64) -> (i32, i32) {
        fn whole(owed: &mut f64, step: f64) -> i32 {
            let total = *owed + step;
            let whole = total.trunc();
            *owed = total - whole;
            whole as i32
        }
        (whole(&mut self.x, x), whole(&mut self.y, y))
    }
}

/// The `input.key` packets that turn `old` into `new` on the other side,
/// where the cursor stays at the end: a backspace per character removed
/// after what the two share, then the new text, line breaks as Enter.
pub fn typed(old: &str, new: &str) -> Vec<Packet> {
    let common = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let removed = old.chars().count() - common;
    let mut packets: Vec<Packet> = (0..removed)
        .map(|_| Packet::InputKey(InputKey::key("backspace")))
        .collect();
    let mut text = String::new();
    let flush = |text: &mut String, packets: &mut Vec<Packet>| {
        if !text.is_empty() {
            packets.push(Packet::InputKey(InputKey::text(std::mem::take(text))));
        }
    };
    let mut chars = new.chars().skip(common).peekable();
    while let Some(c) = chars.next() {
        if c == '\r' || c == '\n' {
            if c == '\r' {
                chars.next_if_eq(&'\n');
            }
            flush(&mut text, &mut packets);
            packets.push(Packet::InputKey(InputKey::key("enter")));
        } else {
            text.push(c);
        }
    }
    flush(&mut text, &mut packets);
    packets
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packets::Spot;

    fn down(key: u16) -> Stroke {
        Stroke::Key {
            key,
            extended: false,
            up: false,
        }
    }

    fn up(key: u16) -> Stroke {
        Stroke::Key {
            key,
            extended: false,
            up: true,
        }
    }

    #[test]
    fn packets_become_inputs() {
        let move_and_scroll = Packet::InputPointer(InputPointer {
            dx: Some(2.5),
            dy: None,
            scroll_x: None,
            scroll_y: Some(-1.0),
        });
        assert_eq!(
            of_packet(&move_and_scroll),
            [
                Input::Move { dx: 2.5, dy: 0.0 },
                Input::Scroll { x: 0.0, y: -1.0 }
            ]
        );
        let wild = Packet::InputPointer(InputPointer {
            dx: Some(f64::NAN),
            dy: Some(1e9),
            ..InputPointer::default()
        });
        assert!(of_packet(&wild).is_empty());

        let click = |button: &str, double, down, up| {
            of_packet(&Packet::InputClick(InputClick {
                button: button.into(),
                double,
                down,
                up,
            }))
        };
        assert_eq!(
            click("right", false, false, false),
            [Input::Click {
                button: Button::Right,
                press: Press::Click
            }]
        );
        assert_eq!(
            click("left", true, false, false),
            [Input::Click {
                button: Button::Left,
                press: Press::Double
            }]
        );
        assert_eq!(
            click("middle", false, true, false),
            [Input::Click {
                button: Button::Middle,
                press: Press::Down
            }]
        );
        assert!(click("thumb", false, false, false).is_empty());

        let key = InputKey {
            key: Some("pageDown".into()),
            text: Some("ignored".into()),
            shift: true,
            ..InputKey::default()
        };
        assert_eq!(
            of_packet(&Packet::InputKey(key)),
            [Input::Key {
                key: Key::PageDown,
                modifiers: Modifiers {
                    shift: true,
                    ..Modifiers::default()
                }
            }]
        );
        assert!(of_packet(&Packet::InputKey(InputKey::text(""))).is_empty());
        assert!(of_packet(&Packet::InputKey(InputKey::key("f13"))).is_empty());
    }

    #[test]
    fn presenter_actions_are_keys_and_the_spot_is_clamped() {
        let action = |name: &str| {
            of_packet(&Packet::Presenter(Presenter {
                action: Some(name.into()),
                pointer: None,
            }))
        };
        for (name, key) in [
            ("next", Key::Right),
            ("previous", Key::Left),
            ("start", Key::F5),
            ("stop", Key::Escape),
        ] {
            assert_eq!(
                action(name),
                [Input::Key {
                    key,
                    modifiers: Modifiers::default()
                }]
            );
        }
        assert!(action("blackout").is_empty());
        let spot = |pointer| {
            of_packet(&Packet::Presenter(Presenter {
                action: None,
                pointer,
            }))
        };
        assert_eq!(
            spot(Some(Some(Spot { x: 2.0, y: -0.5 }))),
            [Input::Spot(Some((1.0, -0.5)))]
        );
        assert_eq!(spot(Some(None)), [Input::Spot(None)]);
        assert!(spot(None).is_empty());
    }

    #[test]
    fn keys_are_pressed_inside_their_modifiers() {
        let input = Input::Key {
            key: Key::Left,
            modifiers: Modifiers {
                ctrl: true,
                shift: true,
                ..Modifiers::default()
            },
        };
        let arrow = |up| Stroke::Key {
            key: 0x25,
            extended: true,
            up,
        };
        assert_eq!(
            strokes(&input),
            [
                down(VK_CONTROL),
                down(VK_SHIFT),
                arrow(false),
                arrow(true),
                up(VK_SHIFT),
                up(VK_CONTROL)
            ]
        );
        // Ctrl with one letter is a shortcut: the letter's key.
        let select_all = Input::Text {
            text: "a".into(),
            modifiers: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        };
        assert_eq!(
            strokes(&select_all),
            [
                down(VK_CONTROL),
                down(b'A'.into()),
                up(b'A'.into()),
                up(VK_CONTROL)
            ]
        );
    }

    #[test]
    fn text_is_typed_as_units_and_surrogate_pairs_stay_together() {
        let text = |text: &str| {
            strokes(&Input::Text {
                text: text.into(),
                modifiers: Modifiers::default(),
            })
        };
        let unit = |unit, up| Stroke::Unit { unit, up };
        assert_eq!(text("é"), [unit(0xE9, false), unit(0xE9, true)]);
        // U+1F600 is D83D DE00 in UTF-16.
        assert_eq!(
            text("😀"),
            [
                unit(0xD83D, false),
                unit(0xDE00, false),
                unit(0xD83D, true),
                unit(0xDE00, true)
            ]
        );
        // Line breaks are Enter, once for CR LF; other controls are dropped.
        assert_eq!(
            text("a\r\nb\u{7}"),
            [
                unit(b'a'.into(), false),
                unit(b'a'.into(), true),
                down(VK_RETURN),
                up(VK_RETURN),
                unit(b'b'.into(), false),
                unit(b'b'.into(), true)
            ]
        );
        // Shift alone does not make a shortcut.
        let shifted = strokes(&Input::Text {
            text: "a".into(),
            modifiers: Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        });
        assert_eq!(shifted, [unit(b'a'.into(), false), unit(b'a'.into(), true)]);
        assert!(strokes(&Input::Move { dx: 1.0, dy: 1.0 }).is_empty());
    }

    #[test]
    fn fractions_add_up() {
        let mut remainder = Remainder::default();
        assert_eq!(remainder.take(0.4, -0.4), (0, 0));
        assert_eq!(remainder.take(0.4, -0.4), (0, 0));
        assert_eq!(remainder.take(0.4, -0.4), (1, -1));
        assert_eq!(remainder.take(2.0, 0.0), (2, 0));
        // A third of a notch three times is one notch.
        let mut wheel = Remainder::default();
        let third = WHEEL_DELTA / 3.0 + 1e-9;
        let total: i32 = (0..3).map(|_| wheel.take(0.0, third).1).sum();
        assert_eq!(total, 120);
    }

    #[test]
    fn typing_sends_what_changed() {
        let text = |text: &str| Packet::InputKey(InputKey::text(text));
        let backspace = Packet::InputKey(InputKey::key("backspace"));
        let enter = Packet::InputKey(InputKey::key("enter"));
        assert_eq!(typed("", "Hello"), [text("Hello")]);
        assert_eq!(typed("Hello", "Hello!"), [text("!")]);
        assert_eq!(
            typed("Hello", "Hel"),
            [backspace.clone(), backspace.clone()]
        );
        // A change in the middle retypes from there.
        assert_eq!(typed("café", "cafe"), [backspace.clone(), text("e")]);
        assert_eq!(
            typed("Hi", "Hi\rthere\r\n"),
            [enter.clone(), text("there"), enter]
        );
        assert!(typed("same", "same").is_empty());
        assert_eq!(typed("😀", ""), [backspace]);
    }
}
