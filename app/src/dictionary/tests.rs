use super::*;

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(|w| (*w).to_owned()).collect()
}

#[test]
fn adding_a_word_tidies_it_and_refuses_repeats_blanks_and_overflow() {
    let mut list = words(&["Critly"]);
    assert_eq!(add(&mut list, "  Kriyan   app "), Ok(()));
    assert_eq!(list, words(&["Critly", "Kriyan app"]));
    assert_eq!(add(&mut list, "critly"), Err("critly is already in your dictionary.".into()));
    assert_eq!(add(&mut list, "   "), Err("Type a word first.".into()));
    assert_eq!(add(&mut list, &"x".repeat(MAX_WORD_CHARS + 1)), Err("Keep it under 48 characters.".into()));
    let mut full: Vec<String> = (0..MAX_WORDS).map(|i| format!("word{i}")).collect();
    assert_eq!(add(&mut full, "Studi"), Err("Your dictionary is full at 100 words.".into()));
    assert_eq!(list, words(&["Critly", "Kriyan app"]));
}

#[test]
fn whisper_hears_the_words_as_one_short_sentence() {
    assert_eq!(whisper_prompt(&[]), None);
    assert_eq!(whisper_prompt(&words(&["Critly", "Kriyan", "Studi"])), Some("Critly, Kriyan, Studi.".into()));
    // Whisper reads only the end of a long prompt; likh stops before it gets there.
    let many: Vec<String> = (0..MAX_WORDS).map(|i| format!("Project{i:02} Name")).collect();
    let prompt = whisper_prompt(&many).expect("a prompt");
    assert!(prompt.len() <= 601, "{} characters", prompt.len());
    assert!(prompt.starts_with("Project00 Name, Project01 Name,") && prompt.ends_with("Name."));
}
