//! The global shortcut: hold Ctrl+Win (Ctrl+Cmd on macOS) to talk, Esc to cancel.
//!
//! `Keys::on_key` is the whole decision as a pure function of key events, so
//! it is tested here. On Windows a low-level keyboard hook feeds it
//! (`windows.rs`); the hook thread sleeps until a key moves, so the shortcut
//! costs nothing while idle. macOS (alpha) uses handy-keys for now.

use crate::dictation::Input;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(test)]
mod tests;

pub const SHORTCUT: &str = "Ctrl + Win";

/// True while a dictation is recording: only then does Esc belong to murmur.
static WATCH_ESCAPE: AtomicBool = AtomicBool::new(false);

/// Called by the dictation when recording starts and stops.
pub fn watch_escape(on: bool) {
    WATCH_ESCAPE.store(on, Ordering::SeqCst);
}

/// Starts listening for the shortcut on its own thread.
pub fn spawn(dictation: Sender<Input>) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::spawn(dictation)
    }
    #[cfg(not(target_os = "windows"))]
    {
        other::spawn(dictation)
    }
}

/// The keys murmur cares about, by role. The platform layer maps its key codes onto these.
#[cfg_attr(not(target_os = "windows"), allow(dead_code, reason = "macOS uses handy-keys until it gets its own hook"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Ctrl,
    Win,
    Escape,
    /// Anything else.
    Other,
}

/// What one key event means.
#[cfg_attr(not(target_os = "windows"), allow(dead_code, reason = "macOS uses handy-keys until it gets its own hook"))]
#[derive(Debug, Default, PartialEq)]
pub struct Reaction {
    pub input: Option<Input>,
    /// Keep the event from the focused app (Esc while it cancels a dictation).
    pub swallow: bool,
    /// Tell the shell a key happened, so letting go of Win doesn't open the Start menu.
    pub mask_start_menu: bool,
}

/// Which modifiers are down and whether the shortcut is being held.
#[cfg_attr(not(target_os = "windows"), allow(dead_code, reason = "macOS uses handy-keys until it gets its own hook"))]
#[derive(Debug, Default)]
pub struct Keys {
    ctrl: bool,
    win: bool,
    held: bool,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code, reason = "macOS uses handy-keys until it gets its own hook"))]
impl Keys {
    /// `watching_escape` is true while recording.
    pub fn on_key(&mut self, key: Key, down: bool, watching_escape: bool) -> Reaction {
        match key {
            Key::Ctrl => self.ctrl = down,
            Key::Win => self.win = down,
            Key::Escape if down && watching_escape => {
                return Reaction { input: Some(Input::Cancel), swallow: true, mask_start_menu: false };
            }
            // Ctrl+Win+D and friends are Windows shortcuts, not dictation: give up the recording.
            Key::Other | Key::Escape if down && self.held => {
                return Reaction { input: Some(Input::Cancel), swallow: false, mask_start_menu: false };
            }
            Key::Escape | Key::Other => return Reaction::default(),
        }
        match (self.held, self.ctrl && self.win) {
            (false, true) => {
                self.held = true;
                Reaction { input: Some(Input::Hold), swallow: false, mask_start_menu: true }
            }
            (true, false) => {
                self.held = false;
                Reaction { input: Some(Input::Release), swallow: false, mask_start_menu: false }
            }
            _ => Reaction::default(),
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod other {
    use crate::dictation::Input;
    use handy_keys::{Hotkey, HotkeyManager, HotkeyState, Modifiers};
    use std::sync::mpsc::Sender;

    pub fn spawn(dictation: Sender<Input>) -> Result<(), String> {
        let manager = HotkeyManager::new_with_blocking().map_err(|error| error.to_string())?;
        let hold = Hotkey::new(Modifiers::CTRL | Modifiers::CMD, None).map_err(|error| error.to_string())?;
        let id = manager.register(hold).map_err(|error| error.to_string())?;
        std::thread::Builder::new()
            .name("murmur-hotkey".into())
            .spawn(move || {
                while let Ok(event) = manager.recv() {
                    let input = match event.state {
                        HotkeyState::Pressed => Input::Hold,
                        HotkeyState::Released => Input::Release,
                    };
                    if event.id == id && dictation.send(input).is_err() {
                        return;
                    }
                }
            })
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}
