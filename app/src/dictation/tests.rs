use super::*;
use crate::codex_auth::{Account, CodexStatus};

fn after(start: Instant, ms: u64) -> Instant {
    start + Duration::from_millis(ms)
}

#[test]
fn hold_talk_release_pastes_the_transcript() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    assert_eq!(d.handle(Input::Hold, t0), vec![Effect::StartRecording, Effect::Pill(Show::Listening)]);
    assert_eq!(d.handle(Input::Release, after(t0, 2_400)), vec![Effect::Transcribe, Effect::Pill(Show::Thinking)]);
    assert_eq!(d.phase(), Phase::Transcribing);
    assert_eq!(
        d.handle(Input::Transcribed(Ok("Remind me to send the deck tomorrow at nine.".into())), after(t0, 3_300)),
        vec![Effect::Pill(Show::Done), Effect::Paste("Remind me to send the deck tomorrow at nine.".into())]
    );
    assert_eq!(d.phase(), Phase::Idle);
}

#[test]
fn a_stray_tap_is_dropped_without_an_error() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0);
    assert_eq!(d.handle(Input::Release, after(t0, 120)), vec![Effect::Discard, Effect::Pill(Show::Cancelled)]);
    assert_eq!(d.phase(), Phase::Idle);
}

#[test]
fn esc_cancels_and_throws_the_audio_away() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0);
    assert_eq!(d.handle(Input::Cancel, after(t0, 4_000)), vec![Effect::Discard, Effect::Pill(Show::Cancelled)]);
    // The key coming up afterwards does nothing.
    assert_eq!(d.handle(Input::Release, after(t0, 4_100)), vec![]);
}

#[test]
fn a_failed_transcription_says_why_and_pastes_nothing() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0);
    d.handle(Input::Release, after(t0, 2_000));
    let expired = Failure::SignIn(CodexStatus::Expired(Account { email_hint: None, plan: None, expires_at: Some(1) }));
    assert_eq!(
        d.handle(Input::Transcribed(Err(expired)), after(t0, 2_500)),
        vec![Effect::Pill(Show::Error(
            "Your Codex sign-in expired. Open Codex once to refresh it, then try again.".into()
        ))]
    );
    assert_eq!(d.phase(), Phase::Idle);
}

#[test]
fn silence_transcribed_to_nothing_pastes_nothing() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0);
    d.handle(Input::Release, after(t0, 2_000));
    assert_eq!(d.handle(Input::Transcribed(Ok(String::new())), after(t0, 2_600)), vec![Effect::Pill(Show::Cancelled)]);
}

#[test]
fn a_mic_that_wont_open_shows_the_reason() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0);
    assert_eq!(
        d.handle(Input::MicFailed("No microphone is connected.".into()), after(t0, 50)),
        vec![
            Effect::Discard,
            Effect::Pill(Show::Error("murmur can't hear a microphone. No microphone is connected.".into()))
        ]
    );
}

#[test]
fn holding_again_while_transcribing_is_ignored() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0);
    d.handle(Input::Release, after(t0, 1_000));
    assert_eq!(d.handle(Input::Hold, after(t0, 1_200)), vec![]);
    assert_eq!(d.phase(), Phase::Transcribing);
}

#[test]
fn the_last_minute_counts_down_once_per_second_then_stops_at_the_cap() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0);
    assert_eq!(d.handle(Input::Tick, after(t0, 539_900)), vec![]);
    assert_eq!(d.handle(Input::Tick, after(t0, 540_000)), vec![Effect::Pill(Show::Warning { seconds_left: 55 })]);
    // A second tick in the same second changes nothing on screen.
    assert_eq!(d.handle(Input::Tick, after(t0, 540_250)), vec![]);
    assert_eq!(d.handle(Input::Tick, after(t0, 541_000)), vec![Effect::Pill(Show::Warning { seconds_left: 54 })]);
    assert_eq!(d.handle(Input::Tick, after(t0, 595_000)), vec![Effect::Transcribe, Effect::Pill(Show::Thinking)]);
}
