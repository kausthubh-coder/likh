use super::*;

fn press(keys: &mut Keys, key: Key, down: bool) -> Reaction {
    keys.on_key(key, down, false)
}

#[test]
fn ctrl_then_win_holds_and_letting_go_of_either_releases() {
    let mut keys = Keys::default();
    assert_eq!(press(&mut keys, Key::Ctrl, true), Reaction::default());
    assert_eq!(
        press(&mut keys, Key::Win, true),
        Reaction { input: Some(Input::Hold), swallow: false, mask_start_menu: true }
    );
    // Key repeat while held says nothing new.
    assert_eq!(press(&mut keys, Key::Win, true), Reaction::default());
    assert_eq!(
        press(&mut keys, Key::Win, false),
        Reaction { input: Some(Input::Release), swallow: false, mask_start_menu: false }
    );
    assert_eq!(press(&mut keys, Key::Ctrl, false), Reaction::default());
}

#[test]
fn win_then_ctrl_works_too() {
    let mut keys = Keys::default();
    press(&mut keys, Key::Win, true);
    assert_eq!(press(&mut keys, Key::Ctrl, true).input, Some(Input::Hold));
    assert_eq!(press(&mut keys, Key::Ctrl, false).input, Some(Input::Release));
}

#[test]
fn ctrl_alone_or_win_alone_is_not_the_shortcut() {
    let mut keys = Keys::default();
    assert_eq!(press(&mut keys, Key::Ctrl, true).input, None);
    assert_eq!(press(&mut keys, Key::Ctrl, false).input, None);
    assert_eq!(press(&mut keys, Key::Win, true).input, None);
    assert_eq!(press(&mut keys, Key::Win, false).input, None);
}

#[test]
fn esc_cancels_only_while_recording_and_the_app_never_sees_it() {
    let mut keys = Keys::default();
    assert_eq!(keys.on_key(Key::Escape, true, false), Reaction::default());
    assert_eq!(
        keys.on_key(Key::Escape, true, true),
        Reaction { input: Some(Input::Cancel), swallow: true, mask_start_menu: false }
    );
}

#[test]
fn another_key_while_held_is_a_windows_shortcut_so_the_dictation_gives_up() {
    let mut keys = Keys::default();
    press(&mut keys, Key::Ctrl, true);
    press(&mut keys, Key::Win, true);
    // Ctrl+Win+D: the D reaches Windows, and the recording is cancelled.
    assert_eq!(
        press(&mut keys, Key::Other, true),
        Reaction { input: Some(Input::Cancel), swallow: false, mask_start_menu: false }
    );
}

#[test]
fn typing_normally_does_nothing() {
    let mut keys = Keys::default();
    for down in [true, false] {
        assert_eq!(press(&mut keys, Key::Other, down), Reaction::default());
    }
}
