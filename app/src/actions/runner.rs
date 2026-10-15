//! One action at a time, from what was heard to its result, on its own thread
//! so the dictation thread never waits on a script or ChatGPT. The dictation
//! thread asks it three things: is one busy (the action key then says so),
//! is one waiting to be confirmed (the action key then confirms it), and
//! cancel (Esc).

use super::steps::{self, Cancel, Values};
use super::{Action, Catalog, Context, Output, matcher, router, trust};
use crate::dictation::Status;
use crate::pill::{Fix, Pill, Show};
use crate::settings::Settings;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a confirmation waits for the action key before it cancels.
pub const CONFIRM_WITHIN: Duration = Duration::from_secs(5);

/// What the action key heard, and where.
#[derive(Debug, Clone)]
pub struct Job {
    pub said: String,
    /// The window in front when the key went down; results go there or to the clipboard.
    pub window: Option<crate::hotkey::Window>,
    /// How much audio it was, for history.
    pub seconds: f32,
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Running {
        name: String,
        cancel: Cancel,
    },
    Confirming {
        reply: Sender<bool>,
    },
}

/// The last thing each action did wrong, by file, for the Actions tab.
static LAST_ERRORS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

pub fn last_error(id: &str) -> Option<String> {
    LAST_ERRORS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(id).cloned()
}

fn note_error(id: &str, error: Option<String>) {
    let mut errors = LAST_ERRORS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    match error {
        Some(error) => errors.insert(id.to_owned(), error),
        None => errors.remove(id),
    };
}

/// A handle to the one runner. Cheap to clone.
#[derive(Clone)]
pub struct Runner {
    state: Arc<Mutex<State>>,
    pill: Pill,
    status: Arc<Mutex<Status>>,
    ui: iced::futures::channel::mpsc::UnboundedSender<crate::tray::Pick>,
}

impl Runner {
    pub fn new(
        pill: Pill,
        status: Arc<Mutex<Status>>,
        ui: iced::futures::channel::mpsc::UnboundedSender<crate::tray::Pick>,
    ) -> Self {
        Self { state: Arc::default(), pill, status, ui }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Esc belongs to likh while an action runs or waits, so it can be stopped.
    fn set(&self, state: State) {
        let busy = !matches!(state, State::Idle);
        *self.lock() = state;
        crate::hotkey::watch_escape_for_action(busy);
    }

    /// The running action's name, while one runs.
    pub fn busy(&self) -> Option<String> {
        match &*self.lock() {
            State::Running { name, .. } => Some(name.clone()),
            _ => None,
        }
    }

    /// The action key went down: if an action is waiting for it, it runs. True if one was.
    pub fn confirm(&self) -> bool {
        match &*self.lock() {
            State::Confirming { reply } => reply.send(true).is_ok(),
            _ => false,
        }
    }

    /// Esc: stops the running action or drops the one waiting. True if there was one.
    pub fn cancel(&self) -> bool {
        match &*self.lock() {
            State::Confirming { reply } => reply.send(false).is_ok(),
            State::Running { cancel, .. } => {
                cancel.store(true, Ordering::SeqCst);
                true
            }
            State::Idle => false,
        }
    }

    /// Carries out what was heard on its own thread.
    pub fn start(&self, job: Job, settings: Settings) {
        let runner = self.clone();
        let spawned = std::thread::Builder::new().name("likh-action".into()).spawn(move || {
            let cancel: Cancel = Arc::new(AtomicBool::new(false));
            runner.carry_out(job, &settings, cancel);
            runner.set(State::Idle);
        });
        if let Err(error) = spawned {
            self.pill.show(Show::Error(format!("likh couldn't start the action. {error}")));
        }
    }

    fn fail(&self, settings: &Settings, show: Show) {
        if settings.sounds {
            crate::sounds::play(crate::sounds::Cue::Error);
        }
        self.pill.show(show);
    }

    fn carry_out(&self, job: Job, settings: &Settings, cancel: Cancel) {
        let said = job.said.trim().to_owned();
        if said.is_empty() {
            self.pill.show(Show::Cancelled);
            return;
        }
        let Some(dir) = super::folder() else {
            self.fail(settings, Show::Error("likh has no folder for actions.".into()));
            return;
        };
        if let Err(reason) = super::prepare(&dir) {
            eprintln!("likh: couldn't set up the actions folder: {reason}");
        }
        let catalog = Catalog::load(&dir);
        let (index, slots, routed) = match matcher::find(&catalog.actions, &said) {
            Some(found) => (found.action, found.slots, false),
            None => match router::pick(&catalog.actions, &said) {
                Ok(Some(picked)) => (picked.action, picked.slots, true),
                Ok(None) => {
                    self.pill.show(Show::Note(format!("No action for \u{201c}{said}\u{201d}")));
                    return;
                }
                Err(reason) => {
                    eprintln!("likh: no phrase matched and ChatGPT couldn't pick: {reason}");
                    self.pill.show(Show::Note(format!("No action for \u{201c}{said}\u{201d}")));
                    return;
                }
            },
        };
        let action = catalog.actions[index].clone();
        match trust::state(&action) {
            trust::Trust::Approved => {}
            trust::Trust::New | trust::Trust::Changed => {
                let file = action.file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.fail(
                    settings,
                    Show::Problem { text: format!("{file} isn't approved yet, so it won't run."), fix: Fix::Review },
                );
                return;
            }
        }
        if action.confirm || (routed && action.runs_programs()) {
            let (reply, answer) = mpsc::channel();
            self.set(State::Confirming { reply });
            let keys = settings.action_key.map(|key| key.parts().join("+")).unwrap_or_else(|| "the action key".into());
            self.pill.show(Show::Confirm {
                question: format!("Run {}?", action.name),
                keys,
                left: CONFIRM_WITHIN,
                after: CONFIRM_WITHIN,
            });
            if answer.recv_timeout(CONFIRM_WITHIN) != Ok(true) {
                self.set(State::Idle);
                self.pill.show(Show::Cancelled);
                return;
            }
        }
        self.set(State::Running { name: action.name.clone(), cancel: cancel.clone() });
        self.pill.show(Show::Running(action.name.clone()));
        let mut values = match self.read(&action, &said, slots, &job, settings) {
            Ok(values) => values,
            Err(show) => {
                self.pill.show(show);
                return;
            }
        };
        let started = std::time::Instant::now();
        for step in &action.steps {
            if cancel.load(Ordering::SeqCst) {
                self.pill.show(Show::Cancelled);
                return;
            }
            match steps::run(step, &values, &dir, action.timeout, &cancel) {
                Ok(Some(result)) => {
                    values.values.insert("result".into(), result);
                }
                Ok(None) => {}
                Err(why) if cancel.load(Ordering::SeqCst) => {
                    eprintln!("likh: {} stopped: {why}", action.name);
                    self.pill.show(Show::Cancelled);
                    return;
                }
                Err(why) => {
                    note_error(&action.id, Some(why.clone()));
                    self.fail(
                        settings,
                        Show::Problem { text: format!("{} failed: {why}", action.name), fix: Fix::Details },
                    );
                    return;
                }
            }
        }
        note_error(&action.id, None);
        self.deliver(&action, &values, &job, settings, started.elapsed());
    }

    /// What the action gets to work with: what you said, its slots (dictionary
    /// words spelled your way), and whatever it asked to read.
    fn read(
        &self,
        action: &Action,
        said: &str,
        slots: BTreeMap<String, String>,
        job: &Job,
        settings: &Settings,
    ) -> Result<Values, Show> {
        let mut values = Values::default();
        values.values.insert("said".into(), said.to_owned());
        for token in action.patterns.iter().flatten() {
            match token {
                super::Token::List(name) if !values.raw.contains(name) => values.raw.push(name.clone()),
                super::Token::Free(name) => values.free = Some(name.clone()),
                _ => {}
            }
        }
        for (slot, value) in slots {
            let value = if values.raw.contains(&slot) { value } else { matcher::respell(&value, &settings.dictionary) };
            values.values.insert(slot, value);
        }
        for context in &action.input {
            let value = match context {
                Context::Selection => match crate::paste::selection() {
                    Ok(Some(text)) => text,
                    Ok(None) => return Err(Show::Note("Select some text first".into())),
                    Err(reason) => return Err(Show::Error(format!("likh couldn't read the selection. {reason}"))),
                },
                Context::Clipboard => crate::paste::clipboard_text().unwrap_or_default(),
                Context::App => job.window.as_ref().map(|w| w.app.clone()).unwrap_or_default(),
                Context::Title => job.window.as_ref().map(|w| w.title.clone()).unwrap_or_default(),
            };
            values.values.insert(context.slot().into(), value);
        }
        Ok(values)
    }

    fn deliver(&self, action: &Action, values: &Values, job: &Job, settings: &Settings, took: Duration) {
        let result = values.result().trim().to_owned();
        let pasting = matches!(action.output, Output::Paste | Output::Replace);
        let mut pasted = false;
        match action.output {
            Output::Paste | Output::Replace if result.is_empty() => {
                self.pill.show(Show::Note(format!("{} had nothing to paste", action.name)));
            }
            Output::Paste | Output::Replace => {
                // The result only goes where you were: a click elsewhere meanwhile gets it on the clipboard instead.
                let moved = match (&job.window, crate::hotkey::foreground()) {
                    (Some(then), Some(now)) => then.id != now.id,
                    _ => false,
                };
                if moved {
                    match crate::paste::copy(&result) {
                        Ok(()) => self.pill.show(Show::Note("Copied. The window changed.".into())),
                        Err(reason) => self.fail(settings, Show::Error(format!("likh couldn't copy. {reason}"))),
                    }
                } else {
                    match crate::paste::paste(&result) {
                        Ok(restore) => {
                            self.pill.show(Show::Done);
                            restore.finish();
                            pasted = true;
                        }
                        Err(reason) => self.fail(settings, Show::Error(format!("likh couldn't paste. {reason}"))),
                    }
                }
            }
            Output::Notify => {
                let first = result.lines().find(|line| !line.trim().is_empty()).map(str::trim);
                self.pill.show(Show::Finished(first.map_or_else(|| action.name.clone(), str::to_owned)));
            }
            Output::None => self.pill.show(Show::Done),
        }
        if settings.history {
            let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let entry = crate::history::Entry {
                at,
                text: if pasting && !result.is_empty() { result.clone() } else { values.values["said"].clone() },
                seconds: job.seconds,
                action: Some(action.name.clone()),
                pasted,
            };
            if let Some(file) = crate::history::file()
                && let Err(reason) = crate::history::append(&file, &entry)
            {
                eprintln!("likh: couldn't save to history: {reason}");
            }
        }
        if pasted {
            let mut status = self.status.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            status.last_text = Some(result);
            status.actions += 1;
            drop(status);
            let _ = self.ui.unbounded_send(crate::tray::Pick::Dictated);
        } else {
            self.status.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).actions += 1;
        }
        eprintln!("likh: {} done in {} ms", action.name, took.as_millis());
    }
}
