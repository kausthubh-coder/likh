//! Spoken commands: a few phrases that do something instead of being typed.
//!
//! - "new line" and "new paragraph", anywhere, become line breaks.
//! - Ending with "press enter" pastes, then presses Enter (send the message).
//! - Saying only "scratch that" (or "undo that", "delete that") undoes the last paste.
//!
//! The transcriber adds its own punctuation and capitals ("Hi. New line. Bye."),
//! so matching ignores case and eats the punctuation around a command.

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Paste {
        text: String,
        then_enter: bool,
    },
    /// Undo the last paste (Ctrl+Z in the app that has focus).
    Undo,
}

const UNDO: [&str; 3] = ["scratch that", "undo that", "delete that"];
const BREAKS: [(&str, &str); 2] = [("new paragraph", "\n\n"), ("new line", "\n")];
const ENTER: &str = "press enter";

pub fn interpret(transcript: &str) -> Action {
    let bare: String =
        transcript.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect::<String>().to_lowercase();
    if UNDO.contains(&bare.split_whitespace().collect::<Vec<_>>().join(" ").as_str()) {
        return Action::Undo;
    }

    let mut text = transcript.trim().to_string();
    let then_enter = strip_ending(&mut text, ENTER);
    for (phrase, replacement) in BREAKS {
        while let Some(at) = find(&text, phrase) {
            // Eat the punctuation and spaces the transcriber put around the command.
            let start = text[..at].trim_end_matches([',', ' ']).trim_end_matches([' ']).len();
            let after = at + phrase.len();
            let end = after + text[after..].len() - text[after..].trim_start_matches(['.', ',', '!', ' ']).len();
            let rest = capitalize_first(&text[end..]);
            text = format!("{}{replacement}{rest}", &text[..start]);
        }
    }
    Action::Paste { text: text.trim_matches(' ').to_string(), then_enter }
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
