use super::*;
use crate::dictation::Phase;
use crate::update::State;

fn idle() -> Status {
    Status { phase: Phase::Idle, transcriber: "echo", dictations: 0, actions: 0, last: None, last_text: None }
}

#[test]
fn start_stop_and_cancel_drive_a_dictation() {
    assert_eq!(answer("start", &idle(), &State::Idle), (json!({ "ok": true }), Some(Input::Hold)));
    assert_eq!(answer("stop", &idle(), &State::Idle), (json!({ "ok": true }), Some(Input::Stop)));
    assert_eq!(answer("cancel", &idle(), &State::Idle), (json!({ "ok": true }), Some(Input::Cancel)));
}

#[test]
fn action_start_and_stop_drive_the_action_key() {
    assert_eq!(answer("action-start", &idle(), &State::Idle), (json!({ "ok": true }), Some(Input::ActionHold)));
    assert_eq!(answer("action-stop", &idle(), &State::Idle), (json!({ "ok": true }), Some(Input::Stop)));
}

#[test]
fn paste_last_goes_to_the_dictation_and_copy_last_to_the_tray() {
    assert_eq!(answer("paste-last", &idle(), &State::Idle), (json!({ "ok": true }), Some(Input::PasteLast)));
    assert_eq!(answer("copy-last", &idle(), &State::Idle), (json!({ "ok": true }), None));
}

#[test]
fn status_never_prints_the_last_dictations_words() {
    let said = Status { last_text: Some("my bank PIN is 4512".into()), ..idle() };
    let (reply, _) = answer("status", &said, &State::Idle);
    assert!(!reply.to_string().contains("4512"), "{reply}");
}

#[test]
fn status_reports_the_phase_and_transcriber_but_no_text() {
    let (reply, input) = answer("status", &idle(), &State::Idle);
    assert_eq!(input, None);
    assert_eq!(reply["dictation"]["phase"], "idle");
    assert_eq!(reply["dictation"]["transcriber"], "echo");
    assert!(reply["codex"]["state"].is_string());
}

#[test]
fn status_and_update_say_where_updating_stands() {
    let ready = State::Ready { version: "0.1.0-alpha.6".into(), installer: "C:/likh/updates/likh-setup.exe".into() };
    let (reply, _) = answer("status", &idle(), &ready);
    assert_eq!(
        reply["update"],
        json!({ "state": "ready", "version": "0.1.0-alpha.6", "installer": "C:/likh/updates/likh-setup.exe" })
    );
    let (reply, input) = answer("update", &idle(), &State::Checking);
    assert_eq!((reply, input), (json!({ "ok": true, "update": { "state": "checking" } }), None));
}

#[test]
fn an_unknown_command_lists_the_real_ones() {
    let (reply, input) = answer("paste", &idle(), &State::Idle);
    assert_eq!(input, None);
    assert_eq!(reply["ok"], false);
    assert_eq!(
        reply["error"],
        "unknown command \"paste\"; try one of status, start, stop, action-start, action-stop, cancel, paste-last, copy-last, quit, open, update, install-update"
    );
}
