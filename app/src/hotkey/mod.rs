//! The global shortcut (Ctrl+Space unless the user picks another), and Esc to
//! cancel while recording.
//!
//! On Windows the shortcut is a system hotkey (`RegisterHotKey`, `windows.rs`):
//! Windows swallows the keystroke for us, it can't time out, and nothing runs
//! while idle. A low-level keyboard hook was tried first; Windows silently
//! removes those when the machine is busy, and the shortcut stopped working.
//! Letting go is noticed by looking at the keys every 15ms, only while held.
//!
//! A mouse button can start dictations too (`MouseTrigger`). That needs a
//! low-level mouse hook, which wakes likh on every mouse move, so it is
//! installed only while a button is picked; with none picked nothing runs.
//!
//! The action key (`ACTION_KEY`, and its own mouse button) works the same way
//! but sends `Input::ActionHold`: what's said next runs an action (`actions/`).
//! It's Windows only for now.

use crate::dictation::Input;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

pub mod capture;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(test)]
mod tests;

/// A shortcut: some modifiers and one key. Saved as text, like `Ctrl+Space`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Binding {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub key: KeyName,
}

impl Default for Binding {
    fn default() -> Self {
        Self { ctrl: true, alt: false, shift: false, win: false, key: KeyName("Space") }
    }
}

/// Pastes the last dictation again, out of the box. Wispr Flow uses the same
/// keys, so people coming from it already know them.
pub const PASTE_LAST: Binding = Binding { ctrl: false, alt: true, shift: true, win: false, key: KeyName("Z") };

/// Hold it and say an action, out of the box: the dictation shortcut plus Shift.
pub const ACTION_KEY: Binding = Binding { ctrl: true, alt: false, shift: true, win: false, key: KeyName("Space") };

/// The window in front: where an action's result goes, and what `{app}` and `{title}` say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    /// Its handle, only for telling whether the window in front changed.
    pub id: isize,
    /// The program's file name, like `chrome.exe`.
    pub app: String,
    pub title: String,
}

/// The window in front now. None where likh can't tell (macOS for now).
pub fn foreground() -> Option<Window> {
    #[cfg(target_os = "windows")]
    {
        windows::foreground()
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// A mouse button that works like the shortcut: hold to talk, or tap for hands-free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MouseTrigger {
    #[default]
    Off,
    Middle,
    /// The side button nearer you (mouse 4), "back" in browsers.
    Back,
    /// The other side button (mouse 5), "forward" in browsers.
    Forward,
}

impl MouseTrigger {
    pub const ALL: [MouseTrigger; 4] =
        [MouseTrigger::Off, MouseTrigger::Middle, MouseTrigger::Back, MouseTrigger::Forward];

    pub fn name(self) -> &'static str {
        match self {
            MouseTrigger::Off => "Off",
            MouseTrigger::Middle => "Middle",
            MouseTrigger::Back => "Back",
            MouseTrigger::Forward => "Forward",
        }
    }
}

/// One non-modifier key, by the name likh shows and saves. Only keys in `KEYS` exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyName(pub &'static str);

/// Every key a shortcut can end on, with its Windows virtual-key code.
pub const KEYS: [(&str, u16); 58] = [
    ("Space", 0x20),
    ("Enter", 0x0D),
    ("Tab", 0x09),
    ("Backspace", 0x08),
    ("Insert", 0x2D),
    ("Delete", 0x2E),
    ("Home", 0x24),
    ("End", 0x23),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
    ("A", 0x41),
    ("B", 0x42),
    ("C", 0x43),
    ("D", 0x44),
    ("E", 0x45),
    ("F", 0x46),
    ("G", 0x47),
    ("H", 0x48),
    ("I", 0x49),
    ("J", 0x4A),
    ("K", 0x4B),
    ("L", 0x4C),
    ("M", 0x4D),
    ("N", 0x4E),
    ("O", 0x4F),
    ("P", 0x50),
    ("Q", 0x51),
    ("R", 0x52),
    ("S", 0x53),
    ("T", 0x54),
    ("U", 0x55),
    ("V", 0x56),
    ("W", 0x57),
    ("X", 0x58),
    ("Y", 0x59),
    ("Z", 0x5A),
    ("0", 0x30),
    ("1", 0x31),
    ("2", 0x32),
    ("3", 0x33),
    ("4", 0x34),
    ("5", 0x35),
    ("6", 0x36),
    ("7", 0x37),
    ("8", 0x38),
    ("9", 0x39),
    ("F1", 0x70),
    ("F2", 0x71),
    ("F3", 0x72),
    ("F4", 0x73),
    ("F5", 0x74),
    ("F6", 0x75),
    ("F7", 0x76),
    ("F8", 0x77),
    ("F9", 0x78),
    ("F10", 0x79),
    ("F11", 0x7A),
    ("F12", 0x7B),
];

impl KeyName {
    pub fn find(name: &str) -> Option<Self> {
        KEYS.iter().find(|(known, _)| known.eq_ignore_ascii_case(name)).map(|(known, _)| KeyName(known))
    }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code, reason = "only the Windows backend needs key codes"))]
    pub fn virtual_key(self) -> u16 {
        KEYS.iter().find(|(known, _)| *known == self.0).map_or(0, |(_, code)| *code)
    }

    fn is_function_key(self) -> bool {
        self.0.len() > 1 && self.0.starts_with('F') && self.0[1..].chars().all(|c| c.is_ascii_digit())
    }
}

impl Binding {
    /// Builds a shortcut from what was pressed, or says why it can't be one.
    pub fn new(ctrl: bool, alt: bool, shift: bool, win: bool, key: KeyName) -> Result<Self, &'static str> {
        let binding = Self { ctrl, alt, shift, win, key };
        if !(ctrl || alt || win) && !key.is_function_key() {
            return Err("Add Ctrl, Alt or Win, so typing never sets it off.");
        }
        Ok(binding)
    }

    /// The keys in the order people say them, for key caps.
    pub fn parts(self) -> Vec<&'static str> {
        [(self.ctrl, "Ctrl"), (self.alt, "Alt"), (self.shift, "Shift"), (self.win, "Win")]
            .into_iter()
            .filter_map(|(on, name)| on.then_some(name))
            .chain([self.key.0])
            .collect()
    }
}

impl fmt::Display for Binding {
    /// `Ctrl + Space`: how likh writes a shortcut in sentences.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.parts().join(" + "))
    }
}

impl From<Binding> for String {
    fn from(binding: Binding) -> String {
        binding.parts().join("+")
    }
}

impl TryFrom<String> for Binding {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        let (mut ctrl, mut alt, mut shift, mut win, mut key) = (false, false, false, false, None);
        for part in text.split('+').map(str::trim) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                "win" | "super" | "cmd" => win = true,
                _ => key = Some(KeyName::find(part).ok_or_else(|| format!("unknown key {part:?}"))?),
            }
        }
        let key = key.ok_or_else(|| format!("{text:?} has no key besides modifiers"))?;
        Binding::new(ctrl, alt, shift, win, key).map_err(str::to_owned)
    }
}

/// Starts listening for `binding` (and `mouse`, and `paste_last`) on its own
/// thread. Err says why the shortcut couldn't start (on Windows, usually that
/// another app already owns it); a paste-last shortcut that can't is only logged.
/// The keys and buttons `spawn` starts listening for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Triggers {
    pub shortcut: Binding,
    pub paste_last: Option<Binding>,
    pub mouse: MouseTrigger,
    pub action_key: Option<Binding>,
    pub action_mouse: MouseTrigger,
}

impl Triggers {
    pub fn from(settings: &crate::settings::Settings) -> Self {
        Self {
            shortcut: settings.shortcut,
            paste_last: settings.paste_last,
            mouse: settings.mouse,
            action_key: settings.action_key,
            action_mouse: settings.action_mouse,
        }
    }
}

pub fn spawn(dictation: Sender<Input>, triggers: Triggers) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::spawn(dictation, triggers)
    }
    #[cfg(not(target_os = "windows"))]
    {
        other::spawn(dictation, triggers.shortcut, triggers.mouse)
    }
}

/// Another app already had the action key's keys when likh started, so it does nothing until changed.
static ACTION_KEY_TAKEN: AtomicBool = AtomicBool::new(false);

#[cfg_attr(not(target_os = "windows"), allow(dead_code, reason = "only Windows registers the action key"))]
fn note_action_key(taken: bool) {
    ACTION_KEY_TAKEN.store(taken, Ordering::SeqCst);
}

/// The action key's keys belong to another app (the Actions tab says so).
pub fn action_key_taken() -> bool {
    ACTION_KEY_TAKEN.load(Ordering::SeqCst)
}

/// Swaps the action key, or turns it off with None. On error the old one stays.
pub fn set_action_key(binding: Option<Binding>) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::set_action_key(binding)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = binding;
        Err("Actions aren't available on macOS yet.".into())
    }
}

/// Swaps the mouse button that starts actions, or turns it off.
pub fn set_action_mouse(mouse: MouseTrigger) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::set_action_mouse(mouse)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = mouse;
        Err("Actions aren't available on macOS yet.".into())
    }
}

/// Swaps the paste-last shortcut, or turns it off with None. On error the old one stays.
pub fn set_paste_last(binding: Option<Binding>) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::set_paste_last(binding)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = binding;
        Err("Pasting the last dictation isn't available on macOS yet.".into())
    }
}

/// Swaps the mouse button that starts dictations, or turns it off.
pub fn set_mouse(mouse: MouseTrigger) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::set_mouse(mouse)
    }
    #[cfg(not(target_os = "windows"))]
    {
        other::set_mouse(mouse)
    }
}

/// Swaps the shortcut for a new one. On error the old one stays.
pub fn rebind(binding: Binding) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::rebind(binding)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = binding;
        Err("Changing the shortcut isn't available on macOS yet.".into())
    }
}

/// Stops the shortcut working while someone types a new one, then brings it back.
pub fn pause(paused: bool) {
    #[cfg(target_os = "windows")]
    windows::pause(paused);
    #[cfg(not(target_os = "windows"))]
    let _ = paused;
}

/// Who wants Esc right now: a recording that can be cancelled, an action that can be stopped.
static ESCAPE_FOR_DICTATION: AtomicBool = AtomicBool::new(false);
static ESCAPE_FOR_ACTION: AtomicBool = AtomicBool::new(false);

/// Esc belongs to likh only while a recording can be cancelled (or an action stopped).
pub fn watch_escape(on: bool) {
    ESCAPE_FOR_DICTATION.store(on, Ordering::SeqCst);
    apply_escape();
}

/// Esc belongs to likh while an action runs or waits to be confirmed.
pub fn watch_escape_for_action(on: bool) {
    ESCAPE_FOR_ACTION.store(on, Ordering::SeqCst);
    apply_escape();
}

fn apply_escape() {
    let on = ESCAPE_FOR_DICTATION.load(Ordering::SeqCst) || ESCAPE_FOR_ACTION.load(Ordering::SeqCst);
    #[cfg(target_os = "windows")]
    windows::watch_escape(on);
    #[cfg(not(target_os = "windows"))]
    let _ = on;
}

/// macOS (alpha): handy-keys for the shortcut and the mouse button. Untested
/// by hand; CI builds and tests it.
#[cfg(not(target_os = "windows"))]
mod other {
    use super::{Binding, MouseTrigger};
    use crate::dictation::Input;
    use handy_keys::{Hotkey, HotkeyManager, HotkeyState};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc::Sender;
    use std::sync::{Mutex, OnceLock};

    /// Bumped on every mouse change: a listener for an older choice stops at its next event.
    static MOUSE_GENERATION: AtomicU64 = AtomicU64::new(0);
    static DICTATION: OnceLock<Mutex<Sender<Input>>> = OnceLock::new();

    pub fn spawn(dictation: Sender<Input>, binding: Binding, mouse: MouseTrigger) -> Result<(), String> {
        let _ = DICTATION.set(Mutex::new(dictation.clone()));
        if let Err(reason) = set_mouse(mouse) {
            crate::diagnostics::warn("hotkey", format!("the mouse button won't work: {reason}"));
        }
        listen(dictation, String::from(binding), None)
    }

    pub fn set_mouse(mouse: MouseTrigger) -> Result<(), String> {
        let generation = MOUSE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        let button = match mouse {
            MouseTrigger::Off => return Ok(()),
            MouseTrigger::Middle => "MouseMiddle",
            MouseTrigger::Back => "MouseX1",
            MouseTrigger::Forward => "MouseX2",
        };
        let dictation = DICTATION
            .get()
            .ok_or("the shortcut isn't running")?
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        listen(dictation, button.to_string(), Some(generation))
    }

    /// One manager per hotkey, on its own thread. A mouse listener (`generation`
    /// set) goes quiet once a newer choice replaces it.
    fn listen(dictation: Sender<Input>, hotkey: String, generation: Option<u64>) -> Result<(), String> {
        let manager = HotkeyManager::new_with_blocking().map_err(|error| error.to_string())?;
        let hold: Hotkey = hotkey.parse().map_err(|error: handy_keys::Error| error.to_string())?;
        let id = manager.register(hold).map_err(|error| error.to_string())?;
        std::thread::Builder::new()
            .name("likh-hotkey".into())
            .spawn(move || {
                while let Ok(event) = manager.recv() {
                    if generation.is_some_and(|mine| mine != MOUSE_GENERATION.load(Ordering::SeqCst)) {
                        return;
                    }
                    let input = match event.state {
                        HotkeyState::Pressed => Input::Hold,
                        HotkeyState::Released => Input::Release(std::time::Instant::now()),
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
