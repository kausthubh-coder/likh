use super::*;

fn entry(at: u64, text: &str) -> Entry {
    Entry { at, text: text.into(), seconds: 2.5, action: None, pasted: true }
}

#[test]
fn entries_come_back_newest_first_and_limited() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("likh").join("history.jsonl");
    for (at, text) in [(1, "first"), (2, "second"), (3, "third")] {
        append(&path, &entry(at, text)).expect("append");
    }
    let texts: Vec<String> = recent(&path, 2).into_iter().map(|e| e.text).collect();
    assert_eq!(texts, vec!["third", "second"]);
}

#[test]
fn text_with_quotes_and_new_lines_survives() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("history.jsonl");
    let tricky = entry(5, "She said \"ship it\".\nThen left.");
    append(&path, &tricky).expect("append");
    assert_eq!(recent(&path, 10), vec![tricky]);
}

#[test]
fn a_damaged_line_is_skipped_not_fatal() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("history.jsonl");
    append(&path, &entry(1, "kept")).expect("append");
    std::fs::OpenOptions::new().append(true).open(&path).and_then(|mut f| writeln!(f, "{{cut off")).expect("damage");
    assert_eq!(recent(&path, 10), vec![entry(1, "kept")]);
}

#[test]
fn clear_empties_it_and_is_fine_when_already_empty() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("history.jsonl");
    append(&path, &entry(1, "gone soon")).expect("append");
    clear(&path).expect("clear");
    assert!(recent(&path, 10).is_empty());
    clear(&path).expect("clearing again");
}

#[test]
fn ages_read_like_people_say_them() {
    assert_eq!(ago(100, 130), "just now");
    assert_eq!(ago(0, 4 * 60 + 10), "4 min ago");
    assert_eq!(ago(0, 2 * 3600), "2 h ago");
    assert_eq!(ago(0, 30 * 3600), "yesterday");
    assert_eq!(ago(0, 3 * 86_400), "3 days ago");
}

#[test]
fn paste_last_after_a_restart_skips_actions_that_pasted_nothing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("history.jsonl");
    append(&file, &entry(1, "Ship it on Friday.")).expect("append");
    let opened =
        Entry { action: Some("New T3 thread".into()), pasted: false, ..entry(2, "new thread fix the login bug") };
    append(&file, &opened).expect("append");
    assert_eq!(last_pasted(&file).map(|e| e.text), Some("Ship it on Friday.".into()));
    // A dictation's line stays as it always was: no action, no pasted field.
    let text = std::fs::read_to_string(&file).expect("read");
    assert!(text.lines().next().is_some_and(|first| !first.contains("pasted") && !first.contains("action")));
    assert_eq!(recent(&file, 2)[0].action.as_deref(), Some("New T3 thread"));
}
