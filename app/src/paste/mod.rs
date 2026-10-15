//! Puts text where the user is typing: onto the clipboard, Ctrl+V (Cmd+V on
//! macOS) into whatever has focus, then the clipboard goes back to what it was.
//! The pill never takes focus, so "whatever has focus" is the app the user was in.
//!
//! Actions press keys, open links and read the selection through here too.
//!
//! `LIKH_PASTE=<file>` swaps all of that for a log: each paste, key press and
//! opened link becomes one JSON line in the file, and the clipboard, focus and
//! keyboard are never touched (an action's selection is then `LIKH_SELECTION`). Agents test whole dictations with it while the person
//! keeps working, the way `LIKH_AUDIO` and `LIKH_TRANSCRIBER` fake the rest.

use crate::commands::Press;
use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

#[cfg(test)]
mod tests;

/// Long enough for the target app to read the clipboard before we restore it.
const READ_GRACE: Duration = Duration::from_millis(250);

/// Pastes `text` now. The returned `Restore` puts the old clipboard back; call
/// `finish` once you've noted the paste time (it waits for the app to read first).
pub fn paste(text: &str) -> Result<Restore, String> {
    if let Some(log) = log_file() {
        log_line(&log, &serde_json::json!({ "paste": text }))?;
        return Ok(Restore { clipboard: None, previous: None });
    }
    let mut clipboard = retry(Clipboard::new)?;
    // Only text survives the round trip today; an image on the clipboard is replaced.
    let previous = clipboard.get_text().ok();
    retry(|| clipboard.set_text(text))?;

    shortcut('v')?;
    Ok(Restore { clipboard: Some(clipboard), previous })
}

/// Leaves `text` on the clipboard, for the person to paste where they like.
pub fn copy(text: &str) -> Result<(), String> {
    if let Some(log) = log_file() {
        return log_line(&log, &serde_json::json!({ "copy": text }));
    }
    let mut clipboard = retry(Clipboard::new)?;
    retry(|| clipboard.set_text(text))
}

/// "Scratch that": the app's own undo takes back the last paste.
pub fn undo() -> Result<(), String> {
    if let Some(log) = log_file() {
        return log_line(&log, &serde_json::json!({ "press": "undo" }));
    }
    shortcut('z')
}

/// "Press enter" sends what was just pasted, in chats and terminals;
/// "press tab" moves to the next field of a form.
pub fn press(press: Press) -> Result<(), String> {
    if let Some(log) = log_file() {
        return log_line(&log, &serde_json::json!({ "press": press.name() }));
    }
    let key = match press {
        Press::Enter => Key::Return,
        Press::Tab => Key::Tab,
    };
    let mut keys = Enigo::new(&Settings::default()).map_err(|error| error.to_string())?;
    keys.key(key, Direction::Click).map_err(|error| error.to_string())
}

/// An action's `keys` step: one chord, modifiers held around its key.
pub fn chord(chord: &crate::actions::steps::Chord) -> Result<(), String> {
    if let Some(log) = log_file() {
        return log_line(&log, &serde_json::json!({ "press": chord.name() }));
    }
    let key = enigo_key(&chord.key).ok_or_else(|| format!("likh can't press {}", chord.key))?;
    let mut keys = Enigo::new(&Settings::default()).map_err(|error| error.to_string())?;
    let held: Vec<Key> =
        [(chord.ctrl, Key::Control), (chord.alt, Key::Alt), (chord.shift, Key::Shift), (chord.win, Key::Meta)]
            .into_iter()
            .filter_map(|(on, key)| on.then_some(key))
            .collect();
    let mut result = Ok(());
    for modifier in &held {
        result = result.and(keys.key(*modifier, Direction::Press));
    }
    result = result.and(keys.key(key, Direction::Click));
    // Always let go of every modifier, even if something failed.
    for modifier in held.iter().rev() {
        result = result.and(keys.key(*modifier, Direction::Release));
    }
    result.map_err(|error| error.to_string())
}

/// The key a `keys` step names (`actions::steps::NAMED_KEYS`, letters, digits, F1 to F12).
pub fn enigo_key(name: &str) -> Option<Key> {
    let key = match name {
        "enter" => Key::Return,
        "tab" => Key::Tab,
        "esc" => Key::Escape,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "play_pause" => Key::MediaPlayPause,
        "next_track" => Key::MediaNextTrack,
        "previous_track" => Key::MediaPrevTrack,
        "volume_up" => Key::VolumeUp,
        "volume_down" => Key::VolumeDown,
        "mute" => Key::VolumeMute,
        function if function.starts_with('f') && function.len() > 1 => match function[1..].parse::<u8>().ok()? {
            1 => Key::F1,
            2 => Key::F2,
            3 => Key::F3,
            4 => Key::F4,
            5 => Key::F5,
            6 => Key::F6,
            7 => Key::F7,
            8 => Key::F8,
            9 => Key::F9,
            10 => Key::F10,
            11 => Key::F11,
            12 => Key::F12,
            _ => return None,
        },
        single => {
            let mut chars = single.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_alphanumeric() => Key::Unicode(c),
                _ => return None,
            }
        }
    };
    Some(key)
}

/// An action's `open` step: a link, file or app, with the system's own handler.
pub fn open(target: &str) -> Result<(), String> {
    if let Some(log) = log_file() {
        return log_line(&log, &serde_json::json!({ "open": target }));
    }
    crate::shared::open_url(target);
    Ok(())
}

/// The text selected in the app in front, read with Ctrl+C; the clipboard
/// goes back to what it was. None when nothing is selected.
///
/// With `LIKH_PASTE` set, the copy is logged and the selection is `LIKH_SELECTION`.
pub fn selection() -> Result<Option<String>, String> {
    if let Some(log) = log_file() {
        log_line(&log, &serde_json::json!({ "press": "copy" }))?;
        return Ok(std::env::var("LIKH_SELECTION").ok().filter(|text| !text.is_empty()));
    }
    let mut clipboard = retry(Clipboard::new)?;
    let previous = clipboard.get_text().ok();
    // A marker nobody would copy: if it's still there after Ctrl+C, nothing was selected.
    let marker = format!("\u{2063}likh-selection-{}", std::process::id());
    retry(|| clipboard.set_text(marker.clone()))?;
    shortcut('c')?;
    let until = std::time::Instant::now() + Duration::from_millis(400);
    let mut copied = None;
    while std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
        if let Ok(text) = clipboard.get_text()
            && text != marker
        {
            copied = Some(text);
            break;
        }
    }
    let _ = match previous {
        Some(previous) => clipboard.set_text(previous),
        None => clipboard.clear(),
    };
    Ok(copied.filter(|text| !text.is_empty()))
}

/// The clipboard's text, for actions that read it. `LIKH_CLIPBOARD` with `LIKH_PASTE` set.
pub fn clipboard_text() -> Option<String> {
    if log_file().is_some() {
        return std::env::var("LIKH_CLIPBOARD").ok();
    }
    retry(Clipboard::new).ok()?.get_text().ok()
}

fn log_file() -> Option<PathBuf> {
    std::env::var_os("LIKH_PASTE").map(PathBuf::from)
}

/// Appends one JSON line to the `LIKH_PASTE` log.
fn log_line(log: &std::path::Path, line: &serde_json::Value) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|error| format!("can't write {}: {error}", log.display()))?;
    writeln!(file, "{line}").map_err(|error| error.to_string())
}

/// Ctrl (Cmd on macOS) plus a letter, in whatever app has focus.
fn shortcut(letter: char) -> Result<(), String> {
    let mut keys = Enigo::new(&Settings::default()).map_err(|error| error.to_string())?;
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;
    keys.key(modifier, Direction::Press).map_err(|error| error.to_string())?;
    let pressed = keys.key(Key::Unicode(letter), Direction::Click);
    // Always let go of the modifier, even if the letter failed.
    let released = keys.key(modifier, Direction::Release);
    pressed.and(released).map_err(|error| error.to_string())
}

/// Clipboard managers and other apps hold the Windows clipboard for a few
/// milliseconds at a time, so a busy clipboard gets a few quick retries.
fn retry<T>(mut attempt: impl FnMut() -> Result<T, arboard::Error>) -> Result<T, String> {
    let mut last = String::new();
    for _ in 0..5 {
        match attempt() {
            Ok(value) => return Ok(value),
            Err(error) => last = error.to_string(),
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err(last)
}

/// The clipboard as it was before a paste. None when pasting into the `LIKH_PASTE` log.
pub struct Restore {
    clipboard: Option<Clipboard>,
    previous: Option<String>,
}

impl Restore {
    pub fn finish(self) {
        let Some(mut clipboard) = self.clipboard else { return };
        std::thread::sleep(READ_GRACE);
        if let Some(previous) = self.previous {
            let _ = clipboard.set_text(previous);
        }
    }
}
