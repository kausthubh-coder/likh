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
    let status = Arc::new(Mutex::new(Status {
        phase: Phase::Idle,
        transcriber: "echo",
        dictations: 0,
        actions: 0,
        last: None,
        last_text: None,
    }));
    let files = Files {
        settings: Some(dir.path().join("settings.json")),
        history: Some(dir.path().join("history.jsonl")),
        actions: Some(dir.path().join("actions")),
        approved: Some(dir.path().join("approved")),
        logs: Some(dir.path().join("logs")),
        mics: two_mics,
    };
    let panel = Panel::new(window::Id::unique(), settings.clone(), status, Updates::new(None), files);
    Fixture { panel, settings, dir }
}

/// A laptop's own mic (Windows' default) and a Bluetooth headset, never this machine's.
fn two_mics() -> (Vec<Mic>, Option<Mic>) {
    let laptop = Mic { id: "wasapi:laptop".into(), name: "Microphone Array".into(), bluetooth: false };
    let headset = Mic { id: "wasapi:headset".into(), name: "WH-1000XM4".into(), bluetooth: true };
    (vec![laptop.clone(), headset], Some(laptop))
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
fn clean_up_is_off_until_switched_on_and_says_what_it_needs() {
    let _ui = test_ui::lock();
    let mut f = panel();
    press(&mut f.panel, "Transcription");
    // Never this machine's own sign-in: first none, then a made-up one.
    f.panel.codex = CodexStatus::SignedOut;
    assert!(shows(&f.panel, "Needs Codex signed in, whichever engine you pick."));
    f.panel.codex = CodexStatus::Connected(crate::codex_auth::Account {
        email_hint: Some("t\u{2022}\u{2022}\u{2022}@example.com".into()),
        plan: Some("plus".into()),
        expires_at: None,
    });
    assert!(shows(&f.panel, "Drops ums, keeps corrections, makes lists. Can run out on Free."));
    let saved = || Settings::load_from(&f.dir.path().join("settings.json"));
    assert!(!saved().cleanup);
    press(&mut f.panel, "Everywhere");
    assert!(saved().cleanup && !saved().styles);
    press(&mut f.panel, "By app");
    assert!(saved().cleanup && saved().styles);
    assert!(shows(
        &f.panel,
        "Emails get full sentences, chats stay casual, code keeps its terms. Can run out on Free."
    ));
    press(&mut f.panel, "Off");
    assert!(!saved().cleanup);
}

#[test]
fn history_lists_newest_first_and_copies_on_click() {
    let _ui = test_ui::lock();
    let mut f = panel();
    let file = f.dir.path().join("history.jsonl");
    for (at, text) in [(1, "First thing I said."), (2, "Second thing I said.")] {
        history::append(&file, &Entry { at, text: text.into(), seconds: 1.5, action: None, pasted: true })
            .expect("append");
    }
    press(&mut f.panel, "History");
    assert!(shows(&f.panel, "2 dictations, newest first. Click one to copy it."));
    assert_eq!(f.panel.history[0].text, "Second thing I said.");
}

#[test]
fn history_sums_up_every_kept_dictation_not_only_those_shown() {
    let _ui = test_ui::lock();
    let mut f = panel();
    let file = f.dir.path().join("history.jsonl");
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let ten_words = "Send the notes to Priya before the call on Friday.";
    for _ in 0..HISTORY_SHOWN + 50 {
        history::append(&file, &Entry { at: now, text: ten_words.into(), seconds: 3.0, action: None, pasted: true })
            .expect("append");
    }
    press(&mut f.panel, "History");
    assert_eq!(f.panel.history.len(), HISTORY_SHOWN);
    // 250 dictations of 10 words, 3 s each: 2,500 words in 12.5 minutes, all today.
    for figure in ["2,500", "words", "in the last 7 days", "200", "words a minute", "1", "day in a row"] {
        assert!(shows(&f.panel, figure), "no {figure:?} in History");
    }
    // Typing 2,500 words at 40 a minute takes 62.5 minutes; talking took 12.5.
    assert!(shows(&f.panel, "About 50 min faster than typing it at 40 words a minute."));
}

#[test]
fn counts_read_in_groups_of_three() {
    assert_eq!(thousands(0), "0");
    assert_eq!(thousands(999), "999");
    assert_eq!(thousands(12_480), "12,480");
    assert_eq!(thousands(1_000_000), "1,000,000");
}

#[test]
fn clearing_history_asks_once_then_clears() {
    let _ui = test_ui::lock();
    let mut f = panel();
    let file = f.dir.path().join("history.jsonl");
    history::append(&file, &Entry { at: 1, text: "Gone soon.".into(), seconds: 1.0, action: None, pasted: true })
        .expect("append");
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

#[test]
fn dictionary_adds_refuses_a_repeat_removes_and_saves() {
    let _ui = test_ui::lock();
    let mut f = panel();
    press(&mut f.panel, "Dictionary");
    assert!(shows(&f.panel, "Nothing yet. Add the names it mishears: projects, people, places."));
    for word in ["Critly", "  Kriyan ", "critly"] {
        let _ = f.panel.update(Message::NewWord(word.into()));
        press(&mut f.panel, "Add");
    }
    let saved = || Settings::load_from(&f.dir.path().join("settings.json"));
    assert_eq!(saved().dictionary, vec!["Critly".to_string(), "Kriyan".to_string()]);
    assert!(shows(&f.panel, "critly is already in your dictionary."));
    // A word is a chip; clicking it takes it out.
    assert!(shows(&f.panel, "Kriyan"));
    press(&mut f.panel, "Critly");
    assert_eq!(saved().dictionary, vec!["Kriyan".to_string()]);
    assert!(!shows(&f.panel, "Critly"));
    press(&mut f.panel, "Fix misheard names");
    assert!(!saved().fix_names);
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
    f.settings.write().expect("settings").dictionary =
        ["Critly", "Kriyan", "Studi", "Priya", "likh"].map(String::from).to_vec();
    let file = f.dir.path().join("history.jsonl");
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    for (ago, text) in [
        (86_400 * 2, "Ship the beta to the first hundred testers on Friday, then we decide on pricing."),
        (3_600 * 3, "Thanks for the notes. Also, remind me to send the deck to Priya tomorrow at nine."),
        (240, "Eggs, oat milk, coffee beans, the good bread, and something for dinner on Sunday."),
    ] {
        history::append(&file, &Entry { at: now - ago, text: text.into(), seconds: 6.2, action: None, pasted: true })
            .expect("append");
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
        ("dictionary", Tab::Dictionary, Engine::Codex),
        ("history", Tab::History, Engine::Codex),
        ("app", Tab::App, Engine::Codex),
    ];
    for (name, tab, engine) in shots {
        f.settings.write().expect("settings").engine = engine;
        let _ = f.panel.update(Message::Tab(tab));
        // Never this machine's own key on a public board.
        f.panel.groq = GroqSetup::default();
        // Opening Transcription or Dictionary reads this machine's Codex sign-in; the board shows the made-up one.
        if matches!(tab, Tab::Transcription | Tab::Dictionary) {
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
    // The headset picked and being tested: the meter, Stop, and the Bluetooth line.
    f.panel.saved = false;
    let (options, _) = f.panel.mic_options();
    let _ = f.panel.update(Message::Microphone(options[2].clone()));
    f.panel.testing = true;
    f.panel.mic_levels = [0.2, 0.5, 0.8, 0.6, 0.9, 0.4, 0.7];
    snap(&out, "dictation-microphone", &f.panel);
    f.panel.testing = false;
    // Another app has the shortcut: the line under the caps says so, in red.
    let taken = hotkey::pretend_taken(true, false);
    snap(&out, "dictation-taken", &f.panel);
    drop(taken);
    // Actions: the list, Review on the unapproved script, and the form.
    seed_actions(&f);
    open_command_key(&mut f.panel);
    snap(&out, "actions", &f.panel);
    let _ = f.panel.update(Message::Action(actions::Message::Review("deploy-preview".into())));
    snap(&out, "actions-review", &f.panel);
    let _ = f.panel.update(Message::Action(actions::Message::New));
    for message in [
        actions::Message::Name("Make this formal".into()),
        actions::Message::Say("make this {style}".into()),
        actions::Message::Kind(crate::actions::DraftKind::Ask),
        actions::Message::Value("Rewrite this to be {style}: {selection}".into()),
        actions::Message::Output(crate::actions::Output::Replace),
    ] {
        let _ = f.panel.update(Message::Action(message));
    }
    snap(&out, "actions-new", &f.panel);
    write_trace(&f.dir.path().join("logs"));
    open_diagnostics(&mut f);
    let _ = f.panel.update(Message::Sample);
    snap(&out, "diagnostics", &f.panel);
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
    // No key yet: Groq would fail the next dictation, so ChatGPT stays and the field asks for one.
    press(&mut f.panel, "Groq");
    assert_eq!(f.settings.read().expect("settings").engine, Engine::Codex);
    assert!(shows(&f.panel, "Save a key to switch to Groq."));
    assert_eq!(before, (true, true));
    assert!(shows(&f.panel, "Paste your key, gsk_\u{2026}") && shows(&f.panel, "Check again"));
    assert!(shows(&f.panel, "Save key"));
    // With a key saved, the click switches and saves.
    f.panel.groq.saved = true;
    press(&mut f.panel, "Groq");
    assert_eq!(f.settings.read().expect("settings").engine, Engine::Groq);
    assert_eq!(Settings::load_from(&f.dir.path().join("settings.json")).engine, Engine::Groq);
    press(&mut f.panel, "ChatGPT");
    assert_eq!(f.settings.read().expect("settings").engine, Engine::Codex);
}

#[test]
fn a_shortcut_another_app_holds_says_so_on_dictation_and_paste_last_on_history() {
    let _ui = test_ui::lock();
    let mut f = panel();
    let taken = hotkey::pretend_taken(true, true);
    // macOS has no taken keys: there, no key works without the Accessibility permission.
    let (line, tray_line) = if cfg!(target_os = "macos") {
        ("likh needs Accessibility access.", "Allow Accessibility for likh to use Ctrl + Space")
    } else {
        ("Another app has these keys.", "Another app has Ctrl + Space. Open likh to change it.")
    };
    assert!(shows(&f.panel, line), "Dictation says why the shortcut does nothing");
    assert_eq!(hold_to_talk(Binding::default()), tray_line);
    press(&mut f.panel, "History");
    assert!(shows(&f.panel, "Another app has these keys. Change them."), "History says another app has paste last");
    drop(taken);
    assert_eq!(hold_to_talk(Binding::default()), "Hold Ctrl + Space to talk");
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

fn key(code: keyboard::key::Code, modifiers: keyboard::Modifiers) -> keyboard::Event {
    keyboard::Event::KeyPressed {
        key: keyboard::Key::Unidentified,
        modified_key: keyboard::Key::Unidentified,
        physical_key: keyboard::key::Physical::Code(code),
        location: keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    }
}

#[test]
fn history_shows_paste_last_on_its_keys_and_can_turn_it_off() {
    let _ui = test_ui::lock();
    let mut f = panel();
    let _ = f.panel.update(Message::Tab(Tab::History));
    assert!(shows(&f.panel, "Paste last dictation"));
    assert!(shows(&f.panel, "Missed a paste? Puts it in again. After a failed one, tries it again."));
    assert!(shows(&f.panel, "Alt+Shift+Z") && shows(&f.panel, "Turn off"));
    // With paste last off, the row offers it back on its usual keys.
    f.settings.write().expect("settings").paste_last = None;
    assert!(shows(&f.panel, "Turn on") && !shows(&f.panel, "Alt+Shift+Z"));
}

#[test]
fn picking_paste_last_keys_leaves_the_dictation_shortcut_alone_and_refuses_its_keys() {
    let _ui = test_ui::lock();
    let mut f = panel();
    let _ = f.panel.update(Message::Tab(Tab::History));
    let _ = f.panel.update(Message::ChangePasteLast);
    assert!(shows(&f.panel, "Press the new keys.") && shows(&f.panel, "Cancel"));
    let _ = f.panel.update(Message::Key(keyboard::Event::ModifiersChanged(keyboard::Modifiers::CTRL)));
    assert!(!f.panel.keys.moving(), "the dictation shortcut's caps stay put");

    // Ctrl+Space already dictates.
    let _ = f.panel.update(Message::Key(key(keyboard::key::Code::Space, keyboard::Modifiers::CTRL)));
    assert!(shows(&f.panel, "Those keys already do something in likh. Pick others."));
    assert!(shows(&f.panel, "Press the new keys."), "still picking");
    assert_eq!(f.settings.read().expect("settings").paste_last, Some(hotkey::PASTE_LAST));

    // Esc backs out with the old keys.
    let _ = f.panel.update(Message::Key(key(keyboard::key::Code::Escape, keyboard::Modifiers::empty())));
    assert!(shows(&f.panel, "Alt+Shift+Z"));
}

#[test]
fn the_microphone_list_starts_on_windows_default_and_saves_a_pick() {
    let _ui = test_ui::lock();
    let mut f = panel();
    let (options, picked) = f.panel.mic_options();
    let labels: Vec<String> = options.iter().map(ToString::to_string).collect();
    assert_eq!(labels, ["Default \u{b7} Microphone Array", "Microphone Array", "WH-1000XM4"]);
    assert_eq!(picked, options[0]);
    // The list draws its own label, out of the simulator's reach; the row's words are widgets.
    assert!(shows(&f.panel, "Microphone") && shows(&f.panel, "Test"));

    let _ = f.panel.update(Message::Microphone(options[1].clone()));
    let saved = Settings::load_from(&f.dir.path().join("settings.json"));
    assert_eq!(saved.microphone, Some(Choice { id: "wasapi:laptop".into(), name: "Microphone Array".into() }));
}

#[test]
fn a_bluetooth_pick_warns_about_call_quality() {
    let _ui = test_ui::lock();
    let mut f = panel();
    let (options, _) = f.panel.mic_options();
    let _ = f.panel.update(Message::Microphone(options[2].clone()));
    assert!(shows(&f.panel, "Bluetooth headsets drop to call quality while you talk."));
}

#[test]
fn a_picked_mic_thats_unplugged_stays_listed_and_says_whats_used_instead() {
    let _ui = test_ui::lock();
    let f = panel();
    f.settings.write().expect("settings").microphone =
        Some(Choice { id: "wasapi:yeti".into(), name: "Yeti Stereo Microphone".into() });
    let (options, picked) = f.panel.mic_options();
    assert_eq!(picked.to_string(), "Yeti Stereo Microphone (not connected)");
    assert_eq!(options.last(), Some(&picked));
    assert!(shows(&f.panel, "Not connected. Using Microphone Array until it's back."));
}

#[test]
fn the_mic_opens_only_while_testing_and_closes_when_you_leave_the_tab() {
    let _ui = test_ui::lock();
    let mut f = panel();
    assert!(!f.panel.testing, "opening the window never opens the mic");
    let _ = f.panel.update(Message::TestMic(true));
    assert!(shows(&f.panel, "Stop"));
    for level in [0.2, 0.9, 0.5] {
        let _ = f.panel.update(Message::MicHeard(microphone::Event::Level(level)));
    }
    assert_eq!(f.panel.mic_levels[MIC_BARS - 3..], [0.2, 0.9, 0.5]);
    let _ = f.panel.update(Message::Tab(Tab::History));
    assert!(!f.panel.testing);
    let _ = f.panel.update(Message::Tab(Tab::Dictation));
    assert!(shows(&f.panel, "Test"));
}

#[test]
fn language_starts_on_auto_detect_and_saves_a_pinned_code() {
    let _ui = test_ui::lock();
    let mut f = panel();
    let _ = f.panel.update(Message::Tab(Tab::Transcription));
    assert!(shows(&f.panel, "Language"));
    assert!(shows(&f.panel, "Auto handles mixed languages. Pick one if likh writes the wrong one."));
    let _ = f.panel.update(Message::Language(LanguageOption { code: Some("hi"), label: "Hindi" }));
    assert_eq!(Settings::load_from(&f.dir.path().join("settings.json")).language.as_deref(), Some("hi"));
    let _ = f.panel.update(Message::Language(LanguageOption { code: None, label: "Auto-detect" }));
    assert_eq!(Settings::load_from(&f.dir.path().join("settings.json")).language, None);
}

/// An actions folder like a real one: two that run nothing, one script nobody approved yet, and a typo.
fn seed_actions(f: &Fixture) {
    let dir = f.dir.path().join("actions");
    crate::actions::prepare(&dir).expect("prepare");
    let files = [
        (
            "spotify.toml",
            "name = \"Spotify search\"\nsay = [\"spotify {text}\"]\noutput = \"none\"\n[[do]]\nopen = \"spotify:search:{text}\"\n",
        ),
        (
            "next-song.toml",
            "name = \"Next song\"\nsay = [\"next song\"]\noutput = \"none\"\n[[do]]\nkeys = \"next_track\"\n",
        ),
        (
            "deploy-preview.toml",
            "name = \"Deploy preview\"\nsay = [\"deploy preview\"]\n[[do]]\nrun = [\"powershell\", \"-NoProfile\", \"-File\", \"deploy.ps1\"]\n",
        ),
        ("deploy.ps1", "vercel deploy --prebuilt\nWrite-Output \"Preview is up\"\n"),
        ("run-tests.toml", "name = \"Run tests\"\nsay = [\"run tests\"]\n\n[[do]]\nrunn = [\"npm\", \"test\"]\n"),
    ];
    for (name, text) in files {
        std::fs::write(dir.join(name), text).expect("write");
    }
}

/// Commands, on the command key's half: what the pill's Review and Details open.
fn open_command_key(panel: &mut Panel) {
    let _ = panel.update(Message::Tab(Tab::Commands));
    let _ = panel.update(Message::CommandsView(CommandsView::CommandKey));
}

#[test]
fn commands_holds_both_kinds_and_says_what_each_group_does_even_when_on() {
    let _ui = test_ui::lock();
    let mut f = panel();
    press(&mut f.panel, "Commands");
    assert!(shows(&f.panel, "While dictating") && shows(&f.panel, "With the command key"));
    // On by default, and its explanation still shows: three chips that all say "undo" never said where.
    assert!(shows(&f.panel, "Take it back"));
    assert!(shows(
        &f.panel,
        "Alone, it undoes the last paste in the app it landed in. After a sentence, it drops that sentence."
    ));
    assert!(shows(&f.panel, "\u{201c}delete that\u{201d}"));
    press(&mut f.panel, "With the command key");
    assert!(shows(&f.panel, "Command key") && shows(&f.panel, "Hold it and say a command."));
    assert!(shows(&f.panel, "Your commands") && shows(&f.panel, "New command"));
    // The starters a new folder gets: ones anyone can use.
    assert!(shows(&f.panel, "Search the web") && shows(&f.panel, "Open a site") && shows(&f.panel, "Make this"));
    assert!(!shows(&f.panel, "New T3 thread"), "the T3 Code starter is an example until renamed");
    press(&mut f.panel, "While dictating");
    assert!(shows(&f.panel, "Lines and lists"));
}

#[test]
fn there_is_no_separate_actions_tab() {
    let _ui = test_ui::lock();
    let f = panel();
    assert!(!shows(&f.panel, "Actions"));
}

#[test]
fn actions_lists_every_file_and_flags_the_broken_and_the_unapproved() {
    let _ui = test_ui::lock();
    let mut f = panel();
    seed_actions(&f);
    open_command_key(&mut f.panel);
    assert!(shows(&f.panel, "Search the web") && shows(&f.panel, "Spotify search"));
    assert!(shows(&f.panel, "\u{201c}spotify {text}\u{201d}"));
    assert!(shows(&f.panel, "Runs a program. Approve it to use it."));
    assert!(shows(
        &f.panel,
        "run-tests.toml:5 unknown field `runn`, expected one of `paste`, `keys`, `open`, `wait`, `ask`, `run`"
    ));
    assert!(shows(&f.panel, "Ctrl+Shift+Space"));
}

#[test]
fn approving_a_script_after_reading_it_lets_it_run() {
    let _ui = test_ui::lock();
    let mut f = panel();
    seed_actions(&f);
    open_command_key(&mut f.panel);
    press(&mut f.panel, "Review");
    assert!(shows(&f.panel, "Deploy preview runs a program you haven't approved yet"));
    assert!(shows(&f.panel, "+ vercel deploy --prebuilt"));
    assert!(shows(&f.panel, "powershell -NoProfile -File deploy.ps1"));
    press(&mut f.panel, "Approve");
    assert!(shows(&f.panel, "Approved. It runs from now on, until its files change."));
    assert!(!shows(&f.panel, "Review"), "nothing is left waiting");
    let action = crate::actions::Catalog::load(&f.dir.path().join("actions")).find("deploy-preview").cloned();
    let state = action.map(|a| crate::actions::trust::state_in(&f.dir.path().join("approved"), &a));
    assert_eq!(state, Some(crate::actions::trust::Trust::Approved));
}

#[test]
fn the_switch_turns_an_action_off_in_its_file() {
    let _ui = test_ui::lock();
    let mut f = panel();
    seed_actions(&f);
    open_command_key(&mut f.panel);
    press(&mut f.panel, "Next song");
    let text = std::fs::read_to_string(f.dir.path().join("actions").join("next-song.toml")).expect("read");
    assert!(text.contains("enabled = false"), "{text}");
}

#[test]
fn new_action_writes_a_file_that_works_on_the_next_press() {
    let _ui = test_ui::lock();
    let mut f = panel();
    open_command_key(&mut f.panel);
    press(&mut f.panel, "New command");
    for message in [
        actions::Message::Name("Pause music".into()),
        actions::Message::Say("pause the music, stop the music".into()),
        actions::Message::Kind(crate::actions::DraftKind::Keys),
        actions::Message::Value("play_pause".into()),
    ] {
        let _ = f.panel.update(Message::Action(message));
    }
    assert!(shows(&f.panel, "saves pause-music.toml"));
    press(&mut f.panel, "Save");
    assert!(shows(&f.panel, "Saved pause-music.toml. Hold the command key and say it."));
    let catalog = crate::actions::Catalog::load(&f.dir.path().join("actions"));
    let found = crate::actions::matcher::find(&catalog.actions, "Pause the music.").expect("it matches");
    assert_eq!(catalog.actions[found.action].name, "Pause music");
    // Each comma-separated phrase is its own way to say it.
    let found = crate::actions::matcher::find(&catalog.actions, "Stop the music.").expect("the second phrase matches");
    assert_eq!(catalog.actions[found.action].name, "Pause music");
}

/// A trace like a day of use: two dictations, one that couldn't reach ChatGPT, a slow cleanup, a warning.
fn write_trace(logs: &std::path::Path) {
    use crate::diagnostics::Record;
    let now = diagnostics::now_ms();
    let trace = Trace::new(logs.to_path_buf());
    let span = |name: &str, ago_ms: u64, duration_ms: f64, cause: Option<&str>| Record::Span {
        name: name.into(),
        at: now - ago_ms,
        duration_ms,
        cause: cause.map(str::to_owned),
        attributes: serde_json::Map::new(),
    };
    for record in [
        span("transcribe", 600_000, 820.0, None),
        span("paste", 599_000, 45.0, None),
        span("transcribe", 120_000, 310.0, Some("Can't reach ChatGPT. Check your connection and try again.")),
        span("cleanup", 60_000, 4_200.0, Some("timed out")),
        Record::Log {
            level: "warn".into(),
            source: "cleanup".into(),
            message: "couldn't clean up (timed out), so likh pasted what it heard.".into(),
            at: now - 60_000,
        },
    ] {
        trace.append(&record);
    }
}

fn open_diagnostics(f: &mut Fixture) {
    let _ = f.panel.update(Message::Tab(Tab::Diagnostics));
    // The app reads the trace on another thread; the test reads it here and hands it over the same way.
    let report = Trace::new(f.dir.path().join("logs")).read();
    let _ = f.panel.update(Message::Diagnosed(report));
}

#[test]
fn diagnostics_lists_what_failed_what_was_slow_and_the_warnings() {
    let _ui = test_ui::lock();
    let mut f = panel();
    write_trace(&f.dir.path().join("logs"));
    open_diagnostics(&mut f);

    assert!(shows(&f.panel, "5 lines, the first 10 min ago. 2 failed, 1 took over 2.0 s."));
    // What failed: the cause is the line, the step and how often under it.
    assert!(shows(&f.panel, "Can't reach ChatGPT. Check your connection and try again."));
    assert!(shows(&f.panel, "transcribe \u{b7} 1\u{d7} \u{b7} last 2 min ago"));
    // Slowest first, with its time on the right.
    assert!(shows(&f.panel, "4.2 s"));
    assert!(shows(&f.panel, "1 min ago \u{b7} failed: timed out"));
    // Most often: transcribe ran twice.
    assert!(shows(&f.panel, "2\u{d7}"));
    assert!(shows(&f.panel, "average 565 ms \u{b7} longest 820 ms \u{b7} 1 failed"));
    assert!(shows(&f.panel, "couldn't clean up (timed out), so likh pasted what it heard."));
    assert!(shows(&f.panel, "warn \u{b7} cleanup \u{b7} 1 min ago"));
}

#[test]
fn an_empty_log_says_what_will_show_up() {
    let _ui = test_ui::lock();
    let mut f = panel();
    open_diagnostics(&mut f);
    assert!(shows(&f.panel, "Nothing logged yet. Each dictation adds its steps here."));
    assert!(shows(&f.panel, "Nothing has failed."));
}

#[test]
fn diagnostics_reads_the_process_only_while_its_tab_is_open() {
    let _ui = test_ui::lock();
    let mut f = panel();
    assert!(f.panel.sampler.is_none(), "the window opens on Dictation, reading nothing");
    open_diagnostics(&mut f);
    assert!(f.panel.sampler.is_some());
    let memory = f.panel.usage.expect("the first sample").memory_bytes;
    assert!(memory > 1024 * 1024, "{memory}");
    assert!(shows(&f.panel, "Measuring\u{2026}"), "CPU needs a second sample");
    let _ = f.panel.update(Message::Sample);
    assert!(f.panel.usage.is_some_and(|usage| usage.cpu_percent.is_some()));
    assert!(!shows(&f.panel, "Measuring\u{2026}"));

    let _ = f.panel.update(Message::Tab(Tab::App));
    assert!(f.panel.sampler.is_none() && f.panel.usage.is_none(), "leaving the tab stops the sampling");
}

#[test]
fn when_chatgpts_allowance_ran_out_clean_up_and_names_say_until_when() {
    let _ui = test_ui::lock();
    let mut f = panel();
    press(&mut f.panel, "Transcription");
    let resets_at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) + 30 * 86_400;
    f.panel.allowance = Some(resets_at);
    let line = crate::allowance::paused_line(resets_at);
    assert!(line.starts_with("Paused until ") && line.ends_with(": your plan's allowance ran out."));
    assert!(shows(&f.panel, &line));
    let _ = f.panel.update(Message::Tab(Tab::Dictionary));
    f.panel.allowance = Some(resets_at);
    assert!(shows(&f.panel, &line));
}
