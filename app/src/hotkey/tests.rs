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
        assert_eq!(String::from(binding), text);
    }
}

#[test]
fn reading_is_forgiving_about_case_and_spaces() {
    assert_eq!(parse("ctrl + space"), Ok(Binding::default()));
    assert_eq!(parse("Control+SPACE"), Ok(Binding::default()));
}

#[test]
fn a_shortcut_that_typing_could_trigger_is_refused_with_a_reason() {
    assert_eq!(parse("Space"), Err("Add Ctrl, Alt or Win, so typing never sets it off.".to_string()));
    assert_eq!(parse("Shift+A"), Err("Add Ctrl, Alt or Win, so typing never sets it off.".to_string()));
    // Function keys aren't typed, so they may stand alone.
    assert!(parse("F8").is_ok());
}

#[test]
fn nonsense_is_refused() {
    assert_eq!(parse("Ctrl+Banana"), Err("unknown key \"Banana\"".to_string()));
    assert_eq!(parse("Ctrl+Alt"), Err("\"Ctrl+Alt\" has no key besides modifiers".to_string()));
}

#[test]
fn key_caps_follow_the_usual_order() {
    let binding = Binding::new(true, true, true, true, KeyName("K")).expect("valid");
    assert_eq!(binding.parts(), vec!["Ctrl", "Alt", "Shift", "Win", "K"]);
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
        assert_eq!(held_parts(held), vec!["Ctrl", "Alt"]);
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
            Outcome::Refused("Add Ctrl, Alt or Win, so typing never sets it off.")
        );
    }

    #[test]
    fn keys_likh_cant_bind_say_so() {
        assert!(matches!(on_event(&press(Code::CapsLock, Modifiers::CTRL), Modifiers::CTRL), Outcome::Refused(_)));
        assert_eq!(key_for(Code::Space).map(|k| k.0), Some("Space"));
        assert_eq!(key_for(Code::Digit7).map(|k| k.0), Some("7"));
    }
}
