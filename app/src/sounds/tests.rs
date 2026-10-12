use super::*;

fn peak(wave: &[f32]) -> f32 {
    wave.iter().fold(0.0, |m, s| m.max(s.abs()))
}

#[test]
fn cues_are_as_long_as_the_brand_says() {
    // 90ms, 16ms and 160ms at 48 kHz.
    assert_eq!(samples(Cue::Start, 48_000).len(), 4_320);
    assert_eq!(samples(Cue::Stop, 48_000).len(), 768);
    assert_eq!(samples(Cue::Error, 48_000).len(), 7_680);
}

#[test]
fn cues_stay_quiet() {
    // Gain 0.04 to 0.08 (BRAND.md, Sound): never louder than that.
    for cue in [Cue::Start, Cue::Stop, Cue::Error] {
        let loudest = peak(&samples(cue, 44_100));
        assert!(loudest > 0.02 && loudest <= 0.08, "{cue:?} peaks at {loudest}");
    }
}

#[test]
fn the_start_cue_rises_from_660_to_880() {
    // Count zero crossings in each half: 45ms of 660 Hz is ~59, of 880 Hz is ~79.
    let wave = samples(Cue::Start, 48_000);
    let (low, high) = wave.split_at(wave.len() / 2);
    let crossings = |s: &[f32]| s.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
    assert!(crossings(high) > crossings(low) + 10, "{} vs {}", crossings(low), crossings(high));
}

#[test]
fn cues_start_and_end_without_a_click() {
    for cue in [Cue::Start, Cue::Stop, Cue::Error] {
        let wave = samples(cue, 48_000);
        assert!(wave[0].abs() < 0.01 && wave[wave.len() - 1].abs() < 0.01, "{cue:?}");
    }
}
