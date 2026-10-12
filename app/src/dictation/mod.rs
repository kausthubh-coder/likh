//! One dictation, from key-down to paste. `Dictation::handle` is the whole
//! decision table as a pure function: inputs (keys, a clock tick, the
//! transcript) and the user's choices go in, effects (record, transcribe,
//! paste, sounds, what the pill shows) come out. `run` is the thread that
//! carries the effects out.

use crate::commands::{self, Action};
use crate::microphone::Recording;
use crate::pill::{Pill, Show};
use crate::settings::{Mode, Settings};
use crate::sounds::Cue;
use crate::transcriber::{Failure, Transcriber};
use serde::Serialize;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(test)]
mod tests;

/// The Codex app stops recording here (595,000 ms); so do we (docs/codex.md).
pub const LONGEST: Duration = Duration::from_millis(595_000);
/// The pill starts counting down at 9:00.
pub const WARN_AFTER: Duration = Duration::from_secs(540);
/// A press shorter than this is a tap: hands-free in "Hold or tap", dropped in "Hold only".
pub const TAP: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// The shortcut went down (or `murmur ctl start`).
    Hold,
    /// The shortcut came up.
    Release,
    /// The pill's stop button, or `murmur ctl stop`: finish and paste, however it started.
    Stop,
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
    Paste {
        text: String,
        then_enter: bool,
    },
    /// "Scratch that": undo the last paste.
    Undo,
    Sound(Cue),
    Pill(Show),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    /// Recording while the shortcut is held.
    Recording,
    /// Recording with nothing held, until a tap or the stop button.
    HandsFree,
    Transcribing,
}

/// The parts of the settings a dictation decides with.
#[derive(Debug, Clone, Copy)]
pub struct Choices {
    pub mode: Mode,
    pub commands: bool,
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

    pub fn handle(&mut self, input: Input, now: Instant, choices: Choices) -> Vec<Effect> {
        let held = self.started.map_or(Duration::ZERO, |started| now.saturating_duration_since(started));
        let recording = matches!(self.phase, Phase::Recording | Phase::HandsFree);
        match (self.phase, input) {
            (Phase::Idle, Input::Hold) => {
                self.started = Some(now);
                self.warned_at = None;
                let (phase, show) = match choices.mode {
                    Mode::Tap => (Phase::HandsFree, Show::HandsFree),
                    Mode::Hold | Mode::HoldOrTap => (Phase::Recording, Show::Listening),
                };
                self.phase = phase;
                vec![Effect::StartRecording, Effect::Sound(Cue::Start), Effect::Pill(show)]
            }
            (Phase::Recording, Input::Release) if held < TAP => match choices.mode {
                Mode::HoldOrTap => {
                    self.phase = Phase::HandsFree;
                    vec![Effect::Pill(Show::HandsFree)]
                }
                Mode::Hold | Mode::Tap => self.stop(Effect::Discard, Show::Cancelled),
            },
            (Phase::Recording, Input::Release) => self.transcribe(),
            (Phase::HandsFree, Input::Hold) => self.transcribe(),
            (_, Input::Stop) if recording => self.transcribe(),
            (_, Input::Cancel) if recording => self.stop(Effect::Discard, Show::Cancelled),
            (_, Input::MicFailed(reason)) if recording => {
                let mut effects =
                    self.stop(Effect::Discard, Show::Error(format!("murmur can't hear a microphone. {reason}")));
                effects.push(Effect::Sound(Cue::Error));
                effects
            }
            (_, Input::Tick) if recording && held >= LONGEST => self.transcribe(),
            (_, Input::Tick) if recording && held >= WARN_AFTER => {
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
                    Ok(text) => {
                        let action = if choices.commands {
                            commands::interpret(&text)
                        } else {
                            Action::Paste { text, then_enter: false }
                        };
                        match action {
                            Action::Paste { text, then_enter } => {
                                vec![Effect::Pill(Show::Done), Effect::Paste { text, then_enter }]
                            }
                            Action::Undo => vec![Effect::Pill(Show::Done), Effect::Undo],
                        }
                    }
                    Err(Failure::TooShort) => vec![Effect::Pill(Show::Cancelled)],
                    Err(failure) => vec![Effect::Sound(Cue::Error), Effect::Pill(Show::Error(failure.message()))],
                }
            }
            // Holding again while the last one is still transcribing, a release
            // in hands-free, a late tick: nothing to do.
            _ => vec![],
        }
    }

    fn transcribe(&mut self) -> Vec<Effect> {
        self.phase = Phase::Transcribing;
        vec![Effect::Transcribe, Effect::Sound(Cue::Stop), Effect::Pill(Show::Thinking)]
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

/// Starts the dictation thread on `inputs`. `sender` is the same channel's
/// sending side (the pill and the shortcut have clones). Read `Status` any time.
pub fn spawn(
    sender: Sender<Input>,
    inputs: Receiver<Input>,
    pill: Pill,
    transcriber: Transcriber,
    settings: Arc<RwLock<Settings>>,
) -> Arc<Mutex<Status>> {
    let status =
        Arc::new(Mutex::new(Status { phase: Phase::Idle, transcriber: transcriber.name(), dictations: 0, last: None }));
    let thread_status = status.clone();
    let spawned = std::thread::Builder::new()
        .name("murmur-dictation".into())
        .spawn(move || run(inputs, sender, pill, Arc::new(transcriber), thread_status, settings));
    if let Err(error) = spawned {
        eprintln!("murmur: could not start dictation: {error}");
    }
    status
}

fn run(
    rx: Receiver<Input>,
    tx: Sender<Input>,
    pill: Pill,
    transcriber: Arc<Transcriber>,
    status: Arc<Mutex<Status>>,
    settings: Arc<RwLock<Settings>>,
) {
    let mut dictation = Dictation::default();
    let mut recording: Option<Recording> = None;
    let mut released: Option<(Instant, f32)> = None;
    loop {
        // Idle waits without waking; only a recording needs the clock.
        let listening = matches!(dictation.phase(), Phase::Recording | Phase::HandsFree);
        let input = if listening {
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
        let chosen = settings.read().map(|s| s.clone()).unwrap_or_else(|poisoned| poisoned.into_inner().clone());
        let choices = Choices { mode: chosen.mode, commands: chosen.commands };
        for effect in dictation.handle(input, Instant::now(), choices) {
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
                Effect::Sound(cue) if chosen.sounds => crate::sounds::play(cue),
                Effect::Sound(_) => {}
                Effect::Undo => {
                    if let Err(reason) = crate::paste::undo() {
                        pill.show(Show::Error(format!("murmur couldn't undo. {reason}")));
                    }
                }
                Effect::Paste { text, then_enter } => {
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
                    if restore.is_some() && chosen.history {
                        let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
                        let entry = crate::history::Entry { at, text: text.clone(), seconds };
                        if let Some(file) = crate::history::file()
                            && let Err(reason) = crate::history::append(&file, &entry)
                        {
                            eprintln!("murmur: couldn't save to history: {reason}");
                        }
                    }
                    {
                        let mut status = status.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                        status.dictations += 1;
                        status.last = Some(last);
                    }
                    if let Some(restore) = restore {
                        if then_enter && let Err(reason) = crate::paste::press_enter() {
                            pill.show(Show::Error(format!("murmur couldn't press Enter. {reason}")));
                        }
                        restore.finish();
                    }
                }
                Effect::Pill(show) => pill.show(show),
            }
        }
        // Esc belongs to murmur only while a recording can be cancelled.
        crate::hotkey::watch_escape(matches!(dictation.phase(), Phase::Recording | Phase::HandsFree));
        let mut status = status.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        status.phase = dictation.phase();
        if let Some(error) = failed {
            let seconds = released.take().map_or(0.0, |(_, seconds)| seconds);
            status.last = Some(Last { audio_seconds: seconds, latency_ms: 0, characters: 0, error: Some(error) });
        }
    }
}
