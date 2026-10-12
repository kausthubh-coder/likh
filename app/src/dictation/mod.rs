//! One dictation, from key-down to paste. `Dictation::handle` is the whole
//! decision table as a pure function: inputs (keys, a clock tick, the
//! transcript) go in, effects (record, transcribe, paste, what the pill
//! shows) come out. `run` is the thread that carries the effects out.

use crate::microphone::Recording;
use crate::pill::{Pill, Show};
use crate::transcriber::{Failure, Transcriber};
use serde::Serialize;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;

/// The Codex app stops recording here (595,000 ms); so do we (docs/codex.md).
pub const LONGEST: Duration = Duration::from_millis(595_000);
/// The pill starts counting down at 9:00.
pub const WARN_AFTER: Duration = Duration::from_secs(540);
/// Shorter than this is a stray tap of the keys: drop it quietly instead of showing an error.
pub const TAP: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// The shortcut went down (or `murmur ctl start`).
    Hold,
    /// The shortcut came up (or `murmur ctl stop`).
    Release,
    /// Esc, or `murmur ctl cancel`.
    Cancel,
    /// The mic couldn't open.
    MicFailed(String),
    Transcribed(Result<String, Failure>),
    /// Sent every 250ms while recording, for the countdown and the cap.
    Tick,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    StartRecording,
    /// Stop the mic and send what it heard to the transcriber.
    Transcribe,
    /// Stop the mic and throw the audio away.
    Discard,
    Paste(String),
    Pill(Show),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Recording,
    Transcribing,
}

pub struct Dictation {
    phase: Phase,
    started: Option<Instant>,
    warned_at: Option<u64>,
}

impl Default for Dictation {
    fn default() -> Self {
        Self { phase: Phase::Idle, started: None, warned_at: None }
    }
}

impl Dictation {
    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn handle(&mut self, input: Input, now: Instant) -> Vec<Effect> {
        let held = self.started.map_or(Duration::ZERO, |started| now.saturating_duration_since(started));
        match (self.phase, input) {
            (Phase::Idle, Input::Hold) => {
                self.phase = Phase::Recording;
                self.started = Some(now);
                self.warned_at = None;
                vec![Effect::StartRecording, Effect::Pill(Show::Listening)]
            }
            (Phase::Recording, Input::Release) if held < TAP => self.stop(Effect::Discard, Show::Cancelled),
            (Phase::Recording, Input::Release) => self.transcribe(),
            (Phase::Recording, Input::Cancel) => self.stop(Effect::Discard, Show::Cancelled),
            (Phase::Recording, Input::MicFailed(reason)) => {
                self.stop(Effect::Discard, Show::Error(format!("murmur can't hear a microphone. {reason}")))
            }
            (Phase::Recording, Input::Tick) if held >= LONGEST => self.transcribe(),
            (Phase::Recording, Input::Tick) if held >= WARN_AFTER => {
                let seconds_left = (LONGEST - held).as_secs_f32().ceil() as u64;
                if self.warned_at == Some(seconds_left) {
                    return vec![];
                }
                self.warned_at = Some(seconds_left);
                vec![Effect::Pill(Show::Warning { seconds_left })]
            }
            (Phase::Transcribing, Input::Transcribed(result)) => {
                self.phase = Phase::Idle;
                self.started = None;
                match result {
                    Ok(text) if text.is_empty() => vec![Effect::Pill(Show::Cancelled)],
                    // The pill steps aside as the text lands; the text is the feedback.
                    Ok(text) => vec![Effect::Pill(Show::Done), Effect::Paste(text)],
                    Err(Failure::TooShort) => vec![Effect::Pill(Show::Cancelled)],
                    Err(failure) => vec![Effect::Pill(Show::Error(failure.message()))],
                }
            }
            // Holding again while the last one is still transcribing, a release
            // with nothing recording, a late tick: nothing to do.
            _ => vec![],
        }
    }

    fn transcribe(&mut self) -> Vec<Effect> {
        self.phase = Phase::Transcribing;
        vec![Effect::Transcribe, Effect::Pill(Show::Thinking)]
    }

    fn stop(&mut self, effect: Effect, show: Show) -> Vec<Effect> {
        self.phase = Phase::Idle;
        self.started = None;
        vec![effect, Effect::Pill(show)]
    }
}

/// What the last dictation did, for `murmur ctl status`. Never the text itself.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Last {
    pub audio_seconds: f32,
    /// Key-up to the Ctrl+V that pastes it.
    pub latency_ms: u128,
    pub characters: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub phase: Phase,
    pub transcriber: &'static str,
    pub dictations: u64,
    pub last: Option<Last>,
}

/// Starts the dictation thread. Send it inputs; read `Status` any time.
pub fn spawn(pill: Pill, transcriber: Transcriber) -> (Sender<Input>, Arc<Mutex<Status>>) {
    let (tx, rx) = mpsc::channel();
    let status =
        Arc::new(Mutex::new(Status { phase: Phase::Idle, transcriber: transcriber.name(), dictations: 0, last: None }));
    let (thread_tx, thread_status) = (tx.clone(), status.clone());
    let spawned = std::thread::Builder::new()
        .name("murmur-dictation".into())
        .spawn(move || run(rx, thread_tx, pill, Arc::new(transcriber), thread_status));
    if let Err(error) = spawned {
        eprintln!("murmur: could not start dictation: {error}");
    }
    (tx, status)
}

fn run(rx: Receiver<Input>, tx: Sender<Input>, pill: Pill, transcriber: Arc<Transcriber>, status: Arc<Mutex<Status>>) {
    let mut dictation = Dictation::default();
    let mut recording: Option<Recording> = None;
    let mut released: Option<(Instant, f32)> = None;
    loop {
        // Idle waits without waking; only a recording needs the clock.
        let input = if dictation.phase() == Phase::Recording {
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(input) => input,
                Err(RecvTimeoutError::Timeout) => Input::Tick,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(input) => input,
                Err(_) => return,
            }
        };
        let failed = match &input {
            Input::Transcribed(Err(failure)) => Some(failure.message()),
            Input::MicFailed(reason) => Some(reason.clone()),
            _ => None,
        };
        for effect in dictation.handle(input, Instant::now()) {
            match effect {
                Effect::StartRecording => {
                    let levels = pill.clone();
                    match Recording::start(move |level| levels.level(level)) {
                        Ok(started) => recording = Some(started),
                        Err(reason) => {
                            let _ = tx.send(Input::MicFailed(reason));
                        }
                    }
                }
                Effect::Transcribe => {
                    let samples = recording.take().map(Recording::finish).unwrap_or_default();
                    let seconds = samples.len() as f32 / crate::transcriber::SAMPLE_RATE as f32;
                    released = Some((Instant::now(), seconds));
                    let (tx, transcriber) = (tx.clone(), transcriber.clone());
                    std::thread::spawn(move || {
                        let _ = tx.send(Input::Transcribed(transcriber.transcribe(&samples)));
                    });
                }
                Effect::Discard => recording = None,
                Effect::Paste(text) => {
                    let pasted = crate::paste::paste(&text);
                    let (at, seconds) = released.take().unwrap_or((Instant::now(), 0.0));
                    let mut last = Last {
                        audio_seconds: seconds,
                        latency_ms: at.elapsed().as_millis(),
                        characters: text.chars().count(),
                        error: None,
                    };
                    let restore = match pasted {
                        Ok(restore) => Some(restore),
                        Err(reason) => {
                            let message = format!("murmur couldn't paste. {reason}");
                            pill.show(Show::Error(message.clone()));
                            last.error = Some(message);
                            None
                        }
                    };
                    {
                        let mut status = status.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                        status.dictations += 1;
                        status.last = Some(last);
                    }
                    if let Some(restore) = restore {
                        restore.finish();
                    }
                }
                Effect::Pill(show) => pill.show(show),
            }
        }
        // Esc belongs to murmur only while a recording can be cancelled.
        crate::hotkey::watch_escape(dictation.phase() == Phase::Recording);
        let mut status = status.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        status.phase = dictation.phase();
        if let Some(error) = failed {
            let seconds = released.take().map_or(0.0, |(_, seconds)| seconds);
            status.last = Some(Last { audio_seconds: seconds, latency_ms: 0, characters: 0, error: Some(error) });
        }
    }
}
