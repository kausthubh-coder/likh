//! What the user chose: the shortcut, how it behaves, which engine
//! transcribes, the spoken commands, the dictionary, and whether likh keeps a history. One JSON file in likh's local data folder; anything missing or
//! unreadable falls back to the defaults, so a bad file never stops likh.

use crate::commands::Groups;
use crate::hotkey::{Binding, MouseTrigger};
use crate::transcriber::Engine;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

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

    /// Short enough for the three to sit side by side.
    pub fn name(self) -> &'static str {
        match self {
            Mode::HoldOrTap => "Hold or tap",
            Mode::Hold => "Hold",
            Mode::Tap => "Toggle",
        }
    }

    /// What it does, in one line, under the pick.
    pub fn explain(self) -> &'static str {
        match self {
            Mode::HoldOrTap => "Hold to talk. Tap once for hands-free.",
            Mode::Hold => "Hold to talk, let go to paste.",
            Mode::Tap => "Tap to start, tap again to paste.",
        }
    }
}

/// Hands-free stops by itself after this much quiet, once you've said something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SilenceStop {
    Off,
    #[serde(rename = "3s")]
    Short,
    #[default]
    #[serde(rename = "5s")]
    Medium,
    #[serde(rename = "10s")]
    Long,
}

impl SilenceStop {
    pub const ALL: [SilenceStop; 4] = [SilenceStop::Off, SilenceStop::Short, SilenceStop::Medium, SilenceStop::Long];

    pub fn after(self) -> Option<Duration> {
        match self {
            SilenceStop::Off => None,
            SilenceStop::Short => Some(Duration::from_secs(3)),
            SilenceStop::Medium => Some(Duration::from_secs(5)),
            SilenceStop::Long => Some(Duration::from_secs(10)),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            SilenceStop::Off => "Never",
            SilenceStop::Short => "3 s",
            SilenceStop::Medium => "5 s",
            SilenceStop::Long => "10 s",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub shortcut: Binding,
    /// Pastes the last dictation again (`hotkey::PASTE_LAST` unless changed); None is off.
    pub paste_last: Option<Binding>,
    /// A mouse button that works like the shortcut (hold, or tap for hands-free).
    pub mouse: MouseTrigger,
    pub mode: Mode,
    /// Which service turns speech into text.
    pub engine: Engine,
    pub silence_stop: SilenceStop,
    /// Pause music and videos while you talk, and play them again after.
    pub pause_media: bool,
    /// Keep transcripts on this computer so they can be found and pasted again.
    pub history: bool,
    /// Quiet cues when recording starts, stops or fails.
    pub sounds: bool,
    /// Spoken commands like "new line" and "press enter" (`commands/`).
    pub commands: bool,
    /// Which kinds of spoken command are on, when `commands` is.
    pub command_groups: Groups,
    /// The user's own words: project names, people, jargon (`dictionary/`).
    pub dictionary: Vec<String>,
    /// After transcribing, ChatGPT puts misheard dictionary words right.
    pub fix_names: bool,
    /// Start likh when you sign in to Windows.
    pub start_with_windows: bool,
    /// Look for a newer likh at launch and after dictations, once a day (`update/`).
    pub auto_update: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcut: Binding::default(),
            paste_last: Some(crate::hotkey::PASTE_LAST),
            mouse: MouseTrigger::default(),
            mode: Mode::default(),
            engine: Engine::default(),
            silence_stop: SilenceStop::default(),
            // Only Windows can pause other apps' media today.
            pause_media: cfg!(target_os = "windows"),
            history: true,
            sounds: true,
            commands: true,
            command_groups: Groups::default(),
            dictionary: Vec::new(),
            // Does nothing until there's a word in the dictionary.
            fix_names: true,
            start_with_windows: false,
            auto_update: true,
        }
    }
}

/// `%LOCALAPPDATA%\likh` (or the platform's equivalent). `LIKH_HOME` replaces
/// it, so an agent's test instance keeps its settings, history and onboarding
/// apart from the user's own likh (and gets its own `likh ctl` pipe).
pub fn folder() -> Option<PathBuf> {
    match std::env::var_os("LIKH_HOME") {
        Some(home) => Some(PathBuf::from(home)),
        None => dirs::data_local_dir().map(|dir| dir.join("likh")),
    }
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
