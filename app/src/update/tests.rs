use super::*;
use ring::signature::{Ed25519KeyPair, KeyPair};

/// The shape of GitHub's `GET /repos/{owner}/{repo}/releases`, trimmed to what likh reads plus a few
/// fields it must ignore.
fn releases() -> String {
    let release = |tag: &str, draft: bool, prerelease: bool, assets: &[&str]| {
        let assets: Vec<serde_json::Value> = assets
            .iter()
            .map(|name| {
                serde_json::json!({
                    "name": name,
                    "size": 4_812_345,
                    "content_type": "application/octet-stream",
                    "browser_download_url": format!("https://github.com/kausthubh-coder/likh/releases/download/{tag}/{name}"),
                })
            })
            .collect();
        serde_json::json!({
            "tag_name": tag,
            "name": format!("likh {tag}"),
            "draft": draft,
            "prerelease": prerelease,
            "html_url": format!("https://github.com/kausthubh-coder/likh/releases/tag/{tag}"),
            "published_at": "2026-10-12T13:47:47Z",
            "assets": assets,
        })
    };
    serde_json::json!([
        // A draft is never offered, however new.
        release("v0.2.0", true, false, &[INSTALLER, SIGNATURE]),
        // Newest real one, but unsigned: skipped.
        release("v0.1.0-alpha.7", false, true, &[INSTALLER]),
        release("v0.1.0-alpha.6", false, true, &[INSTALLER, SIGNATURE]),
        release("v0.1.0-alpha.5", false, true, &[INSTALLER, SIGNATURE]),
        // Releases from when likh was murmur: no likh installer.
        release("v0.1.0-alpha.2", false, true, &["murmur-windows-x64.exe"]),
        release("not-a-version", false, false, &[INSTALLER, SIGNATURE]),
    ])
    .to_string()
}

fn version(text: &str) -> Version {
    Version::parse(text).expect("a version")
}

#[test]
fn the_newest_signed_release_wins_alpha_ones_included() {
    let found = newest(&releases(), &version("0.1.0-alpha.4")).expect("readable");
    assert_eq!(
        found,
        Some(Release {
            version: "0.1.0-alpha.6".into(),
            installer: "https://github.com/kausthubh-coder/likh/releases/download/v0.1.0-alpha.6/likh-setup-windows-x64.exe"
                .into(),
            signature:
                "https://github.com/kausthubh-coder/likh/releases/download/v0.1.0-alpha.6/likh-setup-windows-x64.exe.sig"
                    .into(),
            page: "https://github.com/kausthubh-coder/likh/releases/tag/v0.1.0-alpha.6".into(),
        })
    );
}

#[test]
fn nothing_is_offered_when_likh_is_already_newest() {
    assert_eq!(newest(&releases(), &version("0.1.0-alpha.6")), Ok(None));
    // 0.1.0 is newer than every 0.1.0 alpha.
    assert_eq!(newest(&releases(), &version("0.1.0")), Ok(None));
}

#[test]
fn an_unreadable_answer_says_so() {
    assert_eq!(
        newest(r#"{"message":"API rate limit exceeded"}"#, &version("0.1.0-alpha.4")),
        Err("GitHub sent back a release list likh can't read.".into())
    );
}

#[test]
fn only_an_installer_signed_with_the_release_key_is_accepted() {
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).expect("key");
    let key = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("key pair");
    let installer = b"MZ\x90\x00 pretend installer bytes".to_vec();
    let signature = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(key.sign(&installer).as_ref())
    };
    assert!(verify(&installer, &signature, key.public_key().as_ref()));
    // With the newline a .sig file ends in.
    assert!(verify(&installer, &format!("{signature}\n"), key.public_key().as_ref()));

    let mut swapped = installer.clone();
    swapped[3] ^= 1;
    assert!(!verify(&swapped, &signature, key.public_key().as_ref()), "a changed byte must fail");
    assert!(!verify(&installer, &signature, &public_key()), "another key must fail");
    assert!(!verify(&installer, "not base64!", key.public_key().as_ref()));
}

#[test]
fn the_built_in_release_key_is_a_real_ed25519_key() {
    assert_eq!(public_key().len(), 32);
}

#[test]
fn a_check_is_due_after_a_day_or_when_there_never_was_one() {
    let now = UNIX_EPOCH + Duration::from_secs(1_792_000_000);
    assert!(due(None, now));
    assert!(!due(Some(now - Duration::from_secs(23 * 60 * 60)), now));
    assert!(due(Some(now - EVERY), now));
    // A clock that went backwards doesn't trigger a check storm.
    assert!(!due(Some(now + Duration::from_secs(60)), now));
}

#[test]
fn the_installer_runs_silently_and_opens_likh_again() {
    assert_eq!(installer_args(), ["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/relaunch=1"]);
    let iss = include_str!("../../../installer/windows.iss");
    assert!(iss.contains("{param:relaunch|0}"), "the installer must read the flag likh passes");
}

#[test]
fn the_running_version_is_this_build() {
    assert_eq!(current().to_string(), env!("CARGO_PKG_VERSION"));
}
