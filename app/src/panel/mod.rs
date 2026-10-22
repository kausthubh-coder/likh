//! The likh window, opened from the tray. Seven tabs, each with one job: how
//! you start and stop a dictation (Dictation), which engine transcribes
//! (Transcription), everything you can say to make likh do something
//! (Commands: phrases while dictating, and the command key's commands), your
//! own words (Dictionary), what you dictated (History), startup, updates and
//! links (App), and what went wrong or slow lately (Diagnostics).
//!
//! Every setting is one row in a panel: its name on the left, its control on
//! the right. Every tab fits without scrolling except a long History and Diagnostics.
//!
//! Borderless like onboarding: drag it from anywhere empty, close it with the
//! × or Esc. It draws only when something changes, plus the frames a moving
//! key cap or the update spinner asks for.

mod actions;
mod icons;
mod shortcut;

use crate::codex_auth::CodexStatus;
use crate::commands::Group;
use crate::diagnostics::{self, Report, Sampler, Trace, Usage};
use crate::dictation::Status;
use crate::history::{self, Entry};
use crate::hotkey::{self, Binding, MouseTrigger, capture};
use crate::microphone::{self, Choice, Mic};
use crate::onboarding::{self, GroqSetup, Tone};
use crate::settings::{Mode, Settings, SilenceStop};
use crate::shared::icon;
use crate::shared::theme::{self, Palette, palette, radius, size, space};
use crate::shared::ui::{dropdown, field_style, pill_fill, said, segmented, switch};
use crate::transcriber::{Engine, Failure, glyph, groq, language};
use crate::update::{self, Updates};
use iced::widget::{
    Space, button, canvas as canvas_widget, column, container, mouse_area, operation, row, scrollable, stack, text,
    text_input,
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding, Size, Subscription, Task, border, gradient,
    keyboard, window,
};
use icons::{Icon, Shape};
use shortcut::Keys;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(test)]
mod tests;

/// Tall enough for Dictation with the Microphone row and its one-line note, so no tab scrolls but History.
pub const WINDOW: Size = Size::new(720.0, 580.0);
const REPO: &str = "https://github.com/kausthubh-coder/likh";
/// Plenty to scroll back through; the file keeps everything.
const HISTORY_SHOWN: usize = 200;
/// Long dictations are cut in the list; copying still copies all of it.
const PREVIEW_CHARS: usize = 240;
/// A second "Clear" only counts this soon after the first.
const CLEAR_ARMED: Duration = Duration::from_secs(3);
/// How often the Diagnostics tab reads likh's memory and CPU, only while it's open.
const SAMPLE_EVERY: Duration = Duration::from_secs(2);
const GROQ_KEY: &str = "panel-groq-key";
const NEW_WORD: &str = "panel-new-word";
const BODY: &str = "panel-body";
/// The update button at the foot of the sidebar; tests press it by this.
const UPDATE: &str = "panel-update";
/// The scrolling edges fade over this much. The content is padded by the same
/// amount, so at either end the fade lies over empty space and shows nothing.
const FADE_TOP: f32 = space::S5;
const FADE_BOTTOM: f32 = space::S6;
/// The header holds the title, the close button and up to two lines of notice, always this tall.
const HEADER: f32 = 40.0;
const DOT: f32 = 8.0;
const SIDEBAR: f32 = 184.0;
/// A nav row: its padding around 18px of text, and how far it reaches past its 16px icon.
const NAV_ROW: f32 = space::S2 + 18.0 + space::S2;
/// The engine glyph's box, and where an engine's own setup starts so it lines up with its name.
const GLYPH: Size = Size::new(36.0, 28.0);
const SETUP_INDENT: f32 = space::S4 + 16.0 + space::S3 + 36.0 + space::S3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Dictation,
    Transcription,
    Commands,
    Dictionary,
    History,
    App,
    Diagnostics,
}

/// Clean up's pick: off, the same pass everywhere, or a tone per app (`styles/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupMode {
    Off,
    Everywhere,
    ByApp,
}

/// The Commands tab's two halves: phrases that work inside any dictation, and
/// the commands you start with the command key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommandsView {
    #[default]
    WhileDictating,
    CommandKey,
}

impl Tab {
    const ALL: [Tab; 7] =
        [Tab::Dictation, Tab::Transcription, Tab::Commands, Tab::Dictionary, Tab::History, Tab::App, Tab::Diagnostics];

    fn name(self) -> &'static str {
        match self {
            Tab::Dictation => "Dictation",
            Tab::Transcription => "Transcription",
            Tab::Commands => "Commands",
            Tab::Dictionary => "Dictionary",
            Tab::History => "History",
            Tab::App => "App",
            Tab::Diagnostics => "Diagnostics",
        }
    }

    fn icon(self) -> Shape {
        match self {
            Tab::Dictation => Shape::Mic,
            Tab::Transcription => Shape::Wave,
            Tab::Commands => Shape::Spark,
            Tab::Dictionary => Shape::Book,
            Tab::History => Shape::Clock,
            Tab::App => Shape::Gear,
            Tab::Diagnostics => Shape::Pulse,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Tab(Tab),
    ChangeShortcut,
    KeepShortcut,
    /// Pick new keys for paste last.
    ChangePasteLast,
    /// Paste last on these keys, or off.
    PasteLast(Option<Binding>),
    Microphone(MicOption),
    /// The Microphone list opened: look at what's connected now.
    FindMics,
    /// Test, or Stop testing.
    TestMic(bool),
    MicHeard(microphone::Event),
    Language(LanguageOption),
    Key(keyboard::Event),
    /// A frame while a key cap flies or the update button spins.
    Frame(Instant),
    Mouse(MouseTrigger),
    Mode(Mode),
    SilenceStop(SilenceStop),
    Sounds(bool),
    PauseMedia(bool),
    Commands(bool),
    CommandGroup(Group, bool),
    /// Typing in the dictionary's field.
    NewWord(String),
    AddWord,
    RemoveWord(usize),
    FixNames(bool),
    Cleanup(CleanupMode),
    KeepHistory(bool),
    StartWithWindows(bool),
    Engine(Engine),
    Recheck,
    GroqKey(String),
    SaveGroqKey,
    GroqChecked(String, Option<Result<(), Failure>>),
    ReplaceGroqKey,
    ForgetGroqKey,
    CheckForUpdates,
    /// A newer release's page (macOS, where likh can't install it itself).
    OpenRelease(String),
    /// "Restart to update". Handled by the tray, like the tray's own line.
    Restart,
    AutoUpdate(bool),
    Copy(usize),
    Clear,
    /// The first "Clear" timed out.
    Disarm,
    /// Handled by the tray, which owns onboarding.
    RunSetup,
    /// Read the trace again.
    Rediagnose,
    /// The trace, read off the window's thread.
    Diagnosed(Report),
    /// Time to read likh's memory and CPU again.
    Sample,
    OpenLogs,
    /// The report and likh's numbers as JSON, for a bug report.
    CopyReport,
    Open(&'static str),
    Close,
    Drag,
    /// The Commands tab's command-key half (`actions.rs`).
    Action(actions::Message),
    /// Which half of the Commands tab shows.
    CommandsView(CommandsView),
}

/// The two shortcuts the Dictation tab can pick keys for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Picking {
    Shortcut,
    PasteLast,
    ActionKey,
}

pub struct Panel {
    window: window::Id,
    tab: Tab,
    settings: Arc<RwLock<Settings>>,
    dictation: Arc<Mutex<Status>>,
    updates: Updates,
    codex: CodexStatus,
    /// ChatGPT's allowance ran out until then (`allowance/`): Clean up and Fix names wait.
    allowance: Option<u64>,
    groq: GroqSetup,
    /// Groq's card was clicked before a key was saved: it becomes the engine once one is.
    groq_wanted: bool,
    /// Picking new keys: the modifiers held so far.
    capturing: Option<keyboard::Modifiers>,
    /// Which shortcut the keys being picked are for.
    picking: Picking,
    /// The shortcut's caps and keyboard, and their motion.
    keys: Keys,
    /// A new shortcut was just saved; the line under it says so.
    saved: bool,
    notice: Option<(String, Tone)>,
    /// What's typed in the dictionary's field, not yet added.
    new_word: String,
    history: Vec<Entry>,
    /// Summed from the whole history file, not only the entries shown.
    stats: history::Stats,
    /// "Clear" was pressed once; the next press clears.
    confirm_clear: bool,
    copied: Option<usize>,
    /// Seconds the update button has been spinning, and the last frame, while anything moves.
    spin: f32,
    last_frame: Option<Instant>,
    files: Files,
    /// The inputs connected when the list was last opened, and which one Windows uses.
    mics: Vec<Mic>,
    default_mic: Option<Mic>,
    /// The Test meter is listening. The mic opens only then, never just because the tab is open.
    testing: bool,
    /// The meter's levels, newest last.
    mic_levels: [f32; MIC_BARS],
    /// The command key's commands: the folder as last read, and which view of it is open.
    actions: actions::State,
    commands_view: CommandsView,
    /// The trace summed up; None while it's being read.
    report: Option<Report>,
    /// Reads likh's own process. Held only while Diagnostics is open.
    sampler: Option<Sampler>,
    usage: Option<Usage>,
}

/// The Test meter: the pill's seven bars, small.
const MIC_BARS: usize = 7;

/// One line of the Language list: Auto-detect, or a language by its English name.
#[derive(Debug, Clone, PartialEq)]
pub struct LanguageOption {
    code: Option<&'static str>,
    label: &'static str,
}

impl std::fmt::Display for LanguageOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label)
    }
}

/// One line of the Microphone list: Windows' default, or a mic by name.
#[derive(Debug, Clone, PartialEq)]
pub struct MicOption {
    choice: Option<Choice>,
    label: String,
}

impl std::fmt::Display for MicOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

/// Where the settings, history and logs live, and how to list this computer's
/// mics. The app passes the real ones; tests pass temporary files and made-up mics.
pub struct Files {
    pub settings: Option<PathBuf>,
    pub history: Option<PathBuf>,
    /// The actions folder, and where approvals of their scripts are kept.
    pub actions: Option<PathBuf>,
    pub approved: Option<PathBuf>,
    /// The trace folder Diagnostics reads.
    pub logs: Option<PathBuf>,
    /// The mics connected now, and Windows' default among them.
    pub mics: fn() -> (Vec<Mic>, Option<Mic>),
}

impl Files {
    pub fn real() -> Self {
        Self {
            settings: crate::settings::folder().map(|dir| dir.join("settings.json")),
            history: history::file(),
            actions: crate::actions::folder(),
            approved: crate::actions::trust::store(),
            logs: diagnostics::folder(),
            mics: || (microphone::inputs(), microphone::system_default()),
        }
    }
}

pub fn window_settings() -> window::Settings {
    window::Settings {
        size: WINDOW,
        position: window::Position::Centered,
        resizable: false,
        decorations: false,
        icon: window::icon::from_rgba(icon::rgba(64), 64, 64).ok(),
        #[cfg(target_os = "windows")]
        platform_specific: window::settings::PlatformSpecific {
            undecorated_shadow: true,
            corner_preference: window::settings::platform::CornerPreference::Round,
            ..window::settings::PlatformSpecific::default()
        },
        ..window::Settings::default()
    }
}

impl Panel {
    pub fn new(
        window: window::Id,
        settings: Arc<RwLock<Settings>>,
        dictation: Arc<Mutex<Status>>,
        updates: Updates,
        files: Files,
    ) -> Self {
        let shortcut = settings.read().map(|s| s.shortcut).unwrap_or_default();
        // It opens on Dictation, where the Microphone list is.
        let (mics, default_mic) = (files.mics)();
        Self {
            window,
            tab: Tab::Dictation,
            settings,
            dictation,
            updates,
            codex: onboarding::check_codex_now(),
            allowance: None,
            groq: GroqSetup::look(),
            groq_wanted: false,
            capturing: None,
            picking: Picking::Shortcut,
            keys: Keys::at_rest(&shortcut.parts()),
            saved: false,
            notice: None,
            new_word: String::new(),
            history: Vec::new(),
            stats: history::Stats::default(),
            confirm_clear: false,
            copied: None,
            spin: 0.0,
            last_frame: None,
            mics,
            default_mic,
            files,
            testing: false,
            mic_levels: [0.0; MIC_BARS],
            actions: actions::State::default(),
            commands_view: CommandsView::default(),
            report: None,
            sampler: None,
            usage: None,
        }
    }

    /// Reads the trace on its own thread: a few megabytes at most, but never on the window's.
    fn diagnose(&mut self) -> Task<Message> {
        self.report = None;
        let Some(logs) = self.files.logs.clone() else {
            self.report = Some(Report::default());
            return Task::none();
        };
        Task::perform(crate::shared::off_thread(move || Trace::new(logs).read()), |report| {
            Message::Diagnosed(report.unwrap_or_default())
        })
    }

    fn chosen(&self) -> Settings {
        self.settings.read().map(|s| s.clone()).unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    /// Changes one setting and saves the file; says so if saving failed.
    fn change(&mut self, edit: impl FnOnce(&mut Settings)) {
        let saved = {
            let mut settings = self.settings.write().unwrap_or_else(|poisoned| poisoned.into_inner());
            edit(&mut settings);
            match &self.files.settings {
                Some(file) => settings.save_to(file),
                None => Err("there is no local data folder".into()),
            }
        };
        if let Err(reason) = saved {
            self.notice = Some((format!("That works now but wasn't saved: {reason}"), Tone::Warning));
        }
    }

    fn load_history(&mut self) {
        let all = self.files.history.as_deref().map(|file| history::recent(file, usize::MAX)).unwrap_or_default();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        self.stats = history::stats(&all, history::local_day(now), history::local_day);
        self.history = all.into_iter().take(HISTORY_SHOWN).collect();
        self.copied = None;
        self.confirm_clear = false;
    }

    /// Looks again at Codex's sign-in (a Groq key only changes in likh itself).
    fn recheck(&mut self) {
        if self.chosen().engine == Engine::Codex {
            self.codex = onboarding::check_codex_now();
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tab(tab) => {
                self.stop_capturing();
                // Leaving the row lets go of the mic.
                self.testing = false;
                if tab == Tab::Dictation {
                    self.find_mics();
                }
                self.tab = tab;
                self.notice = None;
                self.saved = false;
                self.keys = Keys::at_rest(&self.chosen().shortcut.parts());
                // Leaving Diagnostics stops reading the process.
                self.sampler = None;
                self.usage = None;
                let read = match tab {
                    Tab::History => {
                        self.load_history();
                        Task::none()
                    }
                    // Both engines show where they stand, so look at both. Fixing names needs Codex too.
                    Tab::Transcription | Tab::Dictionary => {
                        self.codex = onboarding::check_codex_now();
                        self.allowance = crate::allowance::paused_until_now();
                        Task::none()
                    }
                    // Read the folder again: an agent may have changed it since.
                    Tab::Commands => {
                        self.load_actions();
                        Task::none()
                    }
                    Tab::Diagnostics => {
                        let mut sampler = Sampler::default();
                        self.usage = sampler.sample();
                        self.sampler = Some(sampler);
                        self.diagnose()
                    }
                    Tab::Dictation | Tab::App => Task::none(),
                };
                // Each tab starts at its top.
                return Task::batch([read, operation::snap_to(BODY, scrollable::RelativeOffset::START)]);
            }
            Message::ChangeShortcut => {
                self.stop_capturing();
                hotkey::pause(true);
                self.capturing = Some(keyboard::Modifiers::empty());
                self.picking = Picking::Shortcut;
                self.notice = None;
                self.saved = false;
                self.keys.listen();
            }
            Message::ChangePasteLast => {
                self.stop_capturing();
                self.keys = Keys::at_rest(&self.chosen().shortcut.parts());
                hotkey::pause(true);
                self.capturing = Some(keyboard::Modifiers::empty());
                self.picking = Picking::PasteLast;
                self.notice = None;
                self.saved = false;
            }
            Message::KeepShortcut => {
                self.stop_capturing();
                self.keys = Keys::at_rest(&self.chosen().shortcut.parts());
            }
            Message::Microphone(option) => {
                self.change(|s| s.microphone = option.choice);
                self.mic_levels = [0.0; MIC_BARS];
            }
            Message::FindMics => self.find_mics(),
            Message::Language(option) => self.change(|s| s.language = option.code.map(String::from)),
            Message::TestMic(on) => {
                self.testing = on;
                self.mic_levels = [0.0; MIC_BARS];
            }
            Message::MicHeard(microphone::Event::Level(level)) => {
                self.mic_levels.rotate_left(1);
                self.mic_levels[MIC_BARS - 1] = level;
            }
            Message::MicHeard(microphone::Event::Opened { .. }) => {}
            Message::MicHeard(microphone::Event::Failed(reason)) => {
                self.testing = false;
                self.notice = Some((format!("likh can't hear that mic. {reason}"), Tone::Danger));
            }
            Message::PasteLast(keys) => match hotkey::set_paste_last(keys) {
                Ok(()) => self.change(|s| s.paste_last = keys),
                Err(reason) => self.notice = Some((reason, Tone::Danger)),
            },
            Message::Key(event) => match self.capturing {
                Some(held) => match capture::on_event(&event, held) {
                    capture::Outcome::Waiting(held) => {
                        self.capturing = Some(held);
                        if self.picking == Picking::Shortcut {
                            self.keys.hold(&capture::held_parts(held));
                        }
                    }
                    capture::Outcome::Cancelled => {
                        self.stop_capturing();
                        self.keys = Keys::at_rest(&self.chosen().shortcut.parts());
                    }
                    capture::Outcome::Refused(why) => self.notice = Some((why.into(), Tone::Danger)),
                    // One set of keys can't do two things.
                    capture::Outcome::Chosen(binding) if self.taken(binding) => {
                        self.notice =
                            Some(("Those keys already do something in likh. Pick others.".into(), Tone::Danger));
                    }
                    capture::Outcome::Chosen(binding) if self.picking == Picking::ActionKey => {
                        match hotkey::set_action_key(Some(binding)) {
                            Ok(()) => {
                                self.change(|s| s.action_key = Some(binding));
                                self.stop_capturing();
                            }
                            Err(reason) => self.notice = Some((reason, Tone::Danger)),
                        }
                    }
                    capture::Outcome::Chosen(binding) if self.picking == Picking::PasteLast => {
                        match hotkey::set_paste_last(Some(binding)) {
                            Ok(()) => {
                                self.change(|s| s.paste_last = Some(binding));
                                self.stop_capturing();
                            }
                            Err(reason) => self.notice = Some((reason, Tone::Danger)),
                        }
                    }
                    capture::Outcome::Chosen(binding) => match hotkey::rebind(binding) {
                        Ok(()) => {
                            self.change(|s| s.shortcut = binding);
                            self.stop_capturing();
                            self.keys.chosen(&binding.parts());
                            self.saved = true;
                        }
                        Err(reason) => self.notice = Some((reason, Tone::Danger)),
                    },
                },
                // Esc closes the window when nothing else wants it.
                None if is_escape(&event) => return window::close(self.window),
                // Tab moves between fields (New command's form has six); iced leaves it to the app.
                None if tab(&event) == Some(false) => return operation::focus_next(),
                None if tab(&event) == Some(true) => return operation::focus_previous(),
                None => {}
            },
            Message::Frame(now) => {
                let dt = self.last_frame.map_or(1.0 / 60.0, |last| (now - last).as_secs_f32().min(0.05));
                self.keys.step(dt);
                self.spin += dt;
                self.last_frame = self.wants_frames().then_some(now);
            }
            Message::Mouse(trigger) => {
                // One button, one job: the hotkey thread takes it from actions too.
                self.change(|s| {
                    s.mouse = trigger;
                    if trigger != MouseTrigger::Off && s.action_mouse == trigger {
                        s.action_mouse = MouseTrigger::Off;
                    }
                });
                if let Err(reason) = hotkey::set_mouse(trigger) {
                    self.notice = Some((format!("The mouse button won't work yet: {reason}"), Tone::Danger));
                }
            }
            Message::Mode(mode) => self.change(|s| s.mode = mode),
            Message::SilenceStop(after) => self.change(|s| s.silence_stop = after),
            Message::Sounds(on) => {
                self.change(|s| s.sounds = on);
                if on {
                    crate::sounds::play(crate::sounds::Cue::Start);
                }
            }
            Message::PauseMedia(on) => self.change(|s| s.pause_media = on),
            Message::Commands(on) => self.change(|s| s.commands = on),
            Message::CommandGroup(group, on) => self.change(|s| group.set(&mut s.command_groups, on)),
            Message::NewWord(word) => {
                self.new_word = word;
                self.notice = None;
            }
            Message::AddWord => {
                let mut words = self.chosen().dictionary;
                match crate::dictionary::add(&mut words, &self.new_word) {
                    Ok(()) => {
                        self.change(|s| s.dictionary = words);
                        self.new_word.clear();
                    }
                    Err(why) => self.notice = Some((why, Tone::Danger)),
                }
                // Ready for the next word.
                return operation::focus(NEW_WORD);
            }
            Message::RemoveWord(index) => self.change(|s| {
                if index < s.dictionary.len() {
                    s.dictionary.remove(index);
                }
            }),
            Message::FixNames(on) => self.change(|s| s.fix_names = on),
            Message::Cleanup(mode) => self.change(|s| {
                s.cleanup = mode != CleanupMode::Off;
                // Off keeps the per-app choice for the next time it's on.
                if mode != CleanupMode::Off {
                    s.styles = mode == CleanupMode::ByApp;
                }
            }),
            Message::KeepHistory(on) => {
                self.change(|s| s.history = on);
                self.load_history();
            }
            Message::StartWithWindows(on) => match crate::autostart::set(on) {
                Ok(()) => self.change(|s| s.start_with_windows = on),
                Err(reason) => self.notice = Some((format!("That didn't work: {reason}"), Tone::Danger)),
            },
            // Groq without a key would fail the next dictation: it switches once a key is saved.
            Message::Engine(Engine::Groq) if !self.groq.saved => {
                self.groq_wanted = true;
                self.groq.notice = Some(("Save a key to switch to Groq.".into(), Tone::Neutral));
                return operation::focus(GROQ_KEY);
            }
            Message::Engine(engine) => {
                self.groq_wanted = false;
                self.change(|s| s.engine = engine);
                self.recheck();
            }
            Message::Recheck => self.recheck(),
            Message::GroqKey(key) => {
                self.groq.key = key;
                self.groq.notice = None;
            }
            Message::SaveGroqKey if self.groq.checking => {}
            Message::SaveGroqKey => return self.groq.save(Message::GroqChecked),
            Message::GroqChecked(key, answer) => {
                self.groq.checked(&key, answer);
                if self.groq.saved && std::mem::take(&mut self.groq_wanted) {
                    self.change(|s| s.engine = Engine::Groq);
                }
            }
            Message::ReplaceGroqKey => {
                self.groq.saved = false;
                self.groq.notice = None;
                return operation::focus(GROQ_KEY);
            }
            Message::ForgetGroqKey => {
                self.groq.forget();
                // Groq can't transcribe without a key: back to ChatGPT, which needs none.
                if !self.groq.saved && self.chosen().engine == Engine::Groq {
                    self.change(|s| s.engine = Engine::Codex);
                    self.recheck();
                }
            }
            Message::CheckForUpdates => {
                self.updates.check_now();
                self.last_frame = None;
            }
            Message::OpenRelease(page) => crate::shared::open_url(&page),
            Message::Restart => {}
            Message::AutoUpdate(on) => self.change(|s| s.auto_update = on),
            Message::Copy(index) => {
                if let Some(entry) = self.history.get(index) {
                    match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(entry.text.clone())) {
                        Ok(()) => self.copied = Some(index),
                        Err(reason) => self.notice = Some((format!("Couldn't copy: {reason}"), Tone::Danger)),
                    }
                }
            }
            Message::Clear if !self.confirm_clear => {
                self.confirm_clear = true;
                return Task::perform(crate::shared::off_thread(|| std::thread::sleep(CLEAR_ARMED)), |_| {
                    Message::Disarm
                });
            }
            Message::Clear => {
                if let Some(file) = &self.files.history
                    && let Err(reason) = history::clear(file)
                {
                    self.notice = Some((format!("Couldn't clear history: {reason}"), Tone::Danger));
                }
                self.load_history();
            }
            Message::Disarm => self.confirm_clear = false,
            Message::RunSetup => {}
            Message::Rediagnose => return self.diagnose(),
            Message::Diagnosed(report) => self.report = Some(report),
            Message::Sample => {
                if let Some(sampler) = &mut self.sampler {
                    self.usage = sampler.sample();
                }
            }
            Message::OpenLogs => {
                if let Some(logs) = &self.files.logs {
                    // Explorer opens the folder even before anything is written to it.
                    let _ = std::fs::create_dir_all(logs);
                    crate::shared::open_url(&logs.to_string_lossy());
                }
            }
            Message::CopyReport => {
                let copy = serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "process": self.usage,
                    "report": self.report,
                });
                let words = serde_json::to_string_pretty(&copy).unwrap_or_default();
                self.notice = match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(words)) {
                    Ok(()) => Some(("Copied. It has no dictation text in it.".into(), Tone::Success)),
                    Err(reason) => Some((format!("Couldn't copy: {reason}"), Tone::Danger)),
                };
            }
            Message::Open(url) => crate::shared::open_url(url),
            Message::Close => return window::close(self.window),
            Message::Drag => return window::drag(self.window),
            Message::Action(message) => return self.update_actions(message),
            Message::CommandsView(view) => {
                self.stop_capturing();
                self.commands_view = view;
                self.actions.view = actions::View::List;
                self.notice = None;
            }
        }
        Task::none()
    }

    /// Another of likh's shortcuts already uses these keys.
    fn taken(&self, binding: Binding) -> bool {
        let chosen = self.chosen();
        let others = match self.picking {
            Picking::Shortcut => [chosen.paste_last, chosen.action_key],
            Picking::PasteLast => [Some(chosen.shortcut), chosen.action_key],
            Picking::ActionKey => [Some(chosen.shortcut), chosen.paste_last],
        };
        others.contains(&Some(binding))
    }

    fn stop_capturing(&mut self) {
        if self.capturing.take().is_some() {
            hotkey::pause(false);
        }
    }

    /// Releases the shortcut if the window closes mid-capture.
    pub fn closing(&mut self) {
        self.stop_capturing();
        self.testing = false;
    }

    fn find_mics(&mut self) {
        (self.mics, self.default_mic) = (self.files.mics)();
    }

    /// The Microphone list: Windows' default first, then each mic, then the
    /// picked one if it isn't connected, so the list still shows what's picked.
    fn mic_options(&self) -> (Vec<MicOption>, MicOption) {
        let default = MicOption {
            choice: None,
            label: match &self.default_mic {
                // Windows' own word for it, with the mic it means today.
                Some(mic) => format!("Default \u{b7} {}", mic.name),
                None => "Default".into(),
            },
        };
        let mut options = vec![default.clone()];
        options.extend(self.mics.iter().map(|mic| MicOption {
            choice: Some(Choice { id: mic.id.clone(), name: mic.name.clone() }),
            label: mic.name.clone(),
        }));
        let picked = match self.chosen().microphone {
            None => default,
            Some(choice) => match options.iter().find(|option| option.choice.as_ref() == Some(&choice)) {
                Some(found) => found.clone(),
                None => {
                    let gone = MicOption { label: format!("{} (not connected)", choice.name), choice: Some(choice) };
                    options.push(gone.clone());
                    gone
                }
            },
        };
        (options, picked)
    }

    /// The Microphone row: the list, Test (or the meter and Stop while
    /// testing), and a line when the pick needs one.
    fn microphone_line(&self, p: &Palette) -> Line<'_> {
        let (options, picked) = self.mic_options();
        let chosen = self.chosen().microphone;
        let connected = chosen.as_ref().is_none_or(|choice| self.mics.iter().any(|mic| mic.id == choice.id));
        let using = match &chosen {
            Some(choice) if connected => self.mics.iter().find(|mic| mic.id == choice.id),
            _ => self.default_mic.as_ref(),
        };
        let what = if !connected {
            let fallback = self.default_mic.as_ref().map_or("the default mic", |mic| mic.name.as_str());
            Some(format!("Not connected. Using {fallback} until it's back."))
        } else if using.is_some_and(|mic| mic.bluetooth) {
            Some("Bluetooth headsets drop to call quality while you talk.".to_string())
        } else {
            None
        };
        let mut control = row![].spacing(space::S3).align_y(Alignment::Center);
        if self.testing {
            let accent = p.accent;
            let bars = row(self.mic_levels.iter().map(|level| {
                container(Space::new()).width(3).height(4.0 + level * 12.0).style(move |_| pill_fill(accent)).into()
            }))
            .spacing(2)
            .height(16)
            .align_y(Alignment::Center);
            control = control.push(bars).push(link(p, "Stop", Message::TestMic(false)));
        } else {
            control = control.push(link(p, "Test", Message::TestMic(true)));
        }
        control = control.push(dropdown(options, Some(picked), Message::Microphone, Some(Message::FindMics)));
        let top = row![container(named(p, "Microphone", None)).width(Length::Fill), control]
            .spacing(space::S4)
            .align_y(Alignment::Center);
        // The list takes most of the row, so its line runs under the whole row, not under the name.
        match what {
            Some(what) => Line::custom(
                column![
                    top,
                    text(what).size(size::SMALL).line_height(theme::line_height(size::SMALL)).color(p.text_muted)
                ]
                .spacing(space::S1)
                .into(),
            ),
            None => Line::custom(top.into()),
        }
    }

    /// A key cap in flight, or the update button spinning. Otherwise the window draws nothing.
    fn wants_frames(&self) -> bool {
        self.keys.moving()
            || matches!(self.updates.state(), update::State::Checking | update::State::Downloading { .. })
    }

    pub fn subscription(&self) -> Subscription<Message> {
        // Keys reach the window only while it has focus: Esc to close, or a new shortcut.
        let keys = keyboard::listen().map(Message::Key);
        let frames = if self.wants_frames() { window::frames().map(Message::Frame) } else { Subscription::none() };
        // Keyed by the picked mic, so picking another while testing listens to that one.
        let mic = if self.testing {
            let picked = self.chosen().microphone.map(|choice| choice.id);
            Subscription::run_with(picked, microphone::listen).map(Message::MicHeard)
        } else {
            Subscription::none()
        };
        // The numbers move only while someone is looking at them.
        let sample = if self.sampler.is_some() {
            iced::time::every(SAMPLE_EVERY).map(|_| Message::Sample)
        } else {
            Subscription::none()
        };
        Subscription::batch([keys, frames, mic, sample])
    }

    pub fn view(&self) -> Element<'_, Message> {
        let p = palette();
        let content = match self.tab {
            Tab::Dictation => self.dictation_tab(&p),
            Tab::Transcription => self.transcription(&p),
            Tab::Commands => self.commands_tab(&p),
            Tab::Dictionary => self.dictionary(&p),
            Tab::History => self.history_list(&p),
            Tab::App => self.app(&p),
            Tab::Diagnostics => self.diagnostics(&p),
        };
        // The notice lives in the header, which is always the same height, so nothing below moves when it shows.
        let words = self.notice.as_ref().or(self.groq.notice.as_ref().filter(|_| self.tab == Tab::Transcription));
        let notice: Element<'_, Message> = match words {
            Some((words, tone)) => text(words.clone())
                .size(size::SMALL)
                .line_height(theme::line_height(size::SMALL))
                .color(tone_color(&p, *tone))
                .width(Length::Fill)
                .into(),
            None => Space::new().width(Length::Fill).into(),
        };
        let close = button(
            canvas_widget(Icon {
                shape: Shape::Cross,
                ink: p.text_muted,
                lit: p.text,
                backdrop: p.bg,
                plate: None,
                reach: Padding::new(space::S3),
            })
            .width(16)
            .height(16),
        )
        .on_press(Message::Close)
        .padding(space::S3)
        .style(|_, _| button::Style::default());
        let header = row![
            text(self.tab.name())
                .size(size::TITLE)
                .line_height(theme::line_height(size::TITLE))
                .font(theme::UI_SEMIBOLD),
            notice,
            close,
        ]
        .spacing(space::S4)
        .height(HEADER)
        .align_y(Alignment::Center);

        // No scrollbar: a line cut off by the bottom fade says there's more.
        let scroll =
            scrollable(container(content).padding(Padding { top: FADE_TOP, bottom: FADE_BOTTOM, ..Padding::ZERO }))
                .id(BODY)
                .height(Length::Fill)
                .direction(scrollable::Direction::Vertical(scrollable::Scrollbar::new().width(0).scroller_width(0)));
        let fades = column![
            fade_edge(p.bg, FADE_TOP, true),
            Space::new().height(Length::Fill),
            fade_edge(p.bg, FADE_BOTTOM, false)
        ];
        let body = stack![scroll, fades].height(Length::Fill);
        let main =
            column![header, body].padding(Padding { top: space::S4, left: space::S6, right: space::S5, bottom: 0.0 });

        let surface =
            container(row![self.sidebar(&p), main]).width(Length::Fill).height(Length::Fill).style(move |_| {
                container::Style {
                    background: Some(Background::Color(p.bg)),
                    text_color: Some(p.text),
                    ..container::Style::default()
                }
            });
        // No title bar, so empty space moves the window. Buttons take their own clicks first.
        mouse_area(surface).on_press(Message::Drag).into()
    }

    fn sidebar(&self, p: &Palette) -> Element<'_, Message> {
        let (accent, surface) = (p.accent, p.surface);
        let mark = row([0.45f32, 0.85, 0.55, 0.85, 0.45]
            .map(|h| container(Space::new()).width(3).height(16.0 * h).style(move |_| pill_fill(accent)).into()))
        .spacing(2)
        .align_y(Alignment::Center);
        let brand = row![mark, text("likh").size(size::BODY).font(theme::UI_SEMIBOLD)]
            .spacing(space::S2)
            .align_y(Alignment::Center);
        let nav = column(Tab::ALL.map(|tab| self.nav_item(p, tab)));
        container(
            column![
                container(brand).padding([0.0, space::S3]),
                Space::new().height(space::S5),
                nav,
                Space::new().height(Length::Fill),
                self.sidebar_update(p),
            ]
            .padding([space::S5, space::S4]),
        )
        .width(SIDEBAR)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(surface)),
            ..container::Style::default()
        })
        .into()
    }

    /// The foot of the sidebar: the version, and next to it the one update
    /// button. It checks, spins while checking, and turns peach once there's
    /// a newer likh to restart into.
    fn sidebar_update(&self, p: &Palette) -> Element<'_, Message> {
        let this = env!("CARGO_PKG_VERSION");
        let turned = self.spin * 360.0;
        let (words, shape, press, peach): (String, Shape, Option<Message>, bool) = match self.updates.state() {
            update::State::Ready { version, .. } => {
                (format!("v{version} ready"), Shape::Up, Some(Message::Restart), true)
            }
            update::State::Available { version, page } => {
                (format!("v{version} is out"), Shape::Up, Some(Message::OpenRelease(page)), true)
            }
            update::State::Downloading { version } => (format!("v{version}\u{2026}"), Shape::Spin(turned), None, false),
            update::State::Checking => (format!("v{this}"), Shape::Spin(turned), None, false),
            update::State::UpToDate { .. } | update::State::Idle | update::State::Failed { .. } => {
                (format!("v{this}"), Shape::Refresh, Some(Message::CheckForUpdates), false)
            }
        };
        let glyph = canvas_widget(Icon {
            shape,
            ink: if peach { p.on_accent } else { p.text_muted },
            lit: if peach { p.on_accent } else { p.text },
            backdrop: p.surface,
            plate: peach.then_some(p.accent),
            reach: Padding::ZERO,
        })
        .width(28)
        .height(28);
        let mut press_it = button(glyph).padding(0).style(|_, _| button::Style::default());
        if let Some(message) = press {
            press_it = press_it.on_press(message);
        }
        row![
            text(words).size(size::CAPTION).font(theme::MONO).color(if peach { p.text_muted } else { p.text_subtle }),
            Space::new().width(Length::Fill),
            container(press_it).id(UPDATE),
        ]
        .padding(Padding { left: space::S3, ..Padding::ZERO })
        .align_y(Alignment::Center)
        .into()
    }

    fn nav_item(&self, p: &Palette, tab: Tab) -> Element<'_, Message> {
        let selected = self.tab == tab;
        let (on, rest, ink) = (p.elevated, p.text_muted, p.text);
        // The icon lights with the whole row; the row reaches 12px left of it and the rest of the sidebar right.
        let inner = SIDEBAR - 2.0 * space::S4;
        let mark = canvas_widget(Icon {
            shape: tab.icon(),
            ink: if selected { p.accent } else { p.text_muted },
            lit: if selected { p.accent } else { p.text },
            backdrop: if selected { on } else { p.surface },
            plate: None,
            reach: Padding {
                left: space::S3,
                right: inner - space::S3 - 16.0,
                top: (NAV_ROW - 16.0) / 2.0,
                bottom: (NAV_ROW - 16.0) / 2.0,
            },
        })
        .width(16)
        .height(16);
        button(
            row![mark, text(tab.name()).size(size::SMALL).font(theme::UI_MEDIUM)]
                .spacing(space::S3)
                .align_y(Alignment::Center),
        )
        .on_press(Message::Tab(tab))
        .width(Length::Fill)
        .padding([space::S2, space::S3])
        .style(move |_, status| button::Style {
            // No hover fill: the icon paints its own backdrop. Words and icon brighten instead.
            background: selected.then_some(Background::Color(on)),
            text_color: if selected || matches!(status, button::Status::Hovered | button::Status::Pressed) {
                ink
            } else {
                rest
            },
            border: Border { radius: radius::CONTROL.into(), ..Border::default() },
            ..button::Style::default()
        })
        .into()
    }

    fn dictation_tab(&self, p: &Palette) -> Element<'_, Message> {
        let chosen = self.chosen();
        // The shortcut: its caps and the little keyboard, with a line under the caps and Change at its end.
        let picking_shortcut = self.capturing.filter(|_| self.picking == Picking::Shortcut);
        let (line, tone, change): (String, Tone, Element<'_, Message>) = match picking_shortcut {
            Some(_) => ("Press your new shortcut.".into(), Tone::Neutral, link(p, "Cancel", Message::KeepShortcut)),
            None if self.saved => (
                format!("Saved. Hold {} to talk.", chosen.shortcut),
                Tone::Success,
                link(p, "Change", Message::ChangeShortcut),
            ),
            // Holding them would do nothing, so the fix is the first thing on the tab.
            // macOS: no keys work until likh has Accessibility, and the fix is there, not new keys.
            None if hotkey::shortcut_taken() && cfg!(target_os = "macos") => (
                hotkey::why_unheard().into(),
                Tone::Danger,
                link(p, "Open settings", Message::Open(crate::shared::ACCESSIBILITY_SETTINGS)),
            ),
            None if hotkey::shortcut_taken() => {
                (hotkey::why_unheard().into(), Tone::Danger, link(p, "Change", Message::ChangeShortcut))
            }
            None => ("Hold to talk, in any app.".into(), Tone::Neutral, link(p, "Change", Message::ChangeShortcut)),
        };
        let art = canvas_widget(self.keys.art(p)).width(Length::Fill).height(shortcut::KEYBOARD.height);
        let under = column![
            Space::new().height(shortcut::CAP_HEIGHT + space::S2),
            row![text(line).size(size::SMALL).color(tone_color(p, tone)), change]
                .spacing(space::S2)
                .align_y(Alignment::Center),
        ];
        let hero = container(stack![art, under]).padding([space::S1, 0.0]).width(Length::Fill);

        let mouse_options = MouseTrigger::ALL.map(|trigger| (trigger, trigger.name()));
        let mode_options = Mode::ALL.map(|mode| (mode, mode.name()));
        let quiet_options = SilenceStop::ALL.map(|after| (after, after.name()));
        let mut talk = vec![
            self.microphone_line(p),
            Line::plain(named(p, "Sounds", None), switch(chosen.sounds)).press(Message::Sounds(!chosen.sounds)),
        ];
        if crate::media::SUPPORTED {
            talk.push(
                Line::plain(named(p, "Pause music and videos", None), switch(chosen.pause_media))
                    .press(Message::PauseMedia(!chosen.pause_media)),
            );
        }
        column![
            group(
                p,
                vec![
                    Line::custom(hero.into()),
                    Line::plain(
                        named(p, "Mouse button", None),
                        segmented(&mouse_options, chosen.mouse, Message::Mouse)
                    ),
                ]
            ),
            group(
                p,
                vec![
                    Line::plain(
                        named(p, "Mode", Some(chosen.mode.explain())),
                        segmented(&mode_options, chosen.mode, Message::Mode)
                    ),
                    Line::plain(
                        named(p, "Stop after quiet", Some("Hands-free only")),
                        segmented(&quiet_options, chosen.silence_stop, Message::SilenceStop)
                    ),
                ]
            ),
            group(p, talk),
        ]
        .spacing(space::S4)
        .into()
    }

    /// Paste last, under Keep history: its keys with Change and Turn off, or Turn on when it's off.
    fn paste_last_line(&self, p: &Palette) -> Line<'_> {
        let picking = self.capturing.is_some() && self.picking == Picking::PasteLast;
        let (what, control): (&str, Element<'_, Message>) = match self.chosen().paste_last {
            _ if picking => ("Press the new keys.", link(p, "Cancel", Message::KeepShortcut)),
            Some(keys) => (
                if hotkey::paste_last_taken() {
                    "Another app has these keys. Change them."
                } else {
                    "Missed a paste? Puts it in again. After a failed one, tries it again."
                },
                row![
                    // The keys as one small mono chip: full key caps belong to the dictation shortcut.
                    container(text(String::from(keys)).size(size::SMALL).font(theme::MONO).color(p.text))
                        .padding([space::S1, space::S3])
                        .style({
                            let fill = p.elevated;
                            move |_| pill_fill(fill)
                        }),
                    link(p, "Change", Message::ChangePasteLast),
                    link(p, "Turn off", Message::PasteLast(None)),
                ]
                .spacing(space::S3)
                .align_y(Alignment::Center)
                .into(),
            ),
            None => ("Off", link(p, "Turn on", Message::PasteLast(Some(hotkey::PASTE_LAST)))),
        };
        Line::plain(named(p, "Paste last dictation", Some(what)), control)
    }

    /// Both engines, each with its own setup always showing, so picking one
    /// only moves the peach: the edge, the radio and the glyph.
    fn transcription(&self, p: &Palette) -> Element<'_, Message> {
        let picked = self.chosen().engine;
        column![
            label(p, "Transcribe with"),
            column(Engine::ALL.iter().map(|engine| self.engine_panel(p, *engine, *engine == picked)))
                .spacing(space::S3),
            // A caption for the cards, so it sits right under them.
            container(
                text("Your audio goes to the one you pick, and nowhere else.").size(size::SMALL).color(p.text_subtle)
            )
            .padding([0.0, space::S4]),
            Space::new().height(space::S2),
            group(p, vec![self.cleanup_line(p), self.language_line(p)]),
        ]
        .spacing(space::S2)
        .into()
    }

    /// Clean up: ChatGPT tidies each dictation after whichever engine heard it.
    fn cleanup_line(&self, p: &Palette) -> Line<'_> {
        let chosen = self.chosen();
        let mode = match (chosen.cleanup, chosen.styles) {
            (false, _) => CleanupMode::Off,
            (true, false) => CleanupMode::Everywhere,
            (true, true) => CleanupMode::ByApp,
        };
        let what = match (self.allowance, &self.codex, mode) {
            (Some(resets_at), _, _) => crate::allowance::paused_line(resets_at),
            (None, CodexStatus::Connected(_), CleanupMode::ByApp) => {
                "A tone for each app: email, chat, notes, code. Can run out on Free.".into()
            }
            (None, CodexStatus::Connected(_), _) => {
                "Drops ums, keeps corrections, makes lists. Can run out on Free.".into()
            }
            (None, _, _) => "Needs Codex signed in, whichever engine you pick.".into(),
        };
        let options =
            [(CleanupMode::Off, "Off"), (CleanupMode::Everywhere, "Everywhere"), (CleanupMode::ByApp, "By app")];
        Line::plain(named_owned(p, "Clean up", what), segmented(&options, mode, Message::Cleanup))
    }

    /// Language: Auto-detect, or one pinned, with why you'd leave it on auto.
    fn language_line(&self, p: &Palette) -> Line<'_> {
        let auto = LanguageOption { code: None, label: "Auto-detect" };
        let mut options = vec![auto.clone()];
        options.extend(language::ALL.iter().map(|l| LanguageOption { code: Some(l.code), label: l.name }));
        let picked = self
            .chosen()
            .language
            .as_deref()
            .and_then(language::find)
            .map_or(auto, |l| LanguageOption { code: Some(l.code), label: l.name });
        let top = row![
            container(named(p, "Language", None)).width(Length::Fill),
            dropdown(options, Some(picked), Message::Language, None)
        ]
        .spacing(space::S4)
        .align_y(Alignment::Center);
        Line::custom(
            column![
                top,
                text("Auto handles mixed languages. Pick one if likh writes the wrong one.")
                    .size(size::SMALL)
                    .line_height(theme::line_height(size::SMALL))
                    .color(p.text_muted)
            ]
            .spacing(space::S1)
            .into(),
        )
    }

    fn engine_panel<'a>(&'a self, p: &Palette, engine: Engine, picked: bool) -> Element<'a, Message> {
        let (accent, ink, muted, subtle, surface, edge, edge_strong) =
            (p.accent, p.text, p.text_muted, p.text_subtle, p.surface, p.edge, p.edge_strong);
        let radio = container(if picked {
            Element::from(container(Space::new()).width(DOT).height(DOT).style(move |_| pill_fill(accent)))
        } else {
            Space::new().into()
        })
        .center(16)
        .style(move |_| container::Style {
            border: Border { radius: 999.0.into(), width: 1.5, color: if picked { accent } else { subtle } },
            ..container::Style::default()
        });
        let mark =
            canvas_widget(glyph::Glyph { engine, color: if picked { accent } else { muted }, backdrop: surface })
                .width(GLYPH.width)
                .height(GLYPH.height);
        let mut head = row![
            radio,
            mark,
            column![
                text(engine.name()).size(size::BODY).font(theme::UI_MEDIUM),
                text(engine.explain()).size(size::SMALL).color(p.text_muted),
            ]
            .width(Length::Fill),
        ]
        .spacing(space::S3)
        .align_y(Alignment::Center);
        if engine == Engine::Groq && !self.groq.saved {
            head = head.push(link(p, "Get a free key \u{2197}", Message::Open(groq::KEYS_URL)));
        }
        let setup = container(self.engine_setup(p, engine, picked))
            .padding(Padding { left: SETUP_INDENT - space::S4, ..Padding::ZERO });
        let body = column![
            container(head).padding([space::S3, space::S4]),
            hairline(p),
            container(setup).padding([space::S3, space::S4]),
        ];
        button(body)
            .on_press(Message::Engine(engine))
            .padding(1)
            .width(Length::Fill)
            .style(move |_, status| button::Style {
                background: Some(Background::Color(surface)),
                text_color: ink,
                border: Border {
                    radius: radius::CARD.into(),
                    width: if picked { 1.5 } else { 1.0 },
                    color: match (picked, status) {
                        (true, _) => accent,
                        (false, button::Status::Hovered | button::Status::Pressed) => edge_strong,
                        _ => edge,
                    },
                },
                ..button::Style::default()
            })
            .into()
    }

    /// What an engine needs, under its name: the Codex sign-in, or the Groq key.
    fn engine_setup<'a>(&'a self, p: &Palette, engine: Engine, picked: bool) -> Element<'a, Message> {
        let state = |tone: Tone, title: String, detail: String| -> Element<'a, Message> {
            let tint = tone_color(p, tone);
            row![
                container(Space::new()).width(DOT).height(DOT).style(move |_| pill_fill(tint)),
                column![
                    text(title).size(size::SMALL).font(theme::UI_MEDIUM),
                    text(detail).size(size::CAPTION).color(p.text_muted),
                ]
                .width(Length::Fill),
            ]
            .spacing(space::S3)
            .align_y(Alignment::Center)
            .into()
        };
        match engine {
            Engine::Codex => {
                let copy = onboarding::codex_copy(&self.codex);
                let mut line = row![state(copy.tone, copy.title.to_string(), copy.detail)]
                    .spacing(space::S4)
                    .align_y(Alignment::Center);
                if self.codex == CodexStatus::NotInstalled {
                    line = line.push(link(p, "Get Codex \u{2197}", Message::Open(crate::shared::CODEX_SETUP)));
                }
                line.push(link(p, "Check again", Message::Recheck)).into()
            }
            Engine::Groq if self.groq.saved => row![
                state(Tone::Success, "Key saved".into(), "On this computer, in its credential store.".into()),
                link(p, "Use another key", Message::ReplaceGroqKey),
                link(p, "Remove", Message::ForgetGroqKey),
            ]
            .spacing(space::S4)
            .align_y(Alignment::Center)
            .into(),
            Engine::Groq => {
                let field = text_input("Paste your key, gsk_\u{2026}", &self.groq.key)
                    .id(GROQ_KEY)
                    .secure(true)
                    .on_input(Message::GroqKey)
                    .on_paste(Message::GroqKey)
                    .on_submit(Message::SaveGroqKey)
                    .size(size::SMALL)
                    .padding([space::S2, space::S3])
                    .style(|_, status| field_style(status));
                let words = if self.groq.checking { "Checking\u{2026}" } else { "Save key" };
                // Peach only when Groq is the pick: then saving the key is the thing to do.
                row![field, small_button(p, words, Message::SaveGroqKey, picked)]
                    .spacing(space::S3)
                    .align_y(Alignment::Center)
                    .into()
            }
        }
    }

    /// Both kinds of command, one pick above them. Review and New take the whole tab.
    fn commands_tab(&self, p: &Palette) -> Element<'_, Message> {
        if self.commands_view == CommandsView::CommandKey && self.actions.view != actions::View::List {
            return self.actions_tab(p);
        }
        let options =
            [(CommandsView::WhileDictating, "While dictating"), (CommandsView::CommandKey, "With the command key")];
        let pick = segmented(&options, self.commands_view, Message::CommandsView);
        let body = match self.commands_view {
            CommandsView::WhileDictating => self.commands(p),
            CommandsView::CommandKey => self.actions_tab(p),
        };
        column![pick, body].spacing(space::S4).into()
    }

    fn commands(&self, p: &Palette) -> Element<'_, Message> {
        let chosen = self.chosen();
        let master = Line::plain(
            named(p, "Spoken commands", Some("Phrases that do something instead of being typed.")),
            switch(chosen.commands),
        )
        .press(Message::Commands(!chosen.commands));
        if !chosen.commands {
            return column![group(p, vec![master]), hint(p, "Off. Everything you say is typed as you said it.")]
                .spacing(space::S3)
                .into();
        }
        let groups = Group::ALL.map(|group| {
            let on = group.is_on(&chosen.command_groups);
            let phrases = group.phrases();
            // A result every phrase shares ("undo", three times) says nothing, so it's left off.
            let shared = phrases.windows(2).all(|pair| pair[0].1 == pair[1].1) && phrases.len() > 1;
            // What a group does always shows: three chips that all say "undo" never said where.
            let mut left = column![
                text(group.name()).size(size::BODY).font(theme::UI_MEDIUM),
                text(group.explain()).size(size::SMALL).color(p.text_muted),
            ]
            .spacing(space::S1)
            .width(Length::Fill);
            if on {
                let chips = row(phrases.iter().map(|(phrase, result)| {
                    let mut chip = row![said(phrase)].spacing(space::S1).align_y(Alignment::Center);
                    if !shared {
                        chip = chip.push(text(*result).size(size::CAPTION).font(theme::MONO).color(p.text_subtle));
                    }
                    chip.into()
                }))
                .spacing(space::S3)
                .wrap()
                .vertical_spacing(space::S2);
                left = left.push(Space::new().height(space::S1)).push(chips);
            }
            Line::plain(left.into(), switch(on)).press(Message::CommandGroup(group, !on))
        });
        // The master switch heads the same panel as the groups it turns on.
        group(p, std::iter::once(master).chain(groups).collect())
    }

    /// The words likh should spell your way, and whether ChatGPT puts them right after transcribing.
    fn dictionary(&self, p: &Palette) -> Element<'_, Message> {
        let chosen = self.chosen();
        let field = text_input("A name likh gets wrong, like a project or a person", &self.new_word)
            .id(NEW_WORD)
            .on_input(Message::NewWord)
            .on_paste(Message::NewWord)
            .on_submit(Message::AddWord)
            .size(size::SMALL)
            .padding([space::S2, space::S3])
            .style(|_, status| field_style(status));
        let add = row![field, small_button(p, "Add", Message::AddWord, !self.new_word.trim().is_empty())]
            .spacing(space::S3)
            .align_y(Alignment::Center);
        let words: Element<'_, Message> = if chosen.dictionary.is_empty() {
            text("Nothing yet. Add the names it mishears: projects, people, places.")
                .size(size::SMALL)
                .color(p.text_muted)
                .into()
        } else {
            row(chosen.dictionary.iter().enumerate().map(|(index, word)| word_chip(p, word.clone(), index)))
                .spacing(space::S2)
                .wrap()
                .vertical_spacing(space::S2)
                .into()
        };
        let fix_detail = match (self.allowance, &self.codex) {
            (Some(resets_at), _) => crate::allowance::paused_line(resets_at),
            (None, CodexStatus::Connected(_)) => {
                "ChatGPT reads each dictation and puts your words in. Adds about a second.".into()
            }
            (None, _) => "Needs Codex signed in. Groq still hears your words as it listens.".into(),
        };
        column![
            group(
                p,
                vec![
                    Line::custom(column![add, words].spacing(space::S3).into()),
                    Line::plain(named_owned(p, "Fix misheard names", fix_detail), switch(chosen.fix_names))
                        .press(Message::FixNames(!chosen.fix_names)),
                ]
            ),
            hint(p, "Click a word to remove it. Your words go out only with a dictation: to Groq as it listens, and to ChatGPT to fix names."),
        ]
        .spacing(space::S3)
        .into()
    }

    fn history_list(&self, p: &Palette) -> Element<'_, Message> {
        let chosen = self.chosen();
        let keep = group(
            p,
            vec![
                Line::plain(
                    named(p, "Keep history", Some("On this computer only. Click a dictation to copy it.")),
                    switch(chosen.history),
                )
                .press(Message::KeepHistory(!chosen.history)),
                self.paste_last_line(p),
            ],
        );
        if self.history.is_empty() {
            let why = if chosen.history {
                format!("Hold {} and talk. What you say shows up here, and only here.", chosen.shortcut)
            } else {
                "History is off, so nothing you dictate is kept.".into()
            };
            return column![
                keep,
                column![
                    container(text("Nothing yet").size(size::BODY).font(theme::UI_MEDIUM)).padding([0.0, space::S4]),
                    hint(p, &why)
                ]
                .spacing(space::S1)
            ]
            .spacing(space::S5)
            .into();
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let count = match self.history.len() {
            1 => "1 dictation, newest first. Click one to copy it.".to_string(),
            n if n >= HISTORY_SHOWN => format!("The latest {n}, newest first. Click one to copy it."),
            n => format!("{n} dictations, newest first. Click one to copy it."),
        };
        let clear = if self.confirm_clear { "Click again to clear" } else { "Clear" };
        let top = row![label(p, &count), Space::new().width(Length::Fill), link(p, clear, Message::Clear)]
            .padding(Padding { right: space::S4, ..Padding::ZERO })
            .align_y(Alignment::Center);
        let rows = self.history.iter().enumerate().map(|(i, entry)| self.entry(p, i, entry, now)).collect();
        column![keep, self.stats_figures(p), column![top, group(p, rows)].spacing(space::S2)].spacing(space::S4).into()
    }

    /// Words, speed and streak across everything kept, and the time it saved over typing.
    fn stats_figures(&self, p: &Palette) -> Element<'_, Message> {
        let stats = self.stats;
        let (ink, subtle) = (p.text, p.text_subtle);
        let figure = |value: String, what: &'static str| -> Element<'_, Message> {
            column![
                text(value).size(size::TITLE).font(theme::UI_MEDIUM).color(ink),
                text(what).size(size::CAPTION).color(subtle),
            ]
            .spacing(space::S1)
            .width(Length::Fill)
            .into()
        };
        let days = if stats.streak == 1 { "day in a row" } else { "days in a row" };
        let figures = row![
            figure(thousands(stats.words), "words"),
            figure(thousands(stats.words_this_week), "in the last 7 days"),
            figure(stats.per_minute.map_or("\u{2014}".into(), |n| n.to_string()), "words a minute"),
            figure(stats.streak.to_string(), days),
        ]
        .spacing(space::S3);
        let mut block = column![group(p, vec![Line::custom(figures.into())])].spacing(space::S2);
        if stats.minutes_saved > 0 {
            let saved = format!(
                "About {} faster than typing it at {:.0} words a minute.",
                running_for(u64::from(stats.minutes_saved) * 60),
                history::TYPING_PER_MINUTE
            );
            block = block.push(hint(p, &saved));
        }
        block.into()
    }

    fn entry<'a>(&self, p: &Palette, index: usize, entry: &'a Entry, now: u64) -> Line<'a> {
        let preview: String = if entry.text.chars().count() > PREVIEW_CHARS {
            format!("{}…", entry.text.chars().take(PREVIEW_CHARS).collect::<String>().trim_end())
        } else {
            entry.text.clone()
        };
        let mut meta = format!("{} · {:.1} s", history::ago(entry.at, now), entry.seconds);
        // An action's line says which action ran; its text is what was said, or what it pasted.
        if let Some(action) = &entry.action {
            meta.push_str(&format!(" · {action}"));
        }
        if self.copied == Some(index) {
            meta.push_str(" · Copied");
        }
        let copied = self.copied == Some(index);
        Line::custom(
            column![
                text(preview).size(size::BODY).line_height(theme::line_height(size::BODY)),
                text(meta).size(size::CAPTION).color(if copied { p.success } else { p.text_subtle }),
            ]
            .spacing(space::S1)
            .width(Length::Fill)
            .into(),
        )
        .press(Message::Copy(index))
    }

    fn app(&self, p: &Palette) -> Element<'_, Message> {
        let chosen = self.chosen();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let this = format!("This is likh {}.", env!("CARGO_PKG_VERSION"));
        let (status, action): (String, Option<(&'static str, Message)>) = match self.updates.state() {
            update::State::Idle => (this, None),
            update::State::Checking => ("Checking for a newer likh\u{2026}".into(), None),
            update::State::UpToDate { checked_at } => {
                (format!("{this} It's the newest, checked {}.", history::ago(checked_at, now)), None)
            }
            update::State::Downloading { version } => (format!("Downloading likh {version}\u{2026}"), None),
            update::State::Ready { version, .. } => {
                (format!("likh {version} is downloaded and checked."), Some(("Restart to update", Message::Restart)))
            }
            update::State::Available { version, page } => {
                (format!("likh {version} is out."), Some(("Download it", Message::OpenRelease(page))))
            }
            update::State::Failed { reason } => (reason, None),
        };
        let updates: Element<'_, Message> = match action {
            Some((words, message)) => link(p, words, message),
            None => Space::new().into(),
        };
        let what = if update::INSTALLS {
            "Once a day. You choose when it restarts."
        } else {
            "Once a day. On macOS you download it yourself."
        };
        let dictation =
            self.dictation.lock().map(|s| s.clone()).unwrap_or_else(|poisoned| poisoned.into_inner().clone());
        let session = match (dictation.dictations, &dictation.last) {
            (0, _) if dictation.actions == 1 => "No dictations since likh started, 1 action.".to_string(),
            (0, _) if dictation.actions > 1 => {
                format!("No dictations since likh started, {} actions.", dictation.actions)
            }
            (0, _) => "No dictations since likh started.".to_string(),
            (n, Some(last)) if last.error.is_none() => {
                format!("{n} since likh started. The last one pasted {} ms after you let go.", last.latency_ms)
            }
            (n, _) => format!("{n} since likh started."),
        };
        // The links are words in one line under the panel: three rows of their own made the tab scroll.
        let links = column![
            row![
                link(p, "Website \u{2197}", Message::Open(crate::shared::SITE)),
                link(p, "Source on GitHub \u{2197}", Message::Open(REPO)),
                link(p, "Run setup again", Message::RunSetup),
            ]
            .spacing(space::S5),
            text("Free and open source, MIT.").size(size::SMALL).color(p.text_subtle),
        ]
        .spacing(space::S2)
        .padding([0.0, space::S4]);
        column![
            group(
                p,
                vec![
                    Line::plain(named(p, crate::autostart::NAME_FOR_PEOPLE, None), switch(chosen.start_with_windows))
                        .press(Message::StartWithWindows(!chosen.start_with_windows)),
                    Line::plain(named(p, "Check for updates automatically", Some(what)), switch(chosen.auto_update))
                        .press(Message::AutoUpdate(!chosen.auto_update)),
                    Line::plain(named_owned(p, "Updates", status), updates),
                    Line::plain(named_owned(p, "This session", session), Space::new().into()),
                ]
            ),
            links,
        ]
        .spacing(space::S4)
        .into()
    }

    /// likh's memory and CPU now, and what the trace says went wrong or slow:
    /// the same lists as T3 Code's Diagnostics page.
    fn diagnostics(&self, p: &Palette) -> Element<'_, Message> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let (memory, cpu, running) = match self.usage {
            Some(usage) => (
                format!("{:.0} MB", usage.memory_bytes as f64 / (1024.0 * 1024.0)),
                usage.cpu_percent.map_or("Measuring\u{2026}".into(), |cpu| format!("{cpu:.1}%")),
                running_for(usage.running_seconds),
            ),
            None => ("\u{2014}".into(), "\u{2014}".into(), "\u{2014}".into()),
        };
        let figure = |value: String| -> Element<'_, Message> {
            text(value).size(size::SMALL).font(theme::MONO).color(p.text).into()
        };
        let process = group(
            p,
            vec![
                Line::plain(named(p, "Memory", None), figure(memory)),
                Line::plain(named(p, "CPU", Some("A share of the whole computer.")), figure(cpu)),
                Line::plain(named(p, "Running for", None), figure(running)),
            ],
        );
        let actions = row![
            link(p, "Refresh", Message::Rediagnose),
            link(p, "Copy report", Message::CopyReport),
            link(p, "Open logs folder", Message::OpenLogs),
        ]
        .spacing(space::S5)
        .padding([0.0, space::S4]);

        let Some(report) = &self.report else {
            return column![process, hint(p, "Reading the log\u{2026}")].spacing(space::S4).into();
        };
        let summary = match (self.files.logs.is_some(), report.first_at) {
            (false, _) => "likh has no folder to keep a log in.".to_string(),
            (true, None) => "Nothing logged yet. Each dictation adds its steps here.".to_string(),
            (true, Some(first)) => format!(
                "{} lines, the first {}. {} failed, {} took over {}.",
                report.records,
                ago_ms(first, now),
                report.failures,
                report.slow,
                duration(report.slow_ms),
            ),
        };
        let mut page = column![process, column![hint(p, &summary), actions].spacing(space::S2)].spacing(space::S4);

        let failures: Vec<Line<'_>> = report
            .common_failures
            .iter()
            .map(|failure| {
                let meta = format!(
                    "{} \u{b7} {}\u{d7} \u{b7} last {}",
                    failure.name,
                    failure.count,
                    ago_ms(failure.last_at, now)
                );
                entry_line(p, failure.cause.clone(), meta, p.text_subtle, None)
            })
            .collect();
        page = page.push(section(p, "What failed", failures, "Nothing has failed."));

        let slowest: Vec<Line<'_>> = report
            .slowest
            .iter()
            .map(|span| {
                let meta = match &span.cause {
                    Some(cause) => format!("{} \u{b7} failed: {cause}", ago_ms(span.at, now)),
                    None => ago_ms(span.at, now),
                };
                entry_line(p, span.name.clone(), meta, p.text_subtle, Some(duration(span.duration_ms)))
            })
            .collect();
        page = page.push(section(p, "Slowest", slowest, "Nothing timed yet."));

        let busiest: Vec<Line<'_>> = report
            .busiest
            .iter()
            .map(|step| {
                let meta = match step.failures {
                    0 => format!("average {} \u{b7} longest {}", duration(step.average_ms), duration(step.max_ms)),
                    n => format!(
                        "average {} \u{b7} longest {} \u{b7} {n} failed",
                        duration(step.average_ms),
                        duration(step.max_ms)
                    ),
                };
                entry_line(p, step.name.clone(), meta, p.text_subtle, Some(format!("{}\u{d7}", step.count)))
            })
            .collect();
        page = page.push(section(p, "Most often", busiest, "Nothing timed yet."));

        let logs: Vec<Line<'_>> = report
            .latest_logs
            .iter()
            .map(|line| {
                let tint = if line.level == "error" { p.danger } else { p.warning };
                let meta = format!("{} \u{b7} {} \u{b7} {}", line.level, line.source, ago_ms(line.at, now));
                entry_line(p, line.message.clone(), meta, tint, None)
            })
            .collect();
        page.push(section(p, "Warnings and errors", logs, "None.")).into()
    }
}

/// A heading over a panel of rows, or a quiet line when there are none.
fn section<'a>(p: &Palette, heading: &str, lines: Vec<Line<'a>>, empty: &str) -> Element<'a, Message> {
    let body = if lines.is_empty() { hint(p, empty) } else { group(p, lines) };
    column![label(p, heading), body].spacing(space::S2).into()
}

/// One row of a Diagnostics list: what happened, a line on when and how
/// often, and a figure on the right when there is one.
fn entry_line<'a>(p: &Palette, title: String, meta: String, tint: Color, figure: Option<String>) -> Line<'a> {
    let words = column![
        text(title).size(size::BODY).line_height(theme::line_height(size::BODY)),
        text(meta).size(size::CAPTION).color(tint),
    ]
    .spacing(space::S1)
    .width(Length::Fill);
    match figure {
        Some(figure) => {
            Line::plain(words.into(), text(figure).size(size::SMALL).font(theme::MONO).color(p.text).into())
        }
        None => Line::custom(words.into()),
    }
}

/// "640 ms", or "3.1 s" once it's over a second.
fn duration(ms: f64) -> String {
    if ms < 1000.0 { format!("{ms:.0} ms") } else { format!("{:.1} s", ms / 1000.0) }
}

/// `history::ago` for a time in milliseconds.
fn ago_ms(at_ms: u64, now: u64) -> String {
    history::ago(at_ms / 1000, now)
}

/// "12,480": digits in groups of three, the way people read counts.
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut grouped = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// "45 s", "12 min", "3 h 5 min".
fn running_for(seconds: u64) -> String {
    match seconds {
        0..60 => format!("{seconds} s"),
        60..3600 => format!("{} min", seconds / 60),
        _ => format!("{} h {} min", seconds / 3600, seconds % 3600 / 60),
    }
}

/// One row of a panel: what it shows, and what pressing it does, if anything.
struct Line<'a> {
    body: Element<'a, Message>,
    press: Option<Message>,
}

impl<'a> Line<'a> {
    /// A name on the left, a control on the right.
    fn plain(left: Element<'a, Message>, control: Element<'a, Message>) -> Self {
        let body = row![container(left).width(Length::Fill), control].spacing(space::S4).align_y(Alignment::Center);
        Self { body: body.into(), press: None }
    }

    fn custom(body: Element<'a, Message>) -> Self {
        Self { body, press: None }
    }

    /// The whole row is the button, so the name and the words under it press it too.
    fn press(mut self, message: Message) -> Self {
        self.press = Some(message);
        self
    }
}

/// A panel of rows with hairlines between them. Rows that press light up
/// under the pointer, rounded only where they meet the panel's corners.
fn group<'a>(p: &Palette, lines: Vec<Line<'a>>) -> Element<'a, Message> {
    let (hover, pressed, surface, edge, ink) = (p.hover, p.pressed, p.surface, p.edge, p.text);
    let count = lines.len();
    let mut rows = column![];
    for (i, line) in lines.into_iter().enumerate() {
        if i > 0 {
            rows = rows.push(hairline(p));
        }
        let inner = radius::CARD - 1.0;
        let corners = border::Radius {
            top_left: if i == 0 { inner } else { 0.0 },
            top_right: if i == 0 { inner } else { 0.0 },
            bottom_right: if i + 1 == count { inner } else { 0.0 },
            bottom_left: if i + 1 == count { inner } else { 0.0 },
        };
        let padded = container(line.body).padding([space::S3, space::S4]).width(Length::Fill);
        rows = rows.push(match line.press {
            Some(message) => Element::from(button(padded).on_press(message).padding(0).width(Length::Fill).style(
                move |_, status| button::Style {
                    background: match status {
                        button::Status::Hovered => Some(Background::Color(hover)),
                        button::Status::Pressed => Some(Background::Color(pressed)),
                        _ => None,
                    },
                    text_color: ink,
                    border: Border { radius: corners, ..Border::default() },
                    ..button::Style::default()
                },
            )),
            None => padded.into(),
        });
    }
    // One pixel in from the edge, so a lit row never covers the panel's hairline.
    container(rows)
        .padding(1)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(surface)),
            border: Border { radius: radius::CARD.into(), width: 1.0, color: edge },
            ..container::Style::default()
        })
        .into()
}

fn hairline<'a>(p: &Palette) -> Element<'a, Message> {
    let edge = p.edge;
    container(Space::new())
        .width(Length::Fill)
        .height(1)
        .style(move |_| container::Style { background: Some(Background::Color(edge)), ..container::Style::default() })
        .into()
}

/// A setting's name, and a line on what it does when the name can't say it alone.
fn named<'a>(p: &Palette, name: &'a str, what: Option<&'a str>) -> Element<'a, Message> {
    let mut words = column![text(name).size(size::BODY).font(theme::UI_MEDIUM)];
    if let Some(what) = what {
        words =
            words.push(text(what).size(size::SMALL).line_height(theme::line_height(size::SMALL)).color(p.text_muted));
    }
    words.into()
}

fn named_owned<'a>(p: &Palette, name: &'a str, what: String) -> Element<'a, Message> {
    column![
        text(name).size(size::BODY).font(theme::UI_MEDIUM),
        text(what).size(size::SMALL).line_height(theme::line_height(size::SMALL)).color(p.text_muted),
    ]
    .into()
}

/// A small heading over a panel, on the panel's text edge.
fn label<'a>(p: &Palette, words: &str) -> Element<'a, Message> {
    container(text(words.to_string()).size(size::CAPTION).font(theme::UI_MEDIUM).color(p.text_muted))
        .padding([0.0, space::S4])
        .into()
}

/// A line of quieter words, on the panels' text edge.
fn hint<'a>(p: &Palette, words: &str) -> Element<'a, Message> {
    container(
        text(words.to_string()).size(size::SMALL).line_height(theme::line_height(size::SMALL)).color(p.text_muted),
    )
    .padding([0.0, space::S4])
    .into()
}

/// Words that do something: muted until pointed at.
fn link<'a>(p: &Palette, words: &'a str, message: Message) -> Element<'a, Message> {
    let (rest, lit) = (p.text_muted, p.text);
    button(text(words).size(size::SMALL).font(theme::UI_MEDIUM))
        .on_press(message)
        .padding(0)
        .style(move |_, status| button::Style {
            text_color: if matches!(status, button::Status::Hovered | button::Status::Pressed) { lit } else { rest },
            ..button::Style::default()
        })
        .into()
}

/// One dictionary word as a capsule with a ×. Pressing it removes the word.
fn word_chip<'a>(p: &Palette, word: String, index: usize) -> Element<'a, Message> {
    let (fill, edge, edge_strong, ink, cross) = (p.surface, p.edge, p.edge_strong, p.text, p.text_subtle);
    button(
        row![text(word).size(size::SMALL).font(theme::UI_MEDIUM), text("\u{00d7}").size(size::SMALL).color(cross)]
            .spacing(space::S2)
            .align_y(Alignment::Center),
    )
    .on_press(Message::RemoveWord(index))
    .padding([space::S1, space::S3])
    .style(move |_, status| button::Style {
        background: Some(Background::Color(fill)),
        text_color: ink,
        border: Border {
            radius: 999.0.into(),
            width: 1.0,
            color: if matches!(status, button::Status::Hovered | button::Status::Pressed) { edge_strong } else { edge },
        },
        ..button::Style::default()
    })
    .into()
}

/// A button that sits beside a field: peach when it's the screen's main action, neutral otherwise.
fn small_button<'a>(p: &Palette, words: &'a str, message: Message, peach: bool) -> Element<'a, Message> {
    let (fill, hover, pressed, ink) = if peach {
        (p.accent, p.accent_hover, p.accent_pressed, p.on_accent)
    } else {
        (p.elevated, p.elevated, p.elevated, p.text)
    };
    let edge = p.edge_strong;
    button(text(words).size(size::SMALL).font(theme::UI_MEDIUM).center())
        .on_press(message)
        .padding([space::S2, space::S4])
        .style(move |_, status| {
            let pointed = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(match status {
                    button::Status::Hovered => hover,
                    button::Status::Pressed => pressed,
                    _ => fill,
                })),
                text_color: ink,
                border: Border {
                    radius: radius::CONTROL.into(),
                    // The neutral one has no lighter fill to hover to, so its edge lights instead.
                    width: if pointed && !peach { 1.0 } else { 0.0 },
                    color: edge,
                },
                ..button::Style::default()
            }
        })
        .into()
}

/// Tab went down: Some(true) with Shift held.
fn tab(event: &keyboard::Event) -> Option<bool> {
    match event {
        keyboard::Event::KeyPressed {
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Tab),
            modifiers,
            ..
        } => Some(modifiers.shift()),
        _ => None,
    }
}

fn is_escape(event: &keyboard::Event) -> bool {
    matches!(
        event,
        keyboard::Event::KeyPressed { physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Escape), .. }
    )
}

fn tone_color(p: &Palette, tone: Tone) -> Color {
    match tone {
        Tone::Success => p.success,
        Tone::Warning => p.warning,
        Tone::Danger => p.danger,
        Tone::Neutral => p.text_muted,
    }
}

/// A band that fades the scrolling content into the window's background.
fn fade_edge<'a>(bg: Color, height: f32, top: bool) -> Element<'a, Message> {
    // The software renderer rounds gradient colours brighter than flat fills,
    // which left the band one level lighter than the window around it: a faint
    // box. A step down lands it back on `bg`.
    let step = 1.0 / 255.0;
    let bg = Color { r: bg.r - step, g: bg.g - step, b: bg.b - step, ..bg };
    let clear = Color { a: 0.0, ..bg };
    let (from, to) = if top { (bg, clear) } else { (clear, bg) };
    // 180° runs top to bottom.
    let band = gradient::Linear::new(iced::Degrees(180.0)).add_stop(0.0, from).add_stop(1.0, to);
    container(Space::new())
        .width(Length::Fill)
        .height(height)
        .style(move |_| container::Style {
            background: Some(Background::Gradient(band.into())),
            ..container::Style::default()
        })
        .into()
}

/// The shortcut as people say it, for the tray menu and tooltip, or that another app has it.
pub fn hold_to_talk(binding: Binding) -> String {
    if hotkey::shortcut_taken() && cfg!(target_os = "macos") {
        format!("Allow Accessibility for likh to use {binding}")
    } else if hotkey::shortcut_taken() {
        format!("Another app has {binding}. Open likh to change it.")
    } else {
        format!("Hold {binding} to talk")
    }
}
