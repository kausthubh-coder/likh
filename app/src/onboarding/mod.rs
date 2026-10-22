//! First-run window: a fixed 560 x 640 borderless window with five steps
//! (welcome, Codex, microphone, shortcut, done). Inside the running app the
//! shortcut step is a real first dictation: hold the shortcut and say
//! "continue", and the step moves on by itself once it hears it. The shortcut
//! can be changed right there by pressing a new one.
//!
//! No title bar: drag it from anywhere empty, and Alt+F4 still closes it.
//!
//! Idle draws nothing: frames are requested only while something moves, the
//! Codex check runs only on its step, and the mic is open only on its step.

mod flock;

use crate::codex_auth::{self, CodexStatus};
use crate::dictation::{Phase, Status};
use crate::hotkey::{self, Binding, capture};
use crate::microphone;
use crate::settings::Settings;
use crate::shared::icon;
use crate::shared::spring::{self, Spring};
use crate::shared::theme::{self, Palette, palette, radius, size, space};
use crate::shared::ui::{fade, field_style, ghost, keycap, mix, pill_fill, primary, segmented};
use crate::transcriber::{Engine, Failure, glyph, groq};
use flock::{Flock, Shape};
use iced::widget::canvas::{self, Frame, Path, Stroke};
use iced::widget::{Space, canvas as canvas_widget, column, container, mouse_area, operation, row, text, text_input};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Point, Rectangle, Renderer, Size, Subscription, Task, Theme,
    keyboard, mouse, time, window,
};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(test)]
mod tests;

pub const WINDOW: Size = Size::new(560.0, 640.0);

/// When the welcome text starts to arrive, in seconds after the window opens:
/// as the last birds land, so the logo and the words feel like one motion.
const TEXT_FROM: f32 = 0.85;
/// The gap between title, body and button arriving (Stagger: 30 to 60ms).
const STAGGER: f32 = 0.05;
/// The text rises this far as it fades in.
const RISE: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Welcome,
    Connect,
    Microphone,
    Shortcut,
    Done,
}

const STEPS: [Step; 5] = [Step::Welcome, Step::Connect, Step::Microphone, Step::Shortcut, Step::Done];

impl Step {
    fn index(self) -> usize {
        STEPS.iter().position(|s| *s == self).unwrap_or_default()
    }
}

/// The level bars on the microphone step: the newest level in the middle,
/// older ones spreading outwards, so speech ripples from the centre.
const MIC_HISTORY: usize = 5;

#[derive(Default)]
struct Mic {
    device: Option<String>,
    failure: Option<String>,
    history: [f32; MIC_HISTORY],
}

/// The practice dictation on the shortcut step.
#[derive(Default)]
struct Practice {
    /// What has landed in the box so far.
    words: String,
    /// The shortcut is held (or hands-free) right now.
    listening: bool,
    /// Heard "continue"; the step moves on at this moment.
    advance_at: Option<Instant>,
    /// Picking a new shortcut: the modifiers held so far.
    capturing: Option<keyboard::Modifiers>,
    /// What the last change said, good or bad.
    notice: Option<(String, Tone)>,
}

/// Setting up Groq: the key being typed, and how saving it went.
#[derive(Default)]
pub struct GroqSetup {
    pub key: String,
    /// A key is in the credential store.
    pub saved: bool,
    /// Asking Groq whether the key works.
    pub checking: bool,
    pub notice: Option<(String, Tone)>,
}

impl GroqSetup {
    /// What's saved: whether there's a key (never its value).
    pub fn look() -> Self {
        Self { saved: groq::has_key(), ..Self::default() }
    }

    /// Starts saving the typed key: refuses one that can't be a Groq key,
    /// otherwise asks Groq about it. The answer comes back through `on_checked`.
    pub fn save<M: Send + 'static>(&mut self, on_checked: fn(String, Option<Result<(), Failure>>) -> M) -> Task<M> {
        let key = self.key.trim().to_string();
        if !groq::looks_like_a_key(&key) {
            self.notice = Some(("That isn't a Groq key. Groq keys start with gsk_.".into(), Tone::Danger));
            return Task::none();
        }
        self.checking = true;
        self.notice = None;
        let asked = key.clone();
        Task::perform(crate::shared::off_thread(move || groq::verify(&asked)), move |answer| {
            on_checked(key.clone(), answer)
        })
    }

    /// Groq answered about `key`. Keeps it unless Groq turned it down.
    pub fn checked(&mut self, key: &str, answer: Option<Result<(), Failure>>) {
        self.checking = false;
        let keep = match answer {
            Some(Ok(())) => {
                self.notice = Some(("Saved. Groq is ready.".into(), Tone::Success));
                true
            }
            Some(Err(Failure::GroqRejected)) => {
                self.notice = Some(("Groq turned down that key. Copy it again from Groq.".into(), Tone::Danger));
                false
            }
            // Groq didn't answer: keep the key; the first dictation is its real test.
            _ => {
                self.notice = Some(("Saved, but Groq didn't answer, so it isn't checked yet.".into(), Tone::Warning));
                true
            }
        };
        if keep {
            match groq::save_key(key) {
                Ok(()) => {
                    self.saved = true;
                    self.key.clear();
                }
                Err(reason) => self.notice = Some((format!("likh {reason}"), Tone::Danger)),
            }
        }
    }

    pub fn forget(&mut self) {
        match groq::forget_key() {
            Ok(()) => {
                self.saved = false;
                self.notice = None;
            }
            Err(reason) => self.notice = Some((format!("likh {reason}"), Tone::Danger)),
        }
    }
}

/// The words for each engine's state, beside the picker and in settings.
pub fn engine_copy(engine: Engine, codex: &CodexStatus, groq: &GroqSetup) -> StateCopy {
    match engine {
        Engine::Codex => codex_copy(codex),
        Engine::Groq if groq.saved => StateCopy {
            title: "Groq is ready",
            detail: "Your key is saved on this computer, in its credential store.".into(),
            tone: Tone::Success,
        },
        Engine::Groq => StateCopy {
            title: "Add a free Groq key",
            detail: "Make one at console.groq.com, then paste it here.".into(),
            tone: Tone::Warning,
        },
    }
}

const PRACTICE: &str = "practice";
/// The Groq key field, so Save can hand focus back to it.
pub const GROQ_KEY: &str = "groq-key";
/// A body-size field with `space::S3` padding: 22 + 2 x 12, plus its 1px edges.
const KEY_FIELD_HEIGHT: f32 = 48.0;
/// The widest a step's body runs.
const BODY_WIDTH: f32 = 320.0;
/// Saying this word in the practice box moves onboarding on.
const MAGIC_WORD: &str = "continue";
/// Long enough to read "Heard you" before the step slides away.
const ADVANCE_AFTER: Duration = Duration::from_millis(1200);

pub struct Onboarding {
    /// The window, once iced reports it; needed to move it by dragging.
    window: Option<window::Id>,
    /// The running app's dictation and settings, when onboarding opens inside it.
    dictation: Option<Arc<Mutex<Status>>>,
    settings: Option<Arc<RwLock<Settings>>>,
    practice: Practice,
    step: Step,
    finished: bool,
    /// The engine picked on the connect step; saved to settings inside the app.
    engine: Engine,
    codex: CodexStatus,
    groq: GroqSetup,
    /// 0 → 1 as a step arrives. `direction` is +1 going forward, -1 going back.
    arrival: Spring,
    direction: f32,
    /// Which progress dot is wide; it slides between dots instead of jumping.
    progress: Spring,
    /// Seconds since this step's art started; drives the flock and the stagger.
    clock: f32,
    last_frame: Option<Instant>,
    mic: Mic,
    /// Windows' "Show animations" is off: steps fade instead of sliding, and the flock is already home.
    still: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Next,
    Back,
    Recheck,
    GetCodex,
    Engine(Engine),
    GroqKey(String),
    SaveGroqKey,
    GroqChecked(String, Option<Result<(), Failure>>),
    ReplaceGroqKey,
    GetGroqKey,
    OpenMicSettings,
    /// macOS: the Accessibility list, so likh can hear its keys.
    OpenKeySettings,
    Finish,
    Frame(Instant),
    Mic(microphone::Event),
    /// A key while picking a new shortcut.
    Key(keyboard::Event),
    /// Look at the running dictation (only on the shortcut step, inside the app).
    Watch,
    Practice(String),
    ChangeShortcut,
    KeepShortcut,
    WindowFound(Option<window::Id>),
    Drag,
}

impl Onboarding {
    pub fn new(codex: CodexStatus) -> Self {
        Self {
            window: None,
            dictation: None,
            settings: None,
            practice: Practice::default(),
            step: Step::Welcome,
            finished: false,
            engine: Engine::default(),
            codex,
            groq: GroqSetup::default(),
            arrival: Spring::new(spring::DEFAULT, 1.0),
            direction: 1.0,
            progress: Spring::new(spring::DEFAULT, 0.0),
            clock: 0.0,
            last_frame: None,
            mic: Mic::default(),
            // Moving, unless the window that shows it says otherwise: tests stay the same on any machine.
            still: false,
        }
    }

    /// Windows' "Show animations" is off: steps fade and the flock is already home.
    pub fn hold_still(self, still: bool) -> Self {
        Self { still, ..self }
    }

    fn boot() -> (Self, Task<Message>) {
        let app = Self { engine: Settings::load().engine, groq: GroqSetup::look(), ..Self::new(check_codex_now()) }
            .hold_still(crate::pill::animations_off());
        (app, window::oldest().map(Message::WindowFound))
    }

    /// Onboarding inside the running app, in a window the tray opened.
    pub fn in_app(window: window::Id, dictation: Arc<Mutex<Status>>, settings: Arc<RwLock<Settings>>) -> Self {
        let engine = settings.read().map(|s| s.engine).unwrap_or_default();
        Self {
            window: Some(window),
            dictation: Some(dictation),
            settings: Some(settings),
            engine,
            groq: GroqSetup::look(),
            ..Self::new(check_codex_now())
        }
    }

    /// Saves the picked engine: into the running app's settings, or the file when on its own.
    fn save_engine(&mut self, engine: Engine) {
        self.engine = engine;
        let saved = match &self.settings {
            Some(settings) => {
                let mut chosen = settings.write().unwrap_or_else(|poisoned| poisoned.into_inner());
                chosen.engine = engine;
                chosen.save()
            }
            // `likh onboarding` and tests: nothing running to tell.
            None => Ok(()),
        };
        if let Err(reason) = saved {
            crate::diagnostics::warn("onboarding", format!("couldn't save the engine: {reason}"));
        }
    }

    fn shortcut(&self) -> Binding {
        self.settings.as_ref().and_then(|s| s.read().ok().map(|s| s.shortcut)).unwrap_or_default()
    }

    #[cfg(test)]
    pub fn step(&self) -> Step {
        self.step
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Next => return self.go(STEPS.get(self.step.index() + 1).copied().unwrap_or(Step::Done)),
            Message::Back => return self.go(STEPS[self.step.index().saturating_sub(1)]),
            Message::Recheck => match self.engine {
                Engine::Codex => self.codex = check_codex_now(),
                Engine::Groq => {}
            },
            Message::GetCodex => crate::shared::open_url(crate::shared::CODEX_SETUP),
            // Groq without a key would fail the first dictation: it's shown, and saved once a key is.
            Message::Engine(Engine::Groq) if !self.groq.saved => {
                self.engine = Engine::Groq;
                return operation::focus(GROQ_KEY);
            }
            Message::Engine(engine) => self.save_engine(engine),
            Message::GroqKey(key) => {
                self.groq.key = key;
                self.groq.notice = None;
            }
            Message::SaveGroqKey if self.groq.checking => {}
            Message::SaveGroqKey => return self.groq.save(Message::GroqChecked),
            Message::GroqChecked(key, answer) => {
                self.groq.checked(&key, answer);
                if self.groq.saved && self.engine == Engine::Groq {
                    self.save_engine(Engine::Groq);
                }
            }
            Message::ReplaceGroqKey => {
                self.groq.saved = false;
                self.groq.notice = None;
                return operation::focus(GROQ_KEY);
            }
            Message::GetGroqKey => crate::shared::open_url(groq::KEYS_URL),
            Message::OpenMicSettings => crate::shared::open_url(crate::shared::MIC_SETTINGS),
            Message::OpenKeySettings => crate::shared::open_url(crate::shared::ACCESSIBILITY_SETTINGS),
            // Closing the window ends `likh onboarding`; inside the app, likh stays in the tray.
            Message::Finish => {
                self.finished = true;
                return self.window.map_or_else(iced::exit, window::close);
            }
            Message::WindowFound(id) => self.window = id,
            Message::Drag => return self.window.map_or_else(Task::none, window::drag),
            Message::Frame(now) => {
                let dt = self.last_frame.map_or(1.0 / 60.0, |last| (now - last).as_secs_f32().min(0.05));
                self.clock += dt;
                let arrived = self.arrival.step(dt);
                let progressed = self.progress.step(dt);
                self.last_frame = if arrived && progressed && !self.art_is_moving() { None } else { Some(now) };
            }
            Message::Mic(event) => match event {
                microphone::Event::Opened { device } => {
                    self.mic.device = Some(device);
                    self.mic.failure = None;
                }
                microphone::Event::Failed(reason) => self.mic.failure = Some(reason),
                microphone::Event::Level(level) => {
                    // Attack fast, release slower (BRAND.md, Level bars), at one level per 30ms.
                    let last = self.mic.history[0];
                    let rate = if level > last { 0.75 } else { 0.2 };
                    self.mic.history.rotate_right(1);
                    self.mic.history[0] = last + (level - last) * rate;
                }
            },
            Message::Key(event) => {
                let Some(held) = self.practice.capturing else { return Task::none() };
                match capture::on_event(&event, held) {
                    capture::Outcome::Waiting(held) => self.practice.capturing = Some(held),
                    capture::Outcome::Cancelled => return self.stop_capturing(None),
                    capture::Outcome::Refused(why) => self.practice.notice = Some((why.into(), Tone::Danger)),
                    capture::Outcome::Chosen(binding) => match hotkey::rebind(binding) {
                        Ok(()) => {
                            let saved = self.settings.as_ref().map(|settings| {
                                let mut chosen = settings.write().unwrap_or_else(|poisoned| poisoned.into_inner());
                                chosen.shortcut = binding;
                                chosen.save()
                            });
                            let notice = match saved {
                                Some(Err(reason)) => {
                                    (format!("{binding} works now, but it wasn't saved: {reason}"), Tone::Warning)
                                }
                                _ => (format!("Saved. {binding} it is."), Tone::Success),
                            };
                            return self.stop_capturing(Some(notice));
                        }
                        Err(reason) => self.practice.notice = Some((reason, Tone::Danger)),
                    },
                }
            }
            Message::ChangeShortcut => {
                hotkey::pause(true);
                self.practice.capturing = Some(keyboard::Modifiers::empty());
                self.practice.notice = None;
            }
            Message::KeepShortcut => return self.stop_capturing(None),
            Message::Watch => {
                // macOS: the Accessibility permission just came on, so the keys can start now.
                #[cfg(target_os = "macos")]
                if hotkey::shortcut_taken()
                    && hotkey::accessibility_granted()
                    && let Some(settings) = &self.settings
                {
                    let triggers = hotkey::Triggers::from(&settings.read().map(|s| s.clone()).unwrap_or_default());
                    if let Err(reason) = hotkey::start_again(triggers) {
                        crate::diagnostics::warn("hotkey", format!("still can't hear the keys: {reason}"));
                    }
                }
                let phase =
                    self.dictation.as_ref().map(|d| d.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).phase);
                self.practice.listening = matches!(phase, Some(Phase::Recording | Phase::HandsFree));
                if self.practice.advance_at.is_some_and(|at| Instant::now() >= at) {
                    return self.go(Step::Done);
                }
            }
            Message::Practice(words) => {
                if self.practice.advance_at.is_none() && words.to_lowercase().contains(MAGIC_WORD) {
                    self.practice.advance_at = Some(Instant::now() + ADVANCE_AFTER);
                }
                self.practice.words = words;
            }
        }
        Task::none()
    }

    fn go(&mut self, step: Step) -> Task<Message> {
        if self.step == step {
            return Task::none();
        }
        self.direction = if step.index() > self.step.index() { 1.0 } else { -1.0 };
        self.step = step;
        self.arrival = Spring::new(spring::DEFAULT, 0.0);
        self.arrival.target = 1.0;
        self.progress.target = step.index() as f32;
        if self.still {
            // The wide dot jumps instead of sliding.
            self.progress.value = self.progress.target;
        }
        self.clock = 0.0;
        self.last_frame = None;
        self.mic = Mic::default();
        if self.practice.capturing.is_some() {
            hotkey::pause(false);
        }
        self.practice = Practice::default();
        // The practice box has the focus, so the first dictation pastes into it.
        if step == Step::Shortcut && self.dictation.is_some() { operation::focus(PRACTICE) } else { Task::none() }
    }

    fn stop_capturing(&mut self, notice: Option<(String, Tone)>) -> Task<Message> {
        hotkey::pause(false);
        self.practice.capturing = None;
        self.practice.notice = notice;
        operation::focus(PRACTICE)
    }

    /// Restore the global shortcut when the window closes during capture.
    pub fn closing(&mut self) {
        if self.practice.capturing.take().is_some() {
            hotkey::pause(false);
        }
    }

    /// The flock on welcome and done, and the welcome text arriving.
    fn art_is_moving(&self) -> bool {
        if self.still {
            return false;
        }
        match self.step {
            Step::Welcome => self.clock < Shape::Mark.duration().max(TEXT_FROM + 2.0 * STAGGER + 0.6),
            Step::Done => self.clock < Shape::Word.duration(),
            _ => false,
        }
    }

    /// True while anything on screen is moving. When false the window draws nothing.
    pub fn wants_frames(&self) -> bool {
        !self.arrival.is_settled() || !self.progress.is_settled() || self.art_is_moving()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let frames = if self.wants_frames() { window::frames().map(Message::Frame) } else { Subscription::none() };
        let step = match (&self.step, &self.codex) {
            (Step::Connect, _) if self.wants_recheck() => time::every(Duration::from_secs(2)).map(|_| Message::Recheck),
            (Step::Connect, _) => Subscription::none(),
            // Onboarding comes before any pick, so it listens to Windows' default.
            (Step::Microphone, _) => Subscription::run_with(None, microphone::listen).map(Message::Mic),
            // Picking a new shortcut reads the window's own keys.
            (Step::Shortcut, _) if self.practice.capturing.is_some() => keyboard::listen().map(Message::Key),
            // Otherwise the global shortcut has the keys, so watch the dictation instead.
            (Step::Shortcut, _) if self.dictation.is_some() => {
                time::every(Duration::from_millis(80)).map(|_| Message::Watch)
            }
            (Step::Shortcut, _) => Subscription::none(),
            (Step::Welcome | Step::Done, _) => Subscription::none(),
        };
        Subscription::batch([frames, step])
    }

    pub fn view(&self) -> Element<'_, Message> {
        let p = palette();
        let t = self.arrival.value.clamp(0.0, 1.0);
        let page = match self.step {
            Step::Welcome => self.welcome(&p),
            Step::Connect => self.connect(&p),
            Step::Microphone => microphone_step(&p, &self.mic),
            Step::Shortcut => shortcut(&p, self.shortcut(), &self.practice, self.dictation.is_some()),
            Step::Done => self.done(&p),
        };

        // Only the page slides; the back button and the dots are the same
        // things on every step, so they stay put. Without animations it only fades.
        let offset = if self.still { 0.0 } else { (1.0 - t) * space::S5 * self.direction };
        let sliding = row![
            Space::new().width(offset.max(0.0)),
            page.view(&p, t, self.stagger_for_welcome(), self.still),
            Space::new().width((-offset).max(0.0)),
        ]
        .height(Length::Fill);

        let back: Element<'_, Message> = match self.step {
            Step::Welcome | Step::Done => Space::new().height(space::S6).into(),
            _ => ghost("Back", Message::Back, 1.0),
        };

        let content = column![
            row![back].height(space::S6),
            sliding,
            container(dots(&p, self.progress.value)).center_x(Length::Fill),
        ]
        .spacing(space::S5);

        let surface =
            container(content).padding([space::S5, space::S6]).width(Length::Fill).height(Length::Fill).style(
                move |_| container::Style {
                    background: Some(Background::Color(p.bg)),
                    text_color: Some(p.text),
                    ..container::Style::default()
                },
            );
        // No title bar, so the whole surface moves the window. Buttons take their own clicks first.
        mouse_area(surface).on_press(Message::Drag).into()
    }

    /// The welcome text arrives after the flock; other steps arrive with the slide.
    fn stagger_for_welcome(&self) -> Option<f32> {
        (self.step == Step::Welcome && !self.still).then_some(self.clock - TEXT_FROM)
    }

    /// Where the flock is in its flight; without animations, already landed.
    fn flock_clock(&self, shape: Shape) -> f32 {
        if self.still { shape.duration() } else { self.clock }
    }

    fn welcome(&self, p: &Palette) -> Page<'_> {
        Page {
            art: flock_art(p, Shape::Mark, self.flock_clock(Shape::Mark)),
            title: "Talk. It types.",
            body: "Free dictation with any ChatGPT account. Hold a key, say it, let go.".into(),
            note: None,
            actions: vec![Action::Primary("Get started", Message::Next)],
        }
    }

    /// The flock spells likh above the three keys worth knowing on day one:
    /// talk, cancel, and paste the last one again.
    fn done(&self, p: &Palette) -> Page<'_> {
        let paste_last = self.settings.as_ref().map_or(Some(hotkey::PASTE_LAST), |s| {
            s.read().map(|s| s.paste_last).unwrap_or_else(|poisoned| poisoned.into_inner().paste_last)
        });
        let mut keys = vec![(self.shortcut().parts(), "Hold to talk, let go to paste"), (vec!["Esc"], "Cancel")];
        if let Some(binding) = paste_last {
            keys.push((binding.parts(), "Paste the last one again"));
        }
        let rows = column(keys.into_iter().map(|(parts, what)| key_row(p, parts, what))).spacing(space::S2);
        let art = column![
            flock_art(p, Shape::Word, self.flock_clock(Shape::Word)),
            container(rows).center_x(Length::Fill),
            Space::new().height(space::S4),
        ]
        .spacing(space::S3);
        let tray = if cfg!(target_os = "macos") {
            "likh waits in your menu bar and opens at login."
        } else {
            "likh waits in your tray and starts with Windows."
        };
        Page {
            art: art.into(),
            title: "You're all set",
            body: tray.into(),
            note: None,
            actions: vec![Action::Primary("Finish", Message::Finish)],
        }
    }
}

pub fn check_codex_now() -> CodexStatus {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    match codex_auth::codex_home() {
        Some(home) => codex_auth::detect(&home, now),
        None => CodexStatus::NotInstalled,
    }
}

/// The window onboarding lives in: fixed size, centred, no title bar.
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

/// `likh onboarding`: the window on its own, without the running app.
pub fn run() -> iced::Result {
    let mut app = iced::application(Onboarding::boot, Onboarding::update, Onboarding::view)
        .title("likh")
        .subscription(Onboarding::subscription)
        .window(window_settings())
        .theme(|_: &Onboarding| Theme::Dark)
        .default_font(theme::UI);
    for font in theme::FONT_FILES {
        app = app.font(font);
    }
    app.run()
}

/// The words for each sign-in state. Kept in sync with the table in docs/brand/BRAND.md.
pub struct StateCopy {
    pub title: &'static str,
    pub detail: String,
    pub tone: Tone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Warning,
    Danger,
    Success,
}

pub fn codex_copy(status: &CodexStatus) -> StateCopy {
    match status {
        CodexStatus::NotInstalled => StateCopy {
            title: "Codex isn't installed",
            detail: "likh uses your ChatGPT account through Codex, free on any plan. Install it, sign in once.".into(),
            tone: Tone::Neutral,
        },
        CodexStatus::SignedOut => StateCopy {
            title: "Codex isn't signed in",
            detail: "Open Codex (or run codex login) and sign in.".into(),
            tone: Tone::Warning,
        },
        CodexStatus::Connected(account) => StateCopy {
            title: "Connected through Codex",
            detail: [
                account.email_hint.clone(),
                account.plan.as_deref().map(|plan| format!("ChatGPT {}", plan_name(plan))),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · "),
            tone: Tone::Success,
        },
        CodexStatus::Expired(_) => StateCopy {
            title: "Your Codex sign-in expired",
            detail: "Open Codex once to refresh it, then try again.".into(),
            tone: Tone::Danger,
        },
    }
}

/// One step: art on top, then a title, a line of body, an optional note and the actions.
/// Every step has the same skeleton, so nothing jumps when you move between them.
struct Page<'a> {
    art: Element<'a, Message>,
    title: &'static str,
    body: String,
    note: Option<(String, Color)>,
    actions: Vec<Action>,
}

enum Action {
    Primary(&'static str, Message),
    Secondary(&'static str, Message),
}

impl<'a> Page<'a> {
    /// `t` fades the page in as it slides. `stagger` is seconds since the text
    /// started arriving, when the text arrives line by line instead. `still`
    /// keeps the fade and drops the rise.
    fn view(self, p: &Palette, t: f32, stagger: Option<f32>, still: bool) -> Element<'a, Message> {
        let arrive = |line: usize| match stagger {
            Some(clock) => spring::DEFAULT.progress(clock - line as f32 * STAGGER).clamp(0.0, 1.0),
            None => t,
        };
        let rise = |line: usize, content: Element<'a, Message>| -> Element<'a, Message> {
            let a = if still { 1.0 } else { arrive(line) };
            column![Space::new().height((1.0 - a) * RISE), content, Space::new().height(a * RISE)].into()
        };
        // A body that needs two lines gets a measure just over half its length,
        // so the lines come out even (CSS `text-wrap: balance`) instead of
        // leaving one word on the second. Measured in Geist Medium, a touch
        // wider than the body's Regular, so it never runs to a third line.
        let one_line = crate::pill::ui_text_width(&self.body, size::BODY);
        let measure = if one_line > BODY_WIDTH { (one_line / 2.0 + space::S7).min(BODY_WIDTH) } else { BODY_WIDTH };

        let title = text(self.title)
            .size(size::DISPLAY)
            .line_height(theme::line_height(size::DISPLAY))
            .font(theme::UI_SEMIBOLD)
            .color(fade(p.text, arrive(0)))
            .align_x(Alignment::Center)
            .width(Length::Fill);
        let body = container(
            text(self.body)
                .size(size::BODY)
                .line_height(theme::line_height(size::BODY))
                .color(fade(p.text_muted, arrive(1)))
                .align_x(Alignment::Center)
                .width(Length::Fill),
        )
        .max_width(measure)
        .height(44);
        // The note line keeps its height when empty, so the buttons never move.
        let note = match self.note {
            Some((words, color)) => text(words).size(size::SMALL).color(fade(color, arrive(1))),
            None => text(""),
        };
        let actions = row(self.actions.into_iter().map(|action| match action {
            Action::Primary(label, message) => primary(label, message, arrive(2)),
            Action::Secondary(label, message) => ghost(label, message, arrive(2)),
        }))
        .spacing(space::S2);

        column![
            container(self.art).center(Length::Fill),
            rise(0, title.into()),
            rise(1, container(body).center_x(Length::Fill).into()),
            container(note.line_height(theme::line_height(size::SMALL))).center_x(Length::Fill).height(18),
            Space::new().height(space::S4),
            rise(2, container(actions).center_x(Length::Fill).into()),
        ]
        .spacing(space::S2)
        .width(Length::Fill)
        .into()
    }
}

fn flock_art<'a>(p: &Palette, shape: Shape, clock: f32) -> Element<'a, Message> {
    canvas_widget(Flock { shape, t: clock, alpha: 1.0, colors: flock::Colors::from(p) })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

impl Onboarding {
    /// Re-check on its own while the picked engine isn't ready, so nobody waits
    /// on a button. A Groq key is only ever typed, so there's nothing to watch.
    pub fn wants_recheck(&self) -> bool {
        self.step == Step::Connect && !self.engine_ready() && self.engine != Engine::Groq
    }

    fn engine_ready(&self) -> bool {
        match self.engine {
            Engine::Codex => matches!(self.codex, CodexStatus::Connected(_)),
            Engine::Groq => self.groq.saved,
        }
    }

    /// Pick how likh transcribes, then get that engine ready. The picker sits
    /// on top; the state of the picked engine is the headline, as before.
    fn connect<'a>(&'a self, p: &Palette) -> Page<'a> {
        let copy = engine_copy(self.engine, &self.codex, &self.groq);
        let ready = self.engine_ready();
        let tone = |tone: Tone| match tone {
            Tone::Success => p.success,
            Tone::Warning => p.warning,
            Tone::Danger => p.danger,
            Tone::Neutral => p.text_subtle,
        };
        let options: Vec<(Engine, &'static str)> = Engine::ALL.iter().map(|engine| (*engine, engine.name())).collect();
        // The state is the headline, so there is no card: likh on the left,
        // the engine on the right, and the link between them says how it's going.
        let link = canvas_widget(Link { tone: copy.tone, engine: self.engine, colors: LinkColors::from(p) })
            .width(240)
            .height(64);
        let mut art = column![segmented(&options, self.engine, Message::Engine), link]
            .spacing(space::S5)
            .align_x(Alignment::Center);
        // The key field's slot is always there, so the picker never jumps between engines.
        let slot = container(Space::new()).height(KEY_FIELD_HEIGHT);
        if self.engine != Engine::Groq || self.groq.saved {
            art = art.push(slot);
        } else {
            // It sits above the title (the slot is part of the art), so it says what it's for itself.
            let field = text_input("Paste your key, gsk_\u{2026}", &self.groq.key)
                .id(GROQ_KEY)
                .secure(true)
                .on_input(Message::GroqKey)
                .on_paste(Message::GroqKey)
                .on_submit(Message::SaveGroqKey)
                .size(size::BODY)
                .padding(space::S3)
                .width(320)
                .style(|_, status| field_style(status));
            art = art.push(container(field).height(KEY_FIELD_HEIGHT));
        }
        let (actions, note) = match self.engine {
            Engine::Codex => {
                let actions = match &self.codex {
                    CodexStatus::Connected(_) => vec![Action::Primary("Continue", Message::Next)],
                    CodexStatus::NotInstalled => vec![
                        Action::Secondary("Check again", Message::Recheck),
                        Action::Primary("Get Codex", Message::GetCodex),
                    ],
                    CodexStatus::SignedOut | CodexStatus::Expired(_) => {
                        vec![Action::Primary("Check again", Message::Recheck)]
                    }
                };
                // Plain about Free: dictation stays free, the extras share a monthly allowance.
                let note = if ready {
                    "Dictation is free on every plan. Clean up, a bonus, can run out on Free."
                } else {
                    "likh checks again every 2 seconds."
                };
                (actions, (note.to_string(), p.text_subtle))
            }
            Engine::Groq if self.groq.saved => (
                vec![
                    Action::Secondary("Use another key", Message::ReplaceGroqKey),
                    Action::Primary("Continue", Message::Next),
                ],
                self.groq.notice.clone().map_or_else(
                    || ("Free for about 2,000 dictations a day.".to_string(), p.text_subtle),
                    |(words, t)| (words, tone(t)),
                ),
            ),
            Engine::Groq => (
                vec![
                    Action::Secondary("Get a free key", Message::GetGroqKey),
                    Action::Primary(
                        if self.groq.checking { "Checking\u{2026}" } else { "Save key" },
                        Message::SaveGroqKey,
                    ),
                ],
                self.groq.notice.clone().map_or_else(
                    || ("The key stays on this computer.".to_string(), p.text_subtle),
                    |(words, t)| (words, tone(t)),
                ),
            ),
        };
        Page { art: art.into(), title: copy.title, body: copy.detail, note: Some(note), actions }
    }
}

fn microphone_step<'a>(p: &Palette, mic: &Mic) -> Page<'a> {
    match (&mic.failure, &mic.device) {
        (Some(reason), _) => Page {
            art: level_bars(p.text_subtle, &[0.0; MIC_HISTORY]),
            title: "Can't hear a microphone",
            body: format!("Plug one in, or allow microphone access in {}.", crate::shared::SYSTEM_SETTINGS),
            note: Some((reason.trim_end_matches('.').to_string(), p.danger)),
            actions: vec![
                Action::Secondary("Open settings", Message::OpenMicSettings),
                Action::Primary("Continue", Message::Next),
            ],
        },
        (None, device) => Page {
            art: level_bars(p.accent, &mic.history),
            title: "Say something",
            body: "The bars should move when you talk. likh uses your default microphone.".into(),
            note: Some((device.clone().unwrap_or_else(|| "Opening your microphone…".into()), p.text_subtle)),
            actions: vec![Action::Primary("Sounds good", Message::Next)],
        },
    }
}

/// Nine bars, the newest level in the middle and older ones spreading outwards.
fn level_bars<'a>(color: Color, history: &[f32; MIC_HISTORY]) -> Element<'a, Message> {
    let order = [4, 3, 2, 1, 0, 1, 2, 3, 4];
    let bars = row(order.map(|age| {
        let height = space::S2 + history[age] * (96.0 - space::S2);
        container(Space::new()).width(space::S2).height(height).style(move |_| pill_fill(color)).into()
    }))
    .spacing(6)
    .align_y(Alignment::Center);
    container(bars).center_y(96).into()
}

/// The shortcut step. Inside the app it is a real dictation: say "continue"
/// and it moves on. Opened on its own there is no shortcut to try.
fn shortcut<'a>(p: &Palette, binding: Binding, practice: &'a Practice, in_app: bool) -> Page<'a> {
    let caps: Element<'a, Message> = match practice.capturing {
        Some(held) => {
            let mut caps = row![].spacing(space::S3).align_y(Alignment::Center);
            for part in capture::held_parts(held) {
                caps = caps.push(keycap(part, true, false)).push(text("+").size(size::BODY).color(p.text_subtle));
            }
            caps.push(keycap("…", false, true)).into()
        }
        None => crate::shared::ui::keycaps(&binding.parts(), practice.listening),
    };
    if !in_app {
        return Page {
            art: caps,
            title: "Hold to talk",
            body: format!("Hold {binding}, say it, let go. Esc cancels."),
            note: Some(("Start likh to try it here.".to_string(), p.text_subtle)),
            actions: vec![Action::Primary("Continue", Message::Next)],
        };
    }
    let tone = |tone: Tone| match tone {
        Tone::Success => p.success,
        Tone::Warning => p.warning,
        Tone::Danger => p.danger,
        Tone::Neutral => p.text_subtle,
    };
    let (surface, ink, hint, accent) = (p.surface, p.text, p.text_subtle, p.accent);
    let field = text_input("Your words land here.", &practice.words)
        .id(PRACTICE)
        .on_input(Message::Practice)
        .size(size::BODY)
        .padding(space::S3)
        .width(360)
        .style(move |_, status| text_input::Style {
            background: Background::Color(surface),
            border: Border {
                radius: radius::CONTROL.into(),
                width: 1.0,
                color: if matches!(status, text_input::Status::Focused { .. }) {
                    fade(accent, 0.5)
                } else {
                    Color::TRANSPARENT
                },
            },
            icon: hint,
            placeholder: hint,
            value: ink,
            selection: fade(accent, 0.35),
        });
    let art = column![caps, field].spacing(space::S5).align_x(Alignment::Center);
    if practice.capturing.is_some() {
        let note = practice.notice.clone().map(|(words, t)| (words, tone(t)));
        return Page {
            art: art.into(),
            title: "Press your shortcut",
            body: "Hold the modifiers, then press the key. Esc keeps the one you have.".into(),
            note: note.or(Some(("Ctrl, Alt or Win, plus a letter, number or Space.".to_string(), p.text_subtle))),
            actions: vec![Action::Secondary("Keep current", Message::KeepShortcut)],
        };
    }
    // Holding the keys would do nothing at all, so the step leads with the fix.
    // On macOS that's the Accessibility permission; likh listens again once it's on.
    if hotkey::shortcut_taken() && cfg!(target_os = "macos") {
        return Page {
            art: art.into(),
            title: "Let likh hear its keys",
            body: "Turn likh on in System Settings, Privacy & Security, Accessibility.".into(),
            note: Some(("likh starts listening the moment it's on.".to_string(), p.text_subtle)),
            actions: vec![
                Action::Secondary("Skip", Message::Next),
                Action::Primary("Open settings", Message::OpenKeySettings),
            ],
        };
    }
    if hotkey::shortcut_taken() {
        let note = practice.notice.clone().map_or_else(
            || ("Pick keys no other app uses, like Ctrl + Alt + Space.".to_string(), p.text_subtle),
            |(words, t)| (words, tone(t)),
        );
        return Page {
            art: art.into(),
            title: "Another app has these keys",
            body: format!("{binding} belongs to another app, so likh can't hear it. Pick new keys."),
            note: Some(note),
            actions: vec![
                Action::Secondary("Skip", Message::Next),
                Action::Primary("Change shortcut", Message::ChangeShortcut),
            ],
        };
    }
    let note = if practice.advance_at.is_some() {
        ("Heard you. On to the last step.".to_string(), p.success)
    } else if practice.listening {
        ("Listening. Let go when you're done.".to_string(), p.accent)
    } else if let Some((words, t)) = practice.notice.clone() {
        (words, tone(t))
    } else if !practice.words.is_empty() {
        ("That landed. Now try saying \u{201c}continue\u{201d}.".to_string(), p.text_subtle)
    } else {
        (format!("Tip: tap {binding} once to talk hands-free."), p.text_subtle)
    };
    Page {
        art: art.into(),
        title: if practice.advance_at.is_some() { "That's it" } else { "Try it" },
        body: format!("Hold {binding} and say \u{201c}continue\u{201d}."),
        note: Some(note),
        actions: vec![
            Action::Secondary("Change shortcut", Message::ChangeShortcut),
            Action::Primary("Continue", Message::Next),
        ],
    }
}

/// One key on the done step: small caps on the left, what they do on the right.
fn key_row<'a>(p: &Palette, parts: Vec<&'static str>, what: &'static str) -> Element<'a, Message> {
    let (fill, ink, subtle) = (p.elevated, p.text, p.text_subtle);
    let mut caps = row![].spacing(space::S1).align_y(Alignment::Center);
    for part in parts {
        caps = caps.push(
            container(text(part).size(size::SMALL).font(theme::MONO).color(ink)).padding([space::S1, space::S2]).style(
                move |_| container::Style {
                    background: Some(Background::Color(fill)),
                    border: Border { radius: radius::CONTROL.into(), ..Border::default() },
                    ..container::Style::default()
                },
            ),
        );
    }
    row![container(caps).width(160).align_right(160), text(what).size(size::SMALL).color(subtle).width(200),]
        .spacing(space::S4)
        .align_y(Alignment::Center)
        .into()
}

/// The wide dot slides between positions; `position` is the step index, mid-move it is fractional.
fn dots<'a>(p: &Palette, position: f32) -> Element<'a, Message> {
    let (on, off) = (p.text, p.elevated);
    row((0..STEPS.len()).map(|i| {
        let near = (1.0 - (position - i as f32).abs()).clamp(0.0, 1.0);
        let color = mix(off, on, near);
        container(Space::new()).width(6.0 + 10.0 * near).height(6).style(move |_| pill_fill(color)).into()
    }))
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

/// The connect step's art: likh's bars, a dotted link, and the engine's
/// glyph. The link takes the state's colour and carries its icon.
struct Link {
    tone: Tone,
    engine: Engine,
    colors: LinkColors,
}

#[derive(Clone, Copy)]
struct LinkColors {
    backdrop: Color,
    accent: Color,
    muted: Color,
    subtle: Color,
    fill: Color,
    warning: Color,
    danger: Color,
    success: Color,
}

impl LinkColors {
    fn from(p: &Palette) -> Self {
        Self {
            backdrop: p.bg,
            accent: p.accent,
            muted: p.text_muted,
            subtle: p.text_subtle,
            fill: p.elevated,
            warning: p.warning,
            danger: p.danger,
            success: p.success,
        }
    }
}

impl canvas::Program<Message> for Link {
    type State = ();

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let colors = self.colors;
        // The software renderer repaints only each shape's exact bounds, which misses
        // stroke widths and antialiased rims, so a step sliding away left a ghost of
        // the prompt behind. Painting the backdrop makes it repaint the whole art.
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), colors.backdrop);
        let c = frame.center();
        let ink = match self.tone {
            Tone::Neutral => colors.muted,
            Tone::Warning => colors.warning,
            Tone::Danger => colors.danger,
            Tone::Success => colors.success,
        };
        let at = |x: f32, y: f32| Point::new(c.x + x, c.y + y);
        // likh: the logo's five bars, left.
        for (i, share) in [0.45, 0.85, 0.55, 0.85, 0.45].iter().enumerate() {
            let h = 40.0 * share;
            let bar = Path::rounded_rectangle(at(-112.0 + i as f32 * 10.0, -h / 2.0), Size::new(5.0, h), 2.5.into());
            frame.fill(&bar, colors.accent);
        }
        // The engine, right: a prompt for Codex, a bolt for Groq.
        glyph::draw(&mut frame, self.engine, at(94.0, 0.0), colors.muted);
        // The link: dots either side of the state's icon.
        let link = if self.tone == Tone::Success { colors.success } else { colors.subtle };
        for i in 0..3 {
            let off = 30.0 + i as f32 * 12.0;
            frame.fill(&Path::circle(at(-off, 0.0), 2.5), link);
            frame.fill(&Path::circle(at(off, 0.0), 2.5), link);
        }
        frame.fill(&Path::circle(c, 18.0), colors.fill);
        let icon = Stroke::default().with_width(2.0).with_color(ink).with_line_cap(canvas::LineCap::Round);
        match self.tone {
            Tone::Success => frame.stroke(
                &Path::new(|b| {
                    b.move_to(at(-6.0, 0.5));
                    b.line_to(at(-1.5, 5.0));
                    b.line_to(at(6.5, -4.5));
                }),
                icon,
            ),
            Tone::Warning | Tone::Danger => {
                frame.stroke(&Path::line(at(0.0, -6.5), at(0.0, 2.0)), icon);
                frame.fill(&Path::circle(at(0.0, 6.5), 1.3), ink);
            }
            Tone::Neutral => frame.stroke(
                &Path::new(|b| {
                    b.move_to(at(0.0, -7.0));
                    b.line_to(at(0.0, 6.0));
                    b.move_to(at(-5.0, 1.0));
                    b.line_to(at(0.0, 6.0));
                    b.line_to(at(5.0, 1.0));
                }),
                icon,
            ),
        }
        vec![frame.into_geometry()]
    }
}

/// ChatGPT's plan codes as the plans are called ("prolite" is Pro Lite).
fn plan_name(code: &str) -> String {
    match code.to_ascii_lowercase().as_str() {
        "prolite" | "pro_lite" => "Pro Lite".into(),
        _ => capitalize(code),
    }
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}
