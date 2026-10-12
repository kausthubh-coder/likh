use super::*;

fn press(keys: &mut Keys, key: Key, down: bool) -> Reaction {
    keys.on_key(key, down, false)
}

fn ours(input: Option<Input>) -> Reaction {
    Reaction { input, swallow: true }
}

#[test]
fn ctrl_then_space_holds_and_letting_go_of_space_releases() {
    let mut keys = Keys::default();
    assert_eq!(press(&mut keys, Key::Ctrl, true), Reaction::default());
    assert_eq!(press(&mut keys, Key::Space, true), ours(Some(Input::Hold)));
    // Key repeat while held: swallowed, nothing new.
    assert_eq!(press(&mut keys, Key::Space, true), ours(None));
    assert_eq!(press(&mut keys, Key::Space, false), ours(Some(Input::Release)));
    assert_eq!(press(&mut keys, Key::Ctrl, false), Reaction::default());
}

#[test]
fn letting_go_of_ctrl_first_also_releases_and_the_space_stays_ours() {
    let mut keys = Keys::default();
    press(&mut keys, Key::Ctrl, true);
    press(&mut keys, Key::Space, true);
    assert_eq!(press(&mut keys, Key::Ctrl, false), Reaction { input: Some(Input::Release), swallow: false });
    // The still-held Space keeps repeating; it must not type spaces into the app.
    assert_eq!(press(&mut keys, Key::Space, true), ours(None));
    assert_eq!(press(&mut keys, Key::Space, false), ours(None));
}

#[test]
fn space_without_ctrl_is_just_a_space() {
    let mut keys = Keys::default();
    assert_eq!(press(&mut keys, Key::Space, true), Reaction::default());
    assert_eq!(press(&mut keys, Key::Space, false), Reaction::default());
    // Ctrl pressed after Space is not the shortcut either.
    press(&mut keys, Key::Space, true);
    assert_eq!(press(&mut keys, Key::Ctrl, true), Reaction::default());
}

#[test]
fn esc_cancels_only_while_recording_and_the_app_never_sees_it() {
    let mut keys = Keys::default();
    assert_eq!(keys.on_key(Key::Escape, true, false), Reaction::default());
    assert_eq!(keys.on_key(Key::Escape, true, true), ours(Some(Input::Cancel)));
}

#[test]
fn another_key_while_held_gives_the_dictation_up() {
    let mut keys = Keys::default();
    press(&mut keys, Key::Ctrl, true);
    press(&mut keys, Key::Space, true);
    assert_eq!(press(&mut keys, Key::Other, true), Reaction { input: Some(Input::Cancel), swallow: false });
}

#[test]
fn typing_and_ctrl_shortcuts_pass_through() {
    let mut keys = Keys::default();
    for down in [true, false] {
        assert_eq!(press(&mut keys, Key::Other, down), Reaction::default());
    }
    // Ctrl+C and friends.
    press(&mut keys, Key::Ctrl, true);
    assert_eq!(press(&mut keys, Key::Other, true), Reaction::default());
}
