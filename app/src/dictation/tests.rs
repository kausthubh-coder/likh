use super::*;
use crate::codex_auth::{Account, CodexStatus};

const HOLD_OR_TAP: Choices = Choices { mode: Mode::HoldOrTap, commands: true };
const HOLD_ONLY: Choices = Choices { mode: Mode::Hold, commands: true };
const TAP_ONLY: Choices = Choices { mode: Mode::Tap, commands: true };

fn after(start: Instant, ms: u64) -> Instant {
    start + Duration::from_millis(ms)
}

fn paste(text: &str) -> Effect {
    Effect::Paste { text: text.into(), then_enter: false }
}

const STARTS: [Effect; 3] = [Effect::StartRecording, Effect::Sound(Cue::Start), Effect::Pill(Show::Listening)];

fn transcribing() -> Vec<Effect> {
    vec![Effect::Transcribe, Effect::Sound(Cue::Stop), Effect::Pill(Show::Thinking)]
}

#[test]
fn hold_talk_release_pastes_the_transcript() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    assert_eq!(d.handle(Input::Hold, t0, HOLD_OR_TAP), STARTS.to_vec());
    assert_eq!(d.handle(Input::Release, after(t0, 2_400), HOLD_OR_TAP), transcribing());
    assert_eq!(d.phase(), Phase::Transcribing);
    assert_eq!(
        d.handle(
            Input::Transcribed(Ok("Remind me to send the deck tomorrow at nine.".into())),
            after(t0, 3_300),
            HOLD_OR_TAP
        ),
        vec![Effect::Pill(Show::Done), paste("Remind me to send the deck tomorrow at nine.")]
    );
    assert_eq!(d.phase(), Phase::Idle);
}

#[test]
fn in_hold_or_tap_a_tap_goes_hands_free_and_the_next_tap_finishes() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    assert_eq!(d.handle(Input::Release, after(t0, 150), HOLD_OR_TAP), vec![Effect::Pill(Show::HandsFree)]);
    assert_eq!(d.phase(), Phase::HandsFree);
    // Letting go of the second tap changes nothing; pressing it finished the dictation.
    assert_eq!(d.handle(Input::Hold, after(t0, 9_000), HOLD_OR_TAP), transcribing());
    assert_eq!(d.handle(Input::Release, after(t0, 9_100), HOLD_OR_TAP), vec![]);
}

#[test]
fn in_hold_only_a_tap_is_dropped_quietly() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_ONLY);
    assert_eq!(
        d.handle(Input::Release, after(t0, 120), HOLD_ONLY),
        vec![Effect::Discard, Effect::Pill(Show::Cancelled)]
    );
    assert_eq!(d.phase(), Phase::Idle);
}

#[test]
fn in_tap_mode_one_press_starts_hands_free_and_another_finishes() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    assert_eq!(
        d.handle(Input::Hold, t0, TAP_ONLY),
        vec![Effect::StartRecording, Effect::Sound(Cue::Start), Effect::Pill(Show::HandsFree)]
    );
    assert_eq!(d.handle(Input::Release, after(t0, 2_000), TAP_ONLY), vec![]);
    assert_eq!(d.handle(Input::Hold, after(t0, 6_000), TAP_ONLY), transcribing());
}

#[test]
fn the_pill_stop_button_finishes_hands_free() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, TAP_ONLY);
    assert_eq!(d.handle(Input::Stop, after(t0, 5_000), TAP_ONLY), transcribing());
    // A stray stop with nothing recording does nothing.
    let mut idle = Dictation::default();
    assert_eq!(idle.handle(Input::Stop, t0, TAP_ONLY), vec![]);
}

#[test]
fn esc_cancels_and_throws_the_audio_away_in_any_recording() {
    let t0 = Instant::now();
    for choices in [HOLD_OR_TAP, TAP_ONLY] {
        let mut d = Dictation::default();
        d.handle(Input::Hold, t0, choices);
        assert_eq!(
            d.handle(Input::Cancel, after(t0, 4_000), choices),
            vec![Effect::Discard, Effect::Pill(Show::Cancelled)]
        );
        assert_eq!(d.handle(Input::Release, after(t0, 4_100), choices), vec![]);
    }
}

#[test]
fn spoken_commands_turn_into_actions_unless_switched_off() {
    let t0 = Instant::now();
    let said = "Sounds good, see you at nine. Press enter.";
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    d.handle(Input::Release, after(t0, 2_000), HOLD_OR_TAP);
    assert_eq!(
        d.handle(Input::Transcribed(Ok(said.into())), after(t0, 2_600), HOLD_OR_TAP),
        vec![
            Effect::Pill(Show::Done),
            Effect::Paste { text: "Sounds good, see you at nine.".into(), then_enter: true }
        ]
    );

    let plain = Choices { commands: false, ..HOLD_OR_TAP };
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, plain);
    d.handle(Input::Release, after(t0, 2_000), plain);
    assert_eq!(
        d.handle(Input::Transcribed(Ok(said.into())), after(t0, 2_600), plain),
        vec![Effect::Pill(Show::Done), paste(said)]
    );
}

#[test]
fn scratch_that_undoes_the_last_paste() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    d.handle(Input::Release, after(t0, 1_000), HOLD_OR_TAP);
    assert_eq!(
        d.handle(Input::Transcribed(Ok("Scratch that.".into())), after(t0, 1_500), HOLD_OR_TAP),
        vec![Effect::Pill(Show::Done), Effect::Undo]
    );
}

#[test]
fn a_failed_transcription_says_why_with_a_sound_and_pastes_nothing() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    d.handle(Input::Release, after(t0, 2_000), HOLD_OR_TAP);
    let expired = Failure::SignIn(CodexStatus::Expired(Account { email_hint: None, plan: None, expires_at: Some(1) }));
    assert_eq!(
        d.handle(Input::Transcribed(Err(expired)), after(t0, 2_500), HOLD_OR_TAP),
        vec![
            Effect::Sound(Cue::Error),
            Effect::Pill(Show::Error(
                "Your Codex sign-in expired. Open Codex once to refresh it, then try again.".into()
            ))
        ]
    );
    assert_eq!(d.phase(), Phase::Idle);
}

#[test]
fn silence_transcribed_to_nothing_pastes_nothing() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    d.handle(Input::Release, after(t0, 2_000), HOLD_OR_TAP);
    assert_eq!(
        d.handle(Input::Transcribed(Ok(String::new())), after(t0, 2_600), HOLD_OR_TAP),
        vec![Effect::Pill(Show::Cancelled)]
    );
}

#[test]
fn a_mic_that_wont_open_shows_the_reason() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    assert_eq!(
        d.handle(Input::MicFailed("No microphone is connected.".into()), after(t0, 50), HOLD_OR_TAP),
        vec![
            Effect::Discard,
            Effect::Pill(Show::Error("likh can't hear a microphone. No microphone is connected.".into())),
            Effect::Sound(Cue::Error)
        ]
    );
}

#[test]
fn holding_again_while_transcribing_is_ignored() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    d.handle(Input::Release, after(t0, 1_000), HOLD_OR_TAP);
    assert_eq!(d.handle(Input::Hold, after(t0, 1_200), HOLD_OR_TAP), vec![]);
    assert_eq!(d.phase(), Phase::Transcribing);
}

#[test]
fn the_last_minute_counts_down_once_per_second_then_stops_at_the_cap() {
    let t0 = Instant::now();
    for choices in [HOLD_OR_TAP, TAP_ONLY] {
        let mut d = Dictation::default();
        d.handle(Input::Hold, t0, choices);
        assert_eq!(d.handle(Input::Tick, after(t0, 539_900), choices), vec![]);
        assert_eq!(
            d.handle(Input::Tick, after(t0, 540_000), choices),
            vec![Effect::Pill(Show::Warning { seconds_left: 55 })]
        );
        // A second tick in the same second changes nothing on screen.
        assert_eq!(d.handle(Input::Tick, after(t0, 540_250), choices), vec![]);
        assert_eq!(
            d.handle(Input::Tick, after(t0, 541_000), choices),
            vec![Effect::Pill(Show::Warning { seconds_left: 54 })]
        );
        assert_eq!(d.handle(Input::Tick, after(t0, 595_000), choices), transcribing());
    }
}
