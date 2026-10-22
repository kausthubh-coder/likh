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
/// `LIKH_WINDOW=<program>|<title>` stands in for it, for tests and agents.
pub fn foreground() -> Option<Window> {
    if let Ok(fake) = std::env::var("LIKH_WINDOW") {
        let (app, title) = fake.split_once('|').unwrap_or((fake.as_str(), ""));
        return Some(Window { id: 1, app: app.to_owned(), title: title.to_owned() });
    }
    #[cfg(target_os = "windows")]
    {
        windows::foreground()
    }
    // macOS gives the app, not its window: `app` is its bundle id
    // (`com.tinyspeck.slackmacgap`), `title` its name, `id` its process.
    #[cfg(target_os = "macos")]
    {
        let app = objc2_app_kit::NSWorkspace::sharedWorkspace().frontmostApplication()?;
        Some(Window {
            id: app.processIdentifier() as isize,
            app: app.bundleIdentifier().map(|id| id.to_string()).unwrap_or_default(),
            title: app.localizedName().map(|name| name.to_string()).unwrap_or_default(),
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
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
    let started = windows::spawn(dictation, triggers);
    #[cfg(not(target_os = "windows"))]
    let started = other::spawn(dictation, triggers);
    note_shortcut(started.is_err());
    started
}

/// Another app already had the shortcut's keys when likh started, so holding
/// them does nothing until they change. Onboarding, the Dictation tab and the
/// pill say so; only the trace knew before.
static SHORTCUT_TAKEN: AtomicBool = AtomicBool::new(false);
/// The same for paste last (the History tab says so).
static PASTE_LAST_TAKEN: AtomicBool = AtomicBool::new(false);
/// Another app already had the action key's keys when likh started, so it does nothing until changed.
static ACTION_KEY_TAKEN: AtomicBool = AtomicBool::new(false);

fn note_shortcut(taken: bool) {
    SHORTCUT_TAKEN.store(taken, Ordering::SeqCst);
}

/// Why holding the keys does nothing, when they couldn't start (macOS: no Accessibility permission).
#[cfg_attr(not(target_os = "macos"), allow(dead_code, reason = "only macOS asks for Accessibility"))]
pub const NEEDS_ACCESSIBILITY: &str =
    "likh needs Accessibility access to hear its keys. Allow it in System Settings, Privacy & Security, Accessibility.";

/// macOS: likh is on the Accessibility list, so it can listen for keys.
#[cfg(target_os = "macos")]
pub fn accessibility_granted() -> bool {
    handy_keys::check_accessibility()
}

/// macOS: starts every key again, once the Accessibility permission is on.
#[cfg(target_os = "macos")]
pub fn start_again(triggers: Triggers) -> Result<(), String> {
    let started = other::start_all(triggers);
    note_shortcut(started.is_err());
    started
}

/// Why the shortcut does nothing when it can't start, in the views' few words.
pub fn why_unheard() -> &'static str {
    if cfg!(target_os = "macos") { "likh needs Accessibility access." } else { "Another app has these keys." }
}

/// The shortcut's keys belong to another app (macOS: likh can't listen for any keys yet).
pub fn shortcut_taken() -> bool {
    SHORTCUT_TAKEN.load(Ordering::SeqCst)
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code, reason = "only Windows registers paste last"))]
fn note_paste_last(taken: bool) {
    PASTE_LAST_TAKEN.store(taken, Ordering::SeqCst);
}

/// Paste last's keys belong to another app.
pub fn paste_last_taken() -> bool {
    PASTE_LAST_TAKEN.load(Ordering::SeqCst)
}

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
        other::set(other::Slot::Action, binding.map(String::from))
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
        other::set(other::Slot::ActionMouse, other::mouse_name(mouse))
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
        other::set(other::Slot::PasteLast, binding.map(String::from))
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
        other::set(other::Slot::Mouse, other::mouse_name(mouse))
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
        other::set(other::Slot::Shortcut, Some(String::from(binding)))
    }
}

/// Stops the shortcut working while someone types a new one, then brings it back.
pub fn pause(paused: bool) {
    #[cfg(target_os = "windows")]
    windows::pause(paused);
    #[cfg(not(target_os = "windows"))]
    other::pause(paused);
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
    #[cfg(target_os = "windows")]
    windows::watch_escape(escape_wanted());
}

/// A recording can be cancelled or an action stopped: Esc does it.
fn escape_wanted() -> bool {
    ESCAPE_FOR_DICTATION.load(Ordering::SeqCst) || ESCAPE_FOR_ACTION.load(Ordering::SeqCst)
}

/// macOS: handy-keys for every key and button. Each listener is its own
/// event tap on its own thread, and handy-keys can't change one from outside
/// its thread, so changing a key starts a new listener and retires the old
/// one: it notices it's stale at its next event (its keys stay taken until
/// then, at most one press). Esc has a listener of its own that never blocks
/// the key, so Esc still reaches apps; it cancels only while there's
/// something to cancel. Taken keys aren't a thing here: every listener needs
/// the Accessibility permission, and without it none starts.
#[cfg(not(target_os = "windows"))]
mod other {
    use super::{MouseTrigger, Triggers};
    use crate::dictation::Input;
    use handy_keys::{Hotkey, HotkeyManager, HotkeyState};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc::Sender;
    use std::sync::{Mutex, OnceLock};

    /// What a listener does.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Slot {
        Shortcut,
        PasteLast,
        Action,
        Mouse,
        ActionMouse,
    }

    impl Slot {
        const ALL: [Slot; 5] = [Slot::Shortcut, Slot::PasteLast, Slot::Action, Slot::Mouse, Slot::ActionMouse];

        fn index(self) -> usize {
            Slot::ALL.iter().position(|slot| *slot == self).unwrap_or_default()
        }

        /// What going down and coming up send. Paste last waits for its keys
        /// to come up, so the paste isn't read as a shortcut.
        fn inputs(self) -> (Option<Input>, fn() -> Option<Input>) {
            match self {
                Slot::Shortcut | Slot::Mouse => (Some(Input::Hold), || Some(Input::Release(std::time::Instant::now()))),
                Slot::Action | Slot::ActionMouse => {
                    (Some(Input::ActionHold), || Some(Input::Release(std::time::Instant::now())))
                }
                Slot::PasteLast => (None, || Some(Input::PasteLast)),
            }
        }
    }

    /// Bumped each time a slot's listener is replaced: an older one stops at its next event.
    static GENERATIONS: [AtomicU64; 5] =
        [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
    /// Picking new keys in likh's window: listeners ignore what they hear.
    static PAUSED: AtomicBool = AtomicBool::new(false);
    static DICTATION: OnceLock<Mutex<Sender<Input>>> = OnceLock::new();

    /// Esc's listener is running.
    static ESCAPE_STARTED: AtomicBool = AtomicBool::new(false);

    pub fn spawn(dictation: Sender<Input>, triggers: Triggers) -> Result<(), String> {
        let _ = DICTATION.set(Mutex::new(dictation));
        start_all(triggers)
    }

    /// Every listener, each replacing its slot's old one.
    pub fn start_all(triggers: Triggers) -> Result<(), String> {
        if !ESCAPE_STARTED.load(Ordering::SeqCst)
            && let Err(reason) = listen_for_escape()
        {
            crate::diagnostics::warn("hotkey", format!("Esc won't cancel: {reason}"));
        }
        let optional = [
            (Slot::PasteLast, triggers.paste_last.map(String::from), "pasting the last dictation"),
            (Slot::Action, triggers.action_key.map(String::from), "the action key"),
            (Slot::Mouse, mouse_name(triggers.mouse), "the mouse button"),
            (Slot::ActionMouse, mouse_name(triggers.action_mouse), "the action key's mouse button"),
        ];
        for (slot, keys, what) in optional {
            if let Err(reason) = set(slot, keys) {
                crate::diagnostics::warn("hotkey", format!("{what} won't work: {reason}"));
            }
        }
        set(Slot::Shortcut, Some(String::from(triggers.shortcut)))
    }

    pub fn mouse_name(mouse: MouseTrigger) -> Option<String> {
        match mouse {
            MouseTrigger::Off => None,
            MouseTrigger::Middle => Some("MouseMiddle".into()),
            MouseTrigger::Back => Some("MouseX1".into()),
            MouseTrigger::Forward => Some("MouseX2".into()),
        }
    }

    pub fn pause(paused: bool) {
        PAUSED.store(paused, Ordering::SeqCst);
    }

    /// Puts `keys` on `slot` (None turns it off) and retires its old listener.
    pub fn set(slot: Slot, keys: Option<String>) -> Result<(), String> {
        let generation = GENERATIONS[slot.index()].fetch_add(1, Ordering::SeqCst) + 1;
        let Some(keys) = keys else { return Ok(()) };
        let (down, up) = slot.inputs();
        listen(&keys, true, move |state| {
            if GENERATIONS[slot.index()].load(Ordering::SeqCst) != generation {
                return Flow::Stop;
            }
            if PAUSED.load(Ordering::SeqCst) {
                return Flow::Go;
            }
            match state {
                HotkeyState::Pressed => down.clone().map_or(Flow::Go, send),
                HotkeyState::Released => up().map_or(Flow::Go, send),
            }
        })
    }

    /// Esc, heard but never taken from the app in front.
    fn listen_for_escape() -> Result<(), String> {
        let started = listen("Escape", false, |state| match state {
            HotkeyState::Pressed if super::escape_wanted() => send(Input::Cancel),
            _ => Flow::Go,
        });
        ESCAPE_STARTED.store(started.is_ok(), Ordering::SeqCst);
        started
    }

    enum Flow {
        Go,
        Stop,
    }

    fn send(input: Input) -> Flow {
        let sent = DICTATION
            .get()
            .map(|dictation| dictation.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).send(input).is_ok());
        if sent == Some(false) { Flow::Stop } else { Flow::Go }
    }

    /// One listener for one hotkey on its own thread, until `on` says stop.
    /// `block` keeps the keys from the app in front.
    fn listen(
        hotkey: &str,
        block: bool,
        mut on: impl FnMut(HotkeyState) -> Flow + Send + 'static,
    ) -> Result<(), String> {
        let manager = if block { HotkeyManager::new_with_blocking() } else { HotkeyManager::new() }
            .map_err(|error| explain(&error.to_string()))?;
        let keys: Hotkey = hotkey.parse().map_err(|error: handy_keys::Error| error.to_string())?;
        let id = manager.register(keys).map_err(|error| error.to_string())?;
        std::thread::Builder::new()
            .name("likh-hotkey".into())
            .spawn(move || {
                while let Ok(event) = manager.recv() {
                    if event.id == id && matches!(on(event.state), Flow::Stop) {
                        return;
                    }
                }
            })
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// handy-keys' error, or what to do about the usual one.
    fn explain(error: &str) -> String {
        #[cfg(target_os = "macos")]
        if !handy_keys::check_accessibility() {
            return super::NEEDS_ACCESSIBILITY.into();
        }
        error.to_owned()
    }
}

/// Tests stand in for another app holding the keys. Hold `shared::test_ui::lock()`
/// while it's set: every view that says so reads it.
#[cfg(test)]
pub fn pretend_taken(shortcut: bool, paste_last: bool) {
    note_shortcut(shortcut);
    note_paste_last(paste_last);
}
