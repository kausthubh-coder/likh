use super::*;
use crate::codex_auth::Account;
use crate::settings::Settings;
use crate::shared::test_ui;
use iced_test::simulator;
use std::sync::RwLock;

fn connected() -> CodexStatus {
    CodexStatus::Connected(Account {
        email_hint: Some("k•••@gmail.com".into()),
        plan: Some("plus".into()),
        expires_at: Some(1),
    })
}

fn press(app: &mut Onboarding, label: &str) {
    let mut ui = simulator(app.view());
    ui.click(label).unwrap_or_else(|_| panic!("no button labelled {label:?}"));
    for message in ui.into_messages() {
        let _ = app.update(message);
    }
}

fn shows(app: &Onboarding, words: &str) -> bool {
    simulator(app.view()).find(words).is_ok()
}

/// Feeds 60 fps frames for `seconds`, the way the window's frame subscription does.
fn play(app: &mut Onboarding, seconds: f32) {
    let start = Instant::now();
    for frame in 0..=(seconds * 60.0) as u32 {
        let _ = app.update(Message::Frame(start + Duration::from_secs_f32(frame as f32 / 60.0)));
    }
}

#[test]
fn get_started_works_before_the_intro_has_finished() {
    let _ui = test_ui::lock();
    // No frames played: the flock is still flying, and the button already answers.
    let mut app = Onboarding::new(CodexStatus::NotInstalled);
    press(&mut app, "Get started");
    assert_eq!(app.step(), Step::Connect);
    assert!(shows(&app, "Codex isn't installed"));
}

#[test]
fn every_sign_in_state_shows_its_fix() {
    let _ui = test_ui::lock();
    let cases = [
        (CodexStatus::NotInstalled, "Get Codex"),
        (CodexStatus::SignedOut, "Open Codex (or run codex login) and sign in."),
        (
            CodexStatus::Expired(Account { email_hint: None, plan: None, expires_at: Some(1) }),
            "Your Codex sign-in expired",
        ),
        (connected(), "k•••@gmail.com · ChatGPT Plus"),
    ];
    for (status, expected) in cases {
        let mut app = Onboarding::new(status);
        press(&mut app, "Get started");
        assert!(shows(&app, expected), "missing {expected:?}");
    }
}

#[test]
fn connected_offers_continue_not_recheck() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(connected());
    press(&mut app, "Get started");
    assert!(shows(&app, "Continue"));
    assert!(!shows(&app, "Check again"));
    assert!(!shows(&app, "likh checks again every 2 seconds."));
}

#[test]
fn not_connected_says_it_rechecks_by_itself() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(CodexStatus::SignedOut);
    press(&mut app, "Get started");
    assert!(shows(&app, "likh checks again every 2 seconds."));
}

#[test]
fn walks_every_step_to_the_finish() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(connected());
    press(&mut app, "Get started");
    press(&mut app, "Continue");
    assert_eq!(app.step(), Step::Microphone);
    assert!(shows(&app, "Say something"));
    press(&mut app, "Sounds good");
    assert_eq!(app.step(), Step::Shortcut);
    assert!(shows(&app, "Hold to talk"));
    press(&mut app, "Continue");
    assert_eq!(app.step(), Step::Done);
    assert!(shows(&app, "You're all set"));
    assert!(shows(&app, "Finish"));
}

#[test]
fn back_returns_to_the_step_before() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(connected());
    press(&mut app, "Get started");
    press(&mut app, "Continue");
    press(&mut app, "Back");
    assert_eq!(app.step(), Step::Connect);
    assert!(shows(&app, "Connected through Codex"));
}

#[test]
fn a_missing_microphone_shows_the_fix_and_still_lets_you_continue() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(connected());
    press(&mut app, "Get started");
    press(&mut app, "Continue");
    let _ = app.update(Message::Mic(microphone::Event::Failed("No microphone is connected.".into())));
    assert!(shows(&app, "Can't hear a microphone"));
    assert!(shows(&app, "No microphone is connected"));
    assert!(shows(&app, "Open settings"));
    press(&mut app, "Continue");
    assert_eq!(app.step(), Step::Shortcut);
}

fn in_app() -> (Onboarding, Arc<Mutex<Status>>) {
    let status = Arc::new(Mutex::new(Status {
        phase: Phase::Idle,
        transcriber: "echo",
        dictations: 0,
        actions: 0,
        last: None,
        last_text: None,
    }));
    let settings = Arc::new(RwLock::new(Settings::default()));
    let app = Onboarding::in_app(window::Id::unique(), status.clone(), settings);
    (Onboarding { codex: connected(), ..app }, status)
}

fn to_the_shortcut_step(app: &mut Onboarding) {
    for label in ["Get started", "Continue", "Sounds good"] {
        press(app, label);
    }
    assert_eq!(app.step(), Step::Shortcut);
}

#[test]
fn on_its_own_the_shortcut_step_says_to_start_likh() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(connected());
    to_the_shortcut_step(&mut app);
    assert!(shows(&app, "Start likh to try it here."));
}

#[test]
fn saying_continue_moves_onboarding_on_by_itself() {
    let _ui = test_ui::lock();
    let (mut app, status) = in_app();
    to_the_shortcut_step(&mut app);
    assert!(shows(&app, "Hold Ctrl + Space and say \u{201c}continue\u{201d}."));

    status.lock().expect("status").phase = Phase::Recording;
    let _ = app.update(Message::Watch);
    assert!(shows(&app, "Listening. Let go when you're done."));

    // The dictation pastes into the focused practice box, which is a normal text input.
    status.lock().expect("status").phase = Phase::Idle;
    let _ = app.update(Message::Watch);
    let _ = app.update(Message::Practice("Remind me to send the deck tomorrow at nine.".into()));
    assert!(shows(&app, "That landed. Now try saying \u{201c}continue\u{201d}."));
    assert_eq!(app.step(), Step::Shortcut);

    let _ = app.update(Message::Practice("Okay. Continue.".into()));
    assert!(shows(&app, "Heard you. On to the last step."));
    // A moment later the step moves on without a click.
    app.practice.advance_at = Some(Instant::now());
    let _ = app.update(Message::Watch);
    assert_eq!(app.step(), Step::Done);
}

#[test]
fn changing_the_shortcut_shows_the_keys_as_they_go_down() {
    let _ui = test_ui::lock();
    let (mut app, _) = in_app();
    to_the_shortcut_step(&mut app);
    press(&mut app, "Change shortcut");
    assert!(shows(&app, "Press your shortcut"));

    let held = keyboard::Modifiers::CTRL | keyboard::Modifiers::ALT;
    let _ = app.update(Message::Key(keyboard::Event::ModifiersChanged(held)));
    assert!(shows(&app, "Ctrl") && shows(&app, "Alt"));

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
    let _ = app.update(Message::Key(bare));
    assert!(shows(&app, "Add Ctrl, Alt or Win, so typing never sets it off."));
    assert!(shows(&app, "Press your shortcut"));

    // "Keep current" backs out to the practice.
    press(&mut app, "Keep current");
    assert!(shows(&app, "Try it"));
}

#[test]
fn the_microphone_step_names_the_device_it_opened() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(connected());
    press(&mut app, "Get started");
    press(&mut app, "Continue");
    assert!(shows(&app, "Opening your microphone…"));
    let _ = app.update(Message::Mic(microphone::Event::Opened { device: "Headset (Jabra Evolve2)".into() }));
    assert!(shows(&app, "Headset (Jabra Evolve2)"));
}

#[test]
fn mic_bars_rise_fast_and_fall_slowly() {
    let mut app = Onboarding::new(connected());
    let _ = app.update(Message::Mic(microphone::Event::Level(1.0)));
    assert!((app.mic.history[0] - 0.75).abs() < 1e-6);
    let _ = app.update(Message::Mic(microphone::Event::Level(0.0)));
    assert!((app.mic.history[0] - 0.6).abs() < 1e-6);
    // The older level moved outwards.
    assert!((app.mic.history[1] - 0.75).abs() < 1e-6);
}

#[test]
fn with_animations_off_the_intro_is_already_home_and_steps_only_fade() {
    let mut app = Onboarding::new(connected()).hold_still(true);
    // The flock has landed and the text is there: nothing to play.
    play(&mut app, 0.5);
    assert!(!app.wants_frames(), "a still welcome screen asks for no frames");
    press(&mut app, "Get started");
    // The step fades in, and the wide progress dot is already on the new step.
    assert!((app.progress.value - 1.0).abs() < f32::EPSILON);
}

#[test]
fn the_window_stops_drawing_once_the_intro_lands() {
    let mut app = Onboarding::new(connected());
    assert!(app.wants_frames(), "the intro should be playing");
    play(&mut app, 3.0);
    assert!(!app.wants_frames(), "idle welcome screen still asks for frames");

    let _ = app.update(Message::Next);
    assert!(app.wants_frames(), "the step should be sliding in");
    play(&mut app, 1.5);
    assert!(!app.wants_frames(), "idle Codex step still asks for frames");
}

#[test]
fn the_flock_flies_in_from_the_left_and_lands_on_the_logo() {
    let size = Size::new(496.0, 300.0);
    let start = flock::birds(Shape::Mark, size, 0.0);
    assert!(start.iter().all(|bird| bird.landed == 0.0));
    let mean_x = start.iter().map(|bird| bird.position.x).sum::<f32>() / start.len() as f32;
    assert!(mean_x < -0.3 * size.width, "flock starts at {mean_x}, not off to the left");

    let slots = flock::layout(Shape::Mark, size).slots;
    for (i, bird) in flock::birds(Shape::Mark, size, Shape::Mark.duration()).iter().enumerate() {
        let slot = slots[i % slots.len()];
        let off = (bird.position.x - slot.x).hypot(bird.position.y - slot.y);
        assert!(off < 1.0, "bird {i} is {off}px from its spot");
    }
}

#[test]
fn the_finish_spells_likh_and_settles() {
    let size = Size::new(496.0, 300.0);
    let slots = flock::layout(Shape::Word, size).slots;
    // l, i, k, h: 5 + 4 + 10 + 9 dots.
    assert_eq!(slots.len(), 28);
    for (i, bird) in flock::birds(Shape::Word, size, Shape::Word.duration()).iter().enumerate() {
        let slot = slots[i % slots.len()];
        assert!((bird.position.x - slot.x).hypot(bird.position.y - slot.y) < 1.0, "bird {i} still moving");
    }
}

/// Regenerates the onboarding screenshots on the brand board from the real
/// views, so the board can't drift from the app. Writes docs/brand/onboarding/.
/// Run: cargo test -p likh brand_board_onboarding -- --ignored
#[test]
#[ignore = "writes docs/brand/onboarding/*.png; run by hand after changing a screen"]
fn brand_board_onboarding() {
    let _ui = test_ui::lock();
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/brand/onboarding");
    let _ = std::fs::create_dir_all(&out);
    let idle = || {
        Arc::new(Mutex::new(Status {
            phase: Phase::Idle,
            transcriber: "codex",
            dictations: 0,
            actions: 0,
            last: None,
            last_text: None,
        }))
    };
    let at = |step: Step, codex: CodexStatus| {
        let mut app = Onboarding { dictation: Some(idle()), ..Onboarding::new(codex) };
        let _ = app.go(step);
        app
    };
    let screens: Vec<(&str, Onboarding)> = vec![
        ("1-welcome", Onboarding { clock: 5.0, ..Onboarding::new(connected()) }),
        ("2-codex-missing", at(Step::Connect, CodexStatus::NotInstalled)),
        ("2-codex-connected", at(Step::Connect, connected())),
        ("2-groq-key", {
            let mut app = at(Step::Connect, connected());
            app.engine = Engine::Groq;
            app.groq = GroqSetup { key: "gsk_0123456789abcdef".into(), ..GroqSetup::default() };
            app
        }),
        ("3-microphone", {
            let mut app = at(Step::Microphone, connected());
            let _ = app.update(Message::Mic(microphone::Event::Opened { device: "Microphone (USB Audio)".into() }));
            app.mic.history = [0.9, 0.7, 0.5, 0.3, 0.15];
            app
        }),
        ("4-shortcut", {
            let mut app = at(Step::Shortcut, connected());
            app.practice.words = "Remind me to send the deck tomorrow at nine.".into();
            app
        }),
        ("4-shortcut-taken", at(Step::Shortcut, connected())),
        ("5-done", {
            let mut app = at(Step::Done, connected());
            app.clock = 5.0;
            app
        }),
    ];
    for (name, mut app) in screens {
        let _taken = hotkey::pretend_taken(name.ends_with("-taken"), false);
        app.arrival = Spring::new(spring::DEFAULT, 1.0);
        app.progress = Spring::new(spring::DEFAULT, app.step.index() as f32);
        let settings = iced_test::core::Settings {
            fonts: theme::FONT_FILES.iter().map(|f| std::borrow::Cow::Borrowed(*f)).collect(),
            default_font: theme::UI,
            ..Default::default()
        };
        let mut ui = iced_test::Simulator::with_size(settings, WINDOW, app.view());
        let path = out.join(name);
        // iced writes `<name>-tiny-skia.png`, and only when it's missing.
        let _ = std::fs::remove_file(out.join(format!("{name}-tiny-skia.png")));
        if let Ok(snapshot) = ui.snapshot(&Theme::Dark) {
            let _ = snapshot.matches_image(&path);
        }
    }
}

#[test]
fn plans_read_the_way_chatgpt_names_them() {
    assert_eq!(plan_name("plus"), "Plus");
    assert_eq!(plan_name("prolite"), "Pro Lite");
    assert_eq!(plan_name("pro"), "Pro");
}

#[test]
fn the_connect_step_offers_every_engine_and_starts_on_chatgpt() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(connected());
    press(&mut app, "Get started");
    for engine in Engine::ALL {
        assert!(shows(&app, engine.name()), "{engine:?}");
    }
    assert!(shows(&app, "Connected through Codex"));
}

#[test]
fn picking_groq_asks_for_a_key_and_refuses_one_from_elsewhere() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(CodexStatus::NotInstalled);
    press(&mut app, "Get started");
    press(&mut app, "Groq");
    assert!(shows(&app, "Add a free Groq key"));
    assert!(shows(&app, "Get a free key"));
    let _ = app.update(Message::GroqKey("sk-proj-0123456789abcdef0123".into()));
    press(&mut app, "Save key");
    assert!(shows(&app, "That isn't a Groq key. Groq keys start with gsk_."));
    assert_eq!(app.step(), Step::Connect);
}

#[test]
fn a_saved_groq_key_lets_you_continue() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(CodexStatus::NotInstalled);
    press(&mut app, "Get started");
    let _ = app.update(Message::Engine(Engine::Groq));
    app.groq.saved = true;
    assert!(shows(&app, "Groq is ready"));
    press(&mut app, "Continue");
    assert_eq!(app.step(), Step::Microphone);
}

#[test]
fn welcome_says_dictation_is_free_with_any_chatgpt_account() {
    let _ui = test_ui::lock();
    let app = Onboarding::new(CodexStatus::NotInstalled);
    assert!(shows(&app, "Free dictation with any ChatGPT account. Hold a key, say it, let go."));
}

#[test]
fn without_codex_the_connect_step_says_why_it_needs_it() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(CodexStatus::NotInstalled);
    press(&mut app, "Get started");
    assert!(shows(&app, "likh uses your ChatGPT account through Codex, free on any plan. Install it, sign in once."));
}

#[test]
fn connected_says_plainly_that_clean_up_can_run_out_on_free() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(connected());
    press(&mut app, "Get started");
    assert!(shows(&app, "Dictation is free on every plan. Clean up, a bonus, can run out on Free."));
}

#[test]
fn the_last_step_teaches_talk_cancel_and_paste_last() {
    let _ui = test_ui::lock();
    let (mut app, _) = in_app();
    to_the_shortcut_step(&mut app);
    press(&mut app, "Continue");
    assert_eq!(app.step(), Step::Done);
    for words in ["Hold to talk, let go to paste", "Esc", "Cancel", "Paste the last one again", "Shift", "Z"] {
        assert!(shows(&app, words), "missing {words:?}");
    }
}

#[test]
fn the_last_step_leaves_paste_last_out_when_its_off() {
    let _ui = test_ui::lock();
    let (mut app, _) = in_app();
    if let Some(settings) = &app.settings {
        settings.write().expect("settings").paste_last = None;
    }
    to_the_shortcut_step(&mut app);
    press(&mut app, "Continue");
    assert!(shows(&app, "Cancel"));
    assert!(!shows(&app, "Paste the last one again"));
}

#[test]
fn a_shortcut_another_app_holds_leads_with_picking_new_keys() {
    let _ui = test_ui::lock();
    let (mut app, _) = in_app();
    to_the_shortcut_step(&mut app);
    let _taken = hotkey::pretend_taken(true, false);
    if cfg!(target_os = "macos") {
        // macOS: the fix is the Accessibility permission, not other keys.
        assert!(shows(&app, "Let likh hear its keys"));
        assert!(shows(&app, "Turn likh on in System Settings, Privacy & Security, Accessibility."));
        assert!(shows(&app, "Open settings") && shows(&app, "Skip"));
        return;
    }
    assert!(shows(&app, "Another app has these keys"));
    assert!(shows(&app, "Ctrl + Space belongs to another app, so likh can't hear it. Pick new keys."));
    press(&mut app, "Change shortcut");
    assert!(shows(&app, "Press your shortcut"), "Change shortcut listens for new keys right there");
}

#[test]
fn picking_groq_without_a_key_saves_nothing_until_a_key_is() {
    let _ui = test_ui::lock();
    let (mut app, _) = in_app();
    app.groq = GroqSetup::default();
    press(&mut app, "Get started");
    press(&mut app, "Groq");
    assert!(shows(&app, "Add a free Groq key"));
    let saved = app.settings.as_ref().map(|s| s.read().expect("settings").engine);
    assert_eq!(saved, Some(Engine::Codex), "ChatGPT still transcribes until a Groq key is saved");
}
