//! Spoken commands: phrases that do something instead of being typed. Four
//! groups, each switched on or off in settings:
//!
//! - **Lines:** "new line", "new paragraph" and "new bullet", anywhere, become breaks.
//! - **Keys:** ending with "press enter" or "press tab" pastes, then presses it.
//! - **Scratch:** "scratch that" (or "undo that", "delete that") said alone undoes
//!   the last paste. Said as its own sentence after others, it drops the sentence
//!   before it: "Meet at two. Scratch that. Meet at three." pastes "Meet at three."
//! - **Punctuation** (off by default, "period" is also a word): "comma",
//!   "period", "question mark" and friends become the mark.
//!
//! The transcriber adds its own punctuation and capitals ("Hi. New line. Bye."),
//! so matching ignores case and eats the punctuation around a command.

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Paste `text` (nothing if it's empty), then press `then`.
    Paste { text: String, then: Option<Press> },
    /// Undo the last paste (Ctrl+Z in the app that has focus).
    Undo,
}

/// A key pressed after the paste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Enter,
    Tab,
}

impl Press {
    pub fn name(self) -> &'static str {
        match self {
            Press::Enter => "Enter",
            Press::Tab => "Tab",
        }
    }
}

/// Which groups are on. Saved in settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Groups {
    pub lines: bool,
    pub keys: bool,
    pub scratch: bool,
    pub punctuation: bool,
}

impl Groups {
    pub const DEFAULT: Groups = Groups { lines: true, keys: true, scratch: true, punctuation: false };

    /// Every phrase that does something with these groups on, for the cleanup
    /// pass to leave alone.
    pub fn phrases(&self) -> Vec<&'static str> {
        Group::ALL
            .iter()
            .filter(|group| group.is_on(self))
            .flat_map(|group| group.phrases().iter().map(|(phrase, _)| *phrase))
            .collect()
    }
}

impl Default for Groups {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Lines,
    Keys,
    Scratch,
    Punctuation,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Lines, Group::Keys, Group::Scratch, Group::Punctuation];

    pub fn name(self) -> &'static str {
        match self {
            Group::Lines => "Lines and lists",
            Group::Keys => "Press a key",
            Group::Scratch => "Take it back",
            Group::Punctuation => "Punctuation by name",
        }
    }

    pub fn explain(self) -> &'static str {
        match self {
            Group::Lines => "Anywhere in what you say.",
            Group::Keys => "At the very end, after the text lands.",
            Group::Scratch => {
                "Alone, it undoes the last paste in the app it landed in. After a sentence, it drops that sentence."
            }
            Group::Punctuation => {
                "For when the transcriber misses a mark. Off by default: \u{201c}period\u{201d} is also a word."
            }
        }
    }

    /// What you say, and what you get.
    pub fn phrases(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Group::Lines => {
                &[("new line", "\u{21b5}"), ("new paragraph", "\u{21b5}\u{21b5}"), ("new bullet", "\u{21b5} -")]
            }
            Group::Keys => &[("press enter", "Enter"), ("press tab", "Tab")],
            Group::Scratch => &[("scratch that", "undo"), ("undo that", "undo"), ("delete that", "undo")],
            Group::Punctuation => &[
                ("comma", ","),
                ("period", "."),
                ("question mark", "?"),
                ("exclamation mark", "!"),
                ("colon", ":"),
                ("semicolon", ";"),
            ],
        }
    }

    pub fn is_on(self, groups: &Groups) -> bool {
        match self {
            Group::Lines => groups.lines,
            Group::Keys => groups.keys,
            Group::Scratch => groups.scratch,
            Group::Punctuation => groups.punctuation,
        }
    }

    pub fn set(self, groups: &mut Groups, on: bool) {
        match self {
            Group::Lines => groups.lines = on,
            Group::Keys => groups.keys = on,
            Group::Scratch => groups.scratch = on,
            Group::Punctuation => groups.punctuation = on,
        }
    }
}

const UNDO: [&str; 3] = ["scratch that", "undo that", "delete that"];
const BREAKS: [(&str, &str); 3] = [("new paragraph", "\n\n"), ("new line", "\n"), ("new bullet", "\n- ")];
const KEYS: [(&str, Press); 2] = [("press enter", Press::Enter), ("press tab", Press::Tab)];
/// Longest first, so "exclamation point" isn't read as something shorter.
const MARKS: [(&str, &str); 8] = [
    ("exclamation point", "!"),
    ("exclamation mark", "!"),
    ("question mark", "?"),
    ("full stop", "."),
    ("semicolon", ";"),
    ("period", "."),
    ("comma", ","),
    ("colon", ":"),
];

pub fn interpret(transcript: &str, groups: &Groups) -> Action {
    let bare: String =
        transcript.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect::<String>().to_lowercase();
    if groups.scratch && UNDO.contains(&bare.split_whitespace().collect::<Vec<_>>().join(" ").as_str()) {
        return Action::Undo;
    }

    let mut text = transcript.trim().to_string();
    let then = if groups.keys {
        KEYS.iter().find(|(phrase, _)| strip_ending(&mut text, phrase)).map(|(_, press)| *press)
    } else {
        None
    };
    if groups.scratch {
        for phrase in UNDO {
            text = scratch(&text, phrase);
        }
    }
    if groups.lines {
        for (phrase, replacement) in BREAKS {
            while let Some(at) = find(&text, phrase) {
                // Eat the punctuation and spaces the transcriber put around the command.
                let start = text[..at].trim_end_matches([',', ' ']).len();
                let end = skip_marks(&text, at + phrase.len());
                let rest = capitalize_first(&text[end..]);
                text = format!("{}{replacement}{rest}", &text[..start]);
            }
        }
    }
    if groups.punctuation {
        for (phrase, mark) in MARKS {
            while let Some(at) = find(&text, phrase) {
                let start = text[..at].trim_end_matches([',', '.', ' ']).len();
                let end = skip_marks(&text, at + phrase.len());
                let rest = &text[end..];
                let rest = if matches!(mark, "." | "?" | "!") { capitalize_first(rest) } else { rest.to_string() };
                let gap = if rest.is_empty() || rest.starts_with('\n') { "" } else { " " };
                text = format!("{}{mark}{gap}{rest}", &text[..start]);
            }
        }
    }
    Action::Paste { text: text.trim_matches(' ').to_string(), then }
}

/// "Scratch that" as its own sentence (after a sentence mark, before one or
/// the end) drops the sentence before it. Inside a sentence it's just words.
fn scratch(text: &str, phrase: &str) -> String {
    let mut text = text.to_string();
    let mut from = 0;
    while let Some(found) = find(&text[from..], phrase) {
        let at = from + found;
        let before = text[..at].trim_end_matches(' ');
        let after = &text[at + phrase.len()..];
        let own_sentence = before.is_empty() || before.ends_with(['.', '!', '?', ',', '\n']);
        let ends = after.is_empty() || after.starts_with(['.', '!', ',', '?']);
        if !own_sentence || !ends {
            from = at + phrase.len();
            continue;
        }
        let kept = before.trim_end_matches(['.', '!', '?', ',', ' ']);
        let sentence_start = kept.rfind(['.', '!', '?', '\n']).map_or(0, |i| i + 1);
        let end = skip_marks(&text, at + phrase.len());
        let head = text[..sentence_start].trim_end_matches(' ').to_string();
        let rest = capitalize_first(&text[end..]);
        from = head.len();
        text = if head.is_empty() || rest.is_empty() { format!("{head}{rest}") } else { format!("{head} {rest}") };
    }
    text
}

/// Past the punctuation and spaces that follow a command.
fn skip_marks(text: &str, after: usize) -> usize {
    after + text[after..].len() - text[after..].trim_start_matches(['.', ',', '!', '?', ' ']).len()
}

/// Case-insensitive search on whole words.
fn find(text: &str, phrase: &str) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    lower.match_indices(phrase).map(|(at, _)| at).find(|&at| {
        let before = lower[..at].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        let after = lower[at + phrase.len()..].chars().next().is_none_or(|c| !c.is_alphanumeric());
        before && after
    })
}

/// Removes `phrase` from the end of `text` (with its punctuation). True if it was there.
fn strip_ending(text: &mut String, phrase: &str) -> bool {
    let trimmed = text.trim_end_matches(['.', '!', ',', ' ']);
    let lower = trimmed.to_ascii_lowercase();
    if !lower.ends_with(phrase) || find(trimmed, phrase) != Some(trimmed.len() - phrase.len()) {
        return false;
    }
    let kept = trimmed[..trimmed.len() - phrase.len()].trim_end_matches([',', ' ']).to_string();
    *text = kept;
    true
}

fn capitalize_first(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}
