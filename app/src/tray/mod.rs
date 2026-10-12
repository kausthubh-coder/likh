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

/// Something the person picked from the tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Open,
    Setup,
    Quit,
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

/// The icon and the one menu line that changes (the shortcut). Kept alive for as
/// long as likh runs; dropping it removes the icon.
struct Tray {
    icon: TrayIcon,
    shortcut: MenuItem,
}

/// Runs until "Quit likh". With `first_run_marker`, onboarding opens
/// straight away and the marker is written once it finishes.
pub fn run(
    dictation: Arc<Mutex<Status>>,
    settings: Arc<RwLock<Settings>>,
    first_run_marker: Option<PathBuf>,
    ui_requests: UnboundedReceiver<Pick>,
) -> iced::Result {
    let ui_requests = UiRequests(Arc::new(Mutex::new(Some(ui_requests))));
    let boot = move || {
        let shortcut = settings.read().map(|s| s.shortcut).unwrap_or_default();
        let mut app = App {
            dictation: dictation.clone(),
            settings: settings.clone(),
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
        self.setup = Some((id, Onboarding::in_app(id, self.dictation.clone(), self.settings.clone())));
        opened.discard()
    }

    fn open_panel(&mut self) -> Task<Message> {
        if let Some((id, _)) = &self.panel {
            return window::gain_focus(*id);
        }
        let (id, opened) = window::open(panel::window_settings());
        self.panel = Some((id, Panel::new(id, self.settings.clone(), self.dictation.clone(), panel::Files::real())));
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
            Message::Tray(Pick::Setup) | Message::Panel(_, panel::Message::RunSetup) => self.open_setup(),
            Message::Tray(Pick::Quit) => iced::exit(),
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
                        let _ = marker.parent().map(std::fs::create_dir_all);
                        if let Err(error) = std::fs::write(marker, "") {
                            eprintln!("likh: couldn't save setup completion: {error}");
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
        if let Some(tray) = &self.tray {
            let line = panel::hold_to_talk(self.settings.read().map(|s| s.shortcut).unwrap_or_default());
            if tray.shortcut.text() != line {
                tray.shortcut.set_text(&line);
                let _ = tray.icon.set_tooltip(Some(format!("likh · {line}")));
            }
        }
        task
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
    let menu = Menu::new();
    let shortcut = MenuItem::new(hold_to_talk, false, None);
    let items = [
        &shortcut as &dyn tray_icon::menu::IsMenuItem,
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(OPEN, "Open likh", true, None),
        &MenuItem::with_id(SETUP, "Set up again…", true, None),
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(QUIT, "Quit likh", true, None),
    ];
    if let Err(error) = menu.append_items(&items) {
        eprintln!("likh: no tray menu: {error}");
    }
    let image = tray_icon::Icon::from_rgba(icon::rgba(32), 32, 32).ok();
    let mut builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip(format!("likh · {hold_to_talk}"));
    if let Some(image) = image {
        builder = builder.with_icon(image);
    }
    match builder.build() {
        Ok(icon) => Some(Tray { icon, shortcut }),
        Err(error) => {
            eprintln!("likh: no tray icon: {error}");
            None
        }
    }
}

/// What a menu id means.
pub fn pick_for_menu(id: &str) -> Option<Pick> {
    match id {
        OPEN => Some(Pick::Open),
        SETUP => Some(Pick::Setup),
        QUIT => Some(Pick::Quit),
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
