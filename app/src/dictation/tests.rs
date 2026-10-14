use super::*;
use crate::codex_auth::{Account, CodexStatus};
use crate::commands::{Groups, Press};

const HOLD_OR_TAP: Choices = Choices { mode: Mode::HoldOrTap, commands: Some(Groups::DEFAULT), silence_stop: None };
const HOLD_ONLY: Choices = Choices { mode: Mode::Hold, ..HOLD_OR_TAP };
const TAP_ONLY: Choices = Choices { mode: Mode::Tap, ..HOLD_OR_TAP };

fn after(start: Instant, ms: u64) -> Instant {
    start + Duration::from_millis(ms)
}

fn paste(text: &str) -> Effect {
    Effect::Paste { text: text.into(), then: None }
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
    assert_eq!(d.handle(Input::Release(after(t0, 2_400)), after(t0, 2_400), HOLD_OR_TAP), transcribing());
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
    assert_eq!(
        d.handle(Input::Release(after(t0, 150)), after(t0, 150), HOLD_OR_TAP),
        vec![Effect::Pill(Show::HandsFree)]
    );
    assert_eq!(d.phase(), Phase::HandsFree);
    // Letting go of the second tap changes nothing; pressing it finished the dictation.
    assert_eq!(d.handle(Input::Hold, after(t0, 9_000), HOLD_OR_TAP), transcribing());
    assert_eq!(d.handle(Input::Release(after(t0, 9_100)), after(t0, 9_100), HOLD_OR_TAP), vec![]);
}

#[test]
fn in_hold_only_a_tap_is_dropped_quietly() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_ONLY);
    assert_eq!(
        d.handle(Input::Release(after(t0, 120)), after(t0, 120), HOLD_ONLY),
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
    assert_eq!(d.handle(Input::Release(after(t0, 2_000)), after(t0, 2_000), TAP_ONLY), vec![]);
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
        assert_eq!(d.handle(Input::Release(after(t0, 4_100)), after(t0, 4_100), choices), vec![]);
    }
}

#[test]
fn spoken_commands_turn_into_actions_unless_switched_off() {
    let t0 = Instant::now();
    let said = "Sounds good, see you at nine. Press enter.";
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    d.handle(Input::Release(after(t0, 2_000)), after(t0, 2_000), HOLD_OR_TAP);
    assert_eq!(
        d.handle(Input::Transcribed(Ok(said.into())), after(t0, 2_600), HOLD_OR_TAP),
        vec![
            Effect::Pill(Show::Done),
            Effect::Paste { text: "Sounds good, see you at nine.".into(), then: Some(Press::Enter) }
        ]
    );

    let plain = Choices { commands: None, ..HOLD_OR_TAP };
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, plain);
    d.handle(Input::Release(after(t0, 2_000)), after(t0, 2_000), plain);
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
    d.handle(Input::Release(after(t0, 1_000)), after(t0, 1_000), HOLD_OR_TAP);
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
    d.handle(Input::Release(after(t0, 2_000)), after(t0, 2_000), HOLD_OR_TAP);
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
    d.handle(Input::Release(after(t0, 2_000)), after(t0, 2_000), HOLD_OR_TAP);
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
        d.handle(
            Input::MicFailed { reason: "No microphone is connected.".into(), blocked: false },
            after(t0, 50),
            HOLD_OR_TAP
        ),
        vec![
            Effect::Discard,
            Effect::Pill(Show::Error("likh can't hear a microphone. No microphone is connected.".into())),
            Effect::Sound(Cue::Error)
        ]
    );
}

#[test]
fn a_mic_windows_blocks_offers_its_privacy_page() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    assert_eq!(
        d.handle(Input::MicFailed { reason: "Access is denied.".into(), blocked: true }, after(t0, 50), HOLD_OR_TAP),
        vec![
            Effect::Discard,
            Effect::Pill(Show::Problem { text: "Windows is blocking likh from the mic.".into(), fix: Fix::MicAccess }),
            Effect::Sound(Cue::Error)
        ]
    );
}

fn sound(heard: bool) -> Input {
    Input::Sound { heard, lost: false, device: "Microphone Array".into() }
}

#[test]
fn a_mic_silent_for_two_seconds_says_so_while_you_hold_and_sound_brings_the_bars_back() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    // People take a moment to start talking: nothing yet.
    assert_eq!(d.handle(sound(false), after(t0, 1_750), HOLD_OR_TAP), vec![]);
    assert_eq!(
        d.handle(sound(false), after(t0, 2_000), HOLD_OR_TAP),
        vec![Effect::Pill(Show::MicSilent { device: "Microphone Array".into() })]
    );
    // Said once, not on every tick.
    assert_eq!(d.handle(sound(false), after(t0, 2_250), HOLD_OR_TAP), vec![]);
    // Unmuted mid-dictation: the bars return and the recording carries on.
    assert_eq!(d.handle(sound(true), after(t0, 3_000), HOLD_OR_TAP), vec![Effect::Pill(Show::Listening)]);
    assert_eq!(d.handle(sound(true), after(t0, 3_250), HOLD_OR_TAP), vec![]);
    assert_eq!(d.phase(), Phase::Recording);
}

#[test]
fn hands_free_gets_its_stop_button_back_when_sound_returns() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, TAP_ONLY);
    d.handle(sound(false), after(t0, 2_500), TAP_ONLY);
    assert_eq!(d.handle(sound(true), after(t0, 2_750), TAP_ONLY), vec![Effect::Pill(Show::HandsFree)]);
}

#[test]
fn a_mic_that_sounds_from_the_start_is_never_called_silent() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    for ms in [250, 2_000, 5_000] {
        assert_eq!(d.handle(sound(true), after(t0, ms), HOLD_OR_TAP), vec![]);
    }
}

#[test]
fn no_voice_heard_is_a_grey_note_without_the_error_sound() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    d.handle(Input::Release(after(t0, 2_000)), after(t0, 2_000), HOLD_OR_TAP);
    assert_eq!(
        d.handle(Input::Transcribed(Err(Failure::NoVoice)), after(t0, 2_010), HOLD_OR_TAP),
        vec![Effect::Pill(Show::Note("No voice heard".into()))]
    );
    assert_eq!(d.phase(), Phase::Idle);
}

#[test]
fn a_silent_mic_is_a_problem_with_its_fix_and_the_error_sound() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    d.handle(Input::Release(after(t0, 2_000)), after(t0, 2_000), HOLD_OR_TAP);
    let silent = Failure::MicSilent { device: "Microphone Array".into(), blocked: false };
    assert_eq!(
        d.handle(Input::Transcribed(Err(silent)), after(t0, 2_010), HOLD_OR_TAP),
        vec![
            Effect::Sound(Cue::Error),
            Effect::Pill(Show::Problem {
                text: "No sound from Microphone Array. Is it muted?".into(),
                fix: Fix::PickMic
            })
        ]
    );
}

#[test]
fn paste_last_works_only_between_dictations() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    assert_eq!(d.handle(Input::PasteLast, t0, HOLD_OR_TAP), vec![Effect::PasteLast]);
    // Mid-recording it would land inside the sentence being spoken.
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    assert_eq!(d.handle(Input::PasteLast, after(t0, 500), HOLD_OR_TAP), vec![]);
    d.handle(Input::Release(after(t0, 2_000)), after(t0, 2_000), HOLD_OR_TAP);
    assert_eq!(d.handle(Input::PasteLast, after(t0, 2_100), HOLD_OR_TAP), vec![]);
    assert_eq!(d.phase(), Phase::Transcribing);
}

#[test]
fn only_a_recording_with_a_voice_in_it_is_sent() {
    let ears = |sounded, spoke| Ears { sounded, spoke, device: "Headset".into() };
    // Speech goes to the transcriber.
    assert_eq!(ears(true, true).heard_nothing(2.0), None);
    // A room, a fan or music with nobody talking.
    assert_eq!(ears(true, false).heard_nothing(2.0), Some(Failure::NoVoice));
    // Digital silence: muted, or blocked.
    assert!(
        matches!(ears(false, false).heard_nothing(2.0), Some(Failure::MicSilent { device, .. }) if device == "Headset")
    );
    // Too short to judge: the transcriber drops it quietly, as before.
    assert_eq!(ears(false, false).heard_nothing(0.2), None);
}

#[test]
fn holding_again_while_transcribing_is_ignored() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    d.handle(Input::Release(after(t0, 1_000)), after(t0, 1_000), HOLD_OR_TAP);
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

#[test]
fn hands_free_stops_by_itself_after_the_chosen_quiet() {
    let t0 = Instant::now();
    let quiet_stop = Choices { silence_stop: Some(Duration::from_secs(5)), ..TAP_ONLY };
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, quiet_stop);
    let left = Duration::from_millis(100);
    assert_eq!(
        d.handle(Input::Quiet(Duration::from_millis(4_900)), after(t0, 8_000), quiet_stop),
        vec![Effect::Pill(Show::QuietStop { left, after: Duration::from_secs(5) })]
    );
    assert_eq!(d.phase(), Phase::HandsFree);
    assert_eq!(d.handle(Input::Quiet(Duration::from_secs(5)), after(t0, 8_100), quiet_stop), transcribing());
}

#[test]
fn the_pill_counts_down_the_quiet_after_a_second_and_talking_again_resets_it() {
    let t0 = Instant::now();
    let after_5s = Duration::from_secs(5);
    let quiet_stop = Choices { silence_stop: Some(after_5s), ..TAP_ONLY };
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, quiet_stop);
    // A pause between sentences shows nothing.
    assert_eq!(d.handle(Input::Quiet(Duration::from_millis(750)), after(t0, 3_000), quiet_stop), vec![]);
    assert_eq!(
        d.handle(Input::Quiet(Duration::from_millis(1_500)), after(t0, 3_750), quiet_stop),
        vec![Effect::Pill(Show::QuietStop { left: Duration::from_millis(3_500), after: after_5s })]
    );
    assert_eq!(
        d.handle(Input::Quiet(Duration::from_millis(1_750)), after(t0, 4_000), quiet_stop),
        vec![Effect::Pill(Show::QuietStop { left: Duration::from_millis(3_250), after: after_5s })]
    );
    // Speaking again puts the plain stop button back, once.
    assert_eq!(
        d.handle(Input::Quiet(Duration::from_millis(100)), after(t0, 4_250), quiet_stop),
        vec![Effect::Pill(Show::HandsFree)]
    );
    assert_eq!(d.handle(Input::Quiet(Duration::from_millis(350)), after(t0, 4_500), quiet_stop), vec![]);
    assert_eq!(d.phase(), Phase::HandsFree);
}

#[test]
fn the_last_minute_countdown_wins_over_the_quiet_one() {
    let t0 = Instant::now();
    let quiet_stop = Choices { silence_stop: Some(Duration::from_secs(10)), ..TAP_ONLY };
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, quiet_stop);
    assert_eq!(
        d.handle(Input::Tick, after(t0, 550_000), quiet_stop),
        vec![Effect::Pill(Show::Warning { seconds_left: 45 })]
    );
    assert_eq!(d.handle(Input::Quiet(Duration::from_secs(4)), after(t0, 550_000), quiet_stop), vec![]);
}

#[test]
fn a_tap_counts_from_when_the_key_came_up_not_from_when_likh_got_to_it() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    // Opening the mic held the thread for 600ms; the key came up after 150.
    assert_eq!(
        d.handle(Input::Release(after(t0, 150)), after(t0, 600), HOLD_OR_TAP),
        vec![Effect::Pill(Show::HandsFree)]
    );
    assert_eq!(d.phase(), Phase::HandsFree);
}

#[test]
fn silence_never_stops_a_held_shortcut_or_when_switched_off() {
    let t0 = Instant::now();
    let quiet_stop = Choices { silence_stop: Some(Duration::from_secs(3)), ..HOLD_OR_TAP };
    let mut held = Dictation::default();
    held.handle(Input::Hold, t0, quiet_stop);
    assert_eq!(held.handle(Input::Quiet(Duration::from_secs(20)), after(t0, 25_000), quiet_stop), vec![]);
    assert_eq!(held.phase(), Phase::Recording);

    let mut off = Dictation::default();
    off.handle(Input::Hold, t0, TAP_ONLY);
    assert_eq!(off.handle(Input::Quiet(Duration::from_secs(60)), after(t0, 70_000), TAP_ONLY), vec![]);
    assert_eq!(off.phase(), Phase::HandsFree);
}

#[test]
fn scratching_everything_said_pastes_nothing() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    d.handle(Input::Release(after(t0, 2_000)), after(t0, 2_000), HOLD_OR_TAP);
    assert_eq!(
        d.handle(Input::Transcribed(Ok("Let's meet at two. Scratch that.".into())), after(t0, 2_600), HOLD_OR_TAP),
        vec![Effect::Pill(Show::Cancelled)]
    );
}

#[test]
fn press_tab_presses_tab_after_the_paste() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    d.handle(Input::Release(after(t0, 2_000)), after(t0, 2_000), HOLD_OR_TAP);
    assert_eq!(
        d.handle(Input::Transcribed(Ok("Priya Raman, press tab.".into())), after(t0, 2_600), HOLD_OR_TAP),
        vec![Effect::Pill(Show::Done), Effect::Paste { text: "Priya Raman".into(), then: Some(Press::Tab) }]
    );
}

#[test]
fn a_mic_unplugged_mid_sentence_pastes_what_it_heard_and_says_so() {
    let t0 = Instant::now();
    let mut d = Dictation::default();
    d.handle(Input::Hold, t0, HOLD_OR_TAP);
    let lost = Input::Sound { heard: true, lost: true, device: "WH-1000XM4".into() };
    assert_eq!(d.handle(lost.clone(), after(t0, 3_000), HOLD_OR_TAP), transcribing());
    // Said once: the next tick finds it already transcribing.
    assert_eq!(d.handle(lost, after(t0, 3_250), HOLD_OR_TAP), vec![]);
    assert_eq!(
        d.handle(Input::Transcribed(Ok("Meet at three.".into())), after(t0, 3_900), HOLD_OR_TAP),
        vec![
            Effect::Pill(Show::Note("WH-1000XM4 was disconnected. Pasted what it heard.".into())),
            paste("Meet at three.")
        ]
    );
    // The next dictation is an ordinary one.
    d.handle(Input::Hold, after(t0, 9_000), HOLD_OR_TAP);
    d.handle(Input::Release(after(t0, 11_000)), after(t0, 11_000), HOLD_OR_TAP);
    assert_eq!(
        d.handle(Input::Transcribed(Ok("Okay.".into())), after(t0, 11_500), HOLD_OR_TAP),
        vec![Effect::Pill(Show::Done), paste("Okay.")]
    );
}
