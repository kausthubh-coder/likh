//! Picking a new shortcut by pressing it, in onboarding and in settings.
//!
//! While capturing, the window shows which modifiers are down right now; the
//! first non-modifier key locks the shortcut in. Esc backs out. The current
//! shortcut is paused meanwhile, so pressing it reaches the window too.

use super::{Binding, KeyName};
use iced::keyboard::{self, Modifiers, key::Code, key::Physical};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Still waiting; these modifiers are down.
    Waiting(Modifiers),
    Chosen(Binding),
    Cancelled,
    /// A key that can't make a shortcut, and why.
    Refused(&'static str),
}

/// Reads one window keyboard event while capturing.
pub fn on_event(event: &keyboard::Event, held: Modifiers) -> Outcome {
    match event {
        keyboard::Event::ModifiersChanged(modifiers) => Outcome::Waiting(*modifiers),
        keyboard::Event::KeyPressed { physical_key: Physical::Code(Code::Escape), .. } => Outcome::Cancelled,
        keyboard::Event::KeyPressed { physical_key: Physical::Code(code), modifiers, .. } => match key_for(*code) {
            Some(key) => Binding::new(modifiers.control(), modifiers.alt(), modifiers.shift(), modifiers.logo(), key)
                .map_or_else(Outcome::Refused, Outcome::Chosen),
            None if is_modifier(*code) => Outcome::Waiting(*modifiers),
            None => Outcome::Refused("That key can't be part of a shortcut. Try a letter, a number or Space."),
        },
        _ => Outcome::Waiting(held),
    }
}

/// What the key caps say while waiting: the modifiers held so far.
pub fn held_parts(held: Modifiers) -> Vec<&'static str> {
    [(held.control(), "Ctrl"), (held.alt(), "Alt"), (held.shift(), "Shift"), (held.logo(), "Win")]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect()
}

fn is_modifier(code: Code) -> bool {
    matches!(
        code,
        Code::ControlLeft
            | Code::ControlRight
            | Code::AltLeft
            | Code::AltRight
            | Code::ShiftLeft
            | Code::ShiftRight
            | Code::SuperLeft
            | Code::SuperRight
    )
}

/// The physical key as one of likh's key names (layout-independent: the key
/// in the A position is "A" on any keyboard layout).
pub fn key_for(code: Code) -> Option<KeyName> {
    let name = match code {
        Code::Space => "Space",
        Code::Enter => "Enter",
        Code::Tab => "Tab",
        Code::Backspace => "Backspace",
        Code::Insert => "Insert",
        Code::Delete => "Delete",
        Code::Home => "Home",
        Code::End => "End",
        Code::PageUp => "PageUp",
        Code::PageDown => "PageDown",
        Code::KeyA => "A",
        Code::KeyB => "B",
        Code::KeyC => "C",
        Code::KeyD => "D",
        Code::KeyE => "E",
        Code::KeyF => "F",
        Code::KeyG => "G",
        Code::KeyH => "H",
        Code::KeyI => "I",
        Code::KeyJ => "J",
        Code::KeyK => "K",
        Code::KeyL => "L",
        Code::KeyM => "M",
        Code::KeyN => "N",
        Code::KeyO => "O",
        Code::KeyP => "P",
        Code::KeyQ => "Q",
        Code::KeyR => "R",
        Code::KeyS => "S",
        Code::KeyT => "T",
        Code::KeyU => "U",
        Code::KeyV => "V",
        Code::KeyW => "W",
        Code::KeyX => "X",
        Code::KeyY => "Y",
        Code::KeyZ => "Z",
        Code::Digit0 => "0",
        Code::Digit1 => "1",
        Code::Digit2 => "2",
        Code::Digit3 => "3",
        Code::Digit4 => "4",
        Code::Digit5 => "5",
        Code::Digit6 => "6",
        Code::Digit7 => "7",
        Code::Digit8 => "8",
        Code::Digit9 => "9",
        Code::F1 => "F1",
        Code::F2 => "F2",
        Code::F3 => "F3",
        Code::F4 => "F4",
        Code::F5 => "F5",
        Code::F6 => "F6",
        Code::F7 => "F7",
        Code::F8 => "F8",
        Code::F9 => "F9",
        Code::F10 => "F10",
        Code::F11 => "F11",
        Code::F12 => "F12",
        _ => return None,
    };
    KeyName::find(name)
}
