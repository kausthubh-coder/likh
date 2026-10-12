//! Groq's hosted Whisper with the user's own key. Groq's free tier covers
//! about 2,000 dictations a day with no card (console.groq.com/docs/rate-limits),
//! and it answers faster than anything else we tried.
//!
//! The key lives in the OS credential store (Windows Credential Manager, the
//! macOS keychain), never in settings.json, and never in logs or status.

use super::{Failure, agent, boundary, multipart, read_text};

const TRANSCRIPTIONS: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
const MODELS: &str = "https://api.groq.com/openai/v1/models";
/// Fast and cheap, and accurate enough for dictation. Groq also has
/// `whisper-large-v3`, slower for a small accuracy gain.
const MODEL: &str = "whisper-large-v3-turbo";
pub const KEYS_URL: &str = "https://console.groq.com/keys";
const USER_AGENT: &str = concat!("likh/", env!("CARGO_PKG_VERSION"));
/// Where the key is filed in the credential store.
const SERVICE: &str = "likh";
const ACCOUNT: &str = "groq-api-key";

/// The saved key, if there is one.
pub fn saved_key() -> Option<String> {
    keyring::Entry::new(SERVICE, ACCOUNT).ok()?.get_password().ok().filter(|key| !key.trim().is_empty())
}

pub fn has_key() -> bool {
    saved_key().is_some()
}

pub fn save_key(key: &str) -> Result<(), String> {
    keyring::Entry::new(SERVICE, ACCOUNT)
        .and_then(|entry| entry.set_password(key.trim()))
        .map_err(|error| format!("couldn't save the key: {error}"))
}

pub fn forget_key() -> Result<(), String> {
    match keyring::Entry::new(SERVICE, ACCOUNT).and_then(|entry| entry.delete_credential()) {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!("couldn't remove the key: {error}")),
    }
}

/// Groq keys start with `gsk_`. Catches a pasted sentence or a different
/// service's key before a request is made.
pub fn looks_like_a_key(key: &str) -> bool {
    let key = key.trim();
    key.starts_with("gsk_") && key.len() >= 20 && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Asks Groq whether `key` works, without spending any audio quota.
pub fn verify(key: &str) -> Result<(), Failure> {
    let mut response = agent(0.0, USER_AGENT)
        .get(MODELS)
        .header("Authorization", format!("Bearer {}", key.trim()))
        .call()
        .map_err(|_| Failure::GroqOffline)?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string().unwrap_or_default();
    match interpret(status, &body) {
        // The models list isn't a transcript; any success means the key is good.
        Err(Failure::Unreadable) if (200..300).contains(&status) => Ok(()),
        Err(failure) => Err(failure),
        Ok(_) => Ok(()),
    }
}

pub fn transcribe(key: &str, wav: &[u8], seconds: f32) -> Result<String, Failure> {
    let boundary = boundary();
    // temperature 0 keeps Whisper from getting creative with quiet audio.
    let body = multipart(&boundary, wav, &[("model", MODEL), ("response_format", "json"), ("temperature", "0")]);
    let mut response = agent(seconds, USER_AGENT)
        .post(TRANSCRIPTIONS)
        .header("Authorization", format!("Bearer {}", key.trim()))
        .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
        .send(&body[..])
        .map_err(|_| Failure::GroqOffline)?;
    let status = response.status().as_u16();
    let text = response.body_mut().read_to_string().map_err(|_| Failure::Unreadable)?;
    interpret(status, &text)
}

/// Maps Groq's answer to text or a failure. Groq speaks OpenAI's error shape:
/// `{"error":{"message":"...","type":"...","code":"..."}}`.
pub fn interpret(status: u16, body: &str) -> Result<String, Failure> {
    match status {
        200..=299 => read_text(body),
        401 | 403 => Err(Failure::GroqRejected),
        429 => Err(Failure::GroqRateLimited),
        413 => Err(Failure::TooLong),
        _ if body.contains("too short") || body.contains("audio_too_short") => Err(Failure::TooShort),
        code => Err(Failure::GroqServer(code)),
    }
}
