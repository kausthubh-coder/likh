use super::*;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::json;

const NOW: u64 = 1_800_000_000;

fn jwt(claims: serde_json::Value) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
    let payload = URL_SAFE_NO_PAD.encode(claims.to_string());
    format!("{header}.{payload}.sig")
}

fn codex_home_with(auth: Option<serde_json::Value>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    if let Some(auth) = auth {
        std::fs::write(dir.path().join("auth.json"), auth.to_string()).expect("write auth.json");
    }
    dir
}

fn signed_in(exp: u64) -> serde_json::Value {
    json!({
        "auth_mode": "chatgpt",
        "OPENAI_API_KEY": null,
        "tokens": {
            "id_token": jwt(json!({
                "email": "kausthubh@example.com",
                "https://api.openai.com/auth": { "chatgpt_plan_type": "plus" }
            })),
            "access_token": jwt(json!({ "exp": exp })),
            "refresh_token": "rt_secret",
            "account_id": "acct_123"
        },
        "last_refresh": "2026-10-12T00:00:00Z"
    })
}

#[test]
fn missing_codex_folder_is_not_installed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let missing = dir.path().join("nope");
    assert_eq!(detect(&missing, NOW), CodexStatus::NotInstalled);
}

#[test]
fn folder_without_auth_file_is_signed_out() {
    let home = codex_home_with(None);
    assert_eq!(detect(home.path(), NOW), CodexStatus::SignedOut);
}

#[test]
fn api_key_mode_without_tokens_is_signed_out() {
    let home = codex_home_with(Some(json!({ "auth_mode": "apikey", "OPENAI_API_KEY": "sk-x" })));
    assert_eq!(detect(home.path(), NOW), CodexStatus::SignedOut);
}

#[test]
fn valid_tokens_are_connected_with_masked_email_and_plan() {
    let home = codex_home_with(Some(signed_in(NOW + 3600)));
    assert_eq!(
        detect(home.path(), NOW),
        CodexStatus::Connected(Account {
            email_hint: Some("k•••@example.com".into()),
            plan: Some("plus".into()),
            expires_at: Some(NOW + 3600),
        })
    );
}

#[test]
fn past_expiry_is_expired() {
    let home = codex_home_with(Some(signed_in(NOW - 1)));
    assert!(matches!(detect(home.path(), NOW), CodexStatus::Expired(_)));
}

#[test]
fn status_json_never_contains_tokens_or_full_email() {
    let home = codex_home_with(Some(signed_in(NOW + 3600)));
    let out = serde_json::to_string(&detect(home.path(), NOW)).expect("serialize");
    assert!(out.contains(r#""state":"connected""#), "{out}");
    for secret in ["rt_secret", "acct_123", "kausthubh@", "eyJ"] {
        assert!(!out.contains(secret), "leaked {secret}: {out}");
    }
}

#[test]
fn credentials_come_from_a_live_sign_in() {
    let home = codex_home_with(Some(signed_in(NOW + 3600)));
    let credentials = credentials(home.path(), NOW).expect("connected");
    assert_eq!(credentials.access_token, jwt(json!({ "exp": NOW + 3600 })));
    assert_eq!(credentials.account_id.as_deref(), Some("acct_123"));
}

#[test]
fn credentials_refuse_an_expired_sign_in_and_say_why() {
    let home = codex_home_with(Some(signed_in(NOW - 1)));
    assert!(matches!(credentials(home.path(), NOW), Err(CodexStatus::Expired(_))));
    let empty = codex_home_with(None);
    assert_eq!(credentials(empty.path(), NOW).err(), Some(CodexStatus::SignedOut));
}

#[test]
fn credentials_debug_never_shows_the_token() {
    let home = codex_home_with(Some(signed_in(NOW + 3600)));
    let shown = format!("{:?}", credentials(home.path(), NOW).expect("connected"));
    assert_eq!(shown, r#"Credentials { access_token: "<redacted>", account_id: Some("<redacted>") }"#);
}
