use super::*;

fn paste(text: &str, then_enter: bool) -> Action {
    Action::Paste { text: text.to_string(), then: then_enter.then_some(Press::Enter) }
}

fn interpret(transcript: &str) -> Action {
    super::interpret(transcript, &Groups::default())
}

fn with(groups: Groups, transcript: &str) -> Action {
    super::interpret(transcript, &groups)
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

#[test]
fn press_tab_at_the_end_moves_to_the_next_field() {
    assert_eq!(
        interpret("Priya Raman. Press tab."),
        Action::Paste { text: "Priya Raman.".into(), then: Some(Press::Tab) }
    );
}

#[test]
fn new_bullet_starts_a_list_item() {
    assert_eq!(
        interpret("Groceries. New bullet. Milk. New bullet, eggs."),
        paste(
            "Groceries.
- Milk.
- Eggs.",
            false
        )
    );
}

#[test]
fn scratch_that_after_a_sentence_drops_that_sentence() {
    assert_eq!(
        interpret("Hi Sam. Let's meet at two. Scratch that. Let's meet at three."),
        paste("Hi Sam. Let's meet at three.", false)
    );
    // Taking back the only sentence leaves nothing to paste.
    assert_eq!(interpret("Let's meet at two. Scratch that."), paste("", false));
}

#[test]
fn punctuation_by_name_only_when_switched_on() {
    let said = "Are you coming question mark I hope so comma see you there period";
    assert_eq!(interpret(said), paste(said, false));
    let on = Groups { punctuation: true, ..Groups::default() };
    assert_eq!(with(on, said), paste("Are you coming? I hope so, see you there.", false));
    // The transcriber's own commas around the word don't double up.
    assert_eq!(with(on, "Dear Priya, comma, thanks."), paste("Dear Priya, thanks.", false));
}

#[test]
fn a_group_switched_off_is_just_words() {
    let off = Groups { lines: false, keys: false, scratch: false, punctuation: false };
    assert_eq!(with(off, "Hi. New line. Press enter."), paste("Hi. New line. Press enter.", false));
    assert_eq!(with(off, "Scratch that."), paste("Scratch that.", false));
}
