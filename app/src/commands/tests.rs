use super::*;

fn paste(text: &str, then_enter: bool) -> Action {
    Action::Paste { text: text.to_string(), then_enter }
}

#[test]
fn plain_speech_is_pasted_as_is() {
    assert_eq!(
        interpret("Remind me to send the deck tomorrow at nine."),
        paste("Remind me to send the deck tomorrow at nine.", false)
    );
}

#[test]
fn new_line_and_new_paragraph_become_breaks_with_the_punctuation_around_them_gone() {
    assert_eq!(
        interpret("Thanks for the notes. New line. See you tomorrow."),
        paste("Thanks for the notes.\nSee you tomorrow.", false)
    );
    assert_eq!(
        interpret("Dear Priya, new paragraph, the deck is attached."),
        paste("Dear Priya\n\nThe deck is attached.", false)
    );
}

#[test]
fn ending_with_press_enter_sends_it() {
    assert_eq!(interpret("Sounds good, see you at nine. Press enter."), paste("Sounds good, see you at nine.", true));
    assert_eq!(interpret("ship it press enter"), paste("ship it", true));
}

#[test]
fn press_enter_in_the_middle_is_just_words() {
    assert_eq!(interpret("To press enter, hold the key."), paste("To press enter, hold the key.", false));
}

#[test]
fn scratch_that_alone_undoes_the_last_paste() {
    for said in ["Scratch that.", "undo that", "Delete that!"] {
        assert_eq!(interpret(said), Action::Undo, "{said}");
    }
    // Inside a sentence it's just words.
    assert_eq!(interpret("I said scratch that idea."), paste("I said scratch that idea.", false));
}

#[test]
fn a_command_inside_a_word_is_left_alone() {
    assert_eq!(interpret("The renew lines are long."), paste("The renew lines are long.", false));
}
