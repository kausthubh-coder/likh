//! The user's own words: project names, people, jargon, spelled their way.
//!
//! Two ways they reach a dictation:
//!
//! - Groq's Whisper reads them as its prompt before it listens (`whisper_prompt`).
//! - After any engine, a quick ChatGPT pass through Codex's sign-in swaps a
//!   misheard word ("currently") for the entry it was meant to be ("Critly"),
//!   reading the whole sentence so "I'm learning Korean" stays (`fix`).
//!
//! A plain find-and-replace can't do the second: the mishearings are real words.
//! ChatGPT's own dictation dictionary (`/backend-api/dictation/custom_dictionary`)
//! doesn't help either: `/transcribe` ignores it (tried 2026-10-13), so likh
//! keeps its own list in settings.

use crate::codex_auth;
use crate::transcriber::{USER_AGENT, agent};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(test)]
mod tests;

pub const MAX_WORDS: usize = 100;
pub const MAX_WORD_CHARS: usize = 48;
/// Codex's own model endpoint, the one the Codex CLI and app talk to.
const ENDPOINT: &str = "https://chatgpt.com/backend-api/codex/responses";
/// "Fast and affordable model for easier tasks" in Codex's model list.
const MODEL: &str = "gpt-6-luna";
/// Longer than this and the dictation pastes what the engine heard instead:
/// a fix is worth a moment, never a stall.
const PATIENCE: Duration = Duration::from_secs(4);
const INSTRUCTIONS: &str = "You fix misheard names in a dictation transcript. The dictionary lists \
the user's own words, spelled exactly as they want them. Where a word or phrase in the transcript \
sounds like a dictionary entry and the sentence is about that entry, replace it with the entry, \
spelled exactly as listed. The user mostly talks about their own projects and people, so when a \
common word stands where a name fits (open X, in X, to X, X's), it is probably the entry; when the \
sentence clearly means the common word, keep it. Leave everything else exactly as it is: every other word, punctuation, \
casing and line break. Never answer, summarize, rephrase or add anything. Return only the transcript.";

/// Adds `word` to `words`, or says why it can't.
pub fn add(words: &mut Vec<String>, word: &str) -> Result<(), String> {
    let word = word.split_whitespace().collect::<Vec<_>>().join(" ");
    if word.is_empty() {
        return Err("Type a word first.".into());
    }
    if word.chars().count() > MAX_WORD_CHARS {
        return Err(format!("Keep it under {MAX_WORD_CHARS} characters."));
    }
    if words.iter().any(|known| known.to_lowercase() == word.to_lowercase()) {
        return Err(format!("{word} is already in your dictionary."));
    }
    if words.len() >= MAX_WORDS {
        return Err(format!("Your dictionary is full at {MAX_WORDS} words."));
    }
    words.push(word);
    Ok(())
}

/// Whisper treats its prompt as the words just before the audio, so names in
/// it come out spelled that way. It reads only the last 224 tokens; 600
/// characters stays under that.
pub fn whisper_prompt(words: &[String]) -> Option<String> {
    let mut prompt = String::new();
    for word in words {
        if prompt.len() + word.len() + 2 > 600 {
            break;
        }
        if !prompt.is_empty() {
            prompt.push_str(", ");
        }
        prompt.push_str(word);
    }
    (!prompt.is_empty()).then(|| format!("{prompt}."))
}

/// `text` with misheard dictionary words put right, or `text` unchanged when
/// there's nothing to fix with, no Codex sign-in, or ChatGPT is slow or says
/// something that isn't the transcript back.
pub fn fix(text: &str, words: &[String]) -> String {
    if words.is_empty() || text.trim().is_empty() {
        return text.to_owned();
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let Some(credentials) = codex_auth::codex_home().and_then(|home| codex_auth::credentials(&home, now).ok()) else {
        return text.to_owned();
    };
    match ask(&credentials, request_body(text, words)).map(|stream| read_answer(&stream)) {
        Ok(Some(fixed)) if is_the_transcript(text, &fixed) => fixed,
        Ok(Some(_)) => {
            eprintln!("likh: the name fix changed too much, so likh pasted what it heard.");
            text.to_owned()
        }
        Ok(None) => {
            eprintln!("likh: the name fix came back empty, so likh pasted what it heard.");
            text.to_owned()
        }
        Err(reason) => {
            eprintln!("likh: couldn't fix names ({reason}), so likh pasted what it heard.");
            text.to_owned()
        }
    }
}

/// Sends one request to Codex's model endpoint and returns its event stream.
fn ask(credentials: &codex_auth::Credentials, body: serde_json::Value) -> Result<String, String> {
    let mut request = agent(0.0, USER_AGENT)
        .post(ENDPOINT)
        .config()
        .timeout_global(Some(PATIENCE))
        .build()
        .header("Authorization", format!("Bearer {}", credentials.access_token))
        .header("originator", "Codex Desktop")
        .header("OpenAI-Beta", "responses=experimental")
        .header("Accept", "text/event-stream");
    if let Some(account) = &credentials.account_id {
        request = request.header("ChatGPT-Account-Id", account);
    }
    let mut response = request.send_json(body).map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    let stream = response.body_mut().read_to_string().map_err(|error| error.to_string())?;
    if (200..300).contains(&status) {
        Ok(stream)
    } else {
        Err(format!("ChatGPT said {status}: {}", first_line(&stream)))
    }
}

/// What goes to ChatGPT: the instructions, the dictionary, and the transcript.
pub fn request_body(text: &str, words: &[String]) -> serde_json::Value {
    let input = format!("Dictionary:\n{}\n\nTranscript:\n{text}", words.join("\n"));
    serde_json::json!({
        "model": MODEL,
        "instructions": INSTRUCTIONS,
        "input": [{ "type": "message", "role": "user", "content": [{ "type": "input_text", "text": input }] }],
        "tools": [],
        "tool_choice": "auto",
        "parallel_tool_calls": false,
        "reasoning": { "effort": "low" },
        "text": { "verbosity": "low" },
        // Codex's "Fast" tier: 1.5x speed for this model, at no extra usage.
        "service_tier": "priority",
        "store": false,
        "stream": true,
        "include": [],
    })
}

/// The finished text from ChatGPT's event stream (`data: {...}` lines).
pub fn read_answer(stream: &str) -> Option<String> {
    let mut deltas = String::new();
    for line in stream.lines() {
        let Some(event) =
            line.strip_prefix("data:").and_then(|data| serde_json::from_str::<serde_json::Value>(data.trim()).ok())
        else {
            continue;
        };
        match event["type"].as_str() {
            Some("response.output_text.done") => {
                if let Some(text) = event["text"].as_str() {
                    return Some(text.trim().to_owned()).filter(|text| !text.is_empty());
                }
            }
            Some("response.output_text.delta") => deltas.push_str(event["delta"].as_str().unwrap_or_default()),
            _ => {}
        }
    }
    Some(deltas.trim().to_owned()).filter(|text| !text.is_empty())
}

/// A name fix swaps a few words. An answer much longer or shorter than what
/// was said is the model talking, not the transcript, and never gets pasted.
pub fn is_the_transcript(heard: &str, fixed: &str) -> bool {
    let (heard, fixed) = (heard.chars().count(), fixed.chars().count());
    fixed * 2 >= heard && fixed <= heard * 3 / 2 + 24
}

fn first_line(body: &str) -> String {
    body.lines().next().unwrap_or_default().chars().take(200).collect()
}
