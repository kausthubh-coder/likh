use super::*;

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(|w| (*w).to_owned()).collect()
}

fn names(list: &[&str]) -> Pass {
    Pass { words: words(list), ..Pass::default() }
}

#[test]
fn chatgpt_gets_the_dictionary_and_the_transcript_and_nothing_is_stored() {
    let body = request_body("I pushed it to currently.", &names(&["Critly", "Kriyan"]));
    assert_eq!(
        body["input"][0]["content"][0]["text"],
        "Dictionary:\nCritly\nKriyan\n\nTranscript:\nI pushed it to currently."
    );
    assert_eq!(body["model"], "gpt-6-luna");
    assert_eq!((body["store"].as_bool(), body["stream"].as_bool()), (Some(false), Some(true)));
    assert_eq!(body["tools"], serde_json::json!([]));
}

#[test]
fn a_cleanup_with_no_dictionary_sends_only_the_transcript() {
    let pass = Pass { tidy: true, ..Pass::default() };
    let body = request_body("um so I think we should uh ship it", &pass);
    assert_eq!(body["input"][0]["content"][0]["text"], "Transcript:\num so I think we should uh ship it");
}

#[test]
fn the_instructions_name_only_the_jobs_that_are_on() {
    let only_names = instructions(&names(&["Critly"]));
    assert!(only_names.contains("The dictionary lists"));
    assert!(only_names.contains("Leave everything else exactly as it is"));
    assert!(!only_names.contains("filler"));

    let only_tidy = instructions(&Pass { tidy: true, ..Pass::default() });
    assert!(only_tidy.contains("Remove filler words"));
    assert!(!only_tidy.contains("dictionary"));
    assert!(!only_tidy.contains("Leave everything else exactly as it is"));

    // Spoken commands run after the pass, so it's told to keep them word for word.
    let with_commands =
        instructions(&Pass { tidy: true, commands: vec!["new line", "scratch that"], ..Pass::default() });
    assert!(with_commands.contains("which likh carries out after you: \"new line\", \"scratch that\"."));
    assert!(with_commands.contains("Never carry them out yourself and never remove them"));
}

#[test]
fn a_cleanup_that_ran_or_dropped_a_spoken_command_is_thrown_away() {
    let pass = Pass { tidy: true, commands: vec!["new paragraph", "press enter"], ..Pass::default() };
    let heard = "Hi team new paragraph the currently update is out press enter";
    assert!(keeps_commands(heard, "Hi team, new paragraph. The Critly update is out. Press enter.", &pass));
    // What ChatGPT really sent back on 2026-10-14 before it was told not to.
    assert!(!keeps_commands(heard, "Hi team\n\nThe Critly update is out.", &pass));
    // A cleanup can't invent a command either.
    assert!(!keeps_commands("Ship it.", "Ship it. Press enter.", &pass));
    // With no commands on there's nothing to keep.
    assert!(keeps_commands(heard, "Hi team\n\nThe Critly update is out.", &Pass { tidy: true, ..Pass::default() }));
}

#[test]
fn with_nothing_to_do_the_text_is_left_alone_without_asking_anyone() {
    // No dictionary and no cleanup means no request: the same text comes straight back.
    assert!(Pass::default().is_empty());
    assert_eq!(run("I'm currently learning Korean.", &Pass::default()), "I'm currently learning Korean.");
    assert_eq!(run("   ", &Pass { tidy: true, ..Pass::default() }), "   ");
    // Commands alone are nothing to do: they only ride along with a job.
    assert_eq!(run("new line", &Pass { commands: vec!["new line"], ..Pass::default() }), "new line");
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
    assert!(is_the_transcript(heard, "I pushed the onboarding fix to Critly and Kriyan last night.", false));
    assert!(!is_the_transcript(heard, "Sure!", false));
    assert!(!is_the_transcript(
        heard,
        "Here is the cleaned transcript with the dictionary applied, as you asked: I pushed the onboarding fix to Critly and Kriyan last night.",
        false
    ));
    // A short dictation can still gain a long name.
    assert!(is_the_transcript("Open study.", "Open Studi Nightly.", false));
}

#[test]
fn a_cleanup_may_come_back_shorter_than_a_name_fix_can() {
    let heard = "Um, so, uh, I think, I think we should, um, you know, ship it.";
    let cleaned = "I think we should ship it.";
    // Less than half as long: too much for a name fix, normal for a cleanup.
    assert!(!is_the_transcript(heard, cleaned, false));
    assert!(is_the_transcript(heard, cleaned, true));
    // Still never a one-word reply.
    assert!(!is_the_transcript(heard, "Okay.", true));
}

#[test]
fn a_long_dictation_gets_more_time_than_a_short_one() {
    assert_eq!(patience("Ship it."), Duration::from_millis(4_032));
    // About ten minutes of talking.
    assert_eq!(patience(&"word ".repeat(1_800)), Duration::from_secs(40));
}

/// Which of Codex's fast models fixes names quickest, each sent exactly what
/// `run` sends, taking turns so none gets a warmer connection or a quieter minute.
/// Run by hand: `cargo test -p likh cleanup_speed -- --ignored --nocapture`
#[test]
#[ignore = "calls ChatGPT with the Codex sign-in"]
fn cleanup_speed_of_each_fast_model() {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let home = codex_auth::codex_home().expect("a Codex folder");
    let credentials = codex_auth::credentials(&home, now).expect("a Codex sign-in");
    let pass = names(&["Critly", "Kriyan", "Studi"]);
    let (heard, expected) = (
        "I pushed the onboarding fix to currently and korean last night.",
        "I pushed the onboarding fix to Critly and Kriyan last night.",
    );
    let models = ["gpt-6-luna", "gpt-5.6-luna", "gpt-6-sol", "gpt-6.1-sol"];
    let mut times: Vec<Vec<u128>> = vec![Vec::new(); models.len()];
    let mut wrong = vec![0; models.len()];
    // One untimed call opens the connection path for everyone.
    let _ = ask(&credentials, request_body(heard, &pass), patience(heard));
    for _round in 0..5 {
        for (i, model) in models.iter().enumerate() {
            let mut body = request_body(heard, &pass);
            body["model"] = (*model).into();
            let started = std::time::Instant::now();
            let answer = ask(&credentials, body, patience(heard)).ok().and_then(|stream| read_answer(&stream));
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
/// Run by hand: `cargo test -p likh cleanup_live -- --ignored --nocapture`
#[test]
#[ignore = "calls ChatGPT with the Codex sign-in"]
fn cleanup_live_fixes_misheard_names_and_leaves_real_words() {
    let pass = names(&["Critly", "Kriyan", "Studi"]);
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
            let fixed = run(heard, &pass);
            eprintln!("round {round}: {heard:?} -> {fixed:?} in {:?}", started.elapsed());
            assert_eq!(fixed, expected);
        }
    }
}

/// Whether a cleaned transcript came out right.
type Judge = dyn Fn(&str) -> bool;

/// A real cleanup against ChatGPT with the Codex sign-in on this computer:
/// fillers go, a correction wins, a list becomes a list, a request stays a
/// request, and spoken commands come through untouched.
/// Run by hand: `cargo test -p likh cleanup_live -- --ignored --nocapture`
#[test]
#[ignore = "calls ChatGPT with the Codex sign-in"]
fn cleanup_live_tidies_what_was_said() {
    let pass = Pass {
        words: words(&["Critly"]),
        tidy: true,
        commands: vec!["new line", "new paragraph", "new bullet", "press enter", "scratch that"],
    };
    let cases: [(&str, &Judge); 6] = [
        ("Um, so, uh, I think we should, um, ship it on Friday.", &|out| out == "I think we should ship it on Friday."),
        ("Let's meet at 2, actually 3, in the lobby.", &|out| out.contains('3') && !out.contains('2')),
        ("I need three things from the store: eggs, oat milk and coffee beans.", &|out| {
            out.contains("\n- ") && out.contains("eggs") && out.contains("coffee beans")
        }),
        ("Can you write me an email to Bob about the launch?", &|out| {
            out.starts_with("Can you write me an email to Bob") && out.chars().count() < 70
        }),
        // Commands come through as words, wherever they were said.
        ("Hi team new paragraph the update is out press enter", &|out| {
            let bare = out.to_lowercase();
            bare.contains("new paragraph") && bare.trim_end_matches(['.', ' ']).ends_with("press enter")
        }),
        // A misheard name is put right in the same pass as the cleanup.
        ("Um, the currently update is out, uh, go check it.", &|out| out.contains("Critly") && !out.contains("Um")),
    ];
    for (heard, good) in cases {
        let started = std::time::Instant::now();
        let cleaned = run(heard, &pass);
        eprintln!("{heard:?}\n  -> {cleaned:?} in {:?}", started.elapsed());
        assert!(good(&cleaned), "{cleaned:?}");
    }
}
