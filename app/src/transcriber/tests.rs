use super::*;

fn speech(seconds: f32) -> Vec<f32> {
    (0..(seconds * SAMPLE_RATE as f32) as usize).map(|i| 0.1 * (i as f32 * 0.0577).sin()).collect()
}

#[test]
fn wav_is_16khz_mono_pcm_and_clips_safely() {
    let bytes = wav(&[-2.0, 0.0, 2.0, f32::NAN]);
    let mut reader = hound::WavReader::new(Cursor::new(bytes)).expect("valid wav");
    assert_eq!(reader.spec().sample_rate, 16_000);
    assert_eq!(reader.spec().channels, 1);
    let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap_or(i16::MIN)).collect();
    assert_eq!(samples, vec![-32767, 0, 32767, 0]);
}

#[test]
fn multipart_wraps_the_wav_as_the_file_part() {
    let body = multipart("b0und", b"RIFFdata");
    let text = String::from_utf8_lossy(&body);
    assert!(text.starts_with("--b0und\r\nContent-Disposition: form-data; name=\"file\"; filename=\"speech.wav\"\r\nContent-Type: audio/wav\r\n\r\nRIFFdata"));
    assert!(text.ends_with("\r\n--b0und--\r\n"));
}

#[test]
fn a_transcript_comes_back_trimmed() {
    assert_eq!(
        interpret(200, r#"{"text":"  Remind me to send the deck tomorrow at nine. "}"#),
        Ok("Remind me to send the deck tomorrow at nine.".into())
    );
}

#[test]
fn every_error_maps_to_a_fix_the_user_can_act_on() {
    assert_eq!(interpret(401, ""), Err(Failure::Rejected));
    assert_eq!(interpret(403, "{}"), Err(Failure::Rejected));
    assert_eq!(interpret(429, ""), Err(Failure::RateLimited));
    assert_eq!(interpret(400, r#"{"error":{"code":"audio_too_short"}}"#), Err(Failure::TooShort));
    assert_eq!(interpret(413, r#"{"error":{"code":"file_above_max_size"}}"#), Err(Failure::TooLong));
    assert_eq!(interpret(400, r#"{"error":{"code":"input_too_large"}}"#), Err(Failure::TooLong));
    assert_eq!(interpret(500, "oops"), Err(Failure::Server(500)));
    assert_eq!(interpret(200, "<html>"), Err(Failure::Unreadable));
    assert_eq!(Failure::Rejected.message(), "ChatGPT turned down the sign-in. Sign in to Codex again.");
}

#[test]
fn a_tap_is_too_short_and_never_reaches_chatgpt() {
    // Codex would fail without a sign-in; TooShort proves it never got that far.
    assert_eq!(Transcriber::Codex.transcribe(&speech(0.2)), Err(Failure::TooShort));
}

#[test]
fn the_echo_transcriber_returns_its_text() {
    let echo = Transcriber::Echo("Hello from the fake.".into());
    assert_eq!(echo.transcribe(&speech(1.0)), Ok("Hello from the fake.".into()));
}
