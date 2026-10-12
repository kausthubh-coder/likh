//! Turns a recording into text. Two engines, picked in settings:
//!
//! - ChatGPT through Codex's sign-in, `/backend-api/transcribe` (docs/codex.md).
//! - Groq's Whisper with the user's own free key (`groq.rs`).
//!
//! `LIKH_TRANSCRIBER=echo:<text>` swaps in a fake that returns `<text>`, so
//! tests and agents can drive whole dictations without touching any of them.

use crate::codex_auth::{self, CodexStatus, Credentials};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub mod glyph;
pub mod groq;

#[cfg(test)]
mod tests;

pub const SAMPLE_RATE: u32 = 16_000;
const ENDPOINT: &str = "https://chatgpt.com/backend-api/transcribe";
/// What the Codex desktop app sends (docs/codex.md, The request).
const USER_AGENT: &str = "Codex Desktop/26.1007.2314.0 (Windows NT 10.0; x64)";
/// Anything shorter is a stray tap, not speech; the servers reject it anyway.
pub const SHORTEST: Duration = Duration::from_millis(300);

/// Which service turns speech into text. Saved in settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    #[default]
    Codex,
    Groq,
}

/// An engine likh no longer has (`"windows"`, Windows speech, up to
/// 0.1.0-alpha.5) reads as ChatGPT, so an old settings.json keeps the rest of
/// its choices instead of failing to load.
impl<'de> Deserialize<'de> for Engine {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let id = String::deserialize(deserializer)?;
        Ok(if id == Engine::Groq.id() { Engine::Groq } else { Engine::Codex })
    }
}

impl Engine {
    pub const ALL: [Engine; 2] = [Engine::Codex, Engine::Groq];

    pub fn name(self) -> &'static str {
        match self {
            Engine::Codex => "ChatGPT",
            Engine::Groq => "Groq",
        }
    }

    /// One line on what it costs and what it's like.
    pub fn explain(self) -> &'static str {
        match self {
            Engine::Codex => "Your ChatGPT plan, through Codex. The most accurate.",
            Engine::Groq => "Whisper on a free Groq key. The fastest.",
        }
    }

    /// For `likh ctl status`.
    pub fn id(self) -> &'static str {
        match self {
            Engine::Codex => "codex",
            Engine::Groq => "groq",
        }
    }
}

/// What turns one dictation into text: the chosen engine, or the test fake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transcriber {
    Codex,
    Groq,
    Echo(String),
}

impl Transcriber {
    /// `LIKH_TRANSCRIBER=echo:<text>`, if set. It wins over the engine in settings.
    pub fn fake_from_env() -> Option<String> {
        std::env::var("LIKH_TRANSCRIBER").ok().and_then(|v| v.strip_prefix("echo:").map(str::to_owned))
    }

    pub fn new(engine: Engine, fake: Option<&str>) -> Self {
        match (fake, engine) {
            (Some(text), _) => Transcriber::Echo(text.to_owned()),
            (None, Engine::Codex) => Transcriber::Codex,
            (None, Engine::Groq) => Transcriber::Groq,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Transcriber::Codex => Engine::Codex.id(),
            Transcriber::Groq => Engine::Groq.id(),
            Transcriber::Echo(_) => "echo",
        }
    }

    /// `samples` are 16 kHz mono in -1..1. Blocks until the service answers.
    pub fn transcribe(&self, samples: &[f32]) -> Result<String, Failure> {
        if samples.len() < (SAMPLE_RATE as f32 * SHORTEST.as_secs_f32()) as usize {
            return Err(Failure::TooShort);
        }
        let seconds = samples.len() as f32 / SAMPLE_RATE as f32;
        match self {
            Transcriber::Echo(text) => Ok(text.clone()),
            Transcriber::Codex => {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
                let home = codex_auth::codex_home().ok_or(Failure::SignIn(CodexStatus::NotInstalled))?;
                let credentials = codex_auth::credentials(&home, now).map_err(Failure::SignIn)?;
                post(&credentials, &wav(samples), seconds)
            }
            Transcriber::Groq => {
                let key = groq::saved_key().ok_or(Failure::GroqNoKey)?;
                groq::transcribe(&key, &wav(samples), seconds)
            }
        }
    }
}

/// Why a dictation didn't turn into text, in words the pill can show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    TooShort,
    SignIn(CodexStatus),
    Rejected,
    Offline,
    TooLong,
    RateLimited,
    Server(u16),
    Unreadable,
    GroqNoKey,
    GroqRejected,
    GroqRateLimited,
    GroqOffline,
    GroqServer(u16),
}

impl Failure {
    pub fn message(&self) -> String {
        match self {
            Failure::TooShort => "That was too short to hear. Hold the keys while you talk.".into(),
            Failure::SignIn(CodexStatus::Expired(_)) => {
                "Your Codex sign-in expired. Open Codex once to refresh it, then try again.".into()
            }
            Failure::SignIn(CodexStatus::NotInstalled) => {
                "likh needs Codex. Install it, sign in once, then try again.".into()
            }
            Failure::SignIn(_) => "Codex isn't signed in. Open Codex (or run codex login) and sign in.".into(),
            Failure::Rejected => "ChatGPT turned down the sign-in. Sign in to Codex again.".into(),
            Failure::Offline => "Can't reach ChatGPT. Check your connection and try again.".into(),
            Failure::TooLong => "That recording was too long to send. Try a shorter one.".into(),
            Failure::RateLimited => "ChatGPT is busy with your dictations. Try again in a minute.".into(),
            Failure::Server(code) => format!("ChatGPT couldn't transcribe that (error {code}). Try again."),
            Failure::Unreadable => "The transcriber sent back something likh can't read. Try again.".into(),
            Failure::GroqNoKey => "Add your Groq key in likh's settings, under Transcription.".into(),
            Failure::GroqRejected => "Groq turned down your key. Paste a new one in likh's settings.".into(),
            Failure::GroqRateLimited => "You've hit Groq's free limit for now. Try again in a minute.".into(),
            Failure::GroqOffline => "Can't reach Groq. Check your connection and try again.".into(),
            Failure::GroqServer(code) => format!("Groq couldn't transcribe that (error {code}). Try again."),
        }
    }
}

fn post(credentials: &Credentials, wav: &[u8], seconds: f32) -> Result<String, Failure> {
    let boundary = boundary();
    let body = multipart(&boundary, wav, &[]);
    let mut request = agent(seconds, USER_AGENT)
        .post(ENDPOINT)
        .header("Authorization", format!("Bearer {}", credentials.access_token))
        .header("originator", "Codex Desktop")
        .header("Content-Type", format!("multipart/form-data; boundary={boundary}"));
    if let Some(account) = &credentials.account_id {
        request = request.header("ChatGPT-Account-Id", account);
    }
    let mut response = request.send(&body[..]).map_err(|_| Failure::Offline)?;
    let status = response.status().as_u16();
    let text = response.body_mut().read_to_string().map_err(|_| Failure::Unreadable)?;
    interpret(status, &text)
}

/// An HTTP client whose timeout grows with the recording: uploads scale with
/// length, and a fixed timeout cut long ones off in the prototype.
fn agent(seconds: f32, user_agent: &str) -> ureq::Agent {
    let timeout = Duration::from_secs(15) + Duration::from_secs_f32(seconds * 0.5);
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        .user_agent(user_agent)
        .build()
        .into()
}

fn boundary() -> String {
    format!("likh-{:016x}", SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos()))
}

/// Maps ChatGPT's answer to text or a failure. Split out so tests can feed real-shaped responses.
pub fn interpret(status: u16, body: &str) -> Result<String, Failure> {
    match status {
        200..=299 => read_text(body),
        401 | 403 => Err(Failure::Rejected),
        429 => Err(Failure::RateLimited),
        _ if body.contains("audio_too_short") => Err(Failure::TooShort),
        _ if body.contains("file_above_max_size") || body.contains("input_too_large") => Err(Failure::TooLong),
        _ if body.contains("rate_limit_exceeded") => Err(Failure::RateLimited),
        code => Err(Failure::Server(code)),
    }
}

/// `{ "text": "..." }`, the answer both ChatGPT and Groq give.
fn read_text(body: &str) -> Result<String, Failure> {
    #[derive(Deserialize)]
    struct Transcript {
        text: String,
    }
    serde_json::from_str::<Transcript>(body).map(|t| t.text.trim().to_owned()).map_err(|_| Failure::Unreadable)
}

/// A multipart body: the WAV as the `file` part, then any plain `fields`.
fn multipart(boundary: &str, wav: &[u8], fields: &[(&str, &str)]) -> Vec<u8> {
    let mut body = Vec::with_capacity(wav.len() + 256);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"speech.wav\"\r\nContent-Type: audio/wav\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(wav);
    for (name, value) in fields {
        body.extend_from_slice(
            format!("\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}").as_bytes(),
        );
    }
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

/// 16 kHz mono 16-bit PCM WAV, what the endpoints expect.
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut out = Cursor::new(Vec::with_capacity(samples.len() * 2 + 44));
    // Writing into memory can't fail short of running out of it.
    if let Ok(mut writer) = hound::WavWriter::new(&mut out, spec) {
        for &sample in samples {
            let clean = if sample.is_finite() { sample.clamp(-1.0, 1.0) } else { 0.0 };
            let _ = writer.write_sample((clean * 32767.0).round() as i16);
        }
        let _ = writer.finalize();
    }
    out.into_inner()
}
