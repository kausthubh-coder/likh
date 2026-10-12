use super::*;
use crate::dictation::Phase;

fn idle() -> Status {
    Status { phase: Phase::Idle, transcriber: "echo", dictations: 0, last: None }
}

#[test]
fn start_and_stop_are_the_shortcut_going_down_and_up() {
    assert_eq!(answer("start", &idle()), (json!({ "ok": true }), Some(Input::Hold)));
    assert_eq!(answer("stop", &idle()), (json!({ "ok": true }), Some(Input::Release)));
    assert_eq!(answer("cancel", &idle()), (json!({ "ok": true }), Some(Input::Cancel)));
}

#[test]
fn status_reports_the_phase_and_transcriber_but_no_text() {
    let (reply, input) = answer("status", &idle());
    assert_eq!(input, None);
    assert_eq!(reply["dictation"]["phase"], "idle");
    assert_eq!(reply["dictation"]["transcriber"], "echo");
    assert!(reply["codex"]["state"].is_string());
}

#[test]
fn an_unknown_command_lists_the_real_ones() {
    let (reply, input) = answer("paste", &idle());
    assert_eq!(input, None);
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["error"], "unknown command \"paste\"; try one of status, start, stop, cancel, quit");
}
