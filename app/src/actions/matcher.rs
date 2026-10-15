//! What you said against every action's phrases, on this computer, before
//! anything goes to ChatGPT. Case, punctuation and apostrophes don't count
//! ("New thread, fix it." matches `new thread {text}`); the words do, in order.

use super::{Action, Token};
use std::collections::BTreeMap;

/// Slot names an action's steps already use for other things.
const RESERVED: [&str; 6] = ["said", "result", "selection", "clipboard", "app", "title"];

/// A word of what was said: how it compares, and where it sits in the original text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Word {
    norm: String,
    start: usize,
}

fn split(text: &str) -> Vec<Word> {
    let mut found = Vec::new();
    let mut current: Option<(usize, String)> = None;
    for (at, c) in text.char_indices() {
        if c.is_alphanumeric() {
            let (_, norm) = current.get_or_insert_with(|| (at, String::new()));
            norm.extend(c.to_lowercase());
        } else if matches!(c, '\'' | '\u{2019}') && current.is_some() {
            // "what's" compares as "whats".
        } else if let Some((start, norm)) = current.take() {
            found.push(Word { norm, start });
        }
    }
    if let Some((start, norm)) = current {
        found.push(Word { norm, start });
    }
    found
}

/// The words of `text` as they compare: lowercase letters and digits.
pub fn words(text: &str) -> Vec<String> {
    split(text).into_iter().map(|word| word.norm).collect()
}

/// A phrase from a file, as words and slots. A `{slot}` named after one of
/// the action's lists matches a name from it; any other slot catches the rest
/// of what was said, so it must come last.
pub fn pattern(phrase: &str, lists: &BTreeMap<String, Vec<(Vec<String>, String)>>) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    for part in phrase.split_whitespace() {
        if matches!(tokens.last(), Some(Token::Free(_))) {
            return Err(format!("in \"{phrase}\", a slot that catches the rest must be the last thing"));
        }
        if let Some(name) = part.strip_prefix('{').and_then(|rest| rest.strip_suffix('}')) {
            let valid = name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
            if !valid {
                return Err(format!(
                    "in \"{phrase}\", {{{name}}} isn't a slot name: use lowercase letters, digits and _"
                ));
            }
            if RESERVED.contains(&name) {
                return Err(format!("in \"{phrase}\", {{{name}}} is taken; name the slot something else"));
            }
            if tokens.iter().any(|t| matches!(t, Token::List(n) | Token::Free(n) if n == name)) {
                return Err(format!("in \"{phrase}\", {{{name}}} comes twice"));
            }
            tokens.push(if lists.contains_key(name) { Token::List(name.into()) } else { Token::Free(name.into()) });
        } else if part.contains(['{', '}']) {
            return Err(format!("in \"{phrase}\", put spaces around {{slots}}"));
        } else {
            tokens.extend(words(part).into_iter().map(Token::Word));
        }
    }
    if !tokens.iter().any(|t| matches!(t, Token::Word(_))) {
        return Err(format!("\"{phrase}\" needs at least one word besides slots"));
    }
    Ok(tokens)
}

/// What two phrases would both match, for telling clashes apart: `new thread <rest>`.
pub fn shape(pattern: &[Token]) -> String {
    pattern
        .iter()
        .map(|token| match token {
            Token::Word(word) => word.clone(),
            Token::List(name) => format!("<{name}>"),
            Token::Free(_) => "<rest>".into(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The action a phrase matched, and what each of its slots heard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Index into the actions searched.
    pub action: usize,
    pub slots: BTreeMap<String, String>,
    /// Another action that was likely meant, so ChatGPT gets to pick first:
    /// one whose phrase spelled out more of what was said, then stalled on a
    /// list name ("new thread in Krita fix it" matches `new thread {text}`,
    /// but `new thread in {project} {text}` got further), or one that starts
    /// the same way and has a list name in what was said ("new thread and
    /// lick fix it": "in" heard as "and", and "lick" is a project).
    pub near_miss: Option<usize>,
}

/// The most specific match among the actions that are on: more spelled-out
/// words beat a list name, which beats catching the rest. Ties go to the
/// file that sorts first.
pub fn find(actions: &[Action], said: &str) -> Option<Found> {
    let heard = split(said);
    let mut best: Option<(usize, &[Token], Found)> = None;
    for (index, action) in actions.iter().enumerate().filter(|(_, action)| action.enabled) {
        for pattern in &action.patterns {
            if let Some((score, slots)) = match_at(pattern, &action.lists, &heard, said)
                && best.as_ref().is_none_or(|(top, _, _)| score > *top)
            {
                best = Some((score, pattern, Found { action: index, slots, near_miss: None }));
            }
        }
    }
    let (_, winner, mut found) = best?;
    let spelled = winner.iter().filter(|token| matches!(token, Token::Word(_))).count();
    found.near_miss = actions
        .iter()
        .enumerate()
        .filter(|(index, action)| action.enabled && *index != found.action)
        .find(|(_, action)| {
            action.patterns.iter().any(|pattern| {
                stalls_on_list(pattern, &action.lists, &heard) > Some(spelled)
                    || (pattern.first() == winner.first() && names_from_list(pattern, &action.lists, &heard))
            })
        })
        .map(|(index, _)| index);
    Some(found)
}

/// Whether one of the names in `pattern`'s lists was said, anywhere.
fn names_from_list(pattern: &[Token], lists: &Lists, heard: &[Word]) -> bool {
    pattern.iter().any(|token| {
        let Token::List(name) = token else { return false };
        lists.get(name).is_some_and(|list| {
            list.iter().any(|(spoken, _)| {
                !spoken.is_empty()
                    && heard.windows(spoken.len()).any(|run| run.iter().zip(spoken).all(|(w, s)| w.norm == *s))
            })
        })
    })
}

/// How many spelled-out words of `pattern` matched before it reached a list
/// slot that none of the list's names fit. None when it never got there.
fn stalls_on_list(pattern: &[Token], lists: &Lists, heard: &[Word]) -> Option<usize> {
    for (at, token) in pattern.iter().enumerate() {
        match token {
            Token::Word(word) => {
                if heard.get(at).is_none_or(|next| next.norm != *word) {
                    return None;
                }
            }
            Token::List(name) => {
                let rest = heard.get(at..).unwrap_or_default();
                let fits = lists.get(name)?.iter().any(|(spoken, _)| {
                    rest.len() >= spoken.len() && rest.iter().zip(spoken).all(|(w, s)| w.norm == *s)
                });
                return (!fits).then_some(at);
            }
            Token::Free(_) => return None,
        }
    }
    None
}

type Lists = BTreeMap<String, Vec<(Vec<String>, String)>>;

fn match_at(pattern: &[Token], lists: &Lists, heard: &[Word], said: &str) -> Option<(usize, BTreeMap<String, String>)> {
    let (first, rest) = match pattern.split_first() {
        Some(split) => split,
        None => return heard.is_empty().then(|| (0, BTreeMap::new())),
    };
    match first {
        Token::Word(word) => {
            let (next, after) = heard.split_first()?;
            if next.norm != *word {
                return None;
            }
            let (score, slots) = match_at(rest, lists, after, said)?;
            Some((score + 3, slots))
        }
        Token::List(name) => {
            let mut best: Option<(usize, BTreeMap<String, String>)> = None;
            for (spoken, value) in lists.get(name)? {
                let fits = heard.len() >= spoken.len() && heard.iter().zip(spoken).all(|(w, s)| w.norm == *s);
                if !fits {
                    continue;
                }
                if let Some((score, mut slots)) = match_at(rest, lists, &heard[spoken.len()..], said) {
                    let score = score + 2 * spoken.len();
                    if best.as_ref().is_none_or(|(top, _)| score > *top) {
                        slots.insert(name.clone(), value.clone());
                        best = Some((score, slots));
                    }
                }
            }
            best
        }
        Token::Free(name) => {
            let start = heard.first()?.start;
            // The transcriber's own end mark ("fix it.") isn't part of what was meant.
            let caught = said[start..].trim().trim_end_matches(['.', ',', ';', ':', '!', '?']).trim_end().to_owned();
            Some((1, BTreeMap::from([(name.clone(), caught)])))
        }
    }
}

/// Puts the user's own spelling back on dictionary words in slot text
/// ("critly" becomes "Critly"), on this computer: an action's words never go through cleanup.
pub fn respell(text: &str, dictionary: &[String]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for word in split(text) {
        let end = text[word.start..]
            .char_indices()
            .find(|(_, c)| !(c.is_alphanumeric() || matches!(c, '\'' | '\u{2019}')))
            .map_or(text.len(), |(offset, _)| word.start + offset);
        let original = &text[word.start..end];
        out.push_str(&text[last..word.start]);
        match dictionary.iter().find(|entry| words(entry) == [word.norm.clone()]) {
            Some(entry) if !entry.contains(char::is_whitespace) => out.push_str(entry),
            _ => out.push_str(original),
        }
        last = end;
    }
    out.push_str(&text[last..]);
    out
}
