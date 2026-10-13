use super::eval::{MIC_RATE, Noise, at_level, read_clip};
use super::*;
use std::path::Path;

/// "Yo.", spoken by Windows' voice: one short word, the hardest case to notice.
fn yo() -> Vec<f32> {
    read_clip(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/silence/fixtures/yo.wav"))
}

/// Plays `audio` (at 16 kHz) to a listener at the mic's rate in 10 ms buffers,
/// as cpal would, and returns what the last buffer said.
fn listen(audio: &[f32]) -> Option<Duration> {
    let audio = crate::microphone::resample(audio, 16_000, MIC_RATE);
    let mut listener = Listener::new(MIC_RATE);
    let mut last = None;
    for piece in audio.chunks((MIC_RATE / 100) as usize) {
        last = listener.push(piece);
    }
    last
}

/// A quiet room, then the word at `db`, then `after` seconds of the same room.
fn word_in_room(db: f32, after: f32) -> Vec<f32> {
    let mut noise = Noise::new(5);
    let mut audio = noise.white(8_000, -68.0);
    let word = at_level(&yo(), db);
    audio.extend(word.iter().zip(noise.white(word.len(), -68.0)).map(|(s, n)| s + n));
    audio.extend(noise.white((after * 16_000.0) as usize, -68.0));
    audio
}

#[test]
fn one_word_counts_as_speech() {
    let quiet = listen(&word_in_room(-26.0, 2.0)).expect("heard the word");
    // The clip's own trailing quiet (~0.5 s) plus the two seconds after it.
    assert!(quiet > Duration::from_millis(2_000) && quiet < Duration::from_millis(3_000), "{quiet:?}");
}

#[test]
fn one_word_on_a_quiet_mic_still_counts() {
    // 20 dB under talking at a laptop: the old -37 dBFS line never heard this.
    assert!(listen(&word_in_room(-46.0, 2.0)).is_some());
}

#[test]
fn a_quiet_room_alone_is_never_a_voice() {
    assert_eq!(listen(&Noise::new(9).white(16_000 * 3, -60.0)), None);
}

#[test]
fn a_loud_fan_is_not_a_voice() {
    assert_eq!(listen(&Noise::new(13).fan(16_000 * 3, -34.0)), None);
}

#[test]
fn quiet_counts_from_the_last_voice_not_the_start() {
    let mut audio = word_in_room(-26.0, 0.5);
    let first = listen(&audio).expect("heard");
    audio.extend(word_in_room(-26.0, 0.5));
    let second = listen(&audio).expect("heard");
    // The second word resets the clock: same quiet after it as after the first.
    assert!(first.abs_diff(second) < Duration::from_millis(100), "{first:?} vs {second:?}");
}

#[test]
fn downsampling_keeps_the_count_and_the_level() {
    let mut out = Vec::new();
    let mut down = Downsample::new(48_000, 16_000);
    for _ in 0..4_800 {
        down.push(0.5, |s| out.push(s));
    }
    assert_eq!(out.len(), 1_600);
    assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-6));

    let mut up = Vec::new();
    let mut from_8k = Downsample::new(8_000, 16_000);
    for _ in 0..800 {
        from_8k.push(0.25, |s| up.push(s));
    }
    assert_eq!(up.len(), 1_600);

    let mut odd = Vec::new();
    let mut from_44k = Downsample::new(44_100, 16_000);
    for _ in 0..44_100 {
        from_44k.push(0.1, |s| odd.push(s));
    }
    assert!((15_999..=16_000).contains(&odd.len()), "{}", odd.len());
}
