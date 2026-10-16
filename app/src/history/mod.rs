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
    /// What was pasted; for an action that pasted nothing, what was said.
    pub text: String,
    pub seconds: f32,
    /// The action that ran, when the action key was used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// The text landed at the cursor, so paste last can put it in again.
    #[serde(default = "pasted", skip_serializing_if = "is_pasted")]
    pub pasted: bool,
}

fn pasted() -> bool {
    true
}

fn is_pasted(pasted: &bool) -> bool {
    *pasted
}

/// The newest entry whose text was pasted: what paste last puts in after a restart.
pub fn last_pasted(path: &Path) -> Option<Entry> {
    recent(path, 50).into_iter().find(|entry| entry.pasted)
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

/// The typing speed "faster than typing" is measured against: a common average for adults.
pub const TYPING_PER_MINUTE: f32 = 40.0;

/// What the History tab sums up from every kept dictation.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Stats {
    pub words: usize,
    /// Today and the six days before it.
    pub words_this_week: usize,
    /// Words over the minutes recorded, pauses included; None before anything was recorded.
    pub per_minute: Option<u32>,
    /// Days in a row with a dictation, up to today. Today without one yet doesn't break it.
    pub streak: u32,
    /// Typing the same words at `TYPING_PER_MINUTE`, less the time spent talking.
    pub minutes_saved: u32,
}

/// Sums up `entries`. `day_of` turns a Unix time into a day number on the user's calendar, and
/// `today` is today's. Action presses are left out: their text can be what ChatGPT wrote.
/// Words are counted by spaces, so Chinese or Japanese without spaces counts low.
pub fn stats(entries: &[Entry], today: i32, day_of: impl Fn(u64) -> i32) -> Stats {
    let dictations: Vec<&Entry> = entries.iter().filter(|entry| entry.action.is_none()).collect();
    let words_in = |entry: &Entry| entry.text.split_whitespace().count();
    let words: usize = dictations.iter().map(|entry| words_in(entry)).sum();
    let words_this_week =
        dictations.iter().filter(|entry| day_of(entry.at) > today - 7).map(|entry| words_in(entry)).sum();
    let minutes: f32 = dictations.iter().map(|entry| entry.seconds.max(0.0)).sum::<f32>() / 60.0;
    let per_minute = (minutes > 0.0).then(|| (words as f32 / minutes).round() as u32);
    let minutes_saved = (words as f32 / TYPING_PER_MINUTE - minutes).max(0.0).round() as u32;

    let days: std::collections::BTreeSet<i32> = dictations.iter().map(|entry| day_of(entry.at)).collect();
    let mut day = if days.contains(&today) { today } else { today - 1 };
    let mut streak = 0;
    while days.contains(&day) {
        streak += 1;
        day -= 1;
    }
    Stats { words, words_this_week, per_minute, streak, minutes_saved }
}

/// The day `at` (Unix seconds) falls on in this computer's time zone, as a day number.
pub fn local_day(at: u64) -> i32 {
    use chrono::Datelike;
    let utc = chrono::DateTime::from_timestamp(i64::try_from(at).unwrap_or(0), 0).unwrap_or_default();
    utc.with_timezone(&chrono::Local).date_naive().num_days_from_ce()
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
