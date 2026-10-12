use super::*;

#[test]
fn menu_ids_map_to_what_they_say() {
    assert_eq!(pick_for_menu("open"), Some(Pick::Open));
    assert_eq!(pick_for_menu("setup"), Some(Pick::Setup));
    assert_eq!(pick_for_menu("quit"), Some(Pick::Quit));
    assert_eq!(pick_for_menu("update"), Some(Pick::Update));
    assert_eq!(pick_for_menu("Hold Ctrl + Space to talk"), None);
}

#[test]
fn the_update_line_says_where_updating_stands_and_only_works_when_it_can() {
    let line = |state: State| update_line(&state);
    assert_eq!(line(State::Idle), ("Check for updates".into(), true));
    assert_eq!(line(State::Checking), ("Checking for updates\u{2026}".into(), false));
    assert_eq!(line(State::UpToDate { checked_at: 0 }), ("Up to date \u{b7} Check again".into(), true));
    assert_eq!(
        line(State::Downloading { version: "0.1.0-alpha.6".into() }),
        ("Downloading likh 0.1.0-alpha.6\u{2026}".into(), false)
    );
    assert_eq!(
        line(State::Ready { version: "0.1.0-alpha.6".into(), installer: PathBuf::from("likh-setup.exe") }),
        ("Restart to update to 0.1.0-alpha.6".into(), true)
    );
    assert_eq!(line(State::Failed { reason: "offline".into() }), ("Couldn't check \u{b7} Try again".into(), true));
}

fn app(marker: Option<PathBuf>) -> App {
    let (_, inbox) = iced::futures::channel::mpsc::unbounded();
    App {
        dictation: Arc::new(Mutex::new(Status {
            phase: crate::dictation::Phase::Idle,
            transcriber: "echo",
            dictations: 0,
            last: None,
        })),
        settings: Arc::new(RwLock::new(Settings::default())),
        updates: Updates::new(None),
        tray: None,
        setup: Some((window::Id::unique(), Onboarding::new(crate::codex_auth::CodexStatus::SignedOut))),
        panel: None,
        first_run_marker: marker,
        ui_requests: UiRequests(Arc::new(Mutex::new(Some(inbox)))),
    }
}

#[test]
fn dismissing_setup_keeps_it_unfinished_and_reopening_returns_to_setup() {
    let dir = tempfile::tempdir().expect("temp dir");
    let marker = dir.path().join("onboarded");
    let mut app = app(Some(marker.clone()));
    let id = app.setup.as_ref().expect("setup").0;
    let _ = app.update(Message::Closed(id));
    assert!(!marker.exists());
    assert_eq!(app.first_run_marker, Some(marker));
    assert!(app.setup.is_none());
    assert!(app.panel.is_none());
    let _ = app.update(Message::Tray(Pick::Open));
    assert!(app.setup.is_some());
    assert!(app.panel.is_none());
}

#[test]
fn finish_opens_settings_and_saves_completion() {
    let dir = tempfile::tempdir().expect("temp dir");
    let marker = dir.path().join("onboarded");
    let mut app = app(Some(marker.clone()));
    let id = app.setup.as_ref().expect("setup").0;
    let _ = app.update(Message::Setup(id, onboarding::Message::Finish));
    let _ = app.update(Message::Closed(id));
    assert!(marker.is_file());
    assert!(app.first_run_marker.is_none());
    assert!(app.setup.is_none());
    let panel = app.panel.as_ref().expect("settings opened").0;
    let _ = app.update(Message::Closed(id));
    assert_eq!(app.panel.as_ref().expect("settings still open").0, panel);
    let _ = app.update(Message::Closed(panel));
    assert!(app.panel.is_none());
    let _ = app.update(Message::Tray(Pick::Open));
    assert!(app.panel.is_some());
    assert!(app.setup.is_none());
}

#[test]
fn repeating_setup_also_opens_settings_when_finished() {
    let mut app = app(None);
    let id = app.setup.as_ref().expect("setup").0;
    let _ = app.update(Message::Setup(id, onboarding::Message::Finish));
    let _ = app.update(Message::Closed(id));
    assert!(app.setup.is_none());
    assert!(app.panel.is_some());
}
