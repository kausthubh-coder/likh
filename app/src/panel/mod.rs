//! The likh window, opened from the tray: the shortcut, mouse button and how
//! dictation starts and stops (General), which engine transcribes
//! (Transcription), the spoken commands (Commands), what you dictated
//! (History), and the version and links (About).
//!
//! Borderless like onboarding: drag it from anywhere empty, close it with the
//! × or Esc. Nothing in it animates, so it draws only when something changes.

use crate::codex_auth::CodexStatus;
use crate::commands::Group;
use crate::dictation::Status;
use crate::history::{self, Entry};
use crate::hotkey::{self, Binding, MouseTrigger, capture};
use crate::onboarding::{self, GroqSetup, Tone};
use crate::settings::{Mode, Settings, SilenceStop};
use crate::shared::icon;
use crate::shared::theme::{self, Palette, palette, radius, size, space};
use crate::shared::ui::{field_style, ghost, keycap, keycaps, lighten, pill_fill, primary, said, segmented, switch};
use crate::transcriber::{Engine, Failure, glyph, groq};
use crate::update::{self, Updates};
use iced::widget::{
    Space, button, canvas as canvas_widget, column, container, mouse_area, operation, row, scrollable, text, text_input,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Size, Subscription, Task, keyboard, window};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
mod tests;

pub const WINDOW: Size = Size::new(720.0, 520.0);
const REPO: &str = "https://github.com/kausthubh-coder/likh";
/// Plenty to scroll back through; the file keeps everything.
const HISTORY_SHOWN: usize = 200;
/// Long dictations are cut in the list; copying still copies all of it.
const PREVIEW_CHARS: usize = 240;
const GROQ_KEY: &str = "panel-groq-key";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    General,
    Transcription,
    Commands,
    History,
    About,
}

impl Tab {
    const ALL: [Tab; 5] = [Tab::General, Tab::Transcription, Tab::Commands, Tab::History, Tab::About];

    fn name(self) -> &'static str {
        match self {
            Tab::General => "General",
            Tab::Transcription => "Transcription",
            Tab::Commands => "Commands",
            Tab::History => "History",
            Tab::About => "About",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Tab(Tab),
    ChangeShortcut,
    KeepShortcut,
    Key(keyboard::Event),
    Mouse(MouseTrigger),
    Mode(Mode),
    SilenceStop(SilenceStop),
    Sounds(bool),
    PauseMedia(bool),
    Commands(bool),
    CommandGroup(Group, bool),
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
    updates: Updates,
    codex: CodexStatus,
    groq: GroqSetup,
    /// None until the Transcription tab looks.
    windows: Option<Result<(), Failure>>,
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
        updates: Updates,
        files: Files,
    ) -> Self {
        Self {
            window,
            tab: Tab::General,
            settings,
            dictation,
            updates,
            codex: onboarding::check_codex_now(),
            groq: GroqSetup::look(),
            windows: None,
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

    /// Looks again at whichever engine is picked.
    fn recheck(&mut self) {
        match self.chosen().engine {
            Engine::Codex => self.codex = onboarding::check_codex_now(),
            Engine::Groq => {}
            Engine::Windows => self.windows = Some(onboarding::check_windows_now()),
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tab(tab) => {
                self.stop_capturing();
                self.tab = tab;
                self.notice = None;
                match tab {
                    Tab::History => self.load_history(),
                    // Every card says where its engine stands, so look at all of them.
                    Tab::Transcription => {
                        self.codex = onboarding::check_codex_now();
                        if Engine::available().contains(&Engine::Windows) {
                            self.windows = Some(onboarding::check_windows_now());
                        }
                    }
                    Tab::About => self.recheck(),
                    Tab::General | Tab::Commands => {}
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
            Message::Mouse(trigger) => {
                self.change(|s| s.mouse = trigger);
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
            Message::KeepHistory(on) => self.change(|s| s.history = on),
            Message::StartWithWindows(on) => match crate::autostart::set(on) {
                Ok(()) => self.change(|s| s.start_with_windows = on),
                Err(reason) => self.notice = Some((format!("Windows didn't allow that: {reason}"), Tone::Danger)),
            },
            Message::Engine(engine) => {
                self.change(|s| s.engine = engine);
                self.recheck();
                if engine == Engine::Groq && !self.groq.saved {
                    return operation::focus(GROQ_KEY);
                }
            }
            Message::Recheck => self.recheck(),
            Message::GroqKey(key) => {
                self.groq.key = key;
                self.groq.notice = None;
            }
            Message::SaveGroqKey if self.groq.checking => {}
            Message::SaveGroqKey => return self.groq.save(Message::GroqChecked),
            Message::GroqChecked(key, answer) => self.groq.checked(&key, answer),
            Message::ReplaceGroqKey => {
                self.groq.saved = false;
                self.groq.notice = None;
                return operation::focus(GROQ_KEY);
            }
            Message::ForgetGroqKey => self.groq.forget(),
            Message::CheckForUpdates => self.updates.check_now(),
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
            Tab::Transcription => self.transcription(&p),
            Tab::Commands => self.commands(&p),
            Tab::History => self.history_list(&p),
            Tab::About => self.about(&p),
        };
        let notice: Element<'_, Message> = match &self.notice {
            Some((words, tone)) => text(words.clone()).size(size::SMALL).color(tone_color(&p, *tone)).into(),
            None => Space::new().into(),
        };
        let header = row![
            text(self.tab.name())
                .size(size::TITLE)
                .line_height(theme::line_height(size::TITLE))
                .font(theme::UI_SEMIBOLD),
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
        let brand = row![mark, text("likh").size(size::BODY).font(theme::UI_SEMIBOLD)]
            .spacing(space::S2)
            .align_y(Alignment::Center);
        let nav = column(Tab::ALL.map(|tab| self.nav_item(p, tab))).spacing(space::S1);
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
        let selected = self.tab == tab;
        let (on, hover, rest, ink) = (p.elevated, lighten(p.surface, 0.02), p.text_muted, p.text);
        button(text(tab.name()).size(size::SMALL).font(theme::UI_MEDIUM))
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
        let mouse_options = MouseTrigger::ALL.map(|trigger| (trigger, trigger.name()));
        let mouse = labelled(
            p,
            "Mouse button",
            "Hold it to talk, like the shortcut. Its usual click stops while it's picked.",
            segmented(&mouse_options, chosen.mouse, Message::Mouse),
        );
        let modes = column(Mode::ALL.map(|mode| choice(p, mode, chosen.mode == mode))).spacing(space::S1);
        let quiet_options = SilenceStop::ALL.map(|after| (after, after.name()));
        let quiet = labelled(
            p,
            "Stop after quiet",
            "In hands-free, once you've said something, likh pastes after this much quiet.",
            segmented(&quiet_options, chosen.silence_stop, Message::SilenceStop),
        );
        let mut switches = column![setting(
            p,
            "Sounds",
            "Quiet cues when recording starts and stops.",
            chosen.sounds,
            Message::Sounds(!chosen.sounds)
        )]
        .spacing(space::S4);
        if crate::media::SUPPORTED {
            switches = switches.push(setting(
                p,
                "Pause music and videos",
                "While you talk, then they play again.",
                chosen.pause_media,
                Message::PauseMedia(!chosen.pause_media),
            ));
        }
        let switches = switches
            .push(setting(
                p,
                "Keep history",
                "Your dictations stay on this computer, nowhere else.",
                chosen.history,
                Message::KeepHistory(!chosen.history),
            ))
            .push(setting(
                p,
                "Start with Windows",
                "likh is ready as soon as you sign in.",
                chosen.start_with_windows,
                Message::StartWithWindows(!chosen.start_with_windows),
            ));
        column![
            section(p, "Shortcut", column![shortcut, mouse].spacing(space::S5).into()),
            section(p, "How it starts", column![modes, quiet].spacing(space::S5).into()),
            section(p, "While you talk", switches.into()),
        ]
        .spacing(space::S6)
        .padding(iced::Padding { right: space::S4, ..iced::Padding::ZERO })
        .into()
    }

    fn transcription(&self, p: &Palette) -> Element<'_, Message> {
        let picked = self.chosen().engine;
        let cards = row(Engine::available().iter().map(|engine| self.engine_card(p, *engine, *engine == picked)))
            .spacing(space::S2);
        let copy = onboarding::engine_copy(picked, &self.codex, &self.groq, self.windows.as_ref());
        let tint = tone_color(p, copy.tone);
        let state = row![
            container(Space::new()).width(8).height(8).style(move |_| pill_fill(tint)),
            column![
                text(copy.title).size(size::BODY).font(theme::UI_MEDIUM),
                text(copy.detail).size(size::SMALL).color(p.text_muted),
            ]
            .spacing(2),
        ]
        .spacing(space::S3)
        .align_y(Alignment::Center);
        let actions: Element<'_, Message> = match picked {
            Engine::Codex => {
                let mut actions = row![ghost("Check again", Message::Recheck, 1.0)].spacing(space::S1);
                if self.codex == CodexStatus::NotInstalled {
                    actions = actions.push(ghost("Get Codex", Message::Open(crate::shared::CODEX_SETUP), 1.0));
                }
                actions.into()
            }
            Engine::Groq if self.groq.saved => row![
                ghost("Use another key", Message::ReplaceGroqKey, 1.0),
                ghost("Remove key", Message::ForgetGroqKey, 1.0),
            ]
            .spacing(space::S1)
            .into(),
            Engine::Groq => {
                let field = text_input("Paste your key, gsk_\u{2026}", &self.groq.key)
                    .id(GROQ_KEY)
                    .secure(true)
                    .on_input(Message::GroqKey)
                    .on_paste(Message::GroqKey)
                    .on_submit(Message::SaveGroqKey)
                    .size(size::BODY)
                    .padding(space::S3)
                    .style(|_, status| field_style(status));
                let save = if self.groq.checking { "Checking\u{2026}" } else { "Save key" };
                column![
                    field,
                    row![
                        primary(save, Message::SaveGroqKey, 1.0),
                        ghost("Get a free key", Message::Open(groq::KEYS_URL), 1.0)
                    ]
                    .spacing(space::S2)
                    .align_y(Alignment::Center),
                ]
                .spacing(space::S3)
                .into()
            }
            Engine::Windows => row![
                ghost("Open speech settings", Message::Open(onboarding::SPEECH_SETTINGS_URL), 1.0),
                ghost("Check again", Message::Recheck, 1.0),
            ]
            .spacing(space::S1)
            .into(),
        };
        let groq_notice: Element<'_, Message> = match (&self.groq.notice, picked) {
            (Some((words, tone)), Engine::Groq) => {
                text(words.clone()).size(size::SMALL).color(tone_color(p, *tone)).into()
            }
            _ => Space::new().into(),
        };
        column![
            section(p, "Transcribe with", cards.into()),
            section(p, picked.explain(), column![state, groq_notice, actions].spacing(space::S3).into()),
        ]
        .spacing(space::S6)
        .padding(iced::Padding { right: space::S4, ..iced::Padding::ZERO })
        .into()
    }

    /// One engine as a card: its glyph, its name, and a word on its state.
    /// The picked card lifts and takes the peach edge; its glyph turns peach.
    fn engine_card<'a>(&self, p: &Palette, engine: Engine, picked: bool) -> Element<'a, Message> {
        let (status, ok) = self.short_status(engine);
        let (lifted, rest, accent, ink, muted) = (p.elevated, p.surface, p.accent, p.text, p.text_muted);
        let backdrop = if picked { lifted } else { rest };
        let mark = canvas_widget(glyph::Glyph { engine, color: if picked { accent } else { muted }, backdrop })
            .width(40)
            .height(32);
        let dot = if ok { p.success } else { p.text_subtle };
        let card = column![
            mark,
            text(engine.name()).size(size::BODY).font(theme::UI_MEDIUM).color(ink),
            row![
                container(Space::new()).width(6).height(6).style(move |_| pill_fill(dot)),
                text(status).size(size::CAPTION).color(muted),
            ]
            .spacing(space::S2)
            .align_y(Alignment::Center),
        ]
        .spacing(space::S2);
        let hover = lighten(rest, 0.02);
        button(card)
            .on_press(Message::Engine(engine))
            .width(Length::Fill)
            .padding(space::S4)
            .style(move |_, status| button::Style {
                background: Some(Background::Color(match (picked, status) {
                    (true, _) => lifted,
                    (false, button::Status::Hovered | button::Status::Pressed) => hover,
                    _ => rest,
                })),
                text_color: ink,
                border: Border {
                    radius: radius::CARD.into(),
                    width: if picked { 1.5 } else { 1.0 },
                    color: if picked { accent } else { Color::from_rgba(1.0, 1.0, 1.0, 0.04) },
                },
                ..button::Style::default()
            })
            .into()
    }

    /// A few words for a card: what state the engine is in, and whether it's ready.
    fn short_status(&self, engine: Engine) -> (&'static str, bool) {
        match engine {
            Engine::Codex => match self.codex {
                CodexStatus::Connected(_) => ("Connected", true),
                CodexStatus::NotInstalled => ("Needs Codex", false),
                CodexStatus::SignedOut => ("Signed out", false),
                CodexStatus::Expired(_) => ("Sign-in expired", false),
            },
            Engine::Groq if self.groq.saved => ("Key saved", true),
            Engine::Groq => ("Needs a key", false),
            Engine::Windows => match self.windows {
                None => ("Not checked", false),
                Some(Ok(())) => ("Ready", true),
                Some(Err(Failure::WindowsPrivacy)) => ("Turned off", false),
                Some(Err(_)) => ("Unavailable", false),
            },
        }
    }

    fn commands(&self, p: &Palette) -> Element<'_, Message> {
        let chosen = self.chosen();
        let master = setting(
            p,
            "Spoken commands",
            "Phrases that do something instead of being typed.",
            chosen.commands,
            Message::Commands(!chosen.commands),
        );
        if !chosen.commands {
            return column![master, hint(p, "Off. Everything you say is typed as you said it.")]
                .spacing(space::S4)
                .padding(iced::Padding { right: space::S4, ..iced::Padding::ZERO })
                .into();
        }
        let groups = column(Group::ALL.map(|group| {
            let on = group.is_on(&chosen.command_groups);
            let phrases = row(group.phrases().iter().map(|(phrase, result)| {
                row![said(phrase), text(*result).size(size::CAPTION).font(theme::MONO).color(p.text_subtle)]
                    .spacing(space::S1 + 2.0)
                    .align_y(Alignment::Center)
                    .into()
            }))
            .spacing(space::S4)
            .wrap()
            .vertical_spacing(space::S2);
            let mut block = column![setting(p, group.name(), group.explain(), on, Message::CommandGroup(group, !on))]
                .spacing(space::S3);
            if on {
                block = block.push(phrases);
            }
            block.into()
        }))
        .spacing(space::S5);
        column![master, section(p, "Say", groups.into())]
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
        let engine = self.chosen().engine;
        let copy = onboarding::engine_copy(engine, &self.codex, &self.groq, self.windows.as_ref());
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
            (0, _) => "No dictations since likh started.".to_string(),
            (n, Some(last)) if last.error.is_none() => {
                format!("{n} since likh started. The last one pasted {} ms after you let go.", last.latency_ms)
            }
            (n, _) => format!("{n} since likh started."),
        };
        let audio = match engine {
            Engine::Codex => "Free and open source, MIT. Your audio goes to ChatGPT through Codex and nowhere else.",
            Engine::Groq => "Free and open source, MIT. Your audio goes to Groq with your key and nowhere else.",
            Engine::Windows => {
                "Free and open source, MIT. Your audio goes to Microsoft's speech service and nowhere else."
            }
        };
        column![
            section(p, "Transcribing with", account.into()),
            section(p, "This session", hint(p, &session)),
            section(p, "Updates", self.updates_view(p)),
            section(
                p,
                "likh",
                column![
                    hint(p, audio),
                    row![
                        ghost("Website", Message::Open(crate::shared::SITE), 1.0),
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
        .padding(iced::Padding { right: space::S4, ..iced::Padding::ZERO })
        .into()
    }

    /// The running version, where updating stands, and what you can do about it.
    fn updates_view(&self, p: &Palette) -> Element<'_, Message> {
        let chosen = self.chosen();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let this = format!("This is likh {}.", env!("CARGO_PKG_VERSION"));
        let (line, tone, action): (String, Tone, Option<Element<'_, Message>>) = match self.updates.state() {
            update::State::Idle => (this, Tone::Neutral, Some(ghost("Check now", Message::CheckForUpdates, 1.0))),
            update::State::Checking => ("Checking for a newer likh\u{2026}".into(), Tone::Neutral, None),
            update::State::UpToDate { checked_at } => (
                format!("{this} It's the newest, checked {}.", history::ago(checked_at, now)),
                Tone::Neutral,
                Some(ghost("Check now", Message::CheckForUpdates, 1.0)),
            ),
            update::State::Downloading { version } => {
                (format!("Downloading likh {version}\u{2026}"), Tone::Neutral, None)
            }
            update::State::Ready { version, .. } => (
                format!("likh {version} is downloaded and checked."),
                Tone::Success,
                Some(primary("Restart to update", Message::Restart, 1.0)),
            ),
            update::State::Available { version, page } => (
                format!("likh {version} is out."),
                Tone::Success,
                Some(ghost("Download it", Message::OpenRelease(page), 1.0)),
            ),
            update::State::Failed { reason } => {
                (reason, Tone::Warning, Some(ghost("Try again", Message::CheckForUpdates, 1.0)))
            }
        };
        let status = row![
            text(line).size(size::SMALL).color(tone_color(p, tone)).width(Length::Fill),
            action.unwrap_or_else(|| Space::new().into()),
        ]
        .spacing(space::S4)
        .align_y(Alignment::Center);
        let what = if update::INSTALLS {
            "Once a day, when you open likh or dictate. You choose when it restarts."
        } else {
            "Once a day, when you open likh or dictate. On macOS you download it yourself."
        };
        column![
            status,
            setting(
                p,
                "Check for updates automatically",
                what,
                chosen.auto_update,
                Message::AutoUpdate(!chosen.auto_update)
            ),
        ]
        .spacing(space::S4)
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

/// A name and what it does, with a wider control (a segmented pick) underneath.
fn labelled<'a>(p: &Palette, label: &'a str, explain: &'a str, control: Element<'a, Message>) -> Element<'a, Message> {
    column![
        column![
            text(label).size(size::BODY).font(theme::UI_MEDIUM),
            text(explain).size(size::SMALL).color(p.text_muted)
        ]
        .spacing(2),
        control,
    ]
    .spacing(space::S3)
    .into()
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
