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
    let body = multipart("b0und", b"RIFFdata", &[]);
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
    assert_eq!(Transcriber::Codex.transcribe(&speech(0.2), &[]), Err(Failure::TooShort));
}

#[test]
fn the_echo_transcriber_returns_its_text() {
    let echo = Transcriber::Echo("Hello from the fake.".into());
    assert_eq!(echo.transcribe(&speech(1.0), &["Critly".into()]), Ok("Hello from the fake.".into()));
}

#[test]
fn groq_gets_the_model_as_a_field_after_the_file() {
    let body = multipart("b0und", b"RIFFdata", &[("model", "whisper-large-v3-turbo"), ("temperature", "0")]);
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains(
        "RIFFdata\r\n--b0und\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwhisper-large-v3-turbo\r\n--b0und\r\nContent-Disposition: form-data; name=\"temperature\"\r\n\r\n0\r\n--b0und--\r\n"
    ));
}

#[test]
fn groq_hears_the_dictionary_as_its_prompt() {
    let prompt = crate::dictionary::whisper_prompt(&["Critly".into(), "Kriyan".into()]);
    assert_eq!(prompt.as_deref(), Some("Critly, Kriyan."));
    let body = multipart("b0und", b"RIFFdata", &[("model", "whisper-large-v3-turbo"), ("prompt", "Critly, Kriyan.")]);
    assert!(String::from_utf8_lossy(&body).contains(
        "\r\n--b0und\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\nCritly, Kriyan.\r\n--b0und--\r\n"
    ));
}

#[test]
fn groq_answers_map_to_text_or_a_fix() {
    assert_eq!(
        groq::interpret(200, r#"{"text":" Ship it on Friday.","x_groq":{"id":"req_01"}}"#),
        Ok("Ship it on Friday.".into())
    );
    assert_eq!(
        groq::interpret(
            401,
            r#"{"error":{"message":"Invalid API Key","type":"invalid_request_error","code":"invalid_api_key"}}"#
        ),
        Err(Failure::GroqRejected)
    );
    assert_eq!(groq::interpret(429, r#"{"error":{"code":"rate_limit_exceeded"}}"#), Err(Failure::GroqRateLimited));
    assert_eq!(groq::interpret(413, ""), Err(Failure::TooLong));
    assert_eq!(groq::interpret(503, "busy"), Err(Failure::GroqServer(503)));
    assert_eq!(Failure::GroqRejected.message(), "Groq turned down your key. Paste a new one in likh's settings.");
}

#[test]
fn only_groq_shaped_keys_are_accepted() {
    assert!(groq::looks_like_a_key("  gsk_9fA2bC4dE6fG8hJ0kL2mN4pQ6rS8tU0vW2xY4z  "));
    assert!(!groq::looks_like_a_key("sk-proj-abc123abc123abc123abc123"));
    assert!(!groq::looks_like_a_key("gsk_short"));
    assert!(!groq::looks_like_a_key("gsk_has spaces in the middle of it"));
}

#[test]
fn the_fake_wins_over_every_engine() {
    for engine in Engine::ALL {
        assert_eq!(Transcriber::new(engine, Some("Hi.")), Transcriber::Echo("Hi.".into()));
    }
    assert_eq!(Transcriber::new(Engine::Groq, None), Transcriber::Groq);
}

/// A real Groq round trip: likh's own WAV, multipart and parsing against the
/// live API. Needs a key and a recording, both from the environment, so the
/// key is never written anywhere. Run by hand:
/// `GROQ_API_KEY=gsk_... LIKH_TEST_WAV=.agents/qa/speech.wav cargo test -p likh groq_live -- --ignored --nocapture`
/// (make the WAV with `scripts/tts-wav.ps1`; LIKH_TEST_TEXT is what it says,
/// written as dictation comes out: Whisper writes "nine" as "9").
#[test]
#[ignore = "calls Groq with a real key"]
fn groq_live_transcribes_a_real_recording_and_refuses_a_made_up_key() {
    let key = std::env::var("GROQ_API_KEY").expect("set GROQ_API_KEY");
    let path = std::env::var("LIKH_TEST_WAV").expect("set LIKH_TEST_WAV");
    let expected = std::env::var("LIKH_TEST_TEXT").expect("set LIKH_TEST_TEXT");
    let mut reader = hound::WavReader::open(&path).expect("read the WAV");
    let spec = reader.spec();
    assert_eq!((spec.sample_rate, spec.channels), (SAMPLE_RATE, 1), "tts-wav.ps1 makes 16 kHz mono");
    let samples: Vec<f32> = reader.samples::<i16>().map(|s| f32::from(s.unwrap_or(0)) / 32768.0).collect();

    assert_eq!(groq::verify(&key), Ok(()));
    assert_eq!(groq::verify("gsk_madeUpKeyThatGroqHasNeverIssued0000000000000000"), Err(Failure::GroqRejected));

    let started = std::time::Instant::now();
    let heard =
        groq::transcribe(&key, &wav(&samples), samples.len() as f32 / SAMPLE_RATE as f32, None).expect("a transcript");
    eprintln!("Groq heard {heard:?} in {:?}", started.elapsed());
    let words = |text: &str| -> Vec<String> {
        text.split_whitespace()
            .map(|w| w.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase())
            .filter(|w| !w.is_empty())
            .collect()
    };
    assert_eq!(words(&heard), words(&expected));
}
