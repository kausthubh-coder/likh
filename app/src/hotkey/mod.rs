//! The global shortcut: hold Ctrl+Space to talk, Esc to cancel.
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

pub const SHORTCUT: &str = "Ctrl + Space";

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
    Space,
    Escape,
    /// Anything else.
    Other,
}

/// What one key event means.
#[cfg_attr(not(target_os = "windows"), allow(dead_code, reason = "macOS uses handy-keys until it gets its own hook"))]
#[derive(Debug, Default, PartialEq)]
pub struct Reaction {
    pub input: Option<Input>,
    /// Keep the event from the focused app: the shortcut's Space (no stray
    /// spaces, no app's own Ctrl+Space), and Esc while it cancels a dictation.
    pub swallow: bool,
}

/// Whether Ctrl is down and whether the shortcut is being held.
#[cfg_attr(not(target_os = "windows"), allow(dead_code, reason = "macOS uses handy-keys until it gets its own hook"))]
#[derive(Debug, Default)]
pub struct Keys {
    ctrl: bool,
    held: bool,
    /// The Space that started the shortcut is still down: its repeats and its release are ours too.
    space_is_ours: bool,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code, reason = "macOS uses handy-keys until it gets its own hook"))]
impl Keys {
    /// `watching_escape` is true while recording.
    pub fn on_key(&mut self, key: Key, down: bool, watching_escape: bool) -> Reaction {
        let ours = |input| Reaction { input, swallow: true };
        match (key, down) {
            (Key::Ctrl, _) => {
                self.ctrl = down;
                if !down && self.held {
                    self.held = false;
                    return Reaction { input: Some(Input::Release), swallow: false };
                }
                Reaction::default()
            }
            (Key::Space, true) if self.space_is_ours => ours(None),
            (Key::Space, true) if self.ctrl => {
                self.held = true;
                self.space_is_ours = true;
                ours(Some(Input::Hold))
            }
            (Key::Space, false) if self.space_is_ours => {
                self.space_is_ours = false;
                let was_held = std::mem::take(&mut self.held);
                ours(was_held.then_some(Input::Release))
            }
            (Key::Escape, true) if watching_escape => ours(Some(Input::Cancel)),
            // Ctrl+Space+something is another shortcut, not dictation: give up the recording.
            (Key::Other | Key::Escape, true) if self.held => Reaction { input: Some(Input::Cancel), swallow: false },
            _ => Reaction::default(),
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod other {
    use crate::dictation::Input;
    use handy_keys::{Hotkey, HotkeyManager, HotkeyState, Key, Modifiers};
    use std::sync::mpsc::Sender;

    pub fn spawn(dictation: Sender<Input>) -> Result<(), String> {
        let manager = HotkeyManager::new_with_blocking().map_err(|error| error.to_string())?;
        let hold = Hotkey::new(Modifiers::CTRL, Key::Space).map_err(|error| error.to_string())?;
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
