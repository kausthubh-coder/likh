//! First-run window: a fixed 560 x 640 borderless window with five steps
//! (welcome, Codex, microphone, shortcut, done). Inside the running app the
//! shortcut step is a real first dictation: holding Ctrl+Space records through
//! the global shortcut and the words land in a practice box. Opened on its own
//! (`murmur onboarding`), the step only checks the keys.
//!
//! No title bar: drag it from anywhere empty, and Alt+F4 still closes it.
//!
//! Idle draws nothing: frames are requested only while something moves, the
//! Codex check runs only on its step, and the mic is open only on its step.

mod flock;

use crate::codex_auth::{self, CodexStatus};
use crate::dictation::{Phase, Status};
use crate::hotkey;
use crate::microphone;
use crate::shared::icon;
use crate::shared::spring::{self, Spring};
use crate::shared::theme::{self, Palette, palette, radius, size, space};
use flock::{Flock, Shape};
use iced::widget::canvas::{self, Frame, Path, Stroke};
use iced::widget::{
    Space, button, canvas as canvas_widget, column, container, mouse_area, operation, row, text, text_input,
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Point, Rectangle, Renderer, Size, Subscription, Task, Theme,
    keyboard, mouse, time, window,
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(test)]
mod tests;

const GET_CODEX_URL: &str = "https://developers.openai.com/codex";
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

#[derive(Default)]
struct Keys {
    ctrl: bool,
    space: bool,
    /// Held both and let go at least once.
    tried: bool,
}

const PRACTICE: &str = "practice";

pub struct Onboarding {
    /// The window, once iced reports it; needed to move it by dragging.
    window: Option<window::Id>,
    /// The running app's dictation, when onboarding opens inside it.
    dictation: Option<Arc<Mutex<Status>>>,
    /// Where the practice dictation lands.
    practice: String,
    step: Step,
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
    keys: Keys,
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
    Key(keyboard::Event),
    /// Look at the running dictation (only on the shortcut step, inside the app).
    Watch,
    Practice(String),
    WindowFound(Option<window::Id>),
    Drag,
}

impl Onboarding {
    pub fn new(codex: CodexStatus) -> Self {
        Self {
            window: None,
            dictation: None,
            practice: String::new(),
            step: Step::Welcome,
            codex,
            arrival: Spring::new(spring::DEFAULT, 1.0),
            direction: 1.0,
            progress: Spring::new(spring::DEFAULT, 0.0),
            clock: 0.0,
            last_frame: None,
            mic: Mic::default(),
            keys: Keys::default(),
        }
    }

    fn boot() -> (Self, Task<Message>) {
        (Self::new(check_codex_now()), window::oldest().map(Message::WindowFound))
    }

    /// Onboarding inside the running app, in a window the tray opened.
    pub fn in_app(window: window::Id, dictation: Arc<Mutex<Status>>) -> Self {
        Self { window: Some(window), dictation: Some(dictation), ..Self::new(check_codex_now()) }
    }

    #[cfg(test)]
    pub fn step(&self) -> Step {
        self.step
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Next => return self.go(STEPS.get(self.step.index() + 1).copied().unwrap_or(Step::Done)),
            Message::Back => return self.go(STEPS[self.step.index().saturating_sub(1)]),
            Message::Recheck => self.codex = check_codex_now(),
            Message::GetCodex => open_url(GET_CODEX_URL),
            Message::OpenMicSettings => open_url(MIC_SETTINGS_URL),
            // Closing the window ends `murmur onboarding`; inside the app, murmur stays in the tray.
            Message::Finish => return self.window.map_or_else(iced::exit, window::close),
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
                let was_held = self.keys.ctrl && self.keys.space;
                let space = keyboard::Key::Named(keyboard::key::Named::Space);
                match event {
                    keyboard::Event::ModifiersChanged(modifiers) => self.keys.ctrl = modifiers.control(),
                    keyboard::Event::KeyPressed { key, .. } if key == space => self.keys.space = true,
                    keyboard::Event::KeyReleased { key, .. } if key == space => self.keys.space = false,
                    _ => {}
                }
                if was_held && !(self.keys.ctrl && self.keys.space) {
                    self.keys.tried = true;
                }
            }
            Message::Watch => {
                let phase =
                    self.dictation.as_ref().map(|d| d.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).phase);
                let held = phase == Some(Phase::Recording);
                if self.keys.ctrl && !held {
                    self.keys.tried = true;
                }
                self.keys.ctrl = held;
                self.keys.space = held;
            }
            Message::Practice(words) => self.practice = words,
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
        self.keys = Keys::default();
        // The practice box has the focus, so the first dictation pastes into it.
        if step == Step::Shortcut && self.dictation.is_some() { operation::focus(PRACTICE) } else { Task::none() }
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
            // Inside the app the global shortcut swallows Ctrl+Space, so watch the dictation instead.
            (Step::Shortcut, _) if self.dictation.is_some() => {
                time::every(Duration::from_millis(80)).map(|_| Message::Watch)
            }
            (Step::Shortcut, _) => keyboard::listen().map(Message::Key),
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
            Step::Shortcut => shortcut(&p, &self.keys, self.dictation.is_some().then_some(self.practice.as_str())),
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
            body: "Hold Ctrl + Space in any app to talk. murmur waits in your tray.".into(),
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

/// `murmur onboarding`: the window on its own, without the running app.
pub fn run() -> iced::Result {
    let mut app = iced::application(Onboarding::boot, Onboarding::update, Onboarding::view)
        .title("murmur")
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
                account.plan.as_deref().map(|plan| format!("ChatGPT {}", capitalize(plan))),
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
    // The state is the headline, so there is no card: murmur on the left,
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
            ("murmur checks again every 2 seconds.".to_string(), p.text_subtle)
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
            body: "The bars should move when you talk. murmur uses your default microphone.".into(),
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

/// `practice` is Some inside the running app: a box the first real dictation pastes into.
fn shortcut<'a>(p: &Palette, keys: &Keys, practice: Option<&str>) -> Page<'a> {
    let held = keys.ctrl && keys.space;
    let key = |label: &'static str, down: bool| -> Element<'a, Message> {
        let (fill, ink) = if down { (p.accent, p.on_accent) } else { (p.surface, p.text) };
        container(text(label).size(size::BODY).font(theme::MONO).color(ink))
            .center_x(88)
            .center_y(56)
            .style(move |_| container::Style {
                background: Some(Background::Color(fill)),
                border: Border {
                    radius: radius::CONTROL.into(),
                    width: 1.0,
                    color: Color::from_rgba(1.0, 1.0, 1.0, if down { 0.0 } else { 0.06 }),
                },
                ..container::Style::default()
            })
            .into()
    };
    let keycaps =
        row![key("Ctrl", keys.ctrl), text("+").size(size::BODY).color(p.text_subtle), key("Space", keys.space)]
            .spacing(space::S3)
            .align_y(Alignment::Center);
    let (art, body, note): (Element<'a, Message>, &str, (String, Color)) = match practice {
        Some(words) => {
            let (surface, ink, hint, accent) = (p.surface, p.text, p.text_subtle, p.accent);
            let field = text_input("Your words land here.", words)
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
            let note = if held {
                ("Listening. Let go to paste.".to_string(), p.accent)
            } else if !words.is_empty() {
                ("That's it. It works the same in any app.".to_string(), p.success)
            } else {
                (format!("Hold {} and say anything.", hotkey::SHORTCUT), p.text_subtle)
            };
            let art = column![keycaps, field].spacing(space::S5).align_x(Alignment::Center);
            (art.into(), "Hold the keys, say something, let go. Your words land in the box.", note)
        }
        None => {
            let note = if held {
                ("Now let go. That's when it pastes.".to_string(), p.accent)
            } else if keys.tried {
                ("That's the one. Works in any app.".to_string(), p.success)
            } else {
                ("Try it now: hold both keys.".to_string(), p.text_subtle)
            };
            (keycaps.into(), "Hold Ctrl + Space, say it, let go. Esc cancels.", note)
        }
    };
    Page {
        art,
        title: "Hold to talk",
        body: body.into(),
        note: Some(note),
        actions: vec![Action::Primary("Continue", Message::Next)],
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

/// The Codex step's art: murmur's bars, a dotted link, and a terminal prompt
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
        // murmur: the logo's five bars, left.
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

fn primary<'a>(label: &'a str, on_press: Message, alpha: f32) -> Element<'a, Message> {
    let p = palette();
    let (fill, ink) = (fade(p.accent, alpha), fade(p.on_accent, alpha));
    button(text(label).size(size::SMALL).font(theme::UI_MEDIUM).center().width(Length::Fill))
        .on_press(on_press)
        .padding([11, 16])
        .width(200)
        .style(move |_, status| {
            // Hover is instant and subtle (it happens constantly); press darkens a touch more.
            let fill = match status {
                button::Status::Hovered => lighten(fill, 0.04),
                button::Status::Pressed => lighten(fill, -0.04),
                _ => fill,
            };
            button::Style {
                background: Some(Background::Color(fill)),
                text_color: ink,
                border: Border { radius: radius::CONTROL.into(), ..Border::default() },
                ..button::Style::default()
            }
        })
        .into()
}

/// Secondary actions are just words until you point at them.
fn ghost<'a>(label: &'a str, on_press: Message, alpha: f32) -> Element<'a, Message> {
    let p = palette();
    let (hover, rest, ink) = (p.elevated, p.text_muted, p.text);
    button(text(label).size(size::SMALL).font(theme::UI_MEDIUM).center())
        .on_press(on_press)
        .padding([11, 16])
        .style(move |_, status| {
            let (background, color) = match status {
                button::Status::Hovered => (Some(Background::Color(hover)), ink),
                button::Status::Pressed => (Some(Background::Color(lighten(hover, -0.03))), ink),
                _ => (None, rest),
            };
            button::Style {
                background,
                text_color: fade(color, alpha),
                border: Border { radius: radius::CONTROL.into(), ..Border::default() },
                ..button::Style::default()
            }
        })
        .into()
}

fn pill_fill(fill: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(fill)),
        border: Border { radius: 999.0.into(), ..Border::default() },
        ..container::Style::default()
    }
}

fn fade(color: Color, alpha: f32) -> Color {
    Color { a: color.a * alpha, ..color }
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t, a: a.a + (b.a - a.a) * t }
}

fn lighten(color: Color, amount: f32) -> Color {
    Color {
        r: (color.r + amount).clamp(0.0, 1.0),
        g: (color.g + amount).clamp(0.0, 1.0),
        b: (color.b + amount).clamp(0.0, 1.0),
        a: color.a,
    }
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer.exe").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();
    if let Err(error) = result {
        eprintln!("could not open {url}: {error}");
    }
}
