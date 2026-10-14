use super::*;

/// A 440 Hz tone at 48 kHz mono, like a held vowel.
fn tone(amplitude: f32, samples: usize) -> impl Iterator<Item = f32> + use<> {
    (0..samples).map(move |i| amplitude * (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin())
}

#[test]
fn silence_is_empty_and_a_full_scale_tone_is_full() {
    assert_eq!(level(0.0), 0.0);
    // A full-scale sine has an RMS of 0.707, about -3 dBFS: past the ceiling.
    let mut levels = Vec::new();
    Meter::new(1440).push(tone(1.0, 1440), |l| levels.push(l));
    assert_eq!(levels, vec![1.0]);
}

#[test]
fn normal_speech_level_lands_mid_scale() {
    // Amplitude 0.05 has an RMS of 0.0354, about -29 dBFS: (−29 + 55) / 45 ≈ 0.58.
    let mut levels = Vec::new();
    Meter::new(1440).push(tone(0.05, 1440), |l| levels.push(l));
    assert_eq!(levels.len(), 1);
    assert!((levels[0] - 0.578).abs() < 0.01, "level {}", levels[0]);
}

#[test]
fn one_level_per_full_window_and_the_rest_waits() {
    // 30ms at 48 kHz mono is 1440 samples; 2.5 windows give two levels.
    let mut meter = Meter::new(1440);
    let mut levels = Vec::new();
    meter.push(tone(0.05, 3600), |l| levels.push(l));
    assert_eq!(levels.len(), 2);
    // The leftover half window completes with the next buffer.
    meter.push(tone(0.05, 720), |l| levels.push(l));
    assert_eq!(levels.len(), 3);
}

#[test]
fn downsampling_48k_to_16k_keeps_the_pitch() {
    // One second of 440 Hz at 48 kHz has 880 zero crossings; so should the 16 kHz copy.
    let input: Vec<f32> = tone(0.5, 48_000).collect();
    let output = resample(&input, 48_000, 16_000);
    assert_eq!(output.len(), 16_000);
    let crossings = |s: &[f32]| s.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
    assert!((crossings(&output) as i32 - crossings(&input) as i32).abs() <= 2);
}

#[test]
fn upsampling_8k_interpolates_between_samples() {
    assert_eq!(resample(&[0.0, 1.0], 8_000, 16_000), vec![0.0, 0.5, 1.0, 1.0]);
}

#[test]
fn a_replayed_wav_is_recorded_at_16khz_with_levels() {
    // A real-shaped file: 0.6s of a 44.1 kHz stereo tone, like a phone voice memo.
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("memo.wav");
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).expect("create wav");
    for i in 0..(44_100 * 6 / 10) {
        let s = (0.2 * (i as f32 * 440.0 * std::f32::consts::TAU / 44_100.0).sin() * 32767.0) as i16;
        writer.write_sample(s).expect("left");
        writer.write_sample(s).expect("right");
    }
    writer.finalize().expect("finalize");

    let levels = Arc::new(Mutex::new(Vec::new()));
    let seen = levels.clone();
    let recording =
        Recording::start_from(Some(path), None, move |level| seen.lock().expect("levels").push(level)).expect("start");
    std::thread::sleep(Duration::from_millis(900));
    let samples = recording.finish();
    assert!((samples.len() as i32 - 9_600).abs() <= 2, "{} samples", samples.len());
    let levels = levels.lock().expect("levels");
    assert!(levels.len() >= 18, "{} levels", levels.len());
    assert!(levels.iter().all(|l| *l > 0.5), "a -17 dBFS tone should read loud: {levels:?}");
}

#[test]
fn a_missing_file_fails_to_start_and_says_which() {
    let error = Recording::start_from(Some(PathBuf::from("no/such.wav")), None, |_| {}).err().expect("should fail");
    assert!(error.starts_with(&format!("Can't read {}", PathBuf::from("no/such.wav").display())), "{error}");
}

/// 16 kHz mono: `speech` seconds of a -17 dBFS tone, then `silence` seconds of a quiet room.
fn speech_then_silence(path: &std::path::Path, speech: f32, silence: f32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).expect("create wav");
    for i in 0..(16_000.0 * speech) as usize {
        writer.write_sample((0.2 * (i as f32 * 0.17).sin() * 32767.0) as i16).expect("speech");
    }
    for i in 0..(16_000.0 * silence) as usize {
        // About -60 dBFS of hiss, like a laptop mic in a quiet room.
        writer.write_sample((0.001 * (i as f32 * 1.3).sin() * 32767.0) as i16).expect("room");
    }
    writer.finalize().expect("finalize");
}

#[test]
fn quiet_is_counted_from_the_last_speech() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("then-quiet.wav");
    speech_then_silence(&path, 0.4, 1.6);
    let recording = Recording::start_from(Some(path), None, |_| {}).expect("start");
    std::thread::sleep(Duration::from_millis(1_500));
    let quiet = recording.quiet_for().expect("the tone counts as speech");
    // Speech ended 0.4 s in; at 1.5 s about 1.1 s of quiet has passed.
    assert!(quiet >= Duration::from_millis(900) && quiet <= Duration::from_millis(1_300), "{quiet:?}");
}

#[test]
fn a_quiet_room_alone_never_counts_as_speech() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("room.wav");
    speech_then_silence(&path, 0.0, 1.0);
    let recording = Recording::start_from(Some(path), None, |_| {}).expect("start");
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(recording.quiet_for(), None);
}

#[test]
fn a_muted_mic_never_sounds_but_a_quiet_room_does() {
    let dir = tempfile::tempdir().expect("temp dir");
    // A muted mic: exact zeros, as Windows hands them over.
    let muted = dir.path().join("muted.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&muted, spec).expect("create wav");
    for _ in 0..16_000 {
        writer.write_sample(0i16).expect("zero");
    }
    writer.finalize().expect("finalize");
    let recording = Recording::start_from(Some(muted), None, |_| {}).expect("start");
    std::thread::sleep(Duration::from_millis(600));
    assert!(!recording.sounded());
    assert!(!recording.spoke());
    assert_eq!(recording.device(), "muted.wav");

    // About -60 dBFS of hiss: quiet, but a working mic.
    let room = dir.path().join("room.wav");
    speech_then_silence(&room, 0.0, 1.0);
    let recording = Recording::start_from(Some(room), None, |_| {}).expect("start");
    std::thread::sleep(Duration::from_millis(600));
    assert!(recording.sounded());
    assert!(!recording.spoke());

    let talk = dir.path().join("talk.wav");
    speech_then_silence(&talk, 0.6, 0.2);
    let recording = Recording::start_from(Some(talk), None, |_| {}).expect("start");
    std::thread::sleep(Duration::from_millis(700));
    assert!(recording.sounded() && recording.spoke());
}

#[test]
fn messages_name_the_mic_the_way_windows_sound_settings_do() {
    assert_eq!(short_name("Microphone Array (Realtek(R) Audio)"), "Microphone Array");
    assert_eq!(short_name("Headset (WH-1000XM4 Hands-Free AG Audio)"), "Headset");
    assert_eq!(short_name("Yeti Stereo Microphone"), "Yeti Stereo Microphone");
    assert_eq!(short_name("(Unnamed device)"), "(Unnamed device)");
}

#[test]
fn mics_are_named_like_windows_names_them_with_the_driver_only_to_tell_two_apart() {
    let found = |id: &str, name: &str, driver: &str, bluetooth: bool| {
        (id.to_string(), name.to_string(), Some(clean_driver(driver)), bluetooth)
    };
    let mics = name_mics(&[
        found("a", "Microphone", "Realtek(R) Audio", false),
        found("b", "Microphone", "Intel(R) Smart Sound Technology", false),
        found("c", "Headset", "WH-1000XM4 Hands-Free AG Audio", true),
    ]);
    let names: Vec<&str> = mics.iter().map(|mic| mic.name.as_str()).collect();
    assert_eq!(names, ["Microphone \u{b7} Realtek Audio", "Microphone \u{b7} Intel Smart Sound Technology", "Headset"]);
    assert_eq!(mics.iter().map(|mic| mic.bluetooth).collect::<Vec<_>>(), [false, false, true]);
}

#[test]
fn a_replayed_file_ignores_the_mic_pick_and_is_never_lost() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("talk.wav");
    speech_then_silence(&path, 0.6, 0.2);
    let recording = Recording::start_from(Some(path), Some("wasapi:gone".into()), |_| {}).expect("start");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(recording.device(), "talk.wav");
    assert!(!recording.lost());
}

/// Opens this machine's real inputs for a moment (nothing is kept or sent).
/// Run: cargo test -p likh mic_pick_live -- --ignored --nocapture
#[test]
#[ignore = "opens this machine's microphones"]
fn mic_pick_live() {
    let mics = inputs();
    println!("inputs: {:?}", mics.iter().map(|mic| &mic.name).collect::<Vec<_>>());
    let default = system_default().expect("a default input");
    for mic in &mics {
        let recording = Recording::start_from(None, Some(mic.id.clone()), |_| {}).expect("open the picked mic");
        // How long a real mic sends digital silence after opening: the silent-mic note waits 2 s.
        let opened = std::time::Instant::now();
        while !recording.sounded() && opened.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(10));
        }
        println!("picked {} -> opened {}, first sound after {:?}", mic.name, recording.device(), opened.elapsed());
        assert!(recording.sounded() && opened.elapsed() < Duration::from_secs(1), "{} stayed silent", mic.name);
        assert!(mic.name.starts_with(recording.device()), "{} opened {}", mic.name, recording.device());
    }
    let gone = Recording::start_from(None, Some("wasapi:{unplugged}".into()), |_| {}).expect("fall back");
    println!("unplugged pick -> opened {}", gone.device());
    assert!(default.name.starts_with(gone.device()));
}
