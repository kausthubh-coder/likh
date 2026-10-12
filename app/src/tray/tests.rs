use super::*;

#[test]
fn menu_ids_map_to_what_they_say() {
    assert_eq!(pick_for_menu("open"), Some(Pick::Open));
    assert_eq!(pick_for_menu("setup"), Some(Pick::Setup));
    assert_eq!(pick_for_menu("quit"), Some(Pick::Quit));
    assert_eq!(pick_for_menu("Hold Ctrl + Space to talk"), None);
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
