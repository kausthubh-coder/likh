//! The user's own words: project names, people, jargon, spelled their way.
//!
//! Two ways they reach a dictation:
//!
//! - Groq's Whisper reads them as its prompt before it listens (`whisper_prompt`).
//! - After any engine, the ChatGPT pass in `cleanup/` swaps a misheard word
//!   ("currently") for the entry it was meant to be ("Critly").

#[cfg(test)]
mod tests;

pub const MAX_WORDS: usize = 100;
pub const MAX_WORD_CHARS: usize = 48;

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
