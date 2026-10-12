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
        Recording::start_from(Some(path), move |level| seen.lock().expect("levels").push(level)).expect("start");
    std::thread::sleep(Duration::from_millis(900));
    let samples = recording.finish();
    assert!((samples.len() as i32 - 9_600).abs() <= 2, "{} samples", samples.len());
    let levels = levels.lock().expect("levels");
    assert!(levels.len() >= 18, "{} levels", levels.len());
    assert!(levels.iter().all(|l| *l > 0.5), "a -17 dBFS tone should read loud: {levels:?}");
}

#[test]
fn a_missing_file_fails_to_start_and_says_which() {
    let error = Recording::start_from(Some(PathBuf::from("no/such.wav")), |_| {}).err().expect("should fail");
    assert!(error.starts_with(&format!("Can't read {}", PathBuf::from("no/such.wav").display())), "{error}");
}
