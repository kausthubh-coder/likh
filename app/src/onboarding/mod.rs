//! First-run window. Phase 1 ships Welcome and Connect Codex; the other four
//! steps from docs/brand/BRAND.md (mic, shortcut, try it, done) come in phase 2.

use crate::codex_auth::{self, CodexStatus};
use crate::shared::spring::{self, Spring};
use crate::shared::theme::{self, Palette, palette, radius, size, space};
use iced::widget::{Space, button, column, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Subscription, Task, time, window};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(test)]
mod tests;

const GET_CODEX_URL: &str = "https://developers.openai.com/codex";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Welcome,
    ConnectCodex,
}

impl Step {
    fn index(self) -> usize {
        match self {
            Step::Welcome => 0,
            Step::ConnectCodex => 1,
        }
    }
}

pub struct Onboarding {
    step: Step,
    codex: CodexStatus,
    /// 0 → 1 as a new step arrives: slides in 24px and fades up.
    arrival: Spring,
    last_frame: Option<Instant>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Next,
    Back,
    Recheck,
    GetCodex,
    Frame(Instant),
}

impl Onboarding {
    pub fn new(codex: CodexStatus) -> Self {
        Self { step: Step::Welcome, codex, arrival: Spring::new(spring::DEFAULT, 1.0), last_frame: None }
    }

    #[cfg(test)]
    pub fn step(&self) -> Step {
        self.step
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Next => self.go(Step::ConnectCodex),
            Message::Back => self.go(Step::Welcome),
            Message::Recheck => self.codex = check_codex_now(),
            Message::GetCodex => open_url(GET_CODEX_URL),
            Message::Frame(now) => {
                let dt = self.last_frame.map_or(1.0 / 60.0, |last| (now - last).as_secs_f32().min(0.05));
                self.last_frame = if self.arrival.step(dt) { None } else { Some(now) };
            }
        }
        Task::none()
    }

    fn go(&mut self, step: Step) {
        if self.step != step {
            self.step = step;
            self.arrival = Spring::new(spring::DEFAULT, 0.0);
            self.arrival.target = 1.0;
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        // Idle draws nothing: frames only while a step is arriving, and the
        // Codex check only while the user is looking at it and not connected yet.
        let frames =
            if self.arrival.is_settled() { Subscription::none() } else { window::frames().map(Message::Frame) };
        let recheck = match (&self.step, &self.codex) {
            (Step::ConnectCodex, CodexStatus::Connected(_)) | (Step::Welcome, _) => Subscription::none(),
            (Step::ConnectCodex, _) => time::every(Duration::from_secs(2)).map(|_| Message::Recheck),
        };
        Subscription::batch([frames, recheck])
    }

    pub fn view(&self) -> Element<'_, Message> {
        let p = palette();
        let t = self.arrival.value.clamp(0.0, 1.0);
        let body = match self.step {
            Step::Welcome => welcome(&p, t),
            Step::ConnectCodex => connect_codex(&p, t, &self.codex),
        };
        let content = column![
            text(format!("{} / 6", self.step.index() + 1)).size(size::CAPTION).font(theme::MONO).color(p.text_subtle),
            row![Space::new().width((1.0 - t) * 24.0), body].height(Length::Fill),
            dots(&p, self.step.index()),
        ]
        .spacing(space::S4);

        container(content)
            .padding(space::S5)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(p.bg)),
                text_color: Some(p.text),
                ..container::Style::default()
            })
            .into()
    }
}

pub fn check_codex_now() -> CodexStatus {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    match codex_auth::codex_home() {
        Some(home) => codex_auth::detect(&home, now),
        None => CodexStatus::NotInstalled,
    }
}

pub fn run() -> iced::Result {
    let mut app = iced::application(|| Onboarding::new(check_codex_now()), Onboarding::update, Onboarding::view)
        .title("murmur")
        .subscription(Onboarding::subscription)
        .window_size((560.0, 640.0))
        .resizable(false)
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

fn welcome<'a>(p: &Palette, t: f32) -> Element<'a, Message> {
    column![
        container(mark(p, 2.0)).center(Length::Fill),
        text("Talk. It types.")
            .size(size::DISPLAY)
            .line_height(theme::line_height(size::DISPLAY))
            .font(theme::UI_SEMIBOLD)
            .color(fade(p.text, t)),
        text("Hold a key, say it, let go. Your words land wherever you're typing.")
            .size(size::BODY)
            .line_height(theme::line_height(size::BODY))
            .color(fade(p.text_muted, t)),
        Space::new().height(space::S4),
        primary("Get started", Message::Next),
    ]
    .spacing(space::S2)
    .into()
}

/// The logo: a capsule with five bars whose heights trace an "m" (BRAND.md, Logo).
/// Phase 3 replaces this static mark with the flock that forms it.
fn mark<'a>(p: &Palette, scale: f32) -> Element<'a, Message> {
    let (accent, surface) = (p.accent, p.surface);
    let bars = row([0.45, 0.85, 0.55, 0.85, 0.45].map(|h: f32| {
        container(Space::new()).width(4.0 * scale).height(22.0 * h * scale).style(move |_| circle(accent)).into()
    }))
    .spacing(4.0 * scale)
    .align_y(Alignment::Center);
    container(bars)
        .center_x(64.0 * scale)
        .center_y(40.0 * scale)
        .style(move |_| container::Style {
            background: Some(Background::Color(surface)),
            border: Border { radius: (20.0 * scale).into(), width: 1.0, color: Color::from_rgba(1.0, 1.0, 1.0, 0.16) },
            ..container::Style::default()
        })
        .into()
}

fn connect_codex<'a>(p: &Palette, t: f32, status: &CodexStatus) -> Element<'a, Message> {
    let copy = codex_copy(status);
    let (glyph, tint) = match copy.tone {
        Tone::Neutral => ("↓", p.text_muted),
        Tone::Warning => ("!", p.warning),
        Tone::Danger => ("!", p.danger),
        Tone::Success => ("✓", p.success),
    };
    let surface = p.surface;
    let elevated = p.elevated;
    let card = container(
        row![
            container(text(glyph).size(size::BODY).font(theme::UI_SEMIBOLD).color(tint))
                .center(32)
                .style(move |_| circle(elevated)),
            column![
                text(copy.title).size(size::SMALL).font(theme::UI_MEDIUM).color(fade(p.text, t)),
                text(copy.detail).size(size::CAPTION).color(fade(p.text_muted, t)),
            ]
            .spacing(2),
        ]
        .spacing(space::S3)
        .align_y(Alignment::Center),
    )
    .padding(space::S3)
    .width(Length::Fill)
    .style(move |_| card_style(surface));

    let actions: Element<'a, Message> = match status {
        CodexStatus::Connected(_) => primary("Continue", Message::Next),
        CodexStatus::NotInstalled => {
            row![ghost("Check again", Message::Recheck), primary("Get Codex", Message::GetCodex)]
                .spacing(space::S2)
                .into()
        }
        CodexStatus::SignedOut | CodexStatus::Expired(_) => primary("Check again", Message::Recheck),
    };

    column![
        Space::new().height(Length::Fill),
        card,
        Space::new().height(Length::Fill),
        text("Connect Codex").size(size::TITLE).font(theme::UI_SEMIBOLD).color(fade(p.text, t)),
        text("murmur uses your ChatGPT plan through Codex. Nothing to pay, nothing to paste.")
            .size(size::SMALL)
            .line_height(theme::line_height(size::SMALL))
            .color(fade(p.text_muted, t)),
        Space::new().height(space::S2),
        actions,
        ghost("Back", Message::Back),
    ]
    .spacing(space::S2)
    .into()
}

fn dots<'a>(p: &Palette, current: usize) -> Element<'a, Message> {
    let (on, off) = (p.text, p.elevated);
    row((0..6).map(|i| {
        let color = if i == current { on } else { off };
        container(Space::new()).width(if i == current { 16 } else { 6 }).height(6).style(move |_| circle(color)).into()
    }))
    .spacing(6)
    .into()
}

fn primary<'a>(label: &'a str, on_press: Message) -> Element<'a, Message> {
    let p = palette();
    let (fill, ink) = (p.accent, p.on_accent);
    button(text(label).size(size::SMALL).font(theme::UI_MEDIUM).center().width(Length::Fill))
        .on_press(on_press)
        .padding([9, 16])
        .width(Length::Fill)
        .style(move |_, status| button_style(fill, ink, status))
        .into()
}

fn ghost<'a>(label: &'a str, on_press: Message) -> Element<'a, Message> {
    let p = palette();
    let (fill, ink) = (p.elevated, p.text);
    button(text(label).size(size::SMALL).font(theme::UI_MEDIUM).center().width(Length::Fill))
        .on_press(on_press)
        .padding([9, 16])
        .width(Length::Fill)
        .style(move |_, status| button_style(fill, ink, status))
        .into()
}

fn button_style(fill: Color, ink: Color, status: button::Status) -> button::Style {
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
}

fn card_style(fill: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(fill)),
        border: Border { radius: radius::CARD.into(), ..Border::default() },
        ..container::Style::default()
    }
}

fn circle(fill: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(fill)),
        border: Border { radius: 999.0.into(), ..Border::default() },
        ..container::Style::default()
    }
}

fn fade(color: Color, alpha: f32) -> Color {
    Color { a: color.a * alpha, ..color }
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
