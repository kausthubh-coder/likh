//! Scores silence stop on recordings: does it notice someone spoke, does it
//! cut them off mid-thought, and does it stop when they've finished, in quiet
//! rooms and noisy ones. Real voices (Windows TTS clips from
//! `scripts/silence-eval.ps1`) mixed with synthesized rooms. On 2026-10-13 a
//! loudness line passed 8 of 11 and a room-learning loudness line 10 of 11;
//! the voice detector passed all 11.
//!
//! Run: `powershell -File scripts/silence-eval.ps1`. Writes
//! `.agents/qa/silence/report.md`, and fails if a scene does.

use super::Listener;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The setting under test: hands-free stops after this much quiet.
const AFTER: f32 = 3.0;
/// The app checks quiet on a 250 ms tick (`dictation::run`); so does this.
const TICK: f32 = 0.25;
/// The mic's rate on most Windows laptops; the clips are upsampled to it.
pub(super) const MIC_RATE: u32 = 48_000;
const CLIP_RATE: u32 = 16_000;

/// 16 kHz mono clip, as `tts-wav.ps1` writes it.
pub(super) fn read_clip(path: &Path) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    reader.samples::<i16>().filter_map(Result::ok).map(|s| f32::from(s) / 32768.0).collect()
}

/// Where the voice in a clean clip starts and ends, in samples.
pub(super) fn voiced_span(clip: &[f32]) -> (usize, usize) {
    let peak = clip.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let loud = |s: &&f32| s.abs() > peak * 0.02;
    let start = clip.iter().position(|s| loud(&s)).unwrap_or(0);
    let end = clip.len() - clip.iter().rev().position(|s| loud(&s)).unwrap_or(0);
    (start, end)
}

/// Scales a clip so its voiced part has this RMS level, in dBFS.
pub(super) fn at_level(clip: &[f32], db: f32) -> Vec<f32> {
    let (start, end) = voiced_span(clip);
    let voiced = &clip[start..end.max(start + 1)];
    let rms = (voiced.iter().map(|s| s * s).sum::<f32>() / voiced.len() as f32).sqrt().max(1e-9);
    let gain = 10f32.powf(db / 20.0) / rms;
    clip.iter().map(|s| s * gain).collect()
}

/// A repeatable noise source (xorshift), so every run hears the same room.
pub(super) struct Noise(u32);

impl Noise {
    pub(super) fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }

    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }

    /// Hiss with this RMS level.
    pub(super) fn white(&mut self, samples: usize, db: f32) -> Vec<f32> {
        // Uniform noise in -1..1 has an RMS of 1/sqrt(3).
        let gain = 10f32.powf(db / 20.0) * 3f32.sqrt();
        (0..samples).map(|_| self.next() * gain).collect()
    }

    /// A fan or an air conditioner with this RMS level: broadband noise tilted
    /// towards the low end (a one-pole low-pass at about 250 Hz over hiss).
    pub(super) fn fan(&mut self, samples: usize, db: f32) -> Vec<f32> {
        let mut level = 0.0f32;
        let raw: Vec<f32> = (0..samples)
            .map(|_| {
                let hiss = self.next();
                level = level * 0.9 + hiss * 0.1;
                level + hiss * 0.05
            })
            .collect();
        normalize(raw, db)
    }
}

fn normalize(raw: Vec<f32>, db: f32) -> Vec<f32> {
    let rms = (raw.iter().map(|s| s * s).sum::<f32>() / raw.len().max(1) as f32).sqrt().max(1e-9);
    let gain = 10f32.powf(db / 20.0) / rms;
    raw.into_iter().map(|s| s * gain).collect()
}

/// Music under the voice: chords that change every half second, with a pulse.
fn music(samples: usize, rate: u32, db: f32) -> Vec<f32> {
    let chords = [[220.0, 277.2, 329.6], [196.0, 246.9, 293.7], [174.6, 220.0, 261.6], [196.0, 246.9, 329.6]];
    let raw = (0..samples)
        .map(|i| {
            let t = i as f32 / rate as f32;
            let chord = chords[(t / 0.5) as usize % chords.len()];
            let pulse = 0.6 + 0.4 * (t * std::f32::consts::TAU * 2.0).sin().abs();
            pulse * chord.iter().map(|f| (t * f * std::f32::consts::TAU).sin()).sum::<f32>()
        })
        .collect();
    normalize(raw, db)
}

/// Typing: a short click every 120 to 200 ms.
fn typing(samples: usize, rate: u32, db: f32, noise: &mut Noise) -> Vec<f32> {
    let mut out = vec![0.0; samples];
    let click = (rate as f32 * 0.006) as usize;
    let mut at = 0usize;
    while at + click < samples {
        for (k, s) in noise.white(click, db).into_iter().enumerate() {
            out[at + k] = s * (1.0 - k as f32 / click as f32);
        }
        at += (rate as f32 * (0.12 + 0.08 * (noise.next() * 0.5 + 0.5))) as usize;
    }
    out
}

/// One recording to score: the audio, and where the person really spoke.
struct Scene {
    name: &'static str,
    audio: Vec<f32>,
    /// Voiced spans, in seconds.
    speech: Vec<(f32, f32)>,
    /// What a good method does here.
    expect: Expect,
}

#[derive(Clone, Copy, PartialEq)]
enum Expect {
    /// Notice the speech, never stop during it, stop `AFTER` once it ends.
    StopAfterSpeech,
    /// Another voice keeps talking: stopping is a judgement call, reported only.
    Either,
}

/// Lays clips out on a timeline at 16 kHz, with gaps of silence between them.
struct Timeline {
    audio: Vec<f32>,
    speech: Vec<(f32, f32)>,
}

impl Timeline {
    fn new(lead: f32) -> Self {
        Self { audio: vec![0.0; (lead * CLIP_RATE as f32) as usize], speech: Vec::new() }
    }

    fn say(mut self, clip: &[f32], gap_after: f32) -> Self {
        let (start, end) = voiced_span(clip);
        let at = self.audio.len();
        let seconds = |n: usize| n as f32 / CLIP_RATE as f32;
        self.speech.push((seconds(at + start), seconds(at + end)));
        // The clip's own trailing quiet is part of the gap.
        self.audio.extend_from_slice(&clip[..end]);
        self.audio.extend(std::iter::repeat_n(0.0, (gap_after * CLIP_RATE as f32) as usize));
        self
    }

    /// Adds a sound under everything (a room, a fan), from `from` seconds on.
    fn under(mut self, bed: &[f32], from: f32) -> Self {
        let skip = (from * CLIP_RATE as f32) as usize;
        for (s, b) in self.audio.iter_mut().skip(skip).zip(bed) {
            *s += b;
        }
        self
    }

    /// A real mic always hears its room: about -68 dBFS of hiss under everything.
    fn scene(self, name: &'static str, expect: Expect) -> Scene {
        let room = Noise::new(name.len() as u32 * 31 + 7).white(self.audio.len(), -68.0);
        self.under(&room, 0.0).scene_as_is(name, expect)
    }

    /// No room at all: a mic behind noise suppression hands over pure zeros.
    fn scene_as_is(self, name: &'static str, expect: Expect) -> Scene {
        let audio = crate::microphone::resample(&self.audio, CLIP_RATE, MIC_RATE);
        Scene { name, audio, speech: self.speech, expect }
    }
}

struct Clips {
    word: Vec<f32>,
    s1: Vec<f32>,
    s2: Vec<f32>,
    s3: Vec<f32>,
    babble: Vec<f32>,
}

fn clips_folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../.agents/qa/silence")
}

fn scenes() -> Vec<Scene> {
    let dir = clips_folder().join("clips");
    let load = |name: &str| read_clip(&dir.join(name));
    let c = Clips {
        word: load("word.wav"),
        s1: load("s1.wav"),
        s2: load("s2.wav"),
        s3: load("s3.wav"),
        babble: load("babble.wav"),
    };
    // Talking at a laptop is about -26 dBFS; a quiet mic like the one measured
    // on 2026-10-13 (its room peaked at -54 dBFS) is about 20 dB lower.
    let (normal, quiet) = (-26.0, -46.0);
    let tail = AFTER + 3.0;
    let talk = |lead: f32, db: f32, gaps: [f32; 2]| {
        Timeline::new(lead)
            .say(&at_level(&c.s1, db), gaps[0])
            .say(&at_level(&c.s2, db), gaps[1])
            .say(&at_level(&c.s3, db), tail)
    };
    let usual = || talk(0.4, normal, [1.0, 1.5]);
    let len = |t: &Timeline| t.audio.len();
    let mut fans = Noise::new(11);
    let mut scenes = Vec::new();

    let t = Timeline::new(0.4).say(&at_level(&c.word, normal), tail);
    scenes.push(t.scene("one word (\"yo\")", Expect::StopAfterSpeech));

    let t = Timeline::new(0.4).say(&at_level(&c.word, quiet), tail);
    scenes.push(t.scene("one word, quiet mic", Expect::StopAfterSpeech));

    scenes.push(talk(0.0, normal, [1.0, 1.5]).scene("starts talking at once", Expect::StopAfterSpeech));
    scenes.push(talk(0.4, normal, [2.5, 2.5]).scene("thinking pauses (2.5 s)", Expect::StopAfterSpeech));
    scenes.push(talk(0.4, quiet, [1.0, 1.5]).scene("quiet mic", Expect::StopAfterSpeech));
    scenes.push(usual().scene_as_is("noise-suppressed mic (pure silence)", Expect::StopAfterSpeech));

    let t = usual();
    let bed = fans.fan(len(&t), -46.0);
    scenes.push(t.under(&bed, 0.0).scene("fan", Expect::StopAfterSpeech));

    let t = usual();
    let bed = fans.fan(len(&t), -36.0);
    scenes.push(t.under(&bed, 0.0).scene("loud fan", Expect::StopAfterSpeech));

    let t = usual();
    let bed = fans.fan(len(&t), -40.0);
    scenes.push(t.under(&bed, 6.0).scene("fan switches on mid-way", Expect::StopAfterSpeech));

    let t = usual();
    let bed = music(len(&t), CLIP_RATE, -40.0);
    scenes.push(t.under(&bed, 0.0).scene("music playing", Expect::StopAfterSpeech));

    let t = usual();
    let tail_from = t.speech.last().map_or(0.0, |s| s.1) + 0.3;
    let clicks = typing(len(&t), CLIP_RATE, -30.0, &mut Noise::new(3));
    scenes.push(t.under(&clicks, tail_from).scene("typing once you've finished", Expect::StopAfterSpeech));

    let t = usual();
    let mut other = at_level(&c.babble, normal - 14.0);
    other.resize(len(&t), 0.0);
    scenes.push(t.under(&other, 0.0).scene("someone talking nearby", Expect::Either));
    scenes
}

/// What one method did with one scene.
struct Outcome {
    armed_at: Option<f32>,
    /// Stopped while the person was still mid-thought, at this time.
    cut_off_at: Option<f32>,
    /// Seconds from the end of the last word to the stop.
    stopped_after: Option<f32>,
    /// The longest quiet it measured while the person was still talking, and
    /// the longest real gap between their words.
    longest_quiet_mid_talk: f32,
    longest_real_gap: f32,
    cost: Duration,
}

impl Outcome {
    fn passed(&self) -> bool {
        self.armed_at.is_some()
            && self.cut_off_at.is_none()
            && self.stopped_after.is_some_and(|s| (AFTER - 0.1..=AFTER + 0.6).contains(&s))
    }
}

fn play(scene: &Scene) -> Outcome {
    let mut listener = Listener::new(MIC_RATE);
    // cpal hands over about 10 ms at a time.
    let chunk = (MIC_RATE / 100) as usize;
    let spoken_until = scene.speech.last().map_or(0.0, |s| s.1);
    let mut cost = Duration::ZERO;
    let mut last_voice: Option<f32> = None;
    let mut armed_at = None;
    let mut next_tick = TICK;
    let mut longest_quiet_mid_talk = 0.0f32;
    let mut outcome_stop = None;
    for (i, piece) in scene.audio.chunks(chunk).enumerate() {
        let started = Instant::now();
        let heard = listener.push(piece);
        cost += started.elapsed();
        let now = ((i * chunk + piece.len()) as f32) / MIC_RATE as f32;
        if let Some(ago) = heard {
            last_voice = Some(now - ago.as_secs_f32());
            armed_at.get_or_insert(now);
        }
        if now < next_tick {
            continue;
        }
        next_tick += TICK;
        let Some(quiet) = last_voice.map(|at| now - at) else { continue };
        if now < spoken_until {
            longest_quiet_mid_talk = longest_quiet_mid_talk.max(quiet);
        }
        if quiet >= AFTER {
            outcome_stop = Some(now);
            break;
        }
    }
    let longest_real_gap = scene.speech.windows(2).map(|w| w[1].0 - w[0].1).fold(0.0, f32::max);
    let (cut_off_at, stopped_after) = match outcome_stop {
        Some(at) if at < spoken_until => (Some(at), None),
        Some(at) => (None, Some(at - spoken_until)),
        None => (None, None),
    };
    Outcome { armed_at, cut_off_at, stopped_after, longest_quiet_mid_talk, longest_real_gap, cost }
}

#[test]
#[ignore = "needs the clips from scripts/silence-eval.ps1"]
fn silence_eval() {
    let scenes = scenes();
    let mut report = format!(
        "# Silence stop\n\nSetting: stop after {AFTER} s of quiet. Ticks every {TICK} s, like the app. \
         Audio at {MIC_RATE} Hz in 10 ms buffers, like the mic. A pass notices the speech, never stops mid-thought, \
         and stops {AFTER} to {} s after the last word.\n\n| Scene | Result |\n|---|---|\n",
        AFTER + 0.6
    );
    let (mut passed, mut failed, mut cost) = (0, Vec::new(), Duration::ZERO);
    let audio_seconds: f32 = scenes.iter().map(|s| s.audio.len() as f32 / MIC_RATE as f32).sum();
    for scene in &scenes {
        let o = play(scene);
        cost += o.cost;
        let verdict = match (scene.expect, o.passed()) {
            (Expect::Either, _) => "·",
            (_, true) => {
                passed += 1;
                "✅"
            }
            (_, false) => {
                failed.push(scene.name);
                "❌"
            }
        };
        let what = match (o.armed_at, o.cut_off_at, o.stopped_after) {
            (None, _, _) => "never noticed the speech, never stops".to_string(),
            (_, Some(at), _) => format!("cut off mid-thought at {at:.1} s"),
            (_, _, Some(after)) => format!("stopped {after:.1} s after the last word"),
            (_, _, None) => "never stopped".to_string(),
        };
        let mid = if o.longest_real_gap > 0.0 {
            format!("; longest quiet mid-talk {:.1} s (real gap {:.1} s)", o.longest_quiet_mid_talk, o.longest_real_gap)
        } else {
            String::new()
        };
        report.push_str(&format!("| {} | {verdict} {what}{mid} |\n", scene.name));
    }
    let scored = scenes.iter().filter(|s| s.expect == Expect::StopAfterSpeech).count();
    let per_minute = cost.as_secs_f64() * 1000.0 * 60.0 / f64::from(audio_seconds);
    report.push_str(&format!(
        "| **Passed** | **{passed} / {scored}** |\n| **CPU per minute of audio** | {per_minute:.2} ms |\n\n\
         CPU is this machine's time inside `Listener::push` over {audio_seconds:.0} s of audio{}.\n",
        if cfg!(debug_assertions) {
            " (debug build: run with `--release` for real numbers)"
        } else {
            ", release build"
        }
    ));
    println!("{report}");
    let out = clips_folder().join("report.md");
    std::fs::write(&out, &report).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
    println!("wrote {}", out.display());
    assert!(failed.is_empty(), "failed: {failed:?}");
}
