use super::*;
use crate::codex_auth::Account;
use crate::shared::test_ui;
use iced_test::simulator;

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

/// What the window sees when someone holds (or lets go of) Ctrl, then Space.
fn ctrl_space(app: &mut Onboarding, held: bool) {
    let space = keyboard::Key::Named(keyboard::key::Named::Space);
    let physical = keyboard::key::Physical::Code(keyboard::key::Code::Space);
    let location = keyboard::Location::Standard;
    let events = if held {
        vec![
            keyboard::Event::ModifiersChanged(keyboard::Modifiers::CTRL),
            keyboard::Event::KeyPressed {
                key: space.clone(),
                modified_key: space,
                physical_key: physical,
                location,
                modifiers: keyboard::Modifiers::CTRL,
                text: None,
                repeat: false,
            },
        ]
    } else {
        vec![
            keyboard::Event::KeyReleased {
                key: space.clone(),
                modified_key: space,
                physical_key: physical,
                location,
                modifiers: keyboard::Modifiers::CTRL,
            },
            keyboard::Event::ModifiersChanged(keyboard::Modifiers::empty()),
        ]
    };
    for event in events {
        let _ = app.update(Message::Key(event));
    }
}

#[test]
fn get_started_works_before_the_intro_has_finished() {
    let _ui = test_ui::lock();
    // No frames played: the flock is still flying, and the button already answers.
    let mut app = Onboarding::new(CodexStatus::NotInstalled);
    press(&mut app, "Get started");
    assert_eq!(app.step(), Step::ConnectCodex);
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
    assert!(!shows(&app, "murmur checks again every 2 seconds."));
}

#[test]
fn not_connected_says_it_rechecks_by_itself() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(CodexStatus::SignedOut);
    press(&mut app, "Get started");
    assert!(shows(&app, "murmur checks again every 2 seconds."));
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
    assert_eq!(app.step(), Step::ConnectCodex);
    assert!(shows(&app, "Connected through Codex"));
}

#[test]
fn holding_ctrl_space_lights_the_keys_and_letting_go_confirms_it() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(connected());
    for label in ["Get started", "Continue", "Sounds good"] {
        press(&mut app, label);
    }
    assert!(shows(&app, "Try it now: hold both keys."));
    ctrl_space(&mut app, true);
    assert!(shows(&app, "Now let go. That's when it pastes."));
    ctrl_space(&mut app, false);
    assert!(shows(&app, "That's the one. Works in any app."));
}

#[test]
fn only_ctrl_is_not_the_shortcut() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(connected());
    for label in ["Get started", "Continue", "Sounds good"] {
        press(&mut app, label);
    }
    let _ = app.update(Message::Key(keyboard::Event::ModifiersChanged(keyboard::Modifiers::CTRL)));
    let _ = app.update(Message::Key(keyboard::Event::ModifiersChanged(keyboard::Modifiers::empty())));
    assert!(shows(&app, "Try it now: hold both keys."));
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

#[test]
fn inside_the_app_the_shortcut_step_follows_the_real_dictation() {
    let _ui = test_ui::lock();
    let status = Arc::new(Mutex::new(Status { phase: Phase::Idle, transcriber: "echo", dictations: 0, last: None }));
    let mut app = Onboarding { dictation: Some(status.clone()), ..Onboarding::new(connected()) };
    for label in ["Get started", "Continue", "Sounds good"] {
        press(&mut app, label);
    }
    assert!(shows(&app, "Hold Ctrl + Space and say anything."));

    status.lock().expect("status").phase = Phase::Recording;
    let _ = app.update(Message::Watch);
    assert!(shows(&app, "Listening. Let go to paste."));

    // The dictation pastes into the focused practice box, which is a normal text input.
    status.lock().expect("status").phase = Phase::Idle;
    let _ = app.update(Message::Watch);
    let _ = app.update(Message::Practice("Remind me to send the deck tomorrow at nine.".into()));
    assert!(shows(&app, "That's it. It works the same in any app."));
    assert!(shows(&app, "Remind me to send the deck tomorrow at nine."));
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
fn the_finish_spells_murmur_and_settles() {
    let size = Size::new(496.0, 300.0);
    let slots = flock::layout(Shape::Word, size).slots;
    // m, u, r: 15 + 12 + 8 dots, twice.
    assert_eq!(slots.len(), 70);
    for (i, bird) in flock::birds(Shape::Word, size, Shape::Word.duration()).iter().enumerate() {
        let slot = slots[i % slots.len()];
        assert!((bird.position.x - slot.x).hypot(bird.position.y - slot.y) < 1.0, "bird {i} still moving");
    }
}

/// Regenerates the onboarding screenshots on the brand board from the real
/// views, so the board can't drift from the app. Writes docs/brand/onboarding/.
/// Run: cargo test -p murmur brand_board_onboarding -- --ignored
#[test]
#[ignore = "writes docs/brand/onboarding/*.png; run by hand after changing a screen"]
fn brand_board_onboarding() {
    let _ui = test_ui::lock();
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/brand/onboarding");
    let _ = std::fs::create_dir_all(&out);
    let idle = || Arc::new(Mutex::new(Status { phase: Phase::Idle, transcriber: "codex", dictations: 0, last: None }));
    let at = |step: Step, codex: CodexStatus| {
        let mut app = Onboarding { dictation: Some(idle()), ..Onboarding::new(codex) };
        let _ = app.go(step);
        app
    };
    let screens: Vec<(&str, Onboarding)> = vec![
        ("1-welcome", Onboarding { clock: 5.0, ..Onboarding::new(connected()) }),
        ("2-codex-missing", at(Step::ConnectCodex, CodexStatus::NotInstalled)),
        ("2-codex-connected", at(Step::ConnectCodex, connected())),
        ("3-microphone", {
            let mut app = at(Step::Microphone, connected());
            let _ = app.update(Message::Mic(microphone::Event::Opened { device: "Microphone (USB Audio)".into() }));
            app.mic.history = [0.9, 0.7, 0.5, 0.3, 0.15];
            app
        }),
        ("4-shortcut", {
            let mut app = at(Step::Shortcut, connected());
            app.practice = "Remind me to send the deck tomorrow at nine.".into();
            app
        }),
        ("5-done", {
            let mut app = at(Step::Done, connected());
            app.clock = 5.0;
            app
        }),
    ];
    for (name, mut app) in screens {
        app.arrival = Spring::new(spring::DEFAULT, 1.0);
        app.progress = Spring::new(spring::DEFAULT, app.step.index() as f32);
        let settings = iced_test::core::Settings {
            fonts: theme::FONT_FILES.iter().map(|f| std::borrow::Cow::Borrowed(*f)).collect(),
            default_font: theme::UI,
            ..Default::default()
        };
        let mut ui = iced_test::Simulator::with_size(settings, WINDOW, app.view());
        let path = out.join(name);
        let _ = std::fs::remove_file(path.with_extension("png"));
        if let Ok(snapshot) = ui.snapshot(&Theme::Dark) {
            let _ = snapshot.matches_image(&path);
        }
    }
}
