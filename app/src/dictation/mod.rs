//! One dictation, from key-down to paste. `Dictation::handle` is the whole
//! decision table as a pure function: inputs (keys, a clock tick, the
//! transcript) and the user's choices go in, effects (record, transcribe,
//! paste, sounds, what the pill shows) come out. `run` is the thread that
//! carries the effects out.

use crate::commands::{self, Action, Groups, Press};
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
    /// The shortcut went down (or `likh ctl start`).
    Hold,
    /// The shortcut came up.
    Release,
    /// The pill's stop button, or `likh ctl stop`: finish and paste, however it started.
    Stop,
    /// Esc, or `likh ctl cancel`.
    Cancel,
    /// The mic couldn't open.
    MicFailed(String),
    /// The transcriber can't listen (Windows speech switched off, say).
    #[cfg_attr(not(target_os = "windows"), allow(dead_code, reason = "only Windows speech listens live"))]
    CantListen(Failure),
    Transcribed(Result<String, Failure>),
    /// Sent every 250ms while recording, for the countdown and the cap.
    Tick,
    /// Sent with each tick once someone has spoken: how long it has been quiet since.
    Quiet(Duration),
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
        then: Option<Press>,
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
    /// The spoken commands that are on, or None when they're all off.
    pub commands: Option<Groups>,
    /// Hands-free stops after this much quiet.
    pub silence_stop: Option<Duration>,
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
                    self.stop(Effect::Discard, Show::Error(format!("likh can't hear a microphone. {reason}")));
                effects.push(Effect::Sound(Cue::Error));
                effects
            }
            (_, Input::CantListen(failure)) if recording => {
                let mut effects = self.stop(Effect::Discard, Show::Error(failure.message()));
                effects.push(Effect::Sound(Cue::Error));
                effects
            }
            (_, Input::Tick) if recording && held >= LONGEST => self.transcribe(),
            // Holding the keys means you decide when it ends; only hands-free stops by itself.
            (Phase::HandsFree, Input::Quiet(quiet)) if choices.silence_stop.is_some_and(|after| quiet >= after) => {
                self.transcribe()
            }
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
                        let action = match choices.commands {
                            Some(groups) => commands::interpret(&text, &groups),
                            None => Action::Paste { text, then: None },
                        };
                        match action {
                            // "Scratch that" took back everything that was said.
                            Action::Paste { text, then: None } if text.is_empty() => {
                                vec![Effect::Pill(Show::Cancelled)]
                            }
                            Action::Paste { text, then } => {
                                vec![Effect::Pill(Show::Done), Effect::Paste { text, then }]
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

/// What the last dictation did, for `likh ctl status`. Never the text itself.
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
/// sending side (the pill and the shortcut have clones). `fake` is
/// `LIKH_TRANSCRIBER`'s text, which replaces every engine. Read `Status` any time.
pub fn spawn(
    sender: Sender<Input>,
    inputs: Receiver<Input>,
    pill: Pill,
    fake: Option<String>,
    settings: Arc<RwLock<Settings>>,
) -> Arc<Mutex<Status>> {
    let engine = settings.read().map(|s| s.engine).unwrap_or_default();
    let transcriber = Transcriber::new(engine, fake.as_deref()).name();
    let status = Arc::new(Mutex::new(Status { phase: Phase::Idle, transcriber, dictations: 0, last: None }));
    let thread_status = status.clone();
    let spawned = std::thread::Builder::new()
        .name("likh-dictation".into())
        .spawn(move || run(inputs, sender, pill, fake, thread_status, settings));
    if let Err(error) = spawned {
        eprintln!("likh: could not start dictation: {error}");
    }
    status
}

/// The parts of one dictation that live outside the decision table.
#[derive(Default)]
struct Running {
    recording: Option<Recording>,
    transcriber: Option<Arc<Transcriber>>,
    /// Windows speech, listening alongside the recording.
    #[cfg(target_os = "windows")]
    listening: Option<crate::transcriber::windows_speech::Listening>,
    /// Media this dictation is pausing (pausing takes a moment, so it runs on its own thread).
    paused: Option<std::thread::JoinHandle<crate::media::Paused>>,
}

impl Running {
    /// Plays again whatever this dictation paused, without waiting for it.
    fn resume_media(&mut self) {
        if let Some(pausing) = self.paused.take() {
            std::thread::spawn(move || {
                if let Ok(paused) = pausing.join() {
                    paused.resume();
                }
            });
        }
    }
}

fn run(
    rx: Receiver<Input>,
    tx: Sender<Input>,
    pill: Pill,
    fake: Option<String>,
    status: Arc<Mutex<Status>>,
    settings: Arc<RwLock<Settings>>,
) {
    let mut dictation = Dictation::default();
    let mut running = Running::default();
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
            Input::CantListen(failure) => Some(failure.message()),
            _ => None,
        };
        // Each tick also reports the quiet since the last speech, for silence stop.
        let quiet = match input {
            Input::Tick => running.recording.as_ref().and_then(Recording::quiet_for),
            _ => None,
        };
        let chosen = settings.read().map(|s| s.clone()).unwrap_or_else(|poisoned| poisoned.into_inner().clone());
        let choices = Choices {
            mode: chosen.mode,
            commands: chosen.commands.then_some(chosen.command_groups),
            silence_stop: chosen.silence_stop.after(),
        };
        let now = Instant::now();
        let mut effects = dictation.handle(input, now, choices);
        if let Some(quiet) = quiet {
            effects.extend(dictation.handle(Input::Quiet(quiet), now, choices));
        }
        for effect in effects {
            match effect {
                Effect::StartRecording => {
                    // The engine is read per dictation, so a change in settings applies to the next one.
                    let transcriber = Arc::new(Transcriber::new(chosen.engine, fake.as_deref()));
                    status.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).transcriber = transcriber.name();
                    #[cfg(target_os = "windows")]
                    if transcriber.listens_live() {
                        match crate::transcriber::windows_speech::start() {
                            Ok(listening) => running.listening = Some(listening),
                            Err(failure) => {
                                let _ = tx.send(Input::CantListen(failure));
                            }
                        }
                    }
                    if chosen.pause_media && crate::media::SUPPORTED {
                        running.paused = Some(std::thread::spawn(crate::media::pause_playing));
                    }
                    let levels = pill.clone();
                    match Recording::start(move |level| levels.level(level)) {
                        Ok(started) => running.recording = Some(started),
                        Err(reason) => {
                            let _ = tx.send(Input::MicFailed(reason));
                        }
                    }
                    running.transcriber = Some(transcriber);
                }
                Effect::Transcribe => {
                    running.resume_media();
                    let samples = running.recording.take().map(Recording::finish).unwrap_or_default();
                    let seconds = samples.len() as f32 / crate::transcriber::SAMPLE_RATE as f32;
                    released = Some((Instant::now(), seconds));
                    let transcriber = running
                        .transcriber
                        .take()
                        .unwrap_or_else(|| Arc::new(Transcriber::new(chosen.engine, fake.as_deref())));
                    #[cfg(target_os = "windows")]
                    let live = running.listening.take();
                    let tx = tx.clone();
                    std::thread::spawn(move || {
                        #[cfg(target_os = "windows")]
                        if let Some(live) = live {
                            let heard = live.finish();
                            let too_short = seconds < crate::transcriber::SHORTEST.as_secs_f32();
                            let _ = tx.send(Input::Transcribed(if too_short { Err(Failure::TooShort) } else { heard }));
                            return;
                        }
                        let _ = tx.send(Input::Transcribed(transcriber.transcribe(&samples)));
                    });
                }
                Effect::Discard => {
                    running.resume_media();
                    running.recording = None;
                    running.transcriber = None;
                    #[cfg(target_os = "windows")]
                    if let Some(live) = running.listening.take() {
                        std::thread::spawn(move || drop(live.finish()));
                    }
                }
                Effect::Sound(cue) if chosen.sounds => crate::sounds::play(cue),
                Effect::Sound(_) => {}
                Effect::Undo => {
                    if let Err(reason) = crate::paste::undo() {
                        pill.show(Show::Error(format!("likh couldn't undo. {reason}")));
                    }
                }
                Effect::Paste { text, then } => {
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
                            let message = format!("likh couldn't paste. {reason}");
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
                            eprintln!("likh: couldn't save to history: {reason}");
                        }
                    }
                    {
                        let mut status = status.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                        status.dictations += 1;
                        status.last = Some(last);
                    }
                    if let Some(restore) = restore {
                        if let Some(key) = then
                            && let Err(reason) = crate::paste::press(key)
                        {
                            pill.show(Show::Error(format!("likh couldn't press {}. {reason}", key.name())));
                        }
                        restore.finish();
                    }
                }
                Effect::Pill(show) => pill.show(show),
            }
        }
        // Esc belongs to likh only while a recording can be cancelled.
        crate::hotkey::watch_escape(matches!(dictation.phase(), Phase::Recording | Phase::HandsFree));
        let mut status = status.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        status.phase = dictation.phase();
        if let Some(error) = failed {
            let seconds = released.take().map_or(0.0, |(_, seconds)| seconds);
            status.last = Some(Last { audio_seconds: seconds, latency_ms: 0, characters: 0, error: Some(error) });
        }
    }
}
