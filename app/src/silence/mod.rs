//! Silence stop: is someone talking? The recorder feeds every buffer of mono
//! audio to a `Listener`, which says how long ago it last heard a voice, so
//! hands-free can stop once the person has finished.
//!
//! It listens for a voice, not for loudness: earshot, a small voice detector
//! (pure Rust, about 95 KB, 8 KB of state while recording). A loudness line
//! missed quiet mics entirely and never stopped beside a loud fan; one that
//! learned the room's noise heard typing as talking. `scripts/silence-eval.ps1`
//! scores it on recordings in quiet rooms and noisy ones (2026-10-13).

use std::time::Duration;

#[cfg(test)]
mod eval;
#[cfg(test)]
mod tests;

/// How much voice counts as someone speaking: one short word ("yo") is about
/// 250 ms, a click or a cough well under this.
pub const ARM_AFTER: Duration = Duration::from_millis(120);

/// earshot's frame, 16 ms at 16 kHz, and its suggested threshold.
const RATE: u32 = 16_000;
const FRAME: usize = 256;
const FRAME_LENGTH: Duration = Duration::from_millis(16);
const VOICE_SCORE: f32 = 0.5;

/// Hears one recording. Feed it every buffer, in order.
pub struct Listener {
    down: Downsample,
    frame: Vec<f32>,
    detector: Box<earshot::Detector>,
    rate: u32,
    /// Samples heard so far, at `rate`.
    heard: u64,
    /// How much voice so far; nothing counts until `ARM_AFTER`.
    voiced: Duration,
    /// Where the last voiced frame ended, in samples.
    last_voice: Option<u64>,
}

impl Listener {
    /// `rate` is the sample rate of the mono buffers `push` will get.
    pub fn new(rate: u32) -> Self {
        let rate = rate.max(1);
        Self {
            down: Downsample::new(rate, RATE),
            frame: Vec::with_capacity(FRAME),
            // Boxed straight onto the heap: the audio thread's stack is small.
            detector: earshot::Detector::default_boxed(),
            rate,
            heard: 0,
            voiced: Duration::ZERO,
            last_voice: None,
        }
    }

    /// Takes the next buffer. Returns how long before the end of it a voice was
    /// last heard, once someone has spoken; None before that.
    pub fn push(&mut self, mono: &[f32]) -> Option<Duration> {
        let start = self.heard;
        for (i, sample) in mono.iter().enumerate() {
            let frame = &mut self.frame;
            self.down.push(*sample, |low| frame.push(low.clamp(-1.0, 1.0)));
            if self.frame.len() < FRAME {
                continue;
            }
            let score = self.detector.predict_f32(&self.frame);
            self.frame.clear();
            if score >= VOICE_SCORE {
                self.voiced += FRAME_LENGTH;
                self.last_voice = Some(start + i as u64 + 1);
            }
        }
        self.heard += mono.len() as u64;
        if self.voiced < ARM_AFTER {
            return None;
        }
        let ago = self.heard - self.last_voice?;
        Some(Duration::from_secs_f64(ago as f64 / f64::from(self.rate)))
    }
}

/// Turns audio at one rate into 16 kHz as it streams: each output sample is
/// the mean of the input samples it covers (enough for a voice detector).
struct Downsample {
    /// Output samples per input sample.
    step: f64,
    position: f64,
    sum: f32,
    count: u32,
}

impl Downsample {
    fn new(from: u32, to: u32) -> Self {
        Self { step: f64::from(to) / f64::from(from.max(1)), position: 0.0, sum: 0.0, count: 0 }
    }

    fn push(&mut self, sample: f32, mut out: impl FnMut(f32)) {
        self.sum += sample;
        self.count += 1;
        self.position += self.step;
        if self.position < 1.0 {
            return;
        }
        let mean = self.sum / self.count as f32;
        // Going up (an 8 kHz mic), one input sample covers several outputs.
        while self.position >= 1.0 {
            self.position -= 1.0;
            out(mean);
        }
        self.sum = 0.0;
        self.count = 0;
    }
}
