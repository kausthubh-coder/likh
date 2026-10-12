//! The murmur window, opened from the tray: the shortcut and how it behaves,
//! sounds, voice commands, history and start-with-Windows (General), what you
//! dictated (History), and the Codex sign-in (About).
//!
//! Borderless like onboarding: drag it from anywhere empty, close it with the
//! × or Esc. Nothing in it animates, so it draws only when something changes.

use crate::codex_auth::CodexStatus;
use crate::dictation::Status;
use crate::history::{self, Entry};
use crate::hotkey::{self, Binding, capture};
use crate::onboarding::{self, Tone};
use crate::settings::{Mode, Settings};
use crate::shared::icon;
use crate::shared::theme::{self, Palette, palette, radius, size, space};
use crate::shared::ui::{ghost, keycap, keycaps, lighten, pill_fill, switch};
use iced::widget::{Space, button, column, container, mouse_area, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Size, Subscription, Task, keyboard, window};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
mod tests;

pub const WINDOW: Size = Size::new(720.0, 520.0);
const SITE: &str = "https://murmur-eight-tau.vercel.app";
const REPO: &str = "https://github.com/kausthubh-coder/murmur";
/// Plenty to scroll back through; the file keeps everything.
const HISTORY_SHOWN: usize = 200;
/// Long dictations are cut in the list; copying still copies all of it.
const PREVIEW_CHARS: usize = 240;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    General,
    History,
    About,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tab(Tab),
    ChangeShortcut,
    KeepShortcut,
    Key(keyboard::Event),
    Mode(Mode),
    Sounds(bool),
    Commands(bool),
    KeepHistory(bool),
    StartWithWindows(bool),
    Copy(usize),
    Clear,
    /// Handled by the tray, which owns onboarding.
    RunSetup,
    Open(&'static str),
    Close,
    Drag,
}

pub struct Panel {
    window: window::Id,
    tab: Tab,
    settings: Arc<RwLock<Settings>>,
    dictation: Arc<Mutex<Status>>,
    codex: CodexStatus,
    /// Picking a new shortcut: the modifiers held so far.
    capturing: Option<keyboard::Modifiers>,
    notice: Option<(String, Tone)>,
    history: Vec<Entry>,
    /// "Clear" was pressed once; the next press clears.
    confirm_clear: bool,
    copied: Option<usize>,
    files: Files,
}

/// Where the settings and history live. The app passes the real files; tests pass temporary ones.
pub struct Files {
    pub settings: Option<PathBuf>,
    pub history: Option<PathBuf>,
}

impl Files {
    pub fn real() -> Self {
        Self { settings: crate::settings::folder().map(|dir| dir.join("settings.json")), history: history::file() }
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
        files: Files,
    ) -> Self {
        Self {
            window,
            tab: Tab::General,
            settings,
            dictation,
            codex: onboarding::check_codex_now(),
            capturing: None,
            notice: None,
            history: Vec::new(),
            confirm_clear: false,
            copied: None,
            files,
        }
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
        self.history =
            self.files.history.as_deref().map(|file| history::recent(file, HISTORY_SHOWN)).unwrap_or_default();
        self.copied = None;
        self.confirm_clear = false;
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tab(tab) => {
                self.stop_capturing();
                self.tab = tab;
                self.notice = None;
                match tab {
                    Tab::History => self.load_history(),
                    Tab::About => self.codex = onboarding::check_codex_now(),
                    Tab::General => {}
                }
            }
            Message::ChangeShortcut => {
                hotkey::pause(true);
                self.capturing = Some(keyboard::Modifiers::empty());
                self.notice = None;
            }
            Message::KeepShortcut => self.stop_capturing(),
            Message::Key(event) => match self.capturing {
                Some(held) => match capture::on_event(&event, held) {
                    capture::Outcome::Waiting(held) => self.capturing = Some(held),
                    capture::Outcome::Cancelled => self.stop_capturing(),
                    capture::Outcome::Refused(why) => self.notice = Some((why.into(), Tone::Danger)),
                    capture::Outcome::Chosen(binding) => match hotkey::rebind(binding) {
                        Ok(()) => {
                            self.change(|s| s.shortcut = binding);
                            self.stop_capturing();
                            if self.notice.is_none() {
                                self.notice = Some((format!("Saved. Hold {binding} to talk."), Tone::Success));
                            }
                        }
                        Err(reason) => self.notice = Some((reason, Tone::Danger)),
                    },
                },
                // Esc closes the window when nothing else wants it.
                None if is_escape(&event) => return window::close(self.window),
                None => {}
            },
            Message::Mode(mode) => self.change(|s| s.mode = mode),
            Message::Sounds(on) => {
                self.change(|s| s.sounds = on);
                if on {
                    crate::sounds::play(crate::sounds::Cue::Start);
                }
            }
            Message::Commands(on) => self.change(|s| s.commands = on),
            Message::KeepHistory(on) => self.change(|s| s.history = on),
            Message::StartWithWindows(on) => match crate::autostart::set(on) {
                Ok(()) => self.change(|s| s.start_with_windows = on),
                Err(reason) => self.notice = Some((format!("Windows didn't allow that: {reason}"), Tone::Danger)),
            },
            Message::Copy(index) => {
                if let Some(entry) = self.history.get(index) {
                    match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(entry.text.clone())) {
                        Ok(()) => self.copied = Some(index),
                        Err(reason) => self.notice = Some((format!("Couldn't copy: {reason}"), Tone::Danger)),
                    }
                }
            }
            Message::Clear if !self.confirm_clear => self.confirm_clear = true,
            Message::Clear => {
                if let Some(file) = &self.files.history
                    && let Err(reason) = history::clear(file)
                {
                    self.notice = Some((format!("Couldn't clear history: {reason}"), Tone::Danger));
                }
                self.load_history();
            }
            Message::RunSetup => {}
            Message::Open(url) => crate::shared::open_url(url),
            Message::Close => return window::close(self.window),
            Message::Drag => return window::drag(self.window),
        }
        Task::none()
    }

    fn stop_capturing(&mut self) {
        if self.capturing.take().is_some() {
            hotkey::pause(false);
        }
    }

    /// Releases the shortcut if the window closes mid-capture.
    pub fn closing(&mut self) {
        self.stop_capturing();
    }

    pub fn subscription(&self) -> Subscription<Message> {
        // Keys reach the window only while it has focus: Esc to close, or a new shortcut.
        keyboard::listen().map(Message::Key)
    }

    pub fn view(&self) -> Element<'_, Message> {
        let p = palette();
        let content = match self.tab {
            Tab::General => self.general(&p),
            Tab::History => self.history_list(&p),
            Tab::About => self.about(&p),
        };
        let title = match self.tab {
            Tab::General => "General",
            Tab::History => "History",
            Tab::About => "About",
        };
        let notice: Element<'_, Message> = match &self.notice {
            Some((words, tone)) => text(words.clone()).size(size::SMALL).color(tone_color(&p, *tone)).into(),
            None => Space::new().into(),
        };
        let header = row![
            text(title).size(size::TITLE).line_height(theme::line_height(size::TITLE)).font(theme::UI_SEMIBOLD),
            Space::new().width(Length::Fill),
            ghost("×", Message::Close, 1.0),
        ]
        .align_y(Alignment::Center);
        let (rail, rail_hover) = (p.elevated, p.text_subtle);
        // A thin, quiet scrollbar that only firms up under the pointer.
        let body = scrollable(content)
            .height(Length::Fill)
            .direction(scrollable::Direction::Vertical(scrollable::Scrollbar::new().width(4).scroller_width(4)))
            .style(move |theme, status| {
                let mut style = scrollable::default(theme, status);
                let hovered = matches!(status, scrollable::Status::Hovered { .. } | scrollable::Status::Dragged { .. });
                style.vertical_rail.background = None;
                style.vertical_rail.border = Border::default();
                style.vertical_rail.scroller.background = Background::Color(if hovered { rail_hover } else { rail });
                style.vertical_rail.scroller.border = Border { radius: 2.0.into(), ..Border::default() };
                style
            });
        let main = column![header, notice, body].spacing(space::S3).padding([space::S5, space::S6]);

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
        let brand = row![mark, text("murmur").size(size::BODY).font(theme::UI_SEMIBOLD)]
            .spacing(space::S2)
            .align_y(Alignment::Center);
        let nav = column([Tab::General, Tab::History, Tab::About].map(|tab| self.nav_item(p, tab))).spacing(space::S1);
        container(
            column![
                brand,
                Space::new().height(space::S5),
                nav,
                Space::new().height(Length::Fill),
                text(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .size(size::CAPTION)
                    .font(theme::MONO)
                    .color(p.text_subtle),
            ]
            .padding([space::S5, space::S4]),
        )
        .width(184)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(surface)),
            ..container::Style::default()
        })
        .into()
    }

    fn nav_item(&self, p: &Palette, tab: Tab) -> Element<'_, Message> {
        let label = match tab {
            Tab::General => "General",
            Tab::History => "History",
            Tab::About => "About",
        };
        let selected = self.tab == tab;
        let (on, hover, rest, ink) = (p.elevated, lighten(p.surface, 0.02), p.text_muted, p.text);
        button(text(label).size(size::SMALL).font(theme::UI_MEDIUM))
            .on_press(Message::Tab(tab))
            .width(Length::Fill)
            .padding([space::S2, space::S3])
            .style(move |_, status| button::Style {
                background: match (selected, status) {
                    (true, _) => Some(Background::Color(on)),
                    (false, button::Status::Hovered) => Some(Background::Color(hover)),
                    _ => None,
                },
                text_color: if selected { ink } else { rest },
                border: Border { radius: radius::CONTROL.into(), ..Border::default() },
                ..button::Style::default()
            })
            .into()
    }

    fn general(&self, p: &Palette) -> Element<'_, Message> {
        let chosen = self.chosen();
        let shortcut: Element<'_, Message> = match self.capturing {
            Some(held) => {
                let mut caps = row![].spacing(space::S3).align_y(Alignment::Center);
                for part in capture::held_parts(held) {
                    caps = caps.push(keycap(part, true, false)).push(text("+").size(size::BODY).color(p.text_subtle));
                }
                let caps = caps.push(keycap("…", false, true));
                column![
                    row![caps, Space::new().width(Length::Fill), ghost("Keep current", Message::KeepShortcut, 1.0)]
                        .align_y(Alignment::Center),
                    hint(p, "Hold the modifiers, then press the key. Esc keeps the one you have."),
                ]
                .spacing(space::S2)
                .into()
            }
            None => row![
                keycaps(&chosen.shortcut.parts(), false),
                Space::new().width(Length::Fill),
                ghost("Change", Message::ChangeShortcut, 1.0)
            ]
            .align_y(Alignment::Center)
            .into(),
        };
        let modes = column(Mode::ALL.map(|mode| choice(p, mode, chosen.mode == mode))).spacing(space::S1);
        let switches = column![
            setting(
                p,
                "Sounds",
                "Quiet cues when recording starts and stops.",
                chosen.sounds,
                Message::Sounds(!chosen.sounds)
            ),
            setting(
                p,
                "Voice commands",
                "Say \u{201c}new line\u{201d}, \u{201c}press enter\u{201d} or \u{201c}scratch that\u{201d}.",
                chosen.commands,
                Message::Commands(!chosen.commands)
            ),
            setting(
                p,
                "Keep history",
                "Your dictations stay on this computer, nowhere else.",
                chosen.history,
                Message::KeepHistory(!chosen.history)
            ),
            setting(
                p,
                "Start with Windows",
                "murmur is ready as soon as you sign in.",
                chosen.start_with_windows,
                Message::StartWithWindows(!chosen.start_with_windows)
            ),
        ]
        .spacing(space::S4);
        column![
            section(p, "Shortcut", shortcut),
            section(p, "How it starts", modes.into()),
            section(p, "While you talk", switches.into()),
        ]
        .spacing(space::S6)
        .padding(iced::Padding { right: space::S4, ..iced::Padding::ZERO })
        .into()
    }

    fn history_list(&self, p: &Palette) -> Element<'_, Message> {
        if self.history.is_empty() {
            let shortcut = self.chosen().shortcut;
            let why = if self.chosen().history {
                format!("Hold {shortcut} and talk. What you say shows up here, and only here.")
            } else {
                "History is off. Turn on \u{201c}Keep history\u{201d} in General to keep your dictations.".into()
            };
            return column![
                text("Nothing yet").size(size::BODY).font(theme::UI_MEDIUM),
                text(why).size(size::SMALL).color(p.text_muted),
            ]
            .spacing(space::S1)
            .padding([space::S6, 0.0])
            .into();
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let count = match self.history.len() {
            1 => "1 dictation, newest first. Click one to copy it.".to_string(),
            n if n >= HISTORY_SHOWN => format!("The latest {n}, newest first. Click one to copy it."),
            n => format!("{n} dictations, newest first. Click one to copy it."),
        };
        let clear = if self.confirm_clear { "Click again to clear" } else { "Clear" };
        let top = row![hint(p, &count), Space::new().width(Length::Fill), ghost(clear, Message::Clear, 1.0)]
            .align_y(Alignment::Center);
        let rows =
            column(self.history.iter().enumerate().map(|(i, entry)| self.entry(p, i, entry, now))).spacing(space::S1);
        column![top, rows].spacing(space::S3).padding(iced::Padding { right: space::S4, ..iced::Padding::ZERO }).into()
    }

    fn entry<'a>(&self, p: &Palette, index: usize, entry: &'a Entry, now: u64) -> Element<'a, Message> {
        let preview: String = if entry.text.chars().count() > PREVIEW_CHARS {
            format!("{}…", entry.text.chars().take(PREVIEW_CHARS).collect::<String>().trim_end())
        } else {
            entry.text.clone()
        };
        let mut meta = format!("{} · {:.1} s", history::ago(entry.at, now), entry.seconds);
        if self.copied == Some(index) {
            meta.push_str(" · Copied");
        }
        let (hover, ink, muted, success) = (p.surface, p.text, p.text_subtle, p.success);
        let copied = self.copied == Some(index);
        button(
            column![
                text(preview).size(size::BODY).line_height(theme::line_height(size::BODY)).color(ink),
                text(meta).size(size::CAPTION).color(if copied { success } else { muted }),
            ]
            .spacing(space::S1),
        )
        .on_press(Message::Copy(index))
        .width(Length::Fill)
        .padding([space::S3, space::S3])
        .style(move |_, status| button::Style {
            background: matches!(status, button::Status::Hovered | button::Status::Pressed)
                .then_some(Background::Color(hover)),
            text_color: ink,
            border: Border { radius: radius::CONTROL.into(), ..Border::default() },
            ..button::Style::default()
        })
        .into()
    }

    fn about(&self, p: &Palette) -> Element<'_, Message> {
        let copy = onboarding::codex_copy(&self.codex);
        let status = self.dictation.lock().map(|s| s.clone()).unwrap_or_else(|poisoned| poisoned.into_inner().clone());
        let tint = tone_color(p, copy.tone);
        let account = row![
            container(Space::new()).width(8).height(8).style(move |_| pill_fill(tint)),
            column![
                text(copy.title).size(size::BODY).font(theme::UI_MEDIUM),
                text(copy.detail).size(size::SMALL).color(p.text_muted),
            ]
            .spacing(2),
        ]
        .spacing(space::S3)
        .align_y(Alignment::Center);
        let session = match (status.dictations, &status.last) {
            (0, _) => "No dictations since murmur started.".to_string(),
            (n, Some(last)) if last.error.is_none() => {
                format!("{n} since murmur started. The last one pasted {} ms after you let go.", last.latency_ms)
            }
            (n, _) => format!("{n} since murmur started."),
        };
        column![
            section(p, "Transcribing with", account.into()),
            section(p, "This session", hint(p, &session)),
            section(
                p,
                "murmur",
                column![
                    hint(p, "Free and open source, MIT. Your audio goes to ChatGPT through Codex and nowhere else."),
                    row![
                        ghost("Website", Message::Open(SITE), 1.0),
                        ghost("Source on GitHub", Message::Open(REPO), 1.0),
                        ghost("Run setup again", Message::RunSetup, 1.0),
                    ]
                    .spacing(space::S1),
                ]
                .spacing(space::S2)
                .into()
            ),
        ]
        .spacing(space::S6)
        .into()
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

fn hint<'a>(p: &Palette, words: &str) -> Element<'a, Message> {
    text(words.to_string()).size(size::SMALL).line_height(theme::line_height(size::SMALL)).color(p.text_muted).into()
}

/// A labelled group: small heading, then its controls.
fn section<'a>(p: &Palette, label: &'a str, body: Element<'a, Message>) -> Element<'a, Message> {
    column![text(label).size(size::CAPTION).font(theme::UI_MEDIUM).color(p.text_subtle), body].spacing(space::S3).into()
}

/// One row of a pick-one list: a round marker, the name, and what it does.
fn choice<'a>(p: &Palette, mode: Mode, selected: bool) -> Element<'a, Message> {
    let (accent, edge, hover, ink, muted) = (p.accent, p.text_subtle, p.surface, p.text, p.text_muted);
    let marker = container(if selected {
        Element::from(container(Space::new()).width(8).height(8).style(move |_| pill_fill(accent)))
    } else {
        Space::new().into()
    })
    .center(16)
    .style(move |_| container::Style {
        border: Border { radius: 999.0.into(), width: 1.5, color: if selected { accent } else { edge } },
        ..container::Style::default()
    });
    button(
        row![
            marker,
            column![
                text(mode.name()).size(size::BODY).font(theme::UI_MEDIUM).color(ink),
                text(mode.explain()).size(size::SMALL).color(muted),
            ]
            .spacing(2),
        ]
        .spacing(space::S3)
        .align_y(Alignment::Center),
    )
    .on_press(Message::Mode(mode))
    .width(Length::Fill)
    .padding([space::S2, space::S3])
    .style(move |_, status| button::Style {
        background: matches!(status, button::Status::Hovered).then_some(Background::Color(hover)),
        text_color: ink,
        border: Border { radius: radius::CONTROL.into(), ..Border::default() },
        ..button::Style::default()
    })
    .into()
}

/// A setting with a switch on the right.
fn setting<'a>(p: &Palette, label: &'a str, explain: &'a str, on: bool, flip: Message) -> Element<'a, Message> {
    row![
        column![
            text(label).size(size::BODY).font(theme::UI_MEDIUM),
            text(explain).size(size::SMALL).color(p.text_muted)
        ]
        .spacing(2)
        .width(Length::Fill),
        switch(on, flip),
    ]
    .spacing(space::S4)
    .align_y(Alignment::Center)
    .into()
}

/// The shortcut as people say it, for the tray menu and tooltip.
pub fn hold_to_talk(binding: Binding) -> String {
    format!("Hold {binding} to talk")
}
