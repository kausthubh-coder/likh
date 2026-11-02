use super::*;

fn parse(text: &str) -> Result<Binding, String> {
    Binding::try_from(text.to_string())
}

#[test]
fn the_default_is_ctrl_space() {
    assert_eq!(Binding::default().to_string(), "Ctrl + Space");
    assert_eq!(String::from(Binding::default()), "Ctrl+Space");
}

#[test]
fn a_saved_shortcut_reads_back_the_same() {
    for text in ["Ctrl+Space", "Ctrl+Alt+D", "Ctrl+Shift+Win+F9", "F8"] {
        let binding = parse(text).expect(text);
        // Saved with the platform's own names: Option and Cmd on a Mac.
        assert_eq!(String::from(binding), text.replace("Alt", super::ALT).replace("Win", super::WIN));
        assert_eq!(parse(&String::from(binding)), Ok(binding), "{text}");
    }
}

#[test]
fn mac_names_and_windows_names_read_as_the_same_keys() {
    assert_eq!(parse("Option+Shift+Z"), parse("Alt+Shift+Z"));
    assert_eq!(parse("Cmd+K"), parse("Win+K"));
    assert_eq!(parse("Command+K"), parse("Win+K"));
}

#[test]
fn reading_is_forgiving_about_case_and_spaces() {
    assert_eq!(parse("ctrl + space"), Ok(Binding::default()));
    assert_eq!(parse("Control+SPACE"), Ok(Binding::default()));
}

#[test]
fn a_shortcut_that_typing_could_trigger_is_refused_with_a_reason() {
    assert_eq!(parse("Space"), Err(super::NEEDS_MODIFIER.to_string()));
    assert_eq!(parse("Shift+A"), Err(super::NEEDS_MODIFIER.to_string()));
    // Function keys aren't typed, so they may stand alone.
    assert!(parse("F8").is_ok());
}

#[test]
fn caps_lock_stands_alone_and_reads_back_the_same() {
    assert_eq!(parse("Caps Lock"), Ok(CAPS_LOCK));
    assert_eq!(String::from(CAPS_LOCK), "Caps Lock");
    assert_eq!(CAPS_LOCK.to_string(), "Caps Lock");
    assert_eq!(parse("Ctrl+Caps Lock"), Err(CAPS_LOCK_ALONE.to_string()));
    assert_eq!(parse("Shift+Caps Lock"), Err(CAPS_LOCK_ALONE.to_string()));
    assert_eq!(KeyName("Caps Lock").virtual_key(), 0x14);
}

#[test]
fn a_held_caps_lock_starts_one_dictation_and_ends_it_once() {
    let (held, answer) = caps_event(CapsKey::Up, true, false);
    assert_eq!(answer, CapsAnswer::Pressed);
    // The key repeating while held must not press again: hands-free would stop at the first repeat.
    let (held, answer) = caps_event(held, true, false);
    assert_eq!(answer, CapsAnswer::Swallow);
    assert_eq!(caps_event(held, false, false), (CapsKey::Up, CapsAnswer::Released));
}

#[test]
fn shift_caps_lock_still_reaches_windows_all_the_way_up() {
    let (held, answer) = caps_event(CapsKey::Up, true, true);
    assert_eq!(answer, CapsAnswer::Pass);
    // Shift let go first: it's still Windows' press, so no dictation appears from nowhere.
    let (held, answer) = caps_event(held, true, false);
    assert_eq!(answer, CapsAnswer::Pass);
    assert_eq!(caps_event(held, false, false), (CapsKey::Up, CapsAnswer::Pass));
}

#[test]
fn caps_lock_pressed_to_pick_it_doesnt_start_a_dictation() {
    let (held, answer) = caps_event(CapsKey::Settling, true, false);
    assert_eq!(answer, CapsAnswer::Swallow);
    assert_eq!(caps_event(held, false, false), (CapsKey::Up, CapsAnswer::Swallow));
    assert_eq!(caps_event(CapsKey::Up, false, false), (CapsKey::Up, CapsAnswer::Pass));
}

#[test]
fn nonsense_is_refused() {
    assert_eq!(parse("Ctrl+Banana"), Err("unknown key \"Banana\"".to_string()));
    assert_eq!(parse("Ctrl+Alt"), Err("\"Ctrl+Alt\" has no key besides modifiers".to_string()));
}

#[test]
fn key_caps_follow_the_usual_order() {
    let binding = Binding::new(true, true, true, true, KeyName("K")).expect("valid");
    assert_eq!(binding.parts(), vec!["Ctrl", super::ALT, "Shift", super::WIN, "K"]);
}

#[test]
fn every_key_has_a_windows_code() {
    assert_eq!(KeyName("Space").virtual_key(), 0x20);
    assert_eq!(KeyName("F12").virtual_key(), 0x7B);
    assert!(KEYS.iter().all(|(_, code)| *code != 0));
}

mod capturing {
    use super::super::capture::{Outcome, held_parts, key_for, on_event};
    use iced::keyboard::{self, Key, Location, Modifiers, key::Code, key::Physical};

    fn press(code: Code, modifiers: Modifiers) -> keyboard::Event {
        keyboard::Event::KeyPressed {
            key: Key::Unidentified,
            modified_key: Key::Unidentified,
            physical_key: Physical::Code(code),
            location: Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        }
    }

    #[test]
    fn holding_modifiers_shows_them_until_a_key_locks_it_in() {
        let held = Modifiers::CTRL | Modifiers::ALT;
        assert_eq!(on_event(&keyboard::Event::ModifiersChanged(held), Modifiers::empty()), Outcome::Waiting(held));
        assert_eq!(held_parts(held), vec!["Ctrl", super::ALT]);
        // Pressing the modifier keys themselves keeps waiting.
        assert_eq!(on_event(&press(Code::AltLeft, held), held), Outcome::Waiting(held));
        let chosen = on_event(&press(Code::KeyD, held), held);
        assert_eq!(chosen, Outcome::Chosen(super::parse("Ctrl+Alt+D").expect("valid")));
    }

    #[test]
    fn esc_backs_out() {
        assert_eq!(on_event(&press(Code::Escape, Modifiers::CTRL), Modifiers::CTRL), Outcome::Cancelled);
    }

    #[test]
    fn a_bare_letter_is_refused_with_the_reason() {
        assert_eq!(
            on_event(&press(Code::KeyD, Modifiers::empty()), Modifiers::empty()),
            Outcome::Refused(super::NEEDS_MODIFIER)
        );
    }

    #[test]
    fn keys_likh_cant_bind_say_so() {
        assert!(matches!(on_event(&press(Code::CapsLock, Modifiers::CTRL), Modifiers::CTRL), Outcome::Refused(_)));
        assert_eq!(key_for(Code::Space).map(|k| k.0), Some("Space"));
        assert_eq!(key_for(Code::Digit7).map(|k| k.0), Some("7"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn caps_lock_can_be_picked_by_pressing_it() {
        let chosen = on_event(&press(Code::CapsLock, Modifiers::empty()), Modifiers::empty());
        assert_eq!(chosen, Outcome::Chosen(super::super::CAPS_LOCK));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn caps_lock_cant_be_picked_where_it_cant_be_held() {
        assert!(matches!(
            on_event(&press(Code::CapsLock, Modifiers::empty()), Modifiers::empty()),
            Outcome::Refused(_)
        ));
    }
}
