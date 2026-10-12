//! Turns a recording into text. Today that's Codex's `/backend-api/transcribe`
//! (docs/codex.md); anything else that can do `transcribe(audio) -> text`
//! slots in here.
//!
//! `LIKH_TRANSCRIBER=echo:<text>` swaps in a fake that returns `<text>`, so
//! tests and agents can drive whole dictations without touching ChatGPT.

use crate::codex_auth::{self, CodexStatus, Credentials};
use serde::Deserialize;
use std::io::Cursor;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(test)]
mod tests;

pub const SAMPLE_RATE: u32 = 16_000;
const ENDPOINT: &str = "https://chatgpt.com/backend-api/transcribe";
/// What the Codex desktop app sends (docs/codex.md, The request).
const USER_AGENT: &str = "Codex Desktop/26.1007.2314.0 (Windows NT 10.0; x64)";
/// Anything shorter is a stray tap, not speech; the server rejects it anyway.
pub const SHORTEST: Duration = Duration::from_millis(300);

pub enum Transcriber {
    Codex,
    Echo(String),
}

impl Transcriber {
    pub fn from_env() -> Self {
        match std::env::var("LIKH_TRANSCRIBER").ok().and_then(|v| v.strip_prefix("echo:").map(str::to_owned)) {
            Some(text) => Transcriber::Echo(text),
            None => Transcriber::Codex,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Transcriber::Codex => "codex",
            Transcriber::Echo(_) => "echo",
        }
    }

    /// `samples` are 16 kHz mono in -1..1. Blocks until ChatGPT answers.
    pub fn transcribe(&self, samples: &[f32]) -> Result<String, Failure> {
        if samples.len() < (SAMPLE_RATE as f32 * SHORTEST.as_secs_f32()) as usize {
            return Err(Failure::TooShort);
        }
        match self {
            Transcriber::Echo(text) => Ok(text.clone()),
            Transcriber::Codex => {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
                let home = codex_auth::codex_home().ok_or(Failure::SignIn(CodexStatus::NotInstalled))?;
                let credentials = codex_auth::credentials(&home, now).map_err(Failure::SignIn)?;
                let seconds = samples.len() as f32 / SAMPLE_RATE as f32;
                post(&credentials, &wav(samples), seconds)
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
            Failure::TooLong => "That recording was too long for ChatGPT. Try a shorter one.".into(),
            Failure::RateLimited => "ChatGPT is busy with your dictations. Try again in a minute.".into(),
            Failure::Server(code) => format!("ChatGPT couldn't transcribe that (error {code}). Try again."),
            Failure::Unreadable => "ChatGPT sent back something likh can't read. Try again.".into(),
        }
    }
}

fn post(credentials: &Credentials, wav: &[u8], seconds: f32) -> Result<String, Failure> {
    let boundary = format!("likh-{:016x}", SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    let body = multipart(&boundary, wav);
    // Uploads scale with length; a fixed timeout cut long ones off in the prototype.
    let timeout = Duration::from_secs(15) + Duration::from_secs_f32(seconds * 0.5);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        .user_agent(USER_AGENT)
        .build()
        .into();
    let mut request = agent
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

/// Maps ChatGPT's answer to text or a failure. Split out so tests can feed real-shaped responses.
pub fn interpret(status: u16, body: &str) -> Result<String, Failure> {
    #[derive(Deserialize)]
    struct Transcript {
        text: String,
    }
    match status {
        200..=299 => {
            serde_json::from_str::<Transcript>(body).map(|t| t.text.trim().to_owned()).map_err(|_| Failure::Unreadable)
        }
        401 | 403 => Err(Failure::Rejected),
        429 => Err(Failure::RateLimited),
        _ if body.contains("audio_too_short") => Err(Failure::TooShort),
        _ if body.contains("file_above_max_size") || body.contains("input_too_large") => Err(Failure::TooLong),
        _ if body.contains("rate_limit_exceeded") => Err(Failure::RateLimited),
        code => Err(Failure::Server(code)),
    }
}

fn multipart(boundary: &str, wav: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(wav.len() + 256);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"speech.wav\"\r\nContent-Type: audio/wav\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(wav);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

/// 16 kHz mono 16-bit PCM WAV, what the endpoint expects.
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
