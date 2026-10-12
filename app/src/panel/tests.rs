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
    let shots = [
        ("general", Tab::General, Engine::Codex),
        ("transcription", Tab::Transcription, Engine::Codex),
        ("transcription-groq", Tab::Transcription, Engine::Groq),
        ("commands", Tab::Commands, Engine::Codex),
        ("history", Tab::History, Engine::Codex),
        ("about", Tab::About, Engine::Codex),
    ];
    for (name, tab, engine) in shots {
        f.settings.write().expect("settings").engine = engine;
        let _ = f.panel.update(Message::Tab(tab));
        // Never this machine's own key or Windows settings on a public board.
        f.panel.groq = GroqSetup::default();
        f.panel.windows = Some(Err(Failure::WindowsPrivacy));
        if matches!(tab, Tab::About | Tab::Transcription) {
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

#[test]
fn picking_an_engine_card_saves_it_and_shows_what_it_needs() {
    let _ui = test_ui::lock();
    let mut f = panel();
    f.panel.groq = GroqSetup::default();
    press(&mut f.panel, "Transcription");
    press(&mut f.panel, "Groq");
    assert_eq!(f.settings.read().expect("settings").engine, Engine::Groq);
    assert_eq!(Settings::load_from(&f.dir.path().join("settings.json")).engine, Engine::Groq);
    assert!(shows(&f.panel, "Add a free Groq key"));
    assert!(shows(&f.panel, "Needs a key"));
    assert!(shows(&f.panel, "Save key"));
}

#[test]
fn a_key_that_isnt_groqs_is_refused_before_any_request() {
    let _ui = test_ui::lock();
    let mut f = panel();
    f.panel.groq = GroqSetup::default();
    let _ = f.panel.update(Message::Engine(Engine::Groq));
    let _ = f.panel.update(Message::Tab(Tab::Transcription));
    let _ = f.panel.update(Message::GroqKey("sk-proj-0123456789abcdef0123".into()));
    let _ = f.panel.update(Message::SaveGroqKey);
    assert!(!f.panel.groq.checking, "no request for a key that can't work");
    assert!(shows(&f.panel, "That isn't a Groq key. Groq keys start with gsk_."));
}

#[test]
fn a_key_groq_turns_down_is_not_kept() {
    let _ui = test_ui::lock();
    let mut f = panel();
    f.panel.groq = GroqSetup { key: "gsk_0123456789abcdefABCDEF".into(), checking: true, ..GroqSetup::default() };
    let _ = f.panel.update(Message::Engine(Engine::Groq));
    let _ = f.panel.update(Message::Tab(Tab::Transcription));
    let _ = f.panel.update(Message::GroqChecked("gsk_0123456789abcdefABCDEF".into(), Some(Err(Failure::GroqRejected))));
    assert!(!f.panel.groq.saved && !f.panel.groq.checking);
    assert!(shows(&f.panel, "Groq turned down that key. Copy it again from Groq."));
}

#[cfg(target_os = "windows")]
#[test]
fn windows_speech_switched_off_says_where_to_turn_it_on() {
    let _ui = test_ui::lock();
    let mut f = panel();
    let _ = f.panel.update(Message::Engine(Engine::Windows));
    let _ = f.panel.update(Message::Tab(Tab::Transcription));
    f.panel.windows = Some(Err(Failure::WindowsPrivacy));
    assert!(shows(&f.panel, "Online speech is off"));
    assert!(shows(&f.panel, "Turned off"));
    assert!(shows(&f.panel, "Open speech settings"));
}

#[test]
fn hands_free_quiet_stop_is_picked_and_saved() {
    let _ui = test_ui::lock();
    let mut f = panel();
    press(&mut f.panel, "10 s");
    assert_eq!(Settings::load_from(&f.dir.path().join("settings.json")).silence_stop, SilenceStop::Long);
    press(&mut f.panel, "Never");
    assert_eq!(f.settings.read().expect("settings").silence_stop.after(), None);
}

#[test]
fn each_command_group_shows_its_phrases_and_switches_alone() {
    let _ui = test_ui::lock();
    let mut f = panel();
    press(&mut f.panel, "Commands");
    assert!(shows(&f.panel, "\u{201c}new paragraph\u{201d}"));
    assert!(!shows(&f.panel, "\u{201c}question mark\u{201d}"), "punctuation starts off, its phrases hidden");
    let _ = f.panel.update(Message::CommandGroup(Group::Punctuation, true));
    assert!(shows(&f.panel, "\u{201c}question mark\u{201d}"));
    let saved = Settings::load_from(&f.dir.path().join("settings.json")).command_groups;
    assert!(saved.punctuation && saved.lines, "only that group changed");
}

#[test]
fn spoken_commands_off_hides_every_group() {
    let _ui = test_ui::lock();
    let mut f = panel();
    press(&mut f.panel, "Commands");
    let _ = f.panel.update(Message::Commands(false));
    assert!(shows(&f.panel, "Off. Everything you say is typed as you said it."));
    assert!(!shows(&f.panel, "Lines and lists"));
}
