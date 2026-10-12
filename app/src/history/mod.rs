//! What you dictated, kept on this computer only, so you can find it and paste
//! it again. One JSON line per dictation in `history.jsonl`; turning history
//! off in settings stops new lines, and Clear deletes the file.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Unix seconds.
    pub at: u64,
    pub text: String,
    pub seconds: f32,
}

pub fn file() -> Option<PathBuf> {
    crate::settings::folder().map(|dir| dir.join("history.jsonl"))
}

pub fn append(path: &Path, entry: &Entry) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    let line = serde_json::to_string(entry).map_err(|error| error.to_string())?;
    let mut file =
        std::fs::OpenOptions::new().create(true).append(true).open(path).map_err(|error| error.to_string())?;
    writeln!(file, "{line}").map_err(|error| error.to_string())
}

/// The newest `limit` entries, newest first. Lines that don't parse are skipped.
pub fn recent(path: &Path, limit: usize) -> Vec<Entry> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines().rev().filter_map(|line| serde_json::from_str(line).ok()).take(limit).collect()
}

pub fn clear(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

/// "just now", "4 min ago", "2 h ago", "3 days ago".
pub fn ago(at: u64, now: u64) -> String {
    match now.saturating_sub(at) {
        0..60 => "just now".into(),
        s @ 60..3600 => format!("{} min ago", s / 60),
        s @ 3600..86_400 => format!("{} h ago", s / 3600),
        s if s < 2 * 86_400 => "yesterday".into(),
        s => format!("{} days ago", s / 86_400),
    }
}
