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
use crate::shared::ui::{fade, ghost, keycap, mix, pill_fill, primary};
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

const MIC_SETTINGS_URL: &str = "ms-settings:privacy-microphone";
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
    ConnectCodex,
    Microphone,
    Shortcut,
    Done,
}

const STEPS: [Step; 5] = [Step::Welcome, Step::ConnectCodex, Step::Microphone, Step::Shortcut, Step::Done];

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

const PRACTICE: &str = "practice";
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
    codex: CodexStatus,
    /// 0 → 1 as a step arrives. `direction` is +1 going forward, -1 going back.
    arrival: Spring,
    direction: f32,
    /// Which progress dot is wide; it slides between dots instead of jumping.
    progress: Spring,
    /// Seconds since this step's art started; drives the flock and the stagger.
    clock: f32,
    last_frame: Option<Instant>,
    mic: Mic,
}

#[derive(Debug, Clone)]
pub enum Message {
    Next,
    Back,
    Recheck,
    GetCodex,
    OpenMicSettings,
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
            codex,
            arrival: Spring::new(spring::DEFAULT, 1.0),
            direction: 1.0,
            progress: Spring::new(spring::DEFAULT, 0.0),
            clock: 0.0,
            last_frame: None,
            mic: Mic::default(),
        }
    }

    fn boot() -> (Self, Task<Message>) {
        (Self::new(check_codex_now()), window::oldest().map(Message::WindowFound))
    }

    /// Onboarding inside the running app, in a window the tray opened.
    pub fn in_app(window: window::Id, dictation: Arc<Mutex<Status>>, settings: Arc<RwLock<Settings>>) -> Self {
        Self {
            window: Some(window),
            dictation: Some(dictation),
            settings: Some(settings),
            ..Self::new(check_codex_now())
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
            Message::Recheck => self.codex = check_codex_now(),
            Message::GetCodex => crate::shared::open_url(crate::shared::CODEX_SETUP),
            Message::OpenMicSettings => crate::shared::open_url(MIC_SETTINGS_URL),
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
            (Step::ConnectCodex, CodexStatus::Connected(_)) => Subscription::none(),
            (Step::ConnectCodex, _) => time::every(Duration::from_secs(2)).map(|_| Message::Recheck),
            (Step::Microphone, _) => Subscription::run(microphone::listen).map(Message::Mic),
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
            Step::ConnectCodex => connect_codex(&p, &self.codex),
            Step::Microphone => microphone_step(&p, &self.mic),
            Step::Shortcut => shortcut(&p, self.shortcut(), &self.practice, self.dictation.is_some()),
            Step::Done => self.done(&p),
        };

        // Only the page slides; the back button and the dots are the same
        // things on every step, so they stay put.
        let offset = (1.0 - t) * space::S5 * self.direction;
        let sliding = row![
            Space::new().width(offset.max(0.0)),
            page.view(&p, t, self.stagger_for_welcome()),
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
        (self.step == Step::Welcome).then_some(self.clock - TEXT_FROM)
    }

    fn welcome(&self, p: &Palette) -> Page<'_> {
        Page {
            art: flock_art(p, Shape::Mark, self.clock),
            title: "Talk. It types.",
            body: "Hold a key, say it, let go. Your words land wherever you're typing.".into(),
            note: None,
            actions: vec![Action::Primary("Get started", Message::Next)],
        }
    }

    fn done(&self, p: &Palette) -> Page<'_> {
        Page {
            art: flock_art(p, Shape::Word, self.clock),
            title: "You're all set",
            body: format!("Hold {} in any app to talk. likh waits in your tray.", self.shortcut()),
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
pub struct CodexCopy {
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

pub fn codex_copy(status: &CodexStatus) -> CodexCopy {
    match status {
        CodexStatus::NotInstalled => CodexCopy {
            title: "Codex isn't installed",
            detail: "Install Codex, sign in once, then come back.".into(),
            tone: Tone::Neutral,
        },
        CodexStatus::SignedOut => CodexCopy {
            title: "Codex isn't signed in",
            detail: "Open Codex (or run codex login) and sign in.".into(),
            tone: Tone::Warning,
        },
        CodexStatus::Connected(account) => CodexCopy {
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
        CodexStatus::Expired(_) => CodexCopy {
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
    /// started arriving, when the text arrives line by line instead.
    fn view(self, p: &Palette, t: f32, stagger: Option<f32>) -> Element<'a, Message> {
        let arrive = |line: usize| match stagger {
            Some(clock) => spring::DEFAULT.progress(clock - line as f32 * STAGGER).clamp(0.0, 1.0),
            None => t,
        };
        let rise = |line: usize, content: Element<'a, Message>| -> Element<'a, Message> {
            let a = arrive(line);
            column![Space::new().height((1.0 - a) * RISE), content, Space::new().height(a * RISE)].into()
        };

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
        // Narrow enough that two-line bodies break evenly instead of leaving one word behind.
        .max_width(320)
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

fn connect_codex<'a>(p: &Palette, status: &CodexStatus) -> Page<'a> {
    let copy = codex_copy(status);
    let connected = matches!(status, CodexStatus::Connected(_));
    let actions = match status {
        CodexStatus::Connected(_) => vec![Action::Primary("Continue", Message::Next)],
        CodexStatus::NotInstalled => {
            vec![Action::Secondary("Check again", Message::Recheck), Action::Primary("Get Codex", Message::GetCodex)]
        }
        CodexStatus::SignedOut | CodexStatus::Expired(_) => vec![Action::Primary("Check again", Message::Recheck)],
    };
    // The state is the headline, so there is no card: likh on the left,
    // Codex on the right, and the link between them says how it's going.
    let art = canvas_widget(Link { tone: copy.tone, colors: LinkColors::from(p) }).width(240).height(64);
    Page {
        art: art.into(),
        title: copy.title,
        body: copy.detail,
        // Say that it re-checks on its own, so nobody waits on a button.
        note: Some(if connected {
            ("Nothing to pay, nothing to paste.".to_string(), p.text_subtle)
        } else {
            ("likh checks again every 2 seconds.".to_string(), p.text_subtle)
        }),
        actions,
    }
}

fn microphone_step<'a>(p: &Palette, mic: &Mic) -> Page<'a> {
    match (&mic.failure, &mic.device) {
        (Some(reason), _) => Page {
            art: level_bars(p.text_subtle, &[0.0; MIC_HISTORY]),
            title: "Can't hear a microphone",
            body: "Plug one in, or allow microphone access in Windows Settings.".into(),
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

/// The Codex step's art: likh's bars, a dotted link, and a terminal prompt
/// for Codex. The link takes the state's colour and carries its icon.
struct Link {
    tone: Tone,
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
        // Codex: a terminal prompt, right.
        let prompt = Stroke::default().with_width(3.0).with_color(colors.muted).with_line_cap(canvas::LineCap::Round);
        frame.stroke(
            &Path::new(|b| {
                b.move_to(at(78.0, -10.0));
                b.line_to(at(90.0, 0.0));
                b.line_to(at(78.0, 10.0));
            }),
            prompt,
        );
        frame.stroke(&Path::line(at(96.0, 11.0), at(110.0, 11.0)), prompt);
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
