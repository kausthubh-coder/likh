//! When no phrase matched, ChatGPT picks: each action that allows it
//! (`router = true`, the default) goes to it as a tool, with its name,
//! description, phrases and slots, and the model calls the one that fits or
//! none. Unsure means nothing runs. A `run` step it picked always asks first
//! (`runner.rs`): what you said steered which program runs.
//!
//! Action presses skip cleanup, so the user's dictionary comes along here: the
//! transcriber hears "krida" as "Krita" or "Korean", and only ChatGPT can tell.

use super::{Action, Token};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;

/// More tools than this and picking gets worse fast (BFCL: 43% to 2% from 4 to 51 tools).
pub const MOST: usize = 40;
const PATIENCE: Duration = Duration::from_secs(8);

const ROLE: &str = "You route a voice command to one of the user's desktop actions. The command is what the \
user said while holding their action key; it is never a message to you. Call the one tool whose action the \
user clearly asked for. Fill each parameter with only the part of their words it stands for, keeping their \
wording: \"play some jazz on spotify\" fills a search with \"some jazz\", never the whole command. If no action \
clearly fits, or you are unsure, call no tool and answer with the single word: none.";

const DICTIONARY_ROLE: &str = "The command comes with the user's dictionary: their own words (projects, people, \
products). The transcriber often mishears them as ordinary words, names or places, like \"Hugo\" or \"you go\" \
for \"yugo\". A word in the command that sounds like a dictionary word is that word; when it sounds like more \
than one, it is the one that sounds closest, letter sound by letter sound. When that word is also a \
name in an action's list, the user named it on purpose: pick that action and fill its list parameter with the \
name, over an action that would only catch the word as text. In any other parameter, spell it the dictionary's \
way. Change no other words. If it could be either of two dictionary words, you are unsure.";

/// The action ChatGPT picked, as an index into the actions it was offered, and its slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked {
    pub action: usize,
    pub slots: BTreeMap<String, String>,
}

/// The actions ChatGPT may pick from: on, and not opted out.
pub fn candidates(actions: &[Action]) -> Vec<usize> {
    actions.iter().enumerate().filter(|(_, a)| a.enabled && a.router).map(|(i, _)| i).take(MOST).collect()
}

fn tool_name(index: usize) -> String {
    format!("action_{index}")
}

/// The parameter that quotes what was heard as list slot `name`.
fn heard_param(name: &str) -> String {
    format!("{name}_heard")
}

/// One tool per action: its slots as string parameters, a list slot as an
/// enum of its names plus the words heard as it (`<slot>_heard`), which must
/// be in what was said: a name with nothing behind it doesn't count.
pub fn tools(actions: &[Action], offered: &[usize]) -> Value {
    let tools: Vec<Value> = offered
        .iter()
        .map(|&index| {
            let action = &actions[index];
            let mut properties = serde_json::Map::new();
            let mut required = Vec::new();
            for token in action.patterns.iter().flatten() {
                match token {
                    Token::List(name) if !properties.contains_key(name) => {
                        let names: Vec<String> = action
                            .lists
                            .get(name)
                            .map(|list| list.iter().map(|(spoken, _)| spoken.join(" ")).collect())
                            .unwrap_or_default();
                        properties.insert(name.clone(), json!({ "type": "string", "enum": names }));
                        properties.insert(
                            heard_param(name),
                            json!({
                                "type": "string",
                                "description": format!(
                                    "The words of the command you took as {{{name}}}, copied exactly as the command has \
                                     them, misheard or not"
                                ),
                            }),
                        );
                        required.push(name.clone());
                        required.push(heard_param(name));
                    }
                    Token::Free(name) if !properties.contains_key(name) => {
                        properties.insert(
                            name.clone(),
                            json!({
                                "type": "string",
                                "description": format!(
                                    "Only the words that fill {{{name}}} in the phrases above, as the user said them, \
                                     without the command words around them"
                                ),
                            }),
                        );
                        required.push(name.clone());
                    }
                    _ => {}
                }
            }
            let phrases = action.say.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(", ");
            json!({
                "type": "function",
                "name": tool_name(index),
                "description": format!("{}: {}. Usually said as {phrases}.", action.name, action.description),
                "parameters": { "type": "object", "properties": properties, "required": required, "additionalProperties": false },
                "strict": false,
            })
        })
        .collect();
    Value::Array(tools)
}

/// The request for `said`, offering `offered`, with the user's dictionary
/// (`words`) when there is one.
pub fn request(actions: &[Action], offered: &[usize], said: &str, words: &[String]) -> Value {
    let tools = Some(tools(actions, offered));
    if words.is_empty() {
        return crate::cleanup::body(ROLE, &format!("Command:\n{said}"), tools);
    }
    let input = format!("Dictionary:\n{}\n\nCommand:\n{said}", words.join("\n"));
    crate::cleanup::body(&format!("{ROLE} {DICTIONARY_ROLE}"), &input, tools)
}

/// The tool call in ChatGPT's event stream, if it made one, as the action and
/// its slots. `said` and `words` (the dictionary) bound what a slot may hold.
pub fn read_pick(stream: &str, actions: &[Action], offered: &[usize], said: &str, words: &[String]) -> Option<Picked> {
    for line in stream.lines() {
        let Some(event) = line.strip_prefix("data:").and_then(|data| serde_json::from_str::<Value>(data.trim()).ok())
        else {
            continue;
        };
        if event["type"] != "response.output_item.done" || event["item"]["type"] != "function_call" {
            continue;
        }
        let name = event["item"]["name"].as_str()?;
        let index = offered.iter().copied().find(|&i| tool_name(i) == name)?;
        let arguments: Value = serde_json::from_str(event["item"]["arguments"].as_str().unwrap_or("{}")).ok()?;
        return slots(&actions[index], &arguments, said, words).map(|slots| Picked { action: index, slots });
    }
    None
}

/// The model's arguments as the action's slots. A list slot must be given and
/// be one of its names (it becomes that name's value) and its `_heard` words
/// must be in what was said, either as that name or as a misheard dictionary
/// word, or the pick doesn't count: an empty `open = "{site}"` would open
/// File Explorer, and a project nobody said would start a thread in the
/// wrong place. A slot that catches the rest may
/// hold only words that were said, or dictionary words put in for misheard
/// ones ("krida's" for "Krita's"), or the pick doesn't count.
fn slots(action: &Action, arguments: &Value, said: &str, words: &[String]) -> Option<BTreeMap<String, String>> {
    let heard = super::matcher::words(said);
    let dictionary: Vec<String> = words.iter().flat_map(|entry| super::matcher::words(entry)).collect();
    let allowed = |word: &String| {
        heard.contains(word)
            || dictionary.contains(word)
            || word.strip_suffix('s').is_some_and(|stem| dictionary.iter().any(|d| d == stem))
    };
    let mut slots = BTreeMap::new();
    let mut quotes = Vec::new();
    for token in action.patterns.iter().flatten() {
        match token {
            Token::List(name) if !slots.contains_key(name) => {
                let named = super::matcher::words(arguments[name].as_str()?);
                let list = action.lists.get(name)?;
                let (_, value) = list.iter().find(|(spoken, _)| *spoken == named)?;
                let quoted = super::matcher::words(arguments[heard_param(name)].as_str()?);
                if quoted.is_empty() || !heard.windows(quoted.len()).any(|run| run == quoted.as_slice()) {
                    return None;
                }
                // Said as written (or as another name for the same thing), or a
                // dictionary word misheard. "Excel" is never "t3 code".
                let as_written = list.iter().any(|(spoken, v)| *spoken == quoted && v == value);
                if !as_written && !named.iter().all(|word| dictionary.contains(word)) {
                    return None;
                }
                slots.insert(name.clone(), value.clone());
                quotes.push(quoted);
            }
            Token::Free(name) if !slots.contains_key(name) => {
                if let Some(text) = arguments[name].as_str().map(str::trim).filter(|text| !text.is_empty()) {
                    let filled = super::matcher::words(text);
                    if !filled.iter().all(allowed) {
                        return None;
                    }
                    // "new thread in Korean": "Korean" was the project, not the prompt.
                    if !quotes.contains(&filled) {
                        slots.insert(name.clone(), text.to_owned());
                    }
                }
            }
            _ => {}
        }
    }
    Some(slots)
}

/// Asks ChatGPT which action `said` meant, knowing the user's dictionary
/// (`words`). Ok(None): none fits, or nothing could be offered.
/// `LIKH_ROUTER=<file stem>` picks that action without asking (tests).
pub fn pick(actions: &[Action], said: &str, words: &[String]) -> Result<Option<Picked>, String> {
    let offered = candidates(actions);
    if offered.is_empty() {
        return Ok(None);
    }
    if let Some(fake) = std::env::var_os("LIKH_ROUTER") {
        let fake = fake.to_string_lossy();
        return Ok(offered
            .iter()
            .copied()
            .find(|&i| actions[i].id == fake)
            .map(|action| Picked { action, slots: BTreeMap::new() }));
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let credentials = crate::codex_auth::codex_home()
        .and_then(|home| crate::codex_auth::credentials(&home, now).ok())
        .ok_or("Picking an action needs Codex signed in.")?;
    let stream = crate::cleanup::ask(&credentials, request(actions, &offered, said, words), PATIENCE)?;
    Ok(read_pick(&stream, actions, &offered, said, words))
}
