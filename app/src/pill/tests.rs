use super::*;

/// Steps the look at 60 fps for `seconds`; returns whether it was still moving at the end.
fn play(look: &mut Look, seconds: f32) -> bool {
    let mut moving = true;
    for _ in 0..(seconds * 60.0) as usize {
        moving = look.step(1.0 / 60.0);
    }
    moving
}

fn rgba_at(pixmap: &Pixmap, x: f32, y: f32) -> [u8; 4] {
    let pixel = pixmap.pixel(x as u32, y as u32).map(|p| p.demultiply());
    pixel.map_or([0; 4], |p| [p.red(), p.green(), p.blue(), p.alpha()])
}

/// The pill's centre in canvas pixels for a pill of height `h`.
fn centre(h: f32) -> (f32, f32) {
    (CANVAS.0 / 2.0, CANVAS.1 - LIFT - h / 2.0)
}

#[test]
fn hidden_draws_nothing() {
    let look = Look::default();
    assert!(look.is_hidden());
    let pixmap = look.draw(1.0).expect("pixmap");
    assert!(pixmap.pixels().iter().all(|p| p.alpha() == 0));
}

#[test]
fn listening_is_a_76_by_36_capsule_at_the_bottom_centre_with_peach_bars() {
    let mut look = Look::default();
    look.set(Show::Listening);
    for _ in 0..8 {
        look.level(0.8);
    }
    play(&mut look, 0.6);
    let pixmap = look.draw(1.0).expect("pixmap");
    let (cx, cy) = centre(36.0);
    // Inside, near the right end (past the bars): the solid surface colour.
    assert_eq!(rgba_at(&pixmap, cx + 30.0, cy)[3], 255);
    // Just outside the right end: only the faint shadow.
    assert!(rgba_at(&pixmap, cx + 42.0, cy)[3] < 60);
    // The middle bar (index 3) is peach: rgb(242, 160, 124).
    let bar = rgba_at(&pixmap, cx - 38.0 + BAR_INSET + 3.0 * (BAR_WIDTH + BAR_GAP) + 2.0, cy);
    assert!(bar[0] > 230 && (i32::from(bar[1]) - 160).abs() < 12, "{bar:?}");
}

#[test]
fn it_draws_at_the_monitors_scale() {
    let mut look = Look::default();
    look.set(Show::Thinking);
    play(&mut look, 0.6);
    let pixmap = look.draw(1.5).expect("pixmap");
    assert_eq!((pixmap.width(), pixmap.height()), (600, 168));
}

#[test]
fn done_lingers_as_a_sliver_then_hides_and_stops_drawing() {
    let mut look = Look::default();
    look.set(Show::Listening);
    play(&mut look, 0.5);
    look.set(Show::Done);
    assert!(play(&mut look, 0.1), "should still be moving right after done");
    assert!(!play(&mut look, 1.0), "a finished pill must stop asking for frames");
    assert!(look.is_hidden());
}

#[test]
fn an_error_stays_six_seconds_then_fades() {
    let mut look = Look::default();
    look.set(Show::Error("Can't reach ChatGPT. Check your connection and try again.".into()));
    play(&mut look, 5.5);
    assert!(!look.is_hidden());
    play(&mut look, 1.0);
    assert!(look.is_hidden());
}

#[test]
fn a_note_hugs_its_words_in_one_line_shows_grey_flat_bars_and_leaves_after_three_seconds() {
    let mut look = Look::default();
    look.set(Show::Note("No voice heard".into()));
    assert_eq!(look.width.target, MESSAGE_TEXT_X + text_width("No voice heard", ERROR_TEXT) + 16.0);
    assert_eq!(look.height.target, 36.0);
    play(&mut look, 0.6);
    let pixmap = look.draw(1.0).expect("pixmap");
    let (cx, cy) = centre(36.0);
    // The middle flat bar, where an error's red icon would be: grey, not red.
    let icon = rgba_at(&pixmap, cx - look.width.value / 2.0 + 24.0, cy);
    assert!(icon[0] > 140 && (i32::from(icon[0]) - i32::from(icon[2])).abs() < 30, "{icon:?}");
    assert_eq!(look.target(), None, "a note takes no clicks");
    play(&mut look, 2.0);
    assert!(!look.is_hidden());
    assert!(!play(&mut look, 1.5), "gone, and no more frames");
    assert!(look.is_hidden());
}

#[test]
fn a_problem_has_one_button_on_the_right_that_opens_its_fix() {
    let mut look = Look::default();
    look.set(Show::Problem { text: "No sound from Microphone Array. Is it muted?".into(), fix: Fix::PickMic });
    assert_eq!(look.target(), None, "no button until the pill has grown to hold it");
    play(&mut look, 0.6);
    let ((x, y, w, h), click) = look.target().expect("a button");
    assert_eq!(click, Click::Fix(Fix::PickMic));
    let right = (CANVAS.0 + look.width.value) / 2.0;
    assert!((right - (x + w) - 8.0).abs() < 0.01, "inset 8 from the right end");
    assert_eq!(h, 36.0);
    assert!(look.height.value - h - 16.0 < 0.01, "inset 8 top and bottom: {}", look.height.value);
    let (cx, cy) = centre(look.height.value);
    assert!(look.covers(cx, cy) && !look.covers(cx, cy - 40.0));
    // The label sits inside the button: bright text on the elevated fill.
    let pixmap = look.draw(1.0).expect("pixmap");
    let lit = (0..w as u32)
        .filter(|dx| rgba_at(&pixmap, x + *dx as f32, y + h / 2.0).iter().take(3).all(|c| *c > 200))
        .count();
    assert!(lit > 0, "the label");
}

#[test]
fn a_problem_waits_while_the_pointer_is_on_it() {
    let mut look = Look::default();
    look.set(Show::Problem { text: "Windows is blocking likh from the mic.".into(), fix: Fix::MicAccess });
    assert!(look.reading());
    look.hover(true);
    play(&mut look, 10.0);
    assert!(!look.is_hidden(), "still up after 10 s under the pointer");
    look.hover(false);
    play(&mut look, 6.5);
    assert!(look.is_hidden());
}

#[test]
fn a_silent_mic_stays_up_while_recording_and_settles_without_frames() {
    let mut look = Look::default();
    look.set(Show::Listening);
    play(&mut look, 0.5);
    look.set(Show::MicSilent { device: "Microphone Array".into() });
    assert!(!play(&mut look, 2.0), "a still message draws no frames");
    assert!(!look.is_hidden());
    assert!(!play(&mut look, 30.0));
    assert!(!look.is_hidden(), "it leaves when the dictation does, not on a timer");
}

#[test]
fn long_errors_wrap_to_two_lines_and_trail_off() {
    let short = wrap("Can't reach ChatGPT.", 300.0);
    assert_eq!(short, vec!["Can't reach ChatGPT."]);

    let long = "Your Codex sign-in expired. Open Codex once to refresh it, then try again. If that doesn't work, sign out of Codex and back in, then come back here.";
    let lines = wrap(long, 300.0);
    assert_eq!(lines.len(), 2);
    assert!(lines[1].ends_with('…'), "{lines:?}");
    assert!(lines.iter().all(|line| text_width(line, ERROR_TEXT) <= 300.0 + 0.5), "{lines:?}");
}

#[test]
fn the_error_pill_hugs_its_text_between_200_and_360() {
    let mut look = Look::default();
    look.set(Show::Error("Too short.".into()));
    assert_eq!(look.width.target, 200.0);

    let message = "Your Codex sign-in expired. Open Codex once to refresh it, then try again.";
    look.set(Show::Error(message.into()));
    let widest = wrap(message, 300.0).iter().map(|line| text_width(line, ERROR_TEXT)).fold(0.0, f32::max);
    assert_eq!(look.width.target, widest + 60.0);
    assert!(look.width.target <= 360.0);
    assert_eq!(look.height.target, 52.0);
}

/// Near-white pixels (the stop square or the seconds) inside the stop button.
fn bright_in_button(pixmap: &Pixmap, look: &Look) -> usize {
    let (bx, by, _) = look.stop_button().expect("a stop button");
    let mut bright = 0;
    for dy in -6..=6 {
        for dx in -6..=6 {
            let [r, g, b, _] = rgba_at(pixmap, bx + dx as f32, by + dy as f32);
            if r > 200 && g > 200 && b > 200 {
                bright += 1;
            }
        }
    }
    bright
}

#[test]
fn quiet_stop_keeps_the_stop_button_and_empties_a_peach_ring_on_its_own() {
    let mut look = Look::default();
    let after = Duration::from_secs(5);
    look.set(Show::QuietStop { left: Duration::from_millis(4_500), after });
    play(&mut look, 0.5);
    let (bx, by, r) = look.stop_button().expect("quiet stop keeps the stop button");
    let pixmap = look.draw(1.0).expect("pixmap");
    // 4 of 5 seconds left: the ring still reaches nine o'clock (three quarters round).
    let nine = rgba_at(&pixmap, bx - (r - 1.5), by);
    assert!(nine[0] > 200 && (i32::from(nine[1]) - 160).abs() < 30, "{nine:?}");
    assert!(bright_in_button(&pixmap, &look) >= 60, "the stop square, while more than 3 s are left");

    // Between the dictation's ticks the pill keeps counting down by itself.
    assert!(play(&mut look, 1.0), "a counting pill keeps drawing");
    let pixmap = look.draw(1.0).expect("pixmap");
    let nine = rgba_at(&pixmap, bx - (r - 1.5), by);
    assert!(nine[0] < 120, "3 of 5 seconds left: the ring has left nine o'clock, {nine:?}");
    // The last 3 seconds show the seconds instead of the square.
    let digits = bright_in_button(&pixmap, &look);
    assert!(digits > 0 && digits < 60, "{digits}");
}

#[test]
fn with_animations_off_the_pill_takes_its_shape_at_once_and_thinking_stands_still() {
    let mut look = Look::default();
    look.hold_still(true);
    look.set(Show::Listening);
    // No growing out of the sliver: the very first frame is already 76 x 36.
    assert_eq!((look.width.value, look.height.value), (76.0, 36.0));
    look.set(Show::Error("Can't reach ChatGPT.".into()));
    assert_eq!(look.height.value, 52.0);

    look.set(Show::Thinking);
    play(&mut look, 0.6);
    let first = look.bars;
    play(&mut look, 0.3);
    // The bars stand level and stay put; only their colour pulses.
    assert!(first.iter().all(|bar| (bar - 0.3).abs() < 0.01), "{first:?}");
    assert_eq!(first, look.bars);
}

/// Regenerates the pill pictures on the brand board from the real renderer.
/// Run: cargo test -p likh brand_board_pill -- --ignored
#[test]
#[ignore = "writes docs/brand/pill/*.png; run by hand after changing the pill"]
fn brand_board_pill() {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/brand/pill");
    let _ = std::fs::create_dir_all(&out);
    let states = [
        ("listening", Show::Listening),
        ("thinking", Show::Thinking),
        ("warning", Show::Warning { seconds_left: 55 }),
        ("quiet-stop", Show::QuietStop { left: Duration::from_millis(2_600), after: Duration::from_secs(5) }),
        ("error", Show::Error("Your Codex sign-in expired. Open Codex once to refresh it, then try again.".into())),
        ("problem", Show::Problem { text: "No sound from Microphone Array. Is it muted?".into(), fix: Fix::PickMic }),
        ("note", Show::Note("No voice heard".into())),
        ("mic-silent", Show::MicSilent { device: "Microphone Array".into() }),
        ("action-listening", Show::Spark(Box::new(Show::Listening))),
        ("action-hands-free", Show::Spark(Box::new(Show::HandsFree))),
        ("action-thinking", Show::Spark(Box::new(Show::Thinking))),
        ("action-running", Show::Running("New T3 thread".into())),
        ("action-finished", Show::Finished("Thread started".into())),
        (
            "action-confirm",
            Show::Confirm {
                question: "Run Deploy preview?".into(),
                keys: "Ctrl+Shift+Space".into(),
                left: Duration::from_millis(3_100),
                after: Duration::from_secs(5),
            },
        ),
        (
            "action-review",
            Show::Problem { text: "deploy.toml isn't approved yet, so it won't run.".into(), fix: Fix::Review },
        ),
        ("action-no-match", Show::Note("No action for \u{201c}open spotfy\u{201d}".into())),
    ];
    for (name, show) in states {
        let mut look = Look::default();
        look.set(show);
        for level in [0.95, 0.6, 0.85, 0.4] {
            look.level(level);
        }
        play(&mut look, 0.6);
        if let Some(pixmap) = look.draw(2.0) {
            let _ = pixmap.save_png(out.join(format!("{name}.png")));
        }
    }
}

#[test]
fn an_actions_recording_carries_the_spark_on_the_left_and_is_24_wider() {
    let mut look = Look::default();
    look.set(Show::Spark(Box::new(Show::Listening)));
    play(&mut look, 0.6);
    assert_eq!((look.width.value.round(), look.height.value.round()), (100.0, 36.0));
    let pixmap = look.draw(1.0).expect("pixmap");
    let (cx, cy) = centre(36.0);
    // The spark sits in its slot where a dictation's first bars would be, in the text colour.
    let [r, g, b, _] = rgba_at(&pixmap, cx - 50.0 + SLOT_INSET + SLOT / 2.0, cy);
    assert!(r > 200 && g > 200 && b > 200, "got {r},{g},{b}");
    // Hands-free with the spark keeps its stop button on the right: 124 wide.
    look.set(Show::Spark(Box::new(Show::HandsFree)));
    play(&mut look, 0.6);
    assert_eq!(look.width.value.round(), 124.0);
    assert!(look.stop_button().is_some());
    // The dictation shortcut's own recording has no spark.
    look.set(Show::Listening);
    play(&mut look, 0.6);
    assert_eq!(look.width.value.round(), 76.0);
}

#[test]
fn a_running_action_names_itself_and_its_result_leaves_after_two_seconds() {
    let mut look = Look::default();
    look.set(Show::Running("New T3 thread".into()));
    assert!(play(&mut look, 1.0), "the running wave moves");
    let width = look.width.value;
    assert!(width > 100.0 && width < 220.0, "hugs its name, got {width}");
    look.set(Show::Finished("Thread started".into()));
    assert!(play(&mut look, 1.5));
    assert!(!play(&mut look, 1.0), "gone and drawing nothing after about two seconds");
    assert!(look.is_hidden());
}

#[test]
fn a_confirm_counts_down_on_its_own_and_a_long_result_stays_on_one_line() {
    let mut look = Look::default();
    let after = Duration::from_secs(5);
    look.set(Show::Confirm {
        question: "Run Deploy preview?".into(),
        keys: "Ctrl+Shift+Space".into(),
        left: after,
        after,
    });
    play(&mut look, 1.0);
    match &look.show {
        Show::Confirm { left, .. } => assert!(*left <= Duration::from_millis(4_050), "{left:?}"),
        other => panic!("{other:?}"),
    }
    assert!(look.width.value <= ERROR_MAX_WIDTH, "fits the pill's widest");
    look.set(Show::Finished("word ".repeat(80)));
    assert_eq!(look.lines.len(), 1);
    assert!(look.lines[0].ends_with('\u{2026}'));
    assert!(look.width.target <= ERROR_MAX_WIDTH);
}
