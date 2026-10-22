use super::*;
use crate::hotkey::KeyName;

#[test]
fn an_older_file_keeps_its_choices_and_gets_the_new_defaults() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("settings.json");
    // What likh 0.1.0-alpha.4 wrote.
    std::fs::write(
        &path,
        r#"{ "shortcut": "Ctrl+Space", "mode": "tap", "history": true, "sounds": false, "commands": true, "start_with_windows": true }"#,
    )
    .expect("write");
    let loaded = Settings::load_from(&path);
    assert_eq!(loaded.mode, Mode::Tap);
    assert!(!loaded.sounds && loaded.start_with_windows);
    assert_eq!(loaded.engine, Engine::Codex);
    assert_eq!(loaded.mouse, MouseTrigger::Off);
    assert_eq!(loaded.silence_stop.after(), Some(std::time::Duration::from_secs(5)));
    assert_eq!(loaded.command_groups, Groups { lines: true, keys: true, scratch: true, punctuation: false });
    assert!(loaded.auto_update);
    assert!(loaded.dictionary.is_empty() && loaded.fix_names);
    // An older file has no cleanup choice: it stays off, as it was.
    assert!(!loaded.cleanup);
    assert_eq!(loaded.paste_last.map(String::from), Some(format!("{}+Shift+Z", crate::hotkey::ALT)));
}

#[test]
fn paste_last_saves_its_keys_or_off() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{ "paste_last": null, "sounds": false }"#).expect("write");
    let off = Settings::load_from(&path);
    assert_eq!((off.paste_last, off.sounds), (None, false));
    std::fs::write(&path, r#"{ "paste_last": "Ctrl+Alt+V" }"#).expect("write");
    let moved = Settings::load_from(&path);
    assert_eq!(moved.paste_last, Binding::new(true, true, false, false, KeyName("V")).ok());
}

#[test]
fn a_file_that_picked_windows_speech_moves_to_chatgpt_and_keeps_the_rest() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("settings.json");
    // What likh 0.1.0-alpha.5 wrote with Windows speech picked.
    std::fs::write(&path, r#"{ "mode": "tap", "engine": "windows", "silence_stop": "3s", "sounds": false }"#)
        .expect("write");
    let loaded = Settings::load_from(&path);
    assert_eq!(loaded.engine, Engine::Codex);
    assert_eq!((loaded.mode, loaded.silence_stop, loaded.sounds), (Mode::Tap, SilenceStop::Short, false));
}

#[test]
fn a_fresh_install_gets_the_defaults() {
    let dir = tempfile::tempdir().expect("temp dir");
    let settings = Settings::load_from(&dir.path().join("settings.json"));
    assert_eq!(settings, Settings::default());
    assert_eq!(settings.shortcut.to_string(), "Ctrl + Space");
    assert_eq!(settings.mode, Mode::HoldOrTap);
    assert!(settings.history && settings.sounds && settings.commands);
    // On out of the box where likh can start with the system: Windows, and macOS at login.
    assert_eq!(settings.start_with_windows, cfg!(any(target_os = "windows", target_os = "macos")));
}

#[test]
fn someone_who_turned_start_with_windows_off_keeps_it_off() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{ "start_with_windows": false }"#).expect("write");
    assert!(!Settings::load_from(&path).start_with_windows);
}

#[test]
fn what_you_save_is_what_you_load() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("likh").join("settings.json");
    let chosen = Settings {
        shortcut: crate::hotkey::Binding::new(true, true, false, false, KeyName("D")).expect("valid"),
        paste_last: None,
        mouse: MouseTrigger::Forward,
        action_key: None,
        action_mouse: MouseTrigger::Back,
        mode: Mode::Tap,
        engine: Engine::Groq,
        microphone: Some(crate::microphone::Choice {
            id: "wasapi:{0.0.1.00000000}.{eb70d1c2}".into(),
            name: "Microphone Array".into(),
        }),
        language: Some("te".into()),
        silence_stop: SilenceStop::Long,
        pause_media: false,
        history: false,
        sounds: false,
        commands: false,
        command_groups: Groups { lines: false, keys: true, scratch: false, punctuation: true },
        dictionary: vec!["Critly".into(), "Kriyan".into()],
        fix_names: false,
        cleanup: true,
        styles: false,
        start_with_windows: true,
        auto_update: false,
    };
    chosen.save_to(&path).expect("save");
    assert_eq!(Settings::load_from(&path), chosen);
    let file = std::fs::read_to_string(&path).expect("read");
    assert!(file.contains(&format!("\"shortcut\": \"Ctrl+{}+D\"", crate::hotkey::ALT)));
    assert!(file.contains("\"engine\": \"groq\""));
    assert!(file.contains("\"silence_stop\": \"10s\""));
    assert!(file.contains("\"mouse\": \"forward\""));
    assert!(file.contains("\"cleanup\": true"));
    assert!(file.contains("\"dictionary\": [\n    \"Critly\",\n    \"Kriyan\"\n  ]"));
    // The Groq key lives in the credential store, never in this file.
    assert!(!file.contains("gsk_"));
}

#[test]
fn a_broken_or_partial_file_never_stops_likh() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("settings.json");
    std::fs::write(&path, "{ not json").expect("write");
    assert_eq!(Settings::load_from(&path), Settings::default());
    // A file from an older likh that only knew the mode keeps the other defaults.
    std::fs::write(&path, r#"{ "mode": "hold" }"#).expect("write");
    assert_eq!(Settings::load_from(&path), Settings { mode: Mode::Hold, ..Settings::default() });
}

#[test]
fn a_murmur_folder_moves_to_likh_once() {
    let data = tempfile::tempdir().expect("temp dir");
    let old = data.path().join("murmur");
    std::fs::create_dir_all(&old).expect("mkdir");
    std::fs::write(old.join("settings.json"), r#"{ "mode": "tap" }"#).expect("write");
    std::fs::write(old.join("onboarded"), "").expect("write");

    assert_eq!(adopt_murmur_folder(data.path()), Ok(true));
    assert!(!old.exists());
    assert_eq!(Settings::load_from(&data.path().join("likh").join("settings.json")).mode, Mode::Tap);
    assert!(data.path().join("likh").join("onboarded").exists());

    // A likh folder already there wins; the old one is left alone.
    std::fs::create_dir_all(&old).expect("mkdir");
    assert_eq!(adopt_murmur_folder(data.path()), Ok(false));
    assert!(old.exists());
}
