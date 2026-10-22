//! Where likh lives while it runs: a tray icon with a small menu, and the
//! main thread that owns it.
//!
//! The main thread runs iced as a daemon: no windows until one is needed (the
//! likh window, onboarding), and no frames or timers in between. Everything
//! else (the shortcut, the pill, the dictation, `likh ctl`) runs on its own
//! thread and doesn't depend on it.

use crate::dictation::Status;
use crate::onboarding::{self, Onboarding};
use crate::panel::{self, Panel};
use crate::settings::Settings;
use crate::shared::{icon, theme};
use crate::update::{State, Updates};
use iced::futures::channel::mpsc::{Sender, UnboundedReceiver};
use iced::{Element, Subscription, Task, Theme, window};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[cfg(test)]
mod tests;

const OPEN: &str = "open";
const SETUP: &str = "setup";
const QUIT: &str = "quit";
const UPDATE: &str = "update";
const COPY_LAST: &str = "copy-last";
/// How much of the last dictation its menu line shows.
const PREVIEW_CHARS: usize = 28;

/// Something the person picked from the tray, or news for the windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Open,
    Setup,
    Quit,
    /// The update line: checks, or restarts into a downloaded update.
    Update,
    /// Where updating stands changed (sent by `update::Updates`): refresh the menu and windows.
    UpdateChanged,
    /// "Copy last dictation": puts it on the clipboard. A paste from here would
    /// land nowhere, since opening the menu takes focus from the text box.
    CopyLast,
    /// A dictation was pasted (sent by the dictation thread): refresh the copy line.
    Dictated,
    /// The pill's "Pick mic": open the likh window at the Microphone row.
    #[cfg_attr(
        not(target_os = "windows"),
        allow(dead_code, reason = "only the Windows pill has a Pick mic button; macOS (alpha) has no pill window yet")
    )]
    Microphone,
    /// The pill's Review or Details on a command: open the likh window at Commands, on the command key's half.
    #[cfg_attr(
        not(target_os = "windows"),
        allow(dead_code, reason = "only the Windows pill has action buttons; macOS (alpha) has no pill window yet")
    )]
    Actions,
    /// The pill's "Change keys" when another app has the shortcut: open the
    /// likh window at Dictation, already listening for new keys.
    #[cfg_attr(
        not(target_os = "windows"),
        allow(dead_code, reason = "only the Windows pill has buttons; macOS (alpha) has no pill window yet")
    )]
    Shortcut,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tray(Pick),
    Setup(window::Id, onboarding::Message),
    Panel(window::Id, panel::Message),
    Closed(window::Id),
}

struct App {
    dictation: Arc<Mutex<Status>>,
    settings: Arc<RwLock<Settings>>,
    updates: Updates,
    tray: Option<Tray>,
    setup: Option<(window::Id, Onboarding)>,
    panel: Option<(window::Id, Panel)>,
    /// Written only after Finish, so dismissing setup doesn't skip it forever.
    first_run_marker: Option<PathBuf>,
    ui_requests: UiRequests,
}

#[derive(Clone)]
struct UiRequests(Arc<Mutex<Option<UnboundedReceiver<Pick>>>>);

impl Hash for UiRequests {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

/// The icon and the menu lines that change: the shortcut, the last
/// dictation, and the update line. Kept alive for as long as likh runs;
/// dropping it removes the icon.
struct Tray {
    icon: TrayIcon,
    shortcut: MenuItem,
    copy_last: MenuItem,
    update: MenuItem,
}

/// Runs until "Quit likh". With `first_run_marker`, onboarding opens
/// straight away and the marker is written once it finishes.
pub fn run(
    dictation: Arc<Mutex<Status>>,
    settings: Arc<RwLock<Settings>>,
    first_run_marker: Option<PathBuf>,
    ui_requests: UnboundedReceiver<Pick>,
    updates: Updates,
) -> iced::Result {
    let ui_requests = UiRequests(Arc::new(Mutex::new(Some(ui_requests))));
    let boot = move || {
        let shortcut = settings.read().map(|s| s.shortcut).unwrap_or_default();
        let mut app = App {
            dictation: dictation.clone(),
            settings: settings.clone(),
            updates: updates.clone(),
            tray: build(&panel::hold_to_talk(shortcut)),
            setup: None,
            panel: None,
            first_run_marker: first_run_marker.clone(),
            ui_requests: ui_requests.clone(),
        };
        let task = if app.first_run_marker.is_some() { app.open_setup() } else { Task::none() };
        (app, task)
    };
    let mut daemon = iced::daemon(boot, App::update, App::view)
        .title(|_: &App, _| "likh".to_string())
        .subscription(App::subscription)
        .theme(|_: &App, _| Theme::Dark)
        .default_font(theme::UI);
    for font in theme::FONT_FILES {
        daemon = daemon.font(font);
    }
    daemon.run()
}

impl App {
    fn open_setup(&mut self) -> Task<Message> {
        if let Some((id, _)) = &self.setup {
            return window::gain_focus(*id);
        }
        let (id, opened) = window::open(onboarding::window_settings());
        let setup = Onboarding::in_app(id, self.dictation.clone(), self.settings.clone())
            .hold_still(crate::pill::animations_off());
        self.setup = Some((id, setup));
        opened.discard()
    }

    fn open_panel(&mut self) -> Task<Message> {
        if let Some((id, _)) = &self.panel {
            return window::gain_focus(*id);
        }
        let (id, opened) = window::open(panel::window_settings());
        self.panel = Some((
            id,
            Panel::new(id, self.settings.clone(), self.dictation.clone(), self.updates.clone(), panel::Files::real()),
        ));
        opened.discard()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = match message {
            Message::Tray(Pick::Open) => {
                if let Some((id, _)) = self.setup {
                    window::gain_focus(id)
                } else if self.first_run_marker.is_some() {
                    self.open_setup()
                } else {
                    self.open_panel()
                }
            }
            Message::Tray(pick @ (Pick::Microphone | Pick::Actions | Pick::Shortcut)) => {
                let tab = if pick == Pick::Actions { panel::Tab::Commands } else { panel::Tab::Dictation };
                let opened = self.open_panel();
                match &mut self.panel {
                    Some((id, panel)) => {
                        let id = *id;
                        let mut to_tab = panel.update(panel::Message::Tab(tab));
                        if pick == Pick::Shortcut {
                            to_tab = Task::batch([to_tab, panel.update(panel::Message::ChangeShortcut)]);
                        }
                        // Review and Details are about a command-key command.
                        if pick == Pick::Actions {
                            let view = panel::CommandsView::CommandKey;
                            to_tab = Task::batch([to_tab, panel.update(panel::Message::CommandsView(view))]);
                        }
                        Task::batch([opened, to_tab.map(move |message| Message::Panel(id, message))])
                    }
                    None => opened,
                }
            }
            Message::Tray(Pick::Setup) | Message::Panel(_, panel::Message::RunSetup) => self.open_setup(),
            Message::Tray(Pick::Quit) => iced::exit(),
            Message::Tray(Pick::Update) => {
                match self.updates.state() {
                    State::Ready { .. } => self.restart_to_update(),
                    State::Available { page, .. } => crate::shared::open_url(&page),
                    _ => self.updates.check_now(),
                }
                Task::none()
            }
            Message::Panel(_, panel::Message::Restart) => {
                self.restart_to_update();
                Task::none()
            }
            // Nothing to do here: the menu below, and every window on its next draw, read the new state.
            Message::Tray(Pick::UpdateChanged | Pick::Dictated) => Task::none(),
            Message::Tray(Pick::CopyLast) => {
                let last = self.dictation.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).last_text.clone();
                if let Some(Err(reason)) = last.map(|text| crate::paste::copy(&text)) {
                    crate::diagnostics::error("tray", format!("couldn't copy the last dictation: {reason}"));
                }
                Task::none()
            }
            Message::Setup(id, message) => match &mut self.setup {
                Some((open, onboarding)) if *open == id => {
                    onboarding.update(message).map(move |message| Message::Setup(id, message))
                }
                _ => Task::none(),
            },
            Message::Panel(id, message) => match &mut self.panel {
                Some((open, panel)) if *open == id => {
                    panel.update(message).map(move |message| Message::Panel(id, message))
                }
                _ => Task::none(),
            },
            Message::Closed(id) => {
                let mut finished = false;
                if let Some((_, onboarding)) = self.setup.as_mut().filter(|(open, _)| *open == id) {
                    finished = onboarding.finished();
                    onboarding.closing();
                    self.setup = None;
                    if let Some(marker) = self.first_run_marker.as_ref().filter(|_| finished) {
                        // Start with Windows is on out of the box, so likh is there after a restart.
                        // A test instance (`LIKH_HOME`) leaves the Run key alone: it would start the person's likh.
                        let start = self.settings.read().is_ok_and(|s| s.start_with_windows);
                        if start
                            && std::env::var_os("LIKH_HOME").is_none()
                            && let Err(reason) = crate::autostart::set(true)
                        {
                            crate::diagnostics::warn("autostart", format!("couldn't start with Windows: {reason}"));
                        }
                        let _ = marker.parent().map(std::fs::create_dir_all);
                        if let Err(error) = std::fs::write(marker, "") {
                            crate::diagnostics::warn("onboarding", format!("couldn't save setup completion: {error}"));
                        } else {
                            self.first_run_marker = None;
                        }
                    }
                }
                if let Some((_, panel)) = self.panel.as_mut().filter(|(open, _)| *open == id) {
                    panel.closing();
                    self.panel = None;
                }
                if finished { self.open_panel() } else { Task::none() }
            }
        };
        // The shortcut may have just changed, in either window.
        if let Some(tray) = &mut self.tray {
            let line = panel::hold_to_talk(self.settings.read().map(|s| s.shortcut).unwrap_or_default());
            if tray.shortcut.text() != line {
                tray.shortcut.set_text(&line);
                let _ = tray.icon.set_tooltip(Some(format!("likh · {line}")));
            }
            let last = self.dictation.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).last_text.clone();
            let (line, enabled) = copy_line(last.as_deref());
            if tray.copy_last.text() != line {
                tray.copy_last.set_text(line);
            }
            tray.copy_last.set_enabled(enabled);
            let (line, enabled) = update_line(&self.updates.state());
            if tray.update.text() != line {
                tray.update.set_text(line);
            }
            tray.update.set_enabled(enabled);
        }
        task
    }

    fn restart_to_update(&self) {
        if let Err(reason) = self.updates.apply() {
            crate::diagnostics::error("update", format!("couldn't update: {reason}"));
        }
    }

    fn view(&self, id: window::Id) -> Element<'_, Message> {
        match (&self.setup, &self.panel) {
            (Some((open, onboarding)), _) if *open == id => {
                onboarding.view().map(move |message| Message::Setup(id, message))
            }
            (_, Some((open, panel))) if *open == id => panel.view().map(move |message| Message::Panel(id, message)),
            _ => iced::widget::Space::new().into(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let setup = match &self.setup {
            Some((id, onboarding)) => {
                onboarding.subscription().with(*id).map(|(id, message)| Message::Setup(id, message))
            }
            None => Subscription::none(),
        };
        let panel = match &self.panel {
            Some((id, panel)) => panel.subscription().with(*id).map(|(id, message)| Message::Panel(id, message)),
            None => Subscription::none(),
        };
        Subscription::batch([
            Subscription::run(picks).map(Message::Tray),
            Subscription::run_with(self.ui_requests.clone(), ui_requests).map(Message::Tray),
            window::close_events().map(Message::Closed),
            setup,
            panel,
        ])
    }
}

fn ui_requests(inbox: &UiRequests) -> UnboundedReceiver<Pick> {
    inbox.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take().expect("UI inbox subscribes once")
}

/// The icon and its menu. None if the shell has no tray (then likh still runs; `likh ctl quit` stops it).
fn build(hold_to_talk: &str) -> Option<Tray> {
    // Linux runs likh only for agents and tests (a cloud machine has no Windows):
    // tray-icon needs GTK's own loop running, which iced never starts, so there's
    // no icon there. Everything else runs and `likh ctl` drives it.
    if cfg!(target_os = "linux") {
        crate::diagnostics::info("tray", "no tray icon on Linux; drive likh with `likh ctl`.");
        return None;
    }
    let menu = Menu::new();
    let shortcut = MenuItem::new(hold_to_talk, false, None);
    let (line, enabled) = copy_line(None);
    let copy_last = MenuItem::with_id(COPY_LAST, line, enabled, None);
    let (line, enabled) = update_line(&State::Idle);
    let update = MenuItem::with_id(UPDATE, line, enabled, None);
    let items = [
        &shortcut as &dyn tray_icon::menu::IsMenuItem,
        &copy_last,
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(OPEN, "Open likh", true, None),
        &MenuItem::with_id(SETUP, "Run setup again…", true, None),
        &update,
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(QUIT, "Quit likh", true, None),
    ];
    if let Err(error) = menu.append_items(&items) {
        crate::diagnostics::error("tray", format!("no tray menu: {error}"));
    }
    let image = tray_icon::Icon::from_rgba(icon::rgba(32), 32, 32).ok();
    let mut builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip(format!("likh · {hold_to_talk}"));
    if let Some(image) = image {
        builder = builder.with_icon(image);
    }
    match builder.build() {
        Ok(icon) => Some(Tray { icon, shortcut, copy_last, update }),
        Err(error) => {
            crate::diagnostics::error("tray", format!("no tray icon: {error}"));
            None
        }
    }
}

/// The tray's update line for where updating stands, and whether picking it does anything.
pub fn update_line(state: &State) -> (String, bool) {
    match state {
        State::Idle => ("Check for updates".into(), true),
        State::Checking => ("Checking for updates\u{2026}".into(), false),
        State::UpToDate { .. } => ("Up to date \u{b7} Check again".into(), true),
        State::Downloading { version } => (format!("Downloading likh {version}\u{2026}"), false),
        State::Ready { version, .. } => (format!("Restart to update to {version}"), true),
        State::Available { version, .. } => (format!("Download likh {version}"), true),
        State::Failed { .. } => ("Couldn't check \u{b7} Try again".into(), true),
    }
}

/// The tray's line for the last dictation, with its first words, and whether it can be picked.
pub fn copy_line(last: Option<&str>) -> (String, bool) {
    match last.map(str::trim).filter(|text| !text.is_empty()) {
        None => ("Copy last dictation".into(), false),
        Some(text) => {
            let words: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
            let preview = match words.char_indices().nth(PREVIEW_CHARS) {
                Some((end, _)) => format!("{}\u{2026}", words[..end].trim_end()),
                None => words,
            };
            (format!("Copy \u{201c}{preview}\u{201d}"), true)
        }
    }
}

/// What a menu id means.
pub fn pick_for_menu(id: &str) -> Option<Pick> {
    match id {
        OPEN => Some(Pick::Open),
        COPY_LAST => Some(Pick::CopyLast),
        SETUP => Some(Pick::Setup),
        QUIT => Some(Pick::Quit),
        UPDATE => Some(Pick::Update),
        _ => None,
    }
}

/// Tray clicks and menu picks, as a stream. The menu opens on a right click;
/// a double click on the icon opens the likh window.
fn picks() -> impl iced::futures::Stream<Item = Pick> {
    iced::stream::channel(8, async |out: Sender<Pick>| {
        let menu_out = out.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some(pick) = pick_for_menu(&event.id.0) {
                let _ = menu_out.clone().try_send(pick);
            }
        }));
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } = event {
                let _ = out.clone().try_send(Pick::Open);
            }
        }));
    })
}
