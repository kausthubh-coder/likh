//! An action that runs programs runs only after you approved it in likh's
//! Actions tab, and again after any change to its file or its scripts. An
//! agent (or a shared file) can write an action, but only you can make it
//! runnable: there is no `likh ctl` for approving.
//!
//! An approval is a copy of the files as you saw them, in
//! `LIKH_HOME/approved/<action>.json`, so Review can show what changed since.

use super::Action;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    /// Runs no programs, or runs exactly what you approved.
    Approved,
    /// Never approved.
    New,
    /// Approved once, changed since.
    Changed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Record {
    /// File name to its contents, as approved.
    files: BTreeMap<String, String>,
}

/// Where approvals are kept, next to the settings.
pub fn store() -> Option<PathBuf> {
    crate::settings::folder().map(|dir| dir.join("approved"))
}

/// The files an approval covers, as they are now.
fn current(action: &Action) -> BTreeMap<String, String> {
    std::iter::once(action.file.clone())
        .chain(action.script_files())
        .map(|path| {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            // A file that can't be read can't match what was approved.
            (name, std::fs::read_to_string(&path).unwrap_or_else(|_| "\u{0}unreadable".into()))
        })
        .collect()
}

fn record_path(store: &Path, action: &Action) -> PathBuf {
    store.join(format!("{}.json", action.id))
}

fn approved(store: &Path, action: &Action) -> Option<Record> {
    let text = std::fs::read_to_string(record_path(store, action)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn state(action: &Action) -> Trust {
    match store() {
        Some(store) => state_in(&store, action),
        None => Trust::New,
    }
}

pub fn state_in(store: &Path, action: &Action) -> Trust {
    if !action.runs_programs() {
        return Trust::Approved;
    }
    match approved(store, action) {
        None => Trust::New,
        Some(record) if record.files == current(action) => Trust::Approved,
        Some(_) => Trust::Changed,
    }
}

/// Approves the action's files as they are right now.
pub fn approve_in(store: &Path, action: &Action) -> Result<(), String> {
    std::fs::create_dir_all(store).map_err(|error| error.to_string())?;
    let record = Record { files: current(action) };
    let text = serde_json::to_string_pretty(&record).map_err(|error| error.to_string())?;
    std::fs::write(record_path(store, action), text).map_err(|error| error.to_string())
}

/// One line of a file's change since it was approved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Same(String),
    Added(String),
    Removed(String),
}

/// A file and how it changed; a file never approved shows as all added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub file: String,
    pub lines: Vec<Line>,
}

impl Change {
    pub fn counts(&self) -> (usize, usize) {
        let added = self.lines.iter().filter(|l| matches!(l, Line::Added(_))).count();
        let removed = self.lines.iter().filter(|l| matches!(l, Line::Removed(_))).count();
        (added, removed)
    }
}

/// What changed in each file since it was approved. Files that didn't change are left out.
pub fn changes_in(store: &Path, action: &Action) -> Vec<Change> {
    let before = approved(store, action).unwrap_or_default().files;
    current(action)
        .into_iter()
        .filter(|(name, now)| before.get(name) != Some(now))
        .map(|(name, now)| {
            let was = before.get(&name).map(String::as_str).unwrap_or_default();
            Change { lines: diff(was, &now), file: name }
        })
        .collect()
}

/// A line diff by longest common subsequence. Action files and their scripts are short.
pub fn diff(before: &str, after: &str) -> Vec<Line> {
    let (a, b): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
    if a.len().saturating_mul(b.len()) > 4_000_000 {
        let mut lines: Vec<Line> = a.iter().map(|l| Line::Removed((*l).into())).collect();
        lines.extend(b.iter().map(|l| Line::Added((*l).into())));
        return lines;
    }
    // common[i][j]: the longest common run of a[i..] and b[j..].
    let mut common = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            common[i][j] = if a[i] == b[j] { common[i + 1][j + 1] + 1 } else { common[i + 1][j].max(common[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut lines) = (0, 0, Vec::new());
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            lines.push(Line::Same(a[i].into()));
            i += 1;
            j += 1;
        } else if common[i + 1][j] >= common[i][j + 1] {
            lines.push(Line::Removed(a[i].into()));
            i += 1;
        } else {
            lines.push(Line::Added(b[j].into()));
            j += 1;
        }
    }
    lines.extend(a[i..].iter().map(|l| Line::Removed((*l).into())));
    lines.extend(b[j..].iter().map(|l| Line::Added((*l).into())));
    lines
}
