//! Where murmur lives while it runs: a tray icon with a small menu, and the
//! main thread that owns it.
//!
//! The main thread runs iced as a daemon: no windows until one is needed
//! (onboarding the first time, or "Set up murmur…" from the menu), and no
//! frames or timers in between. Everything else (the shortcut, the pill, the
//! dictation, `murmur ctl`) runs on its own thread and doesn't depend on it.

use crate::dictation::Status;
use crate::onboarding::{self, Onboarding};
use crate::shared::{icon, theme};
use iced::futures::channel::mpsc::Sender;
use iced::{Element, Subscription, Task, Theme, window};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[cfg(test)]
mod tests;

const SETUP: &str = "setup";
const QUIT: &str = "quit";
pub const TOOLTIP: &str = "murmur · Hold Ctrl + Space to talk";

/// Something the person picked from the tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Setup,
    Quit,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tray(Pick),
    Setup(window::Id, onboarding::Message),
    Closed(window::Id),
}

struct App {
    dictation: Arc<Mutex<Status>>,
    /// Kept alive for as long as murmur runs; dropping it removes the icon.
    _icon: Option<TrayIcon>,
    setup: Option<(window::Id, Onboarding)>,
    /// Written when the first-run window closes, so it doesn't open again.
    first_run_marker: Option<PathBuf>,
}

/// Runs until "Quit murmur". With `first_run_marker`, onboarding opens
/// straight away and the marker is written once it closes.
pub fn run(dictation: Arc<Mutex<Status>>, first_run_marker: Option<PathBuf>) -> iced::Result {
    let boot = move || {
        let mut app = App {
            dictation: dictation.clone(),
            _icon: build(),
            setup: None,
            first_run_marker: first_run_marker.clone(),
        };
        let task = if app.first_run_marker.is_some() { app.open_setup() } else { Task::none() };
        (app, task)
    };
    let mut daemon = iced::daemon(boot, App::update, App::view)
        .title(|_: &App, _| "murmur".to_string())
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
        self.setup = Some((id, Onboarding::in_app(id, self.dictation.clone())));
        opened.discard()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tray(Pick::Setup) => self.open_setup(),
            Message::Tray(Pick::Quit) => iced::exit(),
            Message::Setup(id, message) => match &mut self.setup {
                Some((open, onboarding)) if *open == id => {
                    onboarding.update(message).map(move |message| Message::Setup(id, message))
                }
                _ => Task::none(),
            },
            Message::Closed(id) => {
                if self.setup.as_ref().is_some_and(|(open, _)| *open == id) {
                    self.setup = None;
                    if let Some(marker) = self.first_run_marker.take() {
                        let _ = marker.parent().map(std::fs::create_dir_all);
                        let _ = std::fs::write(marker, "");
                    }
                }
                Task::none()
            }
        }
    }

    fn view(&self, id: window::Id) -> Element<'_, Message> {
        match &self.setup {
            Some((open, onboarding)) if *open == id => {
                onboarding.view().map(move |message| Message::Setup(id, message))
            }
            _ => iced::widget::Space::new().into(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let setup = match &self.setup {
            Some((id, onboarding)) => {
                let id = *id;
                onboarding.subscription().with(id).map(|(id, message)| Message::Setup(id, message))
            }
            None => Subscription::none(),
        };
        Subscription::batch([
            Subscription::run(picks).map(Message::Tray),
            window::close_events().map(Message::Closed),
            setup,
        ])
    }
}

/// The icon and its menu. None if the shell has no tray (then murmur still runs; `murmur ctl quit` stops it).
fn build() -> Option<TrayIcon> {
    let menu = Menu::new();
    let items = [
        &MenuItem::new("Hold Ctrl + Space to talk", false, None) as &dyn tray_icon::menu::IsMenuItem,
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(SETUP, "Set up murmur…", true, None),
        &MenuItem::with_id(QUIT, "Quit murmur", true, None),
    ];
    if let Err(error) = menu.append_items(&items) {
        eprintln!("murmur: no tray menu: {error}");
    }
    let image = tray_icon::Icon::from_rgba(icon::rgba(32), 32, 32).ok();
    let mut builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip(TOOLTIP);
    if let Some(image) = image {
        builder = builder.with_icon(image);
    }
    match builder.build() {
        Ok(tray) => Some(tray),
        Err(error) => {
            eprintln!("murmur: no tray icon: {error}");
            None
        }
    }
}

/// What a menu id or a click on the icon means.
pub fn pick_for_menu(id: &str) -> Option<Pick> {
    match id {
        SETUP => Some(Pick::Setup),
        QUIT => Some(Pick::Quit),
        _ => None,
    }
}

/// Tray clicks and menu picks, as a stream. The menu opens on a right click;
/// a double click on the icon opens setup.
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
                let _ = out.clone().try_send(Pick::Setup);
            }
        }));
    })
}
