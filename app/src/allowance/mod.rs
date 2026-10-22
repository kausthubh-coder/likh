//! ChatGPT's allowance for Codex model calls. Transcription doesn't use it;
//! Clean up, fixing names, the action router and an action's `ask` do. On
//! ChatGPT Free it runs out for the month: ChatGPT answers 429
//! `usage_limit_reached` with the moment it resets (seen 2026-10-21).
//!
//! Once it says so, likh stops asking until that moment, so each dictation
//! pastes what it heard without a doomed request first. The reset is kept in
//! `allowance.json` in likh's folder, so a restart doesn't ask again. The first
//! dictation after it ran out says when cleanup comes back; the Transcription
//! tab says it for as long as it lasts.
//!
//! `LIKH_ALLOWANCE_OUT=<unix seconds>` answers every model call the way ChatGPT
//! does when the allowance ran out, resetting then, so this can be proved
//! without spending one.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
mod tests;

/// When ChatGPT names no reset, ask again after this long.
const UNNAMED_RESET: u64 = 60 * 60;

/// The reset of an allowance that just ran out, until a dictation has said so (0: none).
static NEWS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    resets_at: u64,
}

pub fn file() -> Option<PathBuf> {
    crate::settings::folder().map(|dir| dir.join("allowance.json"))
}

/// When the allowance comes back, if it has run out and hasn't yet.
pub fn paused_until(file: &Path, now: u64) -> Option<u64> {
    let saved: Saved = serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
    (saved.resets_at > now).then_some(saved.resets_at)
}

/// The same, for likh's own folder.
pub fn paused_until_now() -> Option<u64> {
    file().and_then(|file| paused_until(&file, now()))
}

/// When ChatGPT's answer says the allowance ran out: the reset it names (or
/// an hour from `now` when it names none). None for any other answer, a
/// passing rate limit included.
pub fn read_limit(status: u16, body: &str, now: u64) -> Option<u64> {
    if status != 429 || !body.contains("usage_limit_reached") {
        return None;
    }
    let json = serde_json::from_str::<serde_json::Value>(body).ok();
    let field = |name: &str| {
        json.as_ref()
            .and_then(|json| json.get(name).or_else(|| json.get("error").and_then(|e| e.get(name))))
            .and_then(serde_json::Value::as_u64)
    };
    let resets_at = field("resets_at").or_else(|| field("resets_in_seconds").map(|seconds| now + seconds));
    Some(resets_at.filter(|at| *at > now).unwrap_or(now + UNNAMED_RESET))
}

/// Keeps the reset, so no model call goes out until then, and leaves the news for the next dictation.
pub fn record(file: &Path, resets_at: u64) -> Result<(), String> {
    NEWS.store(resets_at, Ordering::SeqCst);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string(&Saved { resets_at }).map_err(|error| error.to_string())?;
    std::fs::write(file, text).map_err(|error| error.to_string())
}

/// The reset of an allowance that just ran out, once: the dictation that hit it says so.
pub fn take_news() -> Option<u64> {
    Some(NEWS.swap(0, Ordering::SeqCst)).filter(|at| *at > 0)
}

/// What a model call answers while it's out: ChatGPT's words, not a request.
pub fn refused(resets_at: u64) -> String {
    format!("ChatGPT's allowance for your plan ran out until {}", when(resets_at, now()))
}

/// The pill, once, on the dictation that found it ran out.
pub fn news_line(resets_at: u64) -> String {
    format!("Clean up is off until {}: ChatGPT's allowance ran out.", when(resets_at, now()))
}

/// The Transcription tab, while it lasts.
pub fn paused_line(resets_at: u64) -> String {
    format!("Paused until {}: your plan's allowance ran out.", when(resets_at, now()))
}

/// "17 Nov", or the time when it's today ("18:17"), on the user's own clock.
pub fn when(resets_at: u64, now: u64) -> String {
    let local = |at: u64| {
        chrono::DateTime::from_timestamp(i64::try_from(at).unwrap_or(0), 0)
            .unwrap_or_default()
            .with_timezone(&chrono::Local)
    };
    let (reset, today) = (local(resets_at), local(now));
    if reset.date_naive() == today.date_naive() {
        reset.format("%H:%M").to_string()
    } else {
        reset.format("%-d %b").to_string()
    }
}

/// `LIKH_ALLOWANCE_OUT=<unix seconds>`: ChatGPT's answer when the allowance ran out, resetting then.
pub fn fake_answer() -> Option<(u16, String)> {
    let resets_at: u64 = std::env::var("LIKH_ALLOWANCE_OUT").ok()?.trim().parse().ok()?;
    let body = serde_json::json!({
        "error": {
            "type": "usage_limit_reached",
            "message": "The usage limit has been reached",
            "plan_type": "free",
            "resets_at": resets_at,
        }
    });
    Some((429, body.to_string()))
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}
