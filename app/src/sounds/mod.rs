//! Quiet cues that confirm what the pill already shows (docs/brand/BRAND.md,
//! Sound): two rising notes when recording starts, a tick when it stops, a
//! falling tone on an error. Synthesized, no audio files. The output device is
//! opened only for the length of a cue.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    Start,
    Stop,
    Error,
}

/// One tone: start and end pitch, when it starts, how long, how loud.
struct Tone {
    from_hz: f32,
    to_hz: f32,
    at: f32,
    seconds: f32,
    gain: f32,
}

fn tones(cue: Cue) -> &'static [Tone] {
    match cue {
        Cue::Start => &[
            Tone { from_hz: 660.0, to_hz: 660.0, at: 0.0, seconds: 0.045, gain: 0.06 },
            Tone { from_hz: 880.0, to_hz: 880.0, at: 0.045, seconds: 0.045, gain: 0.06 },
        ],
        Cue::Stop => &[Tone { from_hz: 1200.0, to_hz: 1200.0, at: 0.0, seconds: 0.016, gain: 0.05 }],
        Cue::Error => &[Tone { from_hz: 440.0, to_hz: 330.0, at: 0.0, seconds: 0.16, gain: 0.07 }],
    }
}

/// The cue as mono samples at `rate`: sines with a 4ms attack and an exponential fade.
pub fn samples(cue: Cue, rate: u32) -> Vec<f32> {
    let tones = tones(cue);
    let length = tones.iter().map(|t| t.at + t.seconds).fold(0.0, f32::max);
    let count = (length * rate as f32).round() as usize;
    let mut out = vec![0.0f32; count];
    for tone in tones {
        let mut phase = 0.0f32;
        let first = (tone.at * rate as f32) as usize;
        let n = (tone.seconds * rate as f32) as usize;
        for i in 0..n {
            let t = i as f32 / rate as f32;
            let progress = t / tone.seconds;
            // Exponential glide, like the brand board's Web Audio ramps.
            let hz = tone.from_hz * (tone.to_hz / tone.from_hz).powf(progress);
            phase += std::f32::consts::TAU * hz / rate as f32;
            let attack = (t / 0.004).min(1.0);
            let fade = (1.0 - progress).powi(3);
            if let Some(slot) = out.get_mut(first + i) {
                *slot += tone.gain * attack * fade * phase.sin();
            }
        }
    }
    out
}

/// Plays `cue` on the default output without waiting for it.
pub fn play(cue: Cue) {
    let spawned = std::thread::Builder::new().name("likh-sound".into()).spawn(move || {
        if let Err(reason) = play_now(cue) {
            crate::diagnostics::warn("sounds", format!("couldn't play a sound: {reason}"));
        }
    });
    if let Err(error) = spawned {
        crate::diagnostics::warn("sounds", format!("couldn't play a sound: {error}"));
    }
}

fn play_now(cue: Cue) -> Result<(), String> {
    let device = cpal::default_host().default_output_device().ok_or("no speakers or headphones")?;
    let supported = device.default_output_config().map_err(|error| error.to_string())?;
    let config = supported.config();
    let wave = Arc::new(samples(cue, config.sample_rate));
    let seconds = wave.len() as f32 / config.sample_rate as f32;
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, config, wave),
        SampleFormat::I16 => build::<i16>(&device, config, wave),
        SampleFormat::U16 => build::<u16>(&device, config, wave),
        other => return Err(format!("unsupported output format {other:?}")),
    }?;
    stream.play().map_err(|error| error.to_string())?;
    // Let the cue finish (plus the device's buffer), then close the device.
    std::thread::sleep(Duration::from_secs_f32(seconds) + Duration::from_millis(120));
    Ok(())
}

fn build<T>(device: &cpal::Device, config: cpal::StreamConfig, wave: Arc<Vec<f32>>) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels.max(1));
    let next = AtomicUsize::new(0);
    device
        .build_output_stream::<T, _, _>(
            config,
            move |out: &mut [T], _| {
                for frame in out.chunks_mut(channels) {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let value = T::from_sample(wave.get(i).copied().unwrap_or(0.0));
                    frame.fill(value);
                }
            },
            |error| crate::diagnostics::warn("sounds", format!("speaker error: {error}")),
            None,
        )
        .map_err(|error| error.to_string())
}
