//! What the user chose: the shortcut, how it behaves, and whether likh keeps
//! a history. One JSON file in likh's local data folder; anything missing or
//! unreadable falls back to the defaults, so a bad file never stops likh.

use crate::hotkey::Binding;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

/// How the shortcut starts and stops a dictation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Hold to talk, let go to paste. A quick tap starts hands-free; tap again to stop.
    #[default]
    HoldOrTap,
    /// Hold to talk, let go to paste. Taps do nothing.
    Hold,
    /// Tap to start, tap again to stop. Never needs holding.
    Tap,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::HoldOrTap, Mode::Hold, Mode::Tap];

    pub fn name(self) -> &'static str {
        match self {
            Mode::HoldOrTap => "Hold or tap",
            Mode::Hold => "Hold only",
            Mode::Tap => "Tap to toggle",
        }
    }

    pub fn explain(self) -> &'static str {
        match self {
            Mode::HoldOrTap => "Hold to talk. Tap once for hands-free, tap again to stop.",
            Mode::Hold => "Hold to talk, let go to paste.",
            Mode::Tap => "Tap to start, tap again to stop and paste.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub shortcut: Binding,
    pub mode: Mode,
    /// Keep transcripts on this computer so they can be found and pasted again.
    pub history: bool,
    /// Quiet cues when recording starts, stops or fails.
    pub sounds: bool,
    /// Spoken commands like "new line" and "press enter" (`commands/`).
    pub commands: bool,
    /// Start likh when you sign in to Windows.
    pub start_with_windows: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcut: Binding::default(),
            mode: Mode::default(),
            history: true,
            sounds: true,
            commands: true,
            start_with_windows: false,
        }
    }
}

/// `%LOCALAPPDATA%\likh` (or the platform's equivalent).
pub fn folder() -> Option<PathBuf> {
    dirs::data_local_dir().map(|dir| dir.join("likh"))
}

/// likh was called murmur until 0.1.0-alpha.3. Moves `murmur`'s folder (settings,
/// history, the onboarded marker) to `likh` once, so nobody starts over.
/// Delete once alpha.2 is gone.
pub fn adopt_murmur_folder(data: &Path) -> Result<bool, String> {
    let (old, new) = (data.join("murmur"), data.join("likh"));
    if new.exists() || !old.is_dir() {
        return Ok(false);
    }
    std::fs::rename(&old, &new).map(|()| true).map_err(|error| error.to_string())
}

impl Settings {
    pub fn load() -> Self {
        folder().map(|dir| Self::load_from(&dir.join("settings.json"))).unwrap_or_default()
    }

    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = folder().ok_or("there is no local data folder")?;
        self.save_to(&dir.join("settings.json"))
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(path, text).map_err(|error| error.to_string())
    }
}
