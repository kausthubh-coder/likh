//! One dictation, from key-down to paste. `Dictation::handle` is the whole
//! decision table as a pure function: inputs (keys, a clock tick, the
//! transcript) and the user's choices go in, effects (record, transcribe,
//! paste, sounds, what the pill shows) come out. `run` is the thread that
//! carries the effects out.

use crate::commands::{self, Action, Groups, Press};
use crate::diagnostics::{self, Span};
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

/// A dictation stops by itself here. Wispr Flow's cap is 20 minutes; past that
/// it's a recording, not a dictation.
pub const LONGEST: Duration = Duration::from_secs(30 * 60);
/// The pill counts down the last minute.
pub const WARN_AFTER: Duration = Duration::from_secs(29 * 60);
/// A long dictation goes up in pieces of about this much while you talk, so no
/// upload comes near the Codex app's own 9m55s cap (docs/codex.md) and the
/// wait after you stop is only the last piece's.
pub const PIECE: Duration = Duration::from_secs(180);
/// A press shorter than this is a tap: hands-free in "Hold or tap", dropped in "Hold only".
pub const TAP: Duration = Duration::from_millis(300);
/// Hands-free shows its quiet countdown only after this much quiet, so the
/// pauses between sentences don't make the ring flicker.
pub const QUIET_GRACE: Duration = Duration::from_secs(1);
/// A mic that has sent only digital silence for this long is muted or blocked:
/// the pill says so while there's still time to fix it. People start talking
/// well within it, and a quiet room isn't digital silence.
pub const SILENT_AFTER: Duration = Duration::from_secs(2);
/// Hands-free that has heard no voice by now was a stray tap: it ends as "No
/// voice heard" instead of keeping the mic open until a tap, Esc or the cap.
pub const NO_VOICE_AFTER: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// The shortcut went down (or `likh ctl start`).
    Hold,
    /// The action key went down (or `likh ctl action-start`): what's said next runs an action.
    ActionHold,
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
    /// Sent with each tick until someone has spoken.
    Unspoken,
    /// The pill's Retry, or `likh ctl retry`: send the last failed recording again.
    Retry,
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
    /// Send the kept audio of the last failed dictation to the transcriber again.
    Retry,
    /// Hand what was said to the action runner.
    Act {
        said: String,
    },
    Sound(Cue),
    Pill(Show),
}

/// What the recording is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    /// Typed at the cursor.
    #[default]
    Dictate,
    /// Run as an action (`actions/`).
    Action,
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
    kind: Kind,
    /// The last recording failed in a way that may pass (no connection, a busy
    /// server, a sign-in to refresh): its audio is kept, and this is what it was for.
    retry: Option<Kind>,
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
            kind: Kind::Dictate,
            retry: None,
        }
    }
}

impl Dictation {
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// What the current (or last) recording is for.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// A failed recording is kept and Retry would send it again.
    pub fn can_retry(&self) -> bool {
        self.retry.is_some()
    }

    pub fn handle(&mut self, input: Input, now: Instant, choices: Choices) -> Vec<Effect> {
        let effects = self.decide(input, now, choices);
        if self.kind == Kind::Dictate {
            return effects;
        }
        // An action's recording looks like a dictation's, with the spark on the left.
        effects
            .into_iter()
            .map(|effect| match effect {
                Effect::Pill(
                    show @ (Show::Listening
                    | Show::HandsFree
                    | Show::QuietStop { .. }
                    | Show::Thinking
                    | Show::Warning { .. }),
                ) => Effect::Pill(Show::Spark(Box::new(show))),
                other => other,
            })
            .collect()
    }

    fn decide(&mut self, input: Input, now: Instant, choices: Choices) -> Vec<Effect> {
        let held = self.started.map_or(Duration::ZERO, |started| now.saturating_duration_since(started));
        let recording = matches!(self.phase, Phase::Recording | Phase::HandsFree);
        match (self.phase, input) {
            (Phase::Idle, input @ (Input::Hold | Input::ActionHold)) => {
                self.kind = if input == Input::ActionHold { Kind::Action } else { Kind::Dictate };
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
            // Either key finishes a hands-free recording, whichever started it.
            (Phase::HandsFree, Input::Hold | Input::ActionHold) => self.transcribe(),
            // After a failure, paste last has nothing newer to paste: it tries the failed one again.
            (Phase::Idle, Input::Retry | Input::PasteLast) if self.retry.is_some() => {
                self.kind = self.retry.take().unwrap_or_default();
                self.phase = Phase::Transcribing;
                vec![Effect::Retry, Effect::Pill(Show::Thinking)]
            }
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
            // A stray tap: hands-free that never heard a voice. "Never" in Stop after quiet leaves it open.
            (Phase::HandsFree, Input::Unspoken) if held >= NO_VOICE_AFTER && choices.silence_stop.is_some() => {
                self.stop(Effect::Discard, Show::Note(Failure::NoVoice.message()))
            }
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
                // A newer recording that worked, or that failed for good, replaces a kept one;
                // a stray tap or a silent one leaves it to retry.
                match &result {
                    Err(failure) if failure.retryable() => self.retry = Some(self.kind),
                    Ok(text) if !text.is_empty() => self.retry = None,
                    Err(Failure::TooLong) => self.retry = None,
                    _ => {}
                }
                match result {
                    Ok(text) if text.is_empty() => vec![Effect::Pill(Show::Cancelled)],
                    // The runner takes it from here, and its pill states with it.
                    Ok(said) if self.kind == Kind::Action => vec![Effect::Act { said }],
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
                    Err(failure) if failure.retryable() => vec![
                        Effect::Sound(Cue::Error),
                        Effect::Pill(Show::Problem { text: failure.message(), fix: Fix::Retry }),
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

/// A piece of a long dictation on its way: its audio comes back with its text,
/// so a failed dictation can keep all of it for Retry.
type Piece = std::thread::JoinHandle<(Vec<f32>, Result<String, Failure>)>;

/// The audio of the last dictation that failed in a way that may pass. Filled
/// by the thread that transcribed it, before it reports the failure.
type Kept = Arc<Mutex<Option<Vec<f32>>>>;

/// Transcribes one piece of a long dictation. A piece with no voice in it is
/// skipped (silence can come back as made-up words), and a dropped connection
/// or a busy server gets one more try a second later.
fn transcribe_piece(
    transcriber: &Transcriber,
    samples: &[f32],
    words: &[String],
    language: Option<&str>,
) -> Result<String, Failure> {
    if crate::silence::Listener::new(crate::transcriber::SAMPLE_RATE).push(samples).is_none() {
        return Ok(String::new());
    }
    match transcriber.transcribe(samples, words, language) {
        Err(Failure::TooShort) => Ok(String::new()),
        Err(failure) if failure.passing() => {
            std::thread::sleep(Duration::from_secs(1));
            transcriber.transcribe(samples, words, language)
        }
        heard => heard,
    }
}

/// Turns a recording into text on its own thread and sends `Input::Transcribed`:
/// the pieces sent while you talked, then `samples`, then one cleanup over the
/// whole text so it reads as one dictation. A short dictation is one upload; a
/// retried long one goes up in pieces again, cut every `PIECE`. A failure that
/// may pass leaves all the audio in `kept` first.
#[expect(clippy::too_many_arguments, reason = "one recording and everything its text depends on")]
fn send_off(
    transcriber: Arc<Transcriber>,
    pieces: Vec<Piece>,
    samples: Vec<f32>,
    words: Vec<String>,
    language: Option<&'static str>,
    pass: crate::cleanup::Pass,
    kept: Kept,
    tx: Sender<Input>,
) {
    std::thread::spawn(move || {
        let mut audio = Vec::new();
        let mut heard = Vec::new();
        for piece in pieces {
            let (piece_audio, text) = piece.join().unwrap_or_else(|_| (Vec::new(), Err(Failure::Unreadable)));
            audio.extend(piece_audio);
            heard.push(text);
        }
        let piece_len = (PIECE.as_secs_f32() * crate::transcriber::SAMPLE_RATE as f32) as usize;
        if heard.is_empty() && samples.len() <= piece_len {
            heard.push(transcriber.transcribe(&samples, &words, language));
        } else {
            heard
                .extend(samples.chunks(piece_len).map(|chunk| transcribe_piece(&transcriber, chunk, &words, language)));
        }
        audio.extend(samples);
        let heard = join_pieces(heard).map(|text| crate::cleanup::run(&text, &pass));
        if let Err(failure) = &heard
            && failure.retryable()
        {
            *kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(audio);
        }
        let _ = tx.send(Input::Transcribed(heard));
    });
}

/// One dictation's text from its pieces, in order: every piece's words joined,
/// or the first piece that failed. A gap in the middle would paste as if nothing were missing.
pub fn join_pieces(pieces: Vec<Result<String, Failure>>) -> Result<String, Failure> {
    let mut texts = Vec::new();
    for piece in pieces {
        let text = piece?;
        if !text.trim().is_empty() {
            texts.push(text.trim().to_owned());
        }
    }
    Ok(texts.join(" "))
}

/// What ChatGPT's pass does with a recording's text. An action's words are
/// matched as heard: a cleanup would change the very words that pick it.
fn cleanup_pass(kind: Kind, chosen: &Settings) -> crate::cleanup::Pass {
    match kind {
        Kind::Action => crate::cleanup::Pass::default(),
        Kind::Dictate => crate::cleanup::Pass {
            words: if chosen.fix_names { chosen.dictionary.clone() } else { Vec::new() },
            tidy: chosen.cleanup,
            commands: if chosen.commands { chosen.command_groups.phrases() } else { Vec::new() },
        },
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
    /// Actions finished since likh started.
    pub actions: u64,
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
        .and_then(|file| crate::history::last_pasted(&file))
        .map(|entry| entry.text);
    let status = Arc::new(Mutex::new(Status {
        phase: Phase::Idle,
        transcriber,
        dictations: 0,
        actions: 0,
        last: None,
        last_text,
    }));
    let runner = crate::actions::runner::Runner::new(pill.clone(), status.clone(), ui.clone());
    let thread_status = status.clone();
    let spawned = std::thread::Builder::new()
        .name("likh-dictation".into())
        .spawn(move || run(inputs, sender, pill, runner, fake, thread_status, settings, updates, ui));
    if let Err(error) = spawned {
        diagnostics::error("dictation", format!("could not start dictation: {error}"));
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
    /// A long dictation's pieces already on their way, oldest first.
    pieces: Vec<Piece>,
    /// How much audio those pieces hold.
    pieces_seconds: f32,
    /// The window in front when the action key went down: an action's result goes there.
    window: Option<crate::hotkey::Window>,
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
    runner: crate::actions::runner::Runner,
    fake: Option<String>,
    status: Arc<Mutex<Status>>,
    settings: Arc<RwLock<Settings>>,
    updates: crate::update::Updates,
    ui: UnboundedSender<crate::tray::Pick>,
) {
    let mut dictation = Dictation::default();
    let mut running = Running::default();
    let mut released: Option<(Instant, f32)> = None;
    let kept: Kept = Arc::default();
    // The window the last paste landed in: "scratch that" undoes only there.
    let mut pasted_in: Option<isize> = None;
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
        // The action key and Esc reach a running action first: they confirm, report it busy, or stop it.
        match &input {
            Input::ActionHold if runner.confirm() => continue,
            Input::ActionHold if dictation.phase() == Phase::Idle => {
                if let Some(name) = runner.busy() {
                    pill.show(Show::Note(format!("Busy: {name}")));
                    continue;
                }
                running.window = crate::hotkey::foreground();
            }
            Input::Cancel if !listening && runner.cancel() => continue,
            _ => {}
        }
        let finished = matches!(input, Input::Transcribed(_));
        let failed = match &input {
            Input::Transcribed(Err(failure)) => Some(failure.message()),
            Input::MicFailed { reason, .. } => Some(reason.clone()),
            _ => None,
        };
        // A dictation that pastes is written at the paste; one that doesn't, here.
        if let (Input::Transcribed(heard), Some((at, seconds))) = (&input, released) {
            let span = Span::since("dictation", at).with("audio_seconds", diagnostics::seconds(seconds));
            match heard {
                Ok(text) if text.trim().is_empty() => span.with("result", "nothing heard").end(Ok(())),
                Ok(_) => {}
                Err(Failure::TooShort | Failure::NoVoice) => span.with("result", "nothing heard").end(Ok(())),
                Err(failure) => span.end(Err(failure.message())),
            }
        }
        // Each tick also reports the quiet since the last speech, for silence
        // stop, and whether the mic has sent any sound at all.
        let (quiet, sound) = match (&input, running.recording.as_ref()) {
            (Input::Tick, Some(recording)) => (
                match recording.quiet_for() {
                    Some(quiet) => Some(Input::Quiet(quiet)),
                    None if !recording.spoke() => Some(Input::Unspoken),
                    None => None,
                },
                Some(Input::Sound {
                    heard: recording.sounded(),
                    lost: recording.lost(),
                    device: recording.device().to_string(),
                }),
            ),
            _ => (None, None),
        };
        let chosen = settings.read().map(|s| s.clone()).unwrap_or_else(|poisoned| poisoned.into_inner().clone());
        // Only a code likh knows reaches the engines; anything else is auto-detect.
        let language = chosen.language.as_deref().and_then(crate::transcriber::language::find).map(|l| l.code);
        // A long dictation sends what it has so far and keeps listening.
        if matches!(input, Input::Tick)
            && let (Some(recording), Some(transcriber)) = (running.recording.as_ref(), running.transcriber.clone())
            && let Some(piece) = recording.cut(PIECE)
        {
            running.pieces_seconds += piece.len() as f32 / crate::transcriber::SAMPLE_RATE as f32;
            let words = chosen.dictionary.clone();
            running.pieces.push(std::thread::spawn(move || {
                let text = transcribe_piece(&transcriber, &piece, &words, language);
                (piece, text)
            }));
        }
        let choices = Choices {
            mode: chosen.mode,
            commands: chosen.commands.then_some(chosen.command_groups),
            silence_stop: chosen.silence_stop.after(),
        };
        let now = Instant::now();
        let mut effects = dictation.handle(input, now, choices);
        if let Some(quiet) = quiet {
            effects.extend(dictation.handle(quiet, now, choices));
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
                    let opening = Span::start("microphone");
                    match Recording::start(mic, move |level| levels.level(level)) {
                        Ok(started) => {
                            opening.with("device", started.device()).end(Ok(()));
                            running.recording = Some(started);
                        }
                        Err(reason) => {
                            opening.end(Err(reason.clone()));
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
                    let pieces = std::mem::take(&mut running.pieces);
                    let seconds = std::mem::take(&mut running.pieces_seconds)
                        + samples.len() as f32 / crate::transcriber::SAMPLE_RATE as f32;
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
                    let pass = cleanup_pass(dictation.kind(), &chosen);
                    send_off(transcriber, pieces, samples, chosen.dictionary.clone(), language, pass, kept.clone(), tx);
                }
                Effect::Retry => {
                    let samples = kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
                    let Some(samples) = samples else {
                        let _ = tx.send(Input::Transcribed(Ok(String::new())));
                        continue;
                    };
                    let seconds = samples.len() as f32 / crate::transcriber::SAMPLE_RATE as f32;
                    released = Some((Instant::now(), seconds));
                    // The engine picked now, so fixing a Groq key or switching engines is a way to retry.
                    let transcriber = Arc::new(Transcriber::new(chosen.engine, fake.as_deref()));
                    let pass = cleanup_pass(dictation.kind(), &chosen);
                    let words = chosen.dictionary.clone();
                    send_off(transcriber, Vec::new(), samples, words, language, pass, kept.clone(), tx.clone());
                }
                Effect::Discard => {
                    running.resume_media();
                    running.recording = None;
                    running.transcriber = None;
                    // Pieces already sent finish on their own; nobody reads them.
                    running.pieces.clear();
                    running.pieces_seconds = 0.0;
                }
                Effect::Sound(cue) if chosen.sounds => crate::sounds::play(cue),
                Effect::Sound(_) => {}
                // Ctrl+Z in another app would undo something likh never typed.
                Effect::Undo if pasted_in.is_none() || crate::hotkey::foreground().map(|w| w.id) != pasted_in => {
                    pill.show(Show::Note("Nothing to undo here".into()));
                }
                Effect::Undo => {
                    if let Err(reason) = crate::paste::undo() {
                        diagnostics::error("paste", format!("couldn't undo: {reason}"));
                        pill.show(Show::Error(format!("likh couldn't undo. {reason}")));
                    }
                }
                Effect::Paste { text, then } => {
                    let characters = text.chars().count();
                    let pasting = Span::start("paste").with("characters", characters);
                    let pasted = crate::paste::paste(&text);
                    pasting.end(pasted.as_ref().map(|_| ()).map_err(Clone::clone));
                    let (at, seconds) = released.take().unwrap_or((Instant::now(), 0.0));
                    let mut last =
                        Last { audio_seconds: seconds, latency_ms: at.elapsed().as_millis(), characters, error: None };
                    let restore = match pasted {
                        Ok(restore) => {
                            pasted_in = crate::hotkey::foreground().map(|w| w.id);
                            Some(restore)
                        }
                        Err(reason) => {
                            let message = format!("likh couldn't paste. {reason}");
                            pill.show(Show::Error(message.clone()));
                            last.error = Some(message);
                            None
                        }
                    };
                    // Key-up to the paste: the wait the user feels.
                    Span::since("dictation", at)
                        .with("audio_seconds", diagnostics::seconds(seconds))
                        .with("characters", characters)
                        .end(last.error.clone().map_or(Ok(()), Err));
                    if restore.is_some() && chosen.history {
                        let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
                        let entry =
                            crate::history::Entry { at, text: text.clone(), seconds, action: None, pasted: true };
                        if let Some(file) = crate::history::file()
                            && let Err(reason) = crate::history::append(&file, &entry)
                        {
                            diagnostics::warn("history", format!("couldn't save to history: {reason}"));
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
                            diagnostics::error("paste", format!("couldn't press {}: {reason}", key.name()));
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
                            Ok(restore) => {
                                pasted_in = crate::hotkey::foreground().map(|w| w.id);
                                restore.finish();
                            }
                            Err(reason) => {
                                diagnostics::error("paste", format!("couldn't paste the last dictation: {reason}"));
                                pill.show(Show::Error(format!("likh couldn't paste. {reason}")));
                            }
                        },
                        None => pill.show(Show::Note("Nothing to paste yet".into())),
                    }
                }
                Effect::Act { said } => {
                    let seconds = released.take().map_or(0.0, |(_, seconds)| seconds);
                    let job = crate::actions::runner::Job { said, window: running.window.take(), seconds };
                    runner.start(job, chosen.clone());
                }
                Effect::Pill(show) => pill.show(show),
            }
        }
        // The kept audio lives as long as the table can retry it. Only a result
        // decides that: the thread fills it a moment before its failure arrives.
        if finished && !dictation.can_retry() {
            kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
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
