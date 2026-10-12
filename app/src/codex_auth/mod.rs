//! Is Codex installed and signed in? murmur borrows the ChatGPT sign-in that
//! Codex keeps in `~/.codex/auth.json` (or `$CODEX_HOME/auth.json`).
//!
//! murmur only ever reads this file. It never refreshes or writes tokens,
//! because Codex rotates its refresh token and two writers would sign the user
//! out of Codex. `CodexStatus` carries only a masked email, a plan name and an
//! expiry time. The one thing that holds a token is `Credentials`, which the
//! transcriber reads fresh for each request and whose `Debug` hides it.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CodexStatus {
    /// No Codex folder at all.
    NotInstalled,
    /// Codex is there but has no ChatGPT sign-in (missing file, API-key mode, or no tokens).
    SignedOut,
    Connected(Account),
    /// The access token's `exp` has passed. Opening Codex refreshes it.
    Expired(Account),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Account {
    /// `k•••@gmail.com`, never the full address.
    pub email_hint: Option<String>,
    /// ChatGPT plan, e.g. `plus` or `pro`.
    pub plan: Option<String>,
    /// Unix seconds when the access token expires.
    pub expires_at: Option<u64>,
}

/// `$CODEX_HOME`, else `~/.codex`.
pub fn codex_home() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("CODEX_HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(home));
    }
    dirs::home_dir().map(|home| home.join(".codex"))
}

pub fn detect(codex_home: &Path, now_unix: u64) -> CodexStatus {
    if !codex_home.is_dir() {
        return CodexStatus::NotInstalled;
    }
    let Some(file) = read_auth_file(codex_home) else {
        return CodexStatus::SignedOut;
    };
    let Some(tokens) = file.tokens.filter(|t| !t.access_token.is_empty()) else {
        return CodexStatus::SignedOut;
    };

    let access = jwt_claims(&tokens.access_token);
    let id = tokens.id_token.as_deref().and_then(jwt_claims);
    let account = Account {
        email_hint: id.as_ref().and_then(|c| c.get("email")).and_then(|v| v.as_str()).map(mask_email),
        plan: [id.as_ref(), access.as_ref()]
            .into_iter()
            .flatten()
            .find_map(|c| c.get("https://api.openai.com/auth")?.get("chatgpt_plan_type")?.as_str().map(str::to_owned)),
        expires_at: access.as_ref().and_then(|c| c.get("exp")).and_then(|v| v.as_u64()),
    };

    match account.expires_at {
        Some(exp) if exp <= now_unix => CodexStatus::Expired(account),
        _ => CodexStatus::Connected(account),
    }
}

/// What one transcription request needs. Never logged: `Debug` redacts both values.
pub struct Credentials {
    pub access_token: String,
    pub account_id: Option<String>,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("access_token", &"<redacted>")
            .field("account_id", &self.account_id.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// Reads the sign-in for one request. When there is no usable sign-in, the
/// error is the status that explains why, so the caller can show its fix.
pub fn credentials(codex_home: &Path, now_unix: u64) -> Result<Credentials, CodexStatus> {
    match detect(codex_home, now_unix) {
        CodexStatus::Connected(_) => {}
        other => return Err(other),
    }
    read_auth_file(codex_home)
        .and_then(|file| file.tokens)
        .map(|tokens| Credentials { access_token: tokens.access_token, account_id: tokens.account_id })
        .ok_or(CodexStatus::SignedOut)
}

#[derive(Deserialize)]
struct AuthFile {
    tokens: Option<Tokens>,
}

#[derive(Deserialize)]
struct Tokens {
    #[serde(default)]
    access_token: String,
    id_token: Option<String>,
    account_id: Option<String>,
}

fn read_auth_file(codex_home: &Path) -> Option<AuthFile> {
    let text = std::fs::read_to_string(codex_home.join("auth.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// Decodes the payload of a JWT without verifying it. We only read our own
/// sign-in's expiry and email to show the user; the server does the verifying.
fn jwt_claims(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn mask_email(email: &str) -> String {
    match email.split_once('@') {
        Some((user, domain)) => {
            let first: String = user.chars().take(1).collect();
            format!("{first}•••@{domain}")
        }
        None => "•••".to_owned(),
    }
}
