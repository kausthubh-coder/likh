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

#[test]
fn get_started_moves_to_connect_codex() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(CodexStatus::NotInstalled);
    press(&mut app, "Get started");
    assert_eq!(app.step(), Step::ConnectCodex);
    assert!(simulator(app.view()).find("Codex isn't installed").is_ok());
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
        assert!(simulator(app.view()).find(expected).is_ok(), "missing {expected:?}");
    }
}

#[test]
fn connected_offers_continue_not_recheck() {
    let _ui = test_ui::lock();
    let mut app = Onboarding::new(connected());
    press(&mut app, "Get started");
    let mut ui = simulator(app.view());
    assert!(ui.find("Continue").is_ok());
    assert!(ui.find("Check again").is_err());
}
