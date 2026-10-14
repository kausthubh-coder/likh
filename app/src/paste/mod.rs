//! Puts text where the user is typing: onto the clipboard, Ctrl+V (Cmd+V on
//! macOS) into whatever has focus, then the clipboard goes back to what it was.
//! The pill never takes focus, so "whatever has focus" is the app the user was in.
//!
//! `LIKH_PASTE=<file>` swaps all of that for a log: each paste and key press
//! becomes one JSON line in the file, and the clipboard, focus and keyboard
//! are never touched. Agents test whole dictations with it while the person
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
