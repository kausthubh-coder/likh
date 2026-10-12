use super::*;
use crate::dictation::Phase;
use crate::shared::test_ui;
use iced_test::simulator;

struct Fixture {
    panel: Panel,
    settings: Arc<RwLock<Settings>>,
    dir: tempfile::TempDir,
}

fn panel() -> Fixture {
    let dir = tempfile::tempdir().expect("temp dir");
    let settings = Arc::new(RwLock::new(Settings::default()));
    let status = Arc::new(Mutex::new(Status { phase: Phase::Idle, transcriber: "echo", dictations: 0, last: None }));
    let files =
        Files { settings: Some(dir.path().join("settings.json")), history: Some(dir.path().join("history.jsonl")) };
    let panel = Panel::new(window::Id::unique(), settings.clone(), status, files);
    Fixture { panel, settings, dir }
}

fn press(panel: &mut Panel, label: &str) {
    let mut ui = simulator(panel.view());
    ui.click(label).unwrap_or_else(|_| panic!("no button labelled {label:?}"));
    for message in ui.into_messages() {
        let _ = panel.update(message);
    }
}

fn shows(panel: &Panel, words: &str) -> bool {
    simulator(panel.view()).find(words).is_ok()
}

#[test]
fn general_shows_the_shortcut_and_every_mode() {
    let _ui = test_ui::lock();
    let f = panel();
    assert!(shows(&f.panel, "Ctrl") && shows(&f.panel, "Space"));
    for mode in Mode::ALL {
        assert!(shows(&f.panel, mode.name()), "{mode:?}");
    }
}

#[test]
fn picking_a_mode_saves_it() {
    let _ui = test_ui::lock();
    let mut f = panel();
    press(&mut f.panel, "Tap to toggle");
    assert_eq!(f.settings.read().expect("settings").mode, Mode::Tap);
    assert_eq!(Settings::load_from(&f.dir.path().join("settings.json")).mode, Mode::Tap);
}

#[test]
fn switches_flip_and_save() {
    let _ui = test_ui::lock();
    let mut f = panel();
    let _ = f.panel.update(Message::Commands(false));
    let _ = f.panel.update(Message::KeepHistory(false));
    let saved = Settings::load_from(&f.dir.path().join("settings.json"));
    assert!(!saved.commands && !saved.history);
    assert!(saved.sounds, "untouched settings keep their values");
}

#[test]
fn history_lists_newest_first_and_copies_on_click() {
    let _ui = test_ui::lock();
    let mut f = panel();
    let file = f.dir.path().join("history.jsonl");
    for (at, text) in [(1, "First thing I said."), (2, "Second thing I said.")] {
        history::append(&file, &Entry { at, text: text.into(), seconds: 1.5 }).expect("append");
    }
    press(&mut f.panel, "History");
    assert!(shows(&f.panel, "2 dictations, newest first. Click one to copy it."));
    assert_eq!(f.panel.history[0].text, "Second thing I said.");
}

#[test]
fn clearing_history_asks_once_then_clears() {
    let _ui = test_ui::lock();
    let mut f = panel();
    let file = f.dir.path().join("history.jsonl");
    history::append(&file, &Entry { at: 1, text: "Gone soon.".into(), seconds: 1.0 }).expect("append");
    press(&mut f.panel, "History");
    press(&mut f.panel, "Clear");
    assert!(shows(&f.panel, "Click again to clear"));
    assert_eq!(history::recent(&file, 10).len(), 1, "one click must not clear");
    press(&mut f.panel, "Click again to clear");
    assert!(history::recent(&file, 10).is_empty());
    assert!(shows(&f.panel, "Nothing yet"));
}

#[test]
fn an_empty_history_says_why_and_what_to_do() {
    let _ui = test_ui::lock();
    let mut f = panel();
    press(&mut f.panel, "History");
    assert!(shows(&f.panel, "Hold Ctrl + Space and talk. What you say shows up here, and only here."));
    let _ = f.panel.update(Message::Tab(Tab::General));
    let _ = f.panel.update(Message::KeepHistory(false));
    press(&mut f.panel, "History");
    assert!(shows(
        &f.panel,
        "History is off. Turn on \u{201c}Keep history\u{201d} in General to keep your dictations."
    ));
}

/// Regenerates the likh window's screenshots on the brand board from the real views.
/// Run: cargo test -p likh brand_board_panel -- --ignored
#[test]
#[ignore = "writes docs/brand/panel/*.png; run by hand after changing the window"]
fn brand_board_panel() {
    let _ui = test_ui::lock();
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/brand/panel");
    let _ = std::fs::create_dir_all(&out);
    let mut f = panel();
    let file = f.dir.path().join("history.jsonl");
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    for (ago, text) in [
        (86_400 * 2, "Ship the beta to the first hundred testers on Friday, then we decide on pricing."),
        (3_600 * 3, "Thanks for the notes. Also, remind me to send the deck to Priya tomorrow at nine."),
        (240, "Eggs, oat milk, coffee beans, the good bread, and something for dinner on Sunday."),
    ] {
        history::append(&file, &Entry { at: now - ago, text: text.into(), seconds: 3.4 }).expect("append");
    }
    // A made-up account: the board is public and must never show a real one.
    f.panel.codex = CodexStatus::Connected(crate::codex_auth::Account {
        email_hint: Some("t\u{2022}\u{2022}\u{2022}@example.com".into()),
        plan: Some("plus".into()),
        expires_at: None,
    });
    for (name, tab) in [("general", Tab::General), ("history", Tab::History), ("about", Tab::About)] {
        let _ = f.panel.update(Message::Tab(tab));
        if tab == Tab::About {
            f.panel.codex = CodexStatus::Connected(crate::codex_auth::Account {
                email_hint: Some("t\u{2022}\u{2022}\u{2022}@example.com".into()),
                plan: Some("plus".into()),
                expires_at: None,
            });
        }
        let settings = iced_test::core::Settings {
            fonts: crate::shared::theme::FONT_FILES.iter().map(|f| std::borrow::Cow::Borrowed(*f)).collect(),
            default_font: crate::shared::theme::UI,
            ..Default::default()
        };
        let mut ui = iced_test::Simulator::with_size(settings, WINDOW, f.panel.view());
        let path = out.join(name);
        // iced writes `<name>-tiny-skia.png`, and only when it's missing.
        let _ = std::fs::remove_file(out.join(format!("{name}-tiny-skia.png")));
        if let Ok(snapshot) = ui.snapshot(&iced::Theme::Dark) {
            let _ = snapshot.matches_image(&path);
        }
    }
}
