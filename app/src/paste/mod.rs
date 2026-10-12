//! Puts text where the user is typing: onto the clipboard, Ctrl+V (Cmd+V on
//! macOS) into whatever has focus, then the clipboard goes back to what it was.
//! The pill never takes focus, so "whatever has focus" is the app the user was in.

use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::time::Duration;

/// Long enough for the target app to read the clipboard before we restore it.
const READ_GRACE: Duration = Duration::from_millis(250);

/// Pastes `text` now. The returned `Restore` puts the old clipboard back; call
/// `finish` once you've noted the paste time (it waits for the app to read first).
pub fn paste(text: &str) -> Result<Restore, String> {
    let mut clipboard = retry(Clipboard::new)?;
    // Only text survives the round trip today; an image on the clipboard is replaced.
    let previous = clipboard.get_text().ok();
    retry(|| clipboard.set_text(text))?;

    shortcut('v')?;
    Ok(Restore { clipboard, previous })
}

/// "Scratch that": the app's own undo takes back the last paste.
pub fn undo() -> Result<(), String> {
    shortcut('z')
}

/// "Press enter": sends what was just pasted, in chats and terminals.
pub fn press_enter() -> Result<(), String> {
    let mut keys = Enigo::new(&Settings::default()).map_err(|error| error.to_string())?;
    keys.key(Key::Return, Direction::Click).map_err(|error| error.to_string())
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

pub struct Restore {
    clipboard: Clipboard,
    previous: Option<String>,
}

impl Restore {
    pub fn finish(mut self) {
        std::thread::sleep(READ_GRACE);
        if let Some(previous) = self.previous {
            let _ = self.clipboard.set_text(previous);
        }
    }
}
