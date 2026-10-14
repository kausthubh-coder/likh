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

#[test]
fn chatgpt_gets_the_dictionary_and_the_transcript_and_nothing_is_stored() {
    let body = request_body("I pushed it to currently.", &words(&["Critly", "Kriyan"]));
    assert_eq!(
        body["input"][0]["content"][0]["text"],
        "Dictionary:\nCritly\nKriyan\n\nTranscript:\nI pushed it to currently."
    );
    assert_eq!(body["model"], "gpt-6-luna");
    assert_eq!((body["store"].as_bool(), body["stream"].as_bool()), (Some(false), Some(true)));
    assert_eq!(body["tools"], serde_json::json!([]));
}

/// What `/backend-api/codex/responses` streams back, trimmed to the events likh reads.
const STREAM: &str = r#"event: response.created
data: {"type":"response.created","sequence_number":0,"response":{"id":"resp_1","status":"in_progress"}}

event: response.output_text.delta
data: {"type":"response.output_text.delta","sequence_number":4,"item_id":"msg_1","output_index":1,"content_index":0,"delta":"I pushed it to "}

event: response.output_text.delta
data: {"type":"response.output_text.delta","sequence_number":5,"item_id":"msg_1","output_index":1,"content_index":0,"delta":"Critly."}

event: response.output_text.done
data: {"type":"response.output_text.done","sequence_number":6,"item_id":"msg_1","output_index":1,"content_index":0,"text":"I pushed it to Critly."}

event: response.completed
data: {"type":"response.completed","sequence_number":8,"response":{"id":"resp_1","status":"completed"}}
"#;

#[test]
fn the_answer_is_read_from_the_event_stream() {
    assert_eq!(read_answer(STREAM), Some("I pushed it to Critly.".into()));
    // Cut off before the final event: the pieces so far are the answer.
    let cut = STREAM.split("event: response.output_text.done").next().expect("first half");
    assert_eq!(read_answer(cut), Some("I pushed it to Critly.".into()));
    assert_eq!(read_answer("event: response.failed\ndata: {\"type\":\"response.failed\"}\n"), None);
    assert_eq!(read_answer("<html>busy</html>"), None);
}

#[test]
fn an_answer_that_isnt_the_transcript_never_gets_pasted() {
    let heard = "I pushed the onboarding fix to currently and korean last night.";
    assert!(is_the_transcript(heard, "I pushed the onboarding fix to Critly and Kriyan last night."));
    assert!(!is_the_transcript(heard, "Sure!"));
    assert!(!is_the_transcript(
        heard,
        "Here is the cleaned transcript with the dictionary applied, as you asked: I pushed the onboarding fix to Critly and Kriyan last night."
    ));
    // A short dictation can still gain a long name.
    assert!(is_the_transcript("Open study.", "Open Studi Nightly."));
}

#[test]
fn with_no_words_the_text_is_left_alone_without_asking_anyone() {
    // No dictionary means no request: the same text comes straight back.
    assert_eq!(fix("I'm currently learning Korean.", &[]), "I'm currently learning Korean.");
    assert_eq!(fix("   ", &words(&["Critly"])), "   ");
}

/// Which of Codex's fast models fixes names quickest, each sent exactly what
/// `fix` sends, taking turns so none gets a warmer connection or a quieter minute.
/// Run by hand: `cargo test -p likh dictionary_speed -- --ignored --nocapture`
#[test]
#[ignore = "calls ChatGPT with the Codex sign-in"]
fn dictionary_speed_of_each_fast_model() {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let home = codex_auth::codex_home().expect("a Codex folder");
    let credentials = codex_auth::credentials(&home, now).expect("a Codex sign-in");
    let dictionary = words(&["Critly", "Kriyan", "Studi"]);
    let (heard, expected) = (
        "I pushed the onboarding fix to currently and korean last night.",
        "I pushed the onboarding fix to Critly and Kriyan last night.",
    );
    let models = ["gpt-6-luna", "gpt-5.6-luna", "gpt-6-sol", "gpt-6.1-sol"];
    let mut times: Vec<Vec<u128>> = vec![Vec::new(); models.len()];
    let mut wrong = vec![0; models.len()];
    // One untimed call opens the connection path for everyone.
    let _ = ask(&credentials, request_body(heard, &dictionary));
    for _round in 0..5 {
        for (i, model) in models.iter().enumerate() {
            let mut body = request_body(heard, &dictionary);
            body["model"] = (*model).into();
            let started = std::time::Instant::now();
            let answer = ask(&credentials, body).ok().and_then(|stream| read_answer(&stream));
            times[i].push(started.elapsed().as_millis());
            if answer.as_deref() != Some(expected) {
                wrong[i] += 1;
                eprintln!("{model} answered {answer:?}");
            }
        }
    }
    for (i, model) in models.iter().enumerate() {
        let mut sorted = times[i].clone();
        sorted.sort_unstable();
        eprintln!("{model}: median {} ms, range {} to {} ms, {} wrong of 5", sorted[2], sorted[0], sorted[4], wrong[i]);
    }
}

/// A real name fix against ChatGPT with the Codex sign-in on this computer.
/// Run by hand: `cargo test -p likh dictionary_live -- --ignored --nocapture`
#[test]
#[ignore = "calls ChatGPT with the Codex sign-in"]
fn dictionary_live_fixes_misheard_names_and_leaves_real_words() {
    let dictionary = words(&["Critly", "Kriyan", "Studi"]);
    let cases = [
        (
            "I pushed the onboarding fix to currently and korean last night.",
            "I pushed the onboarding fix to Critly and Kriyan last night.",
        ),
        ("I'm currently learning Korean.", "I'm currently learning Korean."),
        ("Open study and check the nightly build.", "Open Studi and check the nightly build."),
    ];
    for round in 1..=2 {
        for (heard, expected) in cases {
            let started = std::time::Instant::now();
            let fixed = fix(heard, &dictionary);
            eprintln!("round {round}: {heard:?} -> {fixed:?} in {:?}", started.elapsed());
            assert_eq!(fixed, expected);
        }
    }
}
