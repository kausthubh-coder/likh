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
    let panel = Panel::new(window::Id::unique(), settings.clone(), status, Updates::new(None), files);
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
fn dictation_shows_the_shortcut_and_every_mode() {
    let _ui = test_ui::lock();
    let f = panel();
    // The caps themselves are drawn on a canvas; the line under them and Change are widgets.
    assert!(shows(&f.panel, "Hold to talk, in any app.") && shows(&f.panel, "Change"));
    assert!(!f.panel.keys.moving(), "at rest the shortcut asks for no frames");
    for mode in Mode::ALL {
        assert!(shows(&f.panel, mode.name()), "{mode:?}");
    }
    assert!(shows(&f.panel, Mode::HoldOrTap.explain()), "the picked mode says what it does");
}

#[test]
fn changing_the_shortcut_flies_the_keys_in_then_stops_drawing() {
    let _ui = test_ui::lock();
    let mut f = panel();
    press(&mut f.panel, "Change");
    assert!(shows(&f.panel, "Press your new shortcut.") && shows(&f.panel, "Cancel"));

    // Ctrl and Alt go down: their caps lift off the keyboard, and frames run only while they fly.
    let held = keyboard::Modifiers::CTRL | keyboard::Modifiers::ALT;
    let _ = f.panel.update(Message::Key(keyboard::Event::ModifiersChanged(held)));
    assert!(f.panel.keys.moving());
    f.panel.keys.step(1.0);
    assert!(!f.panel.keys.moving(), "landed caps stop asking for frames");

    // A bare letter is refused, with the reason, and capturing goes on.
    let bare = keyboard::Event::KeyPressed {
        key: keyboard::Key::Unidentified,
        modified_key: keyboard::Key::Unidentified,
        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::KeyD),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::empty(),
        text: None,
        repeat: false,
    };
    let _ = f.panel.update(Message::Key(bare));
    assert!(shows(&f.panel, "Add Ctrl, Alt or Win, so typing never sets it off."));
    assert!(shows(&f.panel, "Press your new shortcut."));

    // Cancel puts the old shortcut back.
    press(&mut f.panel, "Cancel");
    assert!(shows(&f.panel, "Hold to talk, in any app."));
    assert_eq!(f.settings.read().expect("settings").shortcut, Binding::default());
}

#[test]
fn a_saved_shortcut_pulses_once_and_then_stays_still() {
    let mut keys = Keys::at_rest(&Binding::default().parts());
    keys.listen();
    keys.chosen(&["Ctrl", "Alt", "D"]);
    assert!(keys.moving());
    keys.step(0.5);
    assert!(keys.moving(), "the pulse runs after the last key lands");
    keys.step(1.0);
    assert!(!keys.moving());
}

#[test]
fn picking_a_mode_saves_it() {
    let _ui = test_ui::lock();
    let mut f = panel();
    press(&mut f.panel, "Toggle");
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
    let _ = f.panel.update(Message::Tab(Tab::Dictation));
    let _ = f.panel.update(Message::KeepHistory(false));
    press(&mut f.panel, "History");
    // History off: it says so, and its switch sits right above.
    assert!(shows(&f.panel, "History is off, so nothing you dictate is kept."));
    press(&mut f.panel, "Keep history");
    assert!(Settings::load_from(&f.dir.path().join("settings.json")).history);
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
        ("dictation", Tab::Dictation, Engine::Codex),
        ("transcription", Tab::Transcription, Engine::Codex),
        ("transcription-groq", Tab::Transcription, Engine::Groq),
        ("commands", Tab::Commands, Engine::Codex),
        ("history", Tab::History, Engine::Codex),
        ("app", Tab::App, Engine::Codex),
    ];
    for (name, tab, engine) in shots {
        f.settings.write().expect("settings").engine = engine;
        let _ = f.panel.update(Message::Tab(tab));
        // Never this machine's own key on a public board.
        f.panel.groq = GroqSetup::default();
        // Opening Transcription reads this machine's Codex sign-in; the board shows the made-up one.
        if tab == Tab::Transcription {
            f.panel.codex = CodexStatus::Connected(crate::codex_auth::Account {
                email_hint: Some("t\u{2022}\u{2022}\u{2022}@example.com".into()),
                plan: Some("plus".into()),
                expires_at: None,
            });
        }
        snap(&out, name, &f.panel);
    }
    // Picking a shortcut, mid-flight and once it's saved, for the board.
    let _ = f.panel.update(Message::Tab(Tab::Dictation));
    let _ = f.panel.update(Message::ChangeShortcut);
    let _ = f.panel.update(Message::Key(keyboard::Event::ModifiersChanged(keyboard::Modifiers::CTRL)));
    f.panel.keys.step(1.0);
    snap(&out, "dictation-picking", &f.panel);
    f.panel.stop_capturing();
    f.panel.capturing = None;
    f.panel.keys.chosen(&["Ctrl", "Alt", "D"]);
    f.panel.keys.step(2.0);
    f.panel.saved = true;
    f.settings.write().expect("settings").shortcut =
        Binding::new(true, true, false, false, crate::hotkey::KeyName("D")).expect("a valid shortcut");
    snap(&out, "dictation-saved", &f.panel);
}

/// Renders the window as it looks now to `<out>/<name>-tiny-skia.png`.
fn snap(out: &std::path::Path, name: &str, panel: &Panel) {
    let settings = iced_test::core::Settings {
        fonts: crate::shared::theme::FONT_FILES.iter().map(|f| std::borrow::Cow::Borrowed(*f)).collect(),
        default_font: crate::shared::theme::UI,
        ..Default::default()
    };
    let mut ui = iced_test::Simulator::with_size(settings, WINDOW, panel.view());
    // iced writes `<name>-tiny-skia.png`, and only when it's missing.
    let _ = std::fs::remove_file(out.join(format!("{name}-tiny-skia.png")));
    if let Ok(snapshot) = ui.snapshot(&iced::Theme::Dark) {
        let _ = snapshot.matches_image(out.join(name));
    }
}

#[test]
fn picking_an_engine_saves_it_and_both_keep_their_setup() {
    let _ui = test_ui::lock();
    let mut f = panel();
    f.panel.groq = GroqSetup::default();
    press(&mut f.panel, "Transcription");
    // Before and after the pick, each engine shows what it needs, so nothing moves.
    let before = (shows(&f.panel, "Paste your key, gsk_\u{2026}"), shows(&f.panel, "Check again"));
    press(&mut f.panel, "Groq");
    assert_eq!(f.settings.read().expect("settings").engine, Engine::Groq);
    assert_eq!(Settings::load_from(&f.dir.path().join("settings.json")).engine, Engine::Groq);
    assert_eq!(before, (true, true));
    assert!(shows(&f.panel, "Paste your key, gsk_\u{2026}") && shows(&f.panel, "Check again"));
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

#[test]
fn app_offers_a_restart_once_an_update_is_downloaded() {
    let _ui = test_ui::lock();
    let mut f = panel();
    f.panel.updates = Updates::with_state(update::State::Ready {
        version: "0.1.0-alpha.6".into(),
        installer: "C:/Users/someone/AppData/Local/likh/updates/likh-setup-0.1.0-alpha.6.exe".into(),
    });
    press(&mut f.panel, "App");
    assert!(shows(&f.panel, "likh 0.1.0-alpha.6 is downloaded and checked."));
    let mut ui = simulator(f.panel.view());
    ui.click("Restart to update").expect("a restart button");
    assert!(
        ui.into_messages().any(|message| matches!(message, Message::Restart)),
        "the tray runs the installer when it hears Restart"
    );
}

#[test]
fn every_tab_has_the_update_button_next_to_the_version() {
    let _ui = test_ui::lock();
    let mut f = panel();
    // Nothing checked yet: the button checks.
    assert!(shows(&f.panel, &format!("v{}", env!("CARGO_PKG_VERSION"))));
    let mut ui = simulator(f.panel.view());
    ui.click(iced::widget::Id::new(UPDATE)).expect("the update button on Dictation");
    assert!(ui.into_messages().any(|message| matches!(message, Message::CheckForUpdates)));

    // An update is ready: the version says so and the same button restarts into it, on any tab.
    f.panel.updates = Updates::with_state(update::State::Ready {
        version: "0.1.0-alpha.7".into(),
        installer: "C:/Users/someone/AppData/Local/likh/updates/likh-setup-0.1.0-alpha.7.exe".into(),
    });
    press(&mut f.panel, "Commands");
    assert!(shows(&f.panel, "v0.1.0-alpha.7 ready"));
    let mut ui = simulator(f.panel.view());
    ui.click(iced::widget::Id::new(UPDATE)).expect("the update button on Commands");
    assert!(ui.into_messages().any(|message| matches!(message, Message::Restart)));
}

#[test]
fn the_update_button_spins_only_while_checking() {
    let _ui = test_ui::lock();
    let mut f = panel();
    assert!(!f.panel.wants_frames());
    f.panel.updates = Updates::with_state(update::State::Checking);
    assert!(f.panel.wants_frames());
    f.panel.updates = Updates::with_state(update::State::Idle);
    assert!(!f.panel.wants_frames(), "a still window draws nothing");
}

#[test]
fn app_says_when_likh_is_newest_and_the_sidebar_checks_again() {
    let _ui = test_ui::lock();
    let mut f = panel();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    f.panel.updates = Updates::with_state(update::State::UpToDate { checked_at: now - 3 * 3600 });
    press(&mut f.panel, "App");
    assert!(shows(
        &f.panel,
        &format!(
            "This is likh {}. It's the newest, checked {}.",
            env!("CARGO_PKG_VERSION"),
            history::ago(now - 3 * 3600, now)
        )
    ));
    // One check button, the sidebar's, on App as on every tab.
    assert!(!shows(&f.panel, "Check now"));
    let mut ui = simulator(f.panel.view());
    ui.click(iced::widget::Id::new(UPDATE)).expect("the sidebar's update button on App");
    assert!(ui.into_messages().any(|message| matches!(message, Message::CheckForUpdates)));
}

#[test]
fn a_switch_flips_from_anywhere_on_its_row() {
    let _ui = test_ui::lock();
    let mut f = panel();
    // Its name, not the switch itself.
    press(&mut f.panel, "Sounds");
    assert!(!Settings::load_from(&f.dir.path().join("settings.json")).sounds);
}

#[test]
fn automatic_update_checks_can_be_switched_off() {
    let _ui = test_ui::lock();
    let mut f = panel();
    press(&mut f.panel, "App");
    let _ = f.panel.update(Message::AutoUpdate(false));
    assert!(!Settings::load_from(&f.dir.path().join("settings.json")).auto_update);
}
