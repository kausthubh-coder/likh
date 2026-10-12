use super::*;
use crate::hotkey::KeyName;

#[test]
fn a_fresh_install_gets_the_defaults() {
    let dir = tempfile::tempdir().expect("temp dir");
    let settings = Settings::load_from(&dir.path().join("settings.json"));
    assert_eq!(settings, Settings::default());
    assert_eq!(settings.shortcut.to_string(), "Ctrl + Space");
    assert_eq!(settings.mode, Mode::HoldOrTap);
    assert!(settings.history && settings.sounds && settings.commands);
    assert!(!settings.start_with_windows);
}

#[test]
fn what_you_save_is_what_you_load() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("murmur").join("settings.json");
    let chosen = Settings {
        shortcut: crate::hotkey::Binding::new(true, true, false, false, KeyName("D")).expect("valid"),
        mode: Mode::Tap,
        history: false,
        sounds: false,
        commands: false,
        start_with_windows: true,
    };
    chosen.save_to(&path).expect("save");
    assert_eq!(Settings::load_from(&path), chosen);
    assert!(std::fs::read_to_string(&path).expect("read").contains("\"shortcut\": \"Ctrl+Alt+D\""));
}

#[test]
fn a_broken_or_partial_file_never_stops_murmur() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("settings.json");
    std::fs::write(&path, "{ not json").expect("write");
    assert_eq!(Settings::load_from(&path), Settings::default());
    // A file from an older murmur that only knew the mode keeps the other defaults.
    std::fs::write(&path, r#"{ "mode": "hold" }"#).expect("write");
    assert_eq!(Settings::load_from(&path), Settings { mode: Mode::Hold, ..Settings::default() });
}
