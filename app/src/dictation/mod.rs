//! One dictation, from key-down to paste. `Dictation::handle` is the whole
//! decision table as a pure function: inputs (keys, a clock tick, the
//! transcript) and the user's choices go in, effects (record, transcribe,
//! paste, sounds, what the pill shows) come out. `run` is the thread that
//! carries the effects out.

use crate::commands::{self, Action, Groups, Press};
use crate::microphone::Recording;
use crate::pill::{Fix, Pill, Show};
use crate::settings::{Mode, Settings};
use crate::sounds::Cue;
use crate::transcriber::{Failure, Transcriber};
use iced::futures::channel::mpsc::UnboundedSender;
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
/// Hands-free shows its quiet countdown only after this much quiet, so the
/// pauses between sentences don't make the ring flicker.
pub const QUIET_GRACE: Duration = Duration::from_secs(1);
/// A mic that has sent only digital silence for this long is muted or blocked:
/// the pill says so while there's still time to fix it. People start talking
/// well within it, and a quiet room isn't digital silence.
pub const SILENT_AFTER: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// The shortcut went down (or `likh ctl start`).
    Hold,
    /// The shortcut came up, at this moment. Stamped where the key is seen:
    /// opening the mic takes 300 to 600ms, and a release that waited behind it
    /// would turn every tap into a hold.
    Release(Instant),
    /// The pill's stop button, or `likh ctl stop`: finish and paste, however it started.
    Stop,
    /// Esc, or `likh ctl cancel`.
    Cancel,
    /// The mic couldn't open. `blocked`: Windows' privacy settings keep likh off it.
    MicFailed {
        reason: String,
        blocked: bool,
    },
    /// Sent with each tick while recording: has the mic sent anything but
    /// digital silence yet, has it gone away, and what is it called.
    Sound {
        heard: bool,
        lost: bool,
        device: String,
    },
    Transcribed(Result<String, Failure>),
    /// Sent every 250ms while recording, for the countdown and the cap.
    Tick,
    /// The paste-last shortcut (once its keys are up), or `likh ctl paste-last`.
    PasteLast,
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
    /// Paste the last dictation's text again, or say there's none yet.
    PasteLast,
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
    /// The pill is counting down to silence stop.
    counting_quiet: bool,
    /// The pill is saying the mic is silent.
    warned_silent: bool,
    /// The mic went away mid-recording; the paste says so.
    lost_mic: Option<String>,
}

impl Default for Dictation {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            started: None,
            warned_at: None,
            counting_quiet: false,
            warned_silent: false,
            lost_mic: None,
        }
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
                self.counting_quiet = false;
                self.warned_silent = false;
                self.lost_mic = None;
                let (phase, show) = match choices.mode {
                    Mode::Tap => (Phase::HandsFree, Show::HandsFree),
                    Mode::Hold | Mode::HoldOrTap => (Phase::Recording, Show::Listening),
                };
                self.phase = phase;
                vec![Effect::StartRecording, Effect::Sound(Cue::Start), Effect::Pill(show)]
            }
            (Phase::Recording, Input::Release(at)) if self.held_until(at) < TAP => match choices.mode {
                Mode::HoldOrTap => {
                    self.phase = Phase::HandsFree;
                    vec![Effect::Pill(Show::HandsFree)]
                }
                Mode::Hold | Mode::Tap => self.stop(Effect::Discard, Show::Cancelled),
            },
            (Phase::Recording, Input::Release(_)) => self.transcribe(),
            (Phase::HandsFree, Input::Hold) => self.transcribe(),
            // Only between dictations: mid-recording it would paste into the sentence being spoken.
            (Phase::Idle, Input::PasteLast) => vec![Effect::PasteLast],
            (_, Input::Stop) if recording => self.transcribe(),
            (_, Input::Cancel) if recording => self.stop(Effect::Discard, Show::Cancelled),
            (_, Input::MicFailed { reason, blocked }) if recording => {
                let show = if blocked {
                    Show::Problem { text: crate::microphone::BLOCKED.into(), fix: Fix::MicAccess }
                } else {
                    Show::Error(format!("likh can't hear a microphone. {reason}"))
                };
                let mut effects = self.stop(Effect::Discard, show);
                effects.push(Effect::Sound(Cue::Error));
                effects
            }
            // Unplugged mid-sentence: keep what it heard and paste it, rather than lose it all.
            (_, Input::Sound { lost: true, device, .. }) if recording => {
                self.lost_mic = Some(device);
                self.transcribe()
            }
            (_, Input::Sound { heard: false, device, .. })
                if recording && held >= SILENT_AFTER && !self.warned_silent =>
            {
                self.warned_silent = true;
                vec![Effect::Pill(Show::MicSilent { device })]
            }
            // Sound came back (unmuted mid-dictation): the bars return.
            (phase, Input::Sound { heard: true, .. }) if recording && self.warned_silent => {
                self.warned_silent = false;
                vec![Effect::Pill(if phase == Phase::HandsFree { Show::HandsFree } else { Show::Listening })]
            }
            (_, Input::Tick) if recording && held >= LONGEST => self.transcribe(),
            // Holding the keys means you decide when it ends; only hands-free stops by itself.
            (Phase::HandsFree, Input::Quiet(quiet)) => match choices.silence_stop {
                Some(after) if quiet >= after => self.transcribe(),
                // The last minute's countdown owns the pill once it starts.
                Some(after) if quiet >= QUIET_GRACE && held < WARN_AFTER => {
                    self.counting_quiet = true;
                    vec![Effect::Pill(Show::QuietStop { left: after - quiet, after })]
                }
                // Talking again puts the stop button back.
                _ if self.counting_quiet && held < WARN_AFTER => {
                    self.counting_quiet = false;
                    vec![Effect::Pill(Show::HandsFree)]
                }
                _ => vec![],
            },
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
                                let show = match self.lost_mic.take() {
                                    Some(device) => {
                                        Show::Note(format!("{device} was disconnected. Pasted what it heard."))
                                    }
                                    None => Show::Done,
                                };
                                vec![Effect::Pill(show), Effect::Paste { text, then }]
                            }
                            Action::Undo => vec![Effect::Pill(Show::Done), Effect::Undo],
                        }
                    }
                    Err(Failure::TooShort) => vec![Effect::Pill(Show::Cancelled)],
                    // Nothing broke: a grey note, and no error sound.
                    Err(failure @ Failure::NoVoice) => vec![Effect::Pill(Show::Note(failure.message()))],
                    Err(failure @ Failure::MicSilent { blocked, .. }) => vec![
                        Effect::Sound(Cue::Error),
                        Effect::Pill(Show::Problem {
                            text: failure.message(),
                            fix: if blocked { Fix::MicAccess } else { Fix::PickMic },
                        }),
                    ],
                    Err(failure) => vec![Effect::Sound(Cue::Error), Effect::Pill(Show::Error(failure.message()))],
                }
            }
            // Holding again while the last one is still transcribing, a release
            // in hands-free, a late tick: nothing to do.
            _ => vec![],
        }
    }

    /// How long the keys were down, if they came up at `at`.
    fn held_until(&self, at: Instant) -> Duration {
        self.started.map_or(Duration::ZERO, |started| at.saturating_duration_since(started))
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

/// What the mic made of a recording, read just before it stops.
#[derive(Debug, Clone, PartialEq)]
pub struct Ears {
    /// Anything but digital silence.
    pub sounded: bool,
    /// At least `silence::ARM_AFTER` of voice.
    pub spoke: bool,
    pub device: String,
}

impl Ears {
    /// Why `seconds` of this recording aren't worth sending, if they aren't.
    /// Too-short recordings are left to the transcriber, which drops them quietly.
    pub fn heard_nothing(&self, seconds: f32) -> Option<Failure> {
        if seconds < crate::transcriber::SHORTEST.as_secs_f32() {
            None
        } else if !self.sounded {
            Some(Failure::MicSilent { device: self.device.clone(), blocked: crate::microphone::blocked_by_windows() })
        } else if !self.spoke {
            Some(Failure::NoVoice)
        } else {
            None
        }
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
    /// What the last dictation pasted, for paste last and the tray's copy.
    /// Never printed: `likh ctl status` leaves it out.
    #[serde(skip)]
    pub last_text: Option<String>,
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
    updates: crate::update::Updates,
    ui: UnboundedSender<crate::tray::Pick>,
) -> Arc<Mutex<Status>> {
    let (engine, history) = settings.read().map(|s| (s.engine, s.history)).unwrap_or_default();
    let transcriber = Transcriber::new(engine, fake.as_deref()).name();
    // Paste last still works after a restart, from history when it's kept.
    let last_text = crate::history::file()
        .filter(|_| history)
        .and_then(|file| crate::history::recent(&file, 1).pop())
        .map(|entry| entry.text);
    let status = Arc::new(Mutex::new(Status { phase: Phase::Idle, transcriber, dictations: 0, last: None, last_text }));
    let thread_status = status.clone();
    let spawned = std::thread::Builder::new()
        .name("likh-dictation".into())
        .spawn(move || run(inputs, sender, pill, fake, thread_status, settings, updates, ui));
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

#[expect(clippy::too_many_arguments, reason = "the thread's whole world, handed over once by `spawn`")]
fn run(
    rx: Receiver<Input>,
    tx: Sender<Input>,
    pill: Pill,
    fake: Option<String>,
    status: Arc<Mutex<Status>>,
    settings: Arc<RwLock<Settings>>,
    updates: crate::update::Updates,
    ui: UnboundedSender<crate::tray::Pick>,
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
            Input::MicFailed { reason, .. } => Some(reason.clone()),
            _ => None,
        };
        // Each tick also reports the quiet since the last speech, for silence
        // stop, and whether the mic has sent any sound at all.
        let (quiet, sound) = match (&input, running.recording.as_ref()) {
            (Input::Tick, Some(recording)) => (
                recording.quiet_for(),
                Some(Input::Sound {
                    heard: recording.sounded(),
                    lost: recording.lost(),
                    device: recording.device().to_string(),
                }),
            ),
            _ => (None, None),
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
        if let Some(sound) = sound {
            effects.extend(dictation.handle(sound, now, choices));
        }
        for effect in effects {
            match effect {
                Effect::StartRecording => {
                    // The engine is read per dictation, so a change in settings applies to the next one.
                    let transcriber = Arc::new(Transcriber::new(chosen.engine, fake.as_deref()));
                    status.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).transcriber = transcriber.name();
                    if chosen.pause_media && crate::media::SUPPORTED {
                        running.paused = Some(std::thread::spawn(crate::media::pause_playing));
                    }
                    let levels = pill.clone();
                    let mic = chosen.microphone.as_ref().map(|choice| choice.id.clone());
                    match Recording::start(mic, move |level| levels.level(level)) {
                        Ok(started) => running.recording = Some(started),
                        Err(reason) => {
                            let blocked = crate::microphone::blocked_by_windows();
                            let _ = tx.send(Input::MicFailed { reason, blocked });
                        }
                    }
                    running.transcriber = Some(transcriber);
                }
                Effect::Transcribe => {
                    running.resume_media();
                    let recording = running.recording.take();
                    let ears = recording.as_ref().map(|r| Ears {
                        sounded: r.sounded(),
                        spoke: r.spoke(),
                        device: r.device().to_string(),
                    });
                    let samples = recording.map(Recording::finish).unwrap_or_default();
                    let seconds = samples.len() as f32 / crate::transcriber::SAMPLE_RATE as f32;
                    released = Some((Instant::now(), seconds));
                    let transcriber = running
                        .transcriber
                        .take()
                        .unwrap_or_else(|| Arc::new(Transcriber::new(chosen.engine, fake.as_deref())));
                    let tx = tx.clone();
                    // Silence goes nowhere: an upload of it costs a call and can come back as made-up words.
                    if let Some(failure) = ears.and_then(|ears| ears.heard_nothing(seconds)) {
                        let _ = tx.send(Input::Transcribed(Err(failure)));
                        continue;
                    }
                    let (words, fix_names) = (chosen.dictionary.clone(), chosen.fix_names);
                    // Only a code likh knows reaches the engines; anything else is auto-detect.
                    let language = chosen.language.as_deref().and_then(crate::transcriber::language::find);
                    std::thread::spawn(move || {
                        let heard = transcriber.transcribe(&samples, &words, language.map(|l| l.code));
                        let heard =
                            heard.map(|text| if fix_names { crate::dictionary::fix(&text, &words) } else { text });
                        let _ = tx.send(Input::Transcribed(heard));
                    });
                }
                Effect::Discard => {
                    running.resume_media();
                    running.recording = None;
                    running.transcriber = None;
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
                        if restore.is_some() {
                            status.last_text = Some(text.clone());
                        }
                    }
                    // The tray's "Copy last dictation" line shows its first words.
                    let _ = ui.unbounded_send(crate::tray::Pick::Dictated);
                    // Someone is using likh right now: a fine moment for the daily update check,
                    // which never needs a timer of its own.
                    if chosen.auto_update {
                        updates.check_if_due();
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
                Effect::PasteLast => {
                    let last = status.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).last_text.clone();
                    match last {
                        // No pill: the text landing is the feedback, as with a dictation.
                        Some(text) => match crate::paste::paste(&text) {
                            Ok(restore) => restore.finish(),
                            Err(reason) => pill.show(Show::Error(format!("likh couldn't paste. {reason}"))),
                        },
                        None => pill.show(Show::Note("Nothing to paste yet".into())),
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
