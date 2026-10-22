#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod actions;
mod autostart;
mod cleanup;
mod codex_auth;
mod commands;
mod ctl;
mod diagnostics;
mod dictation;
mod dictionary;
mod history;
mod hotkey;
mod media;
mod microphone;
mod onboarding;
mod panel;
mod paste;
mod pill;
mod settings;
mod shared;
mod silence;
mod sounds;
mod transcriber;
mod tray;
mod update;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc;
use std::sync::{Arc, RwLock};

/// Native dictation that types with your Codex plan.
#[derive(Parser)]
#[command(name = "likh", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run likh: hold Ctrl+Space anywhere to dictate. The default.
    Run {
        /// Don't open the first-run window, even the first time (agents and tests).
        #[arg(long)]
        skip_onboarding: bool,
    },
    /// Open the first-run window on its own.
    Onboarding,
    /// Print the Codex sign-in state. Agents: use --json.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Drive the running app: status, start, stop, action-start, action-stop, cancel, paste-last, retry, copy-last, diagnostics or quit. Prints JSON.
    Ctl {
        #[arg(value_parser = ctl::COMMANDS)]
        command: String,
    },
    /// Check your actions, or see what a phrase would run, without speaking or running anything.
    Actions {
        #[command(subcommand)]
        command: ActionsCommand,
    },
}

#[derive(Subcommand)]
enum ActionsCommand {
    /// Every action file in the folder, and what's wrong with each, with line numbers.
    Check,
    /// Which action this phrase runs, its slots and its steps. Runs nothing.
    Test { phrase: String },
}

fn main() -> ExitCode {
    match Cli::parse().command.unwrap_or(Command::Run { skip_onboarding: false }) {
        Command::Run { skip_onboarding } => run(skip_onboarding),
        Command::Onboarding => match onboarding::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("likh: {error}");
                ExitCode::FAILURE
            }
        },
        Command::Status { json } => {
            let codex = onboarding::check_codex_now();
            if json {
                let out = serde_json::json!({ "version": env!("CARGO_PKG_VERSION"), "codex": codex });
                println!("{out}");
            } else {
                let copy = onboarding::codex_copy(&codex);
                println!("likh {} · {} · {}", env!("CARGO_PKG_VERSION"), copy.title, copy.detail);
            }
            ExitCode::SUCCESS
        }
        Command::Actions { command } => {
            let Some(dir) = actions::folder() else {
                eprintln!("likh: there is no local data folder");
                return ExitCode::FAILURE;
            };
            match command {
                ActionsCommand::Check => match actions::check(&dir) {
                    Ok(report) => {
                        print!("{report}");
                        ExitCode::SUCCESS
                    }
                    Err(report) => {
                        print!("{report}");
                        ExitCode::FAILURE
                    }
                },
                ActionsCommand::Test { phrase } => {
                    print!("{}", actions::test(&dir, &phrase));
                    ExitCode::SUCCESS
                }
            }
        }
        Command::Ctl { command } => match ctl::send(&command) {
            Ok(answer) => {
                println!("{answer}");
                if answer["ok"] == true { ExitCode::SUCCESS } else { ExitCode::FAILURE }
            }
            Err(_) => {
                eprintln!("likh isn't running. Start it with `likh`.");
                ExitCode::FAILURE
            }
        },
    }
}

/// The app: the pill, the dictation thread, the shortcut, `likh ctl`, and
/// the tray on the main thread. Onboarding opens the first time.
fn run(skip_onboarding: bool) -> ExitCode {
    if ctl::is_running() {
        let _ = ctl::send("open");
        return ExitCode::SUCCESS;
    }
    // A test instance (`LIKH_HOME`) leaves the user's own folders and registry alone.
    let own_home = std::env::var_os("LIKH_HOME").is_none();
    let adopted = dirs::data_local_dir().filter(|_| own_home).map(|data| settings::adopt_murmur_folder(&data));
    // After the move, so the logs land in likh's folder, not murmur's.
    if let Some(folder) = settings::folder() {
        diagnostics::start(&folder);
    }
    diagnostics::info("app", format!("likh {} started.", env!("CARGO_PKG_VERSION")));
    match adopted {
        Some(Ok(true)) => diagnostics::info("app", "moved your settings and history over from murmur."),
        Some(Err(reason)) => diagnostics::warn("app", format!("couldn't move murmur's settings over: {reason}")),
        Some(Ok(false)) | None => {}
    }
    if own_home && let Err(reason) = autostart::adopt_murmur_value() {
        diagnostics::warn("autostart", format!("couldn't move murmur's start-with-Windows entry over: {reason}"));
    }
    let settings = Arc::new(RwLock::new(settings::Settings::load()));
    let triggers = hotkey::Triggers::from(&settings.read().map(|s| s.clone()).unwrap_or_default());
    let shortcut = triggers.shortcut;
    // The starters and README are there to read before the first action.
    if let Some(Err(reason)) = actions::folder().map(|dir| actions::prepare(&dir)) {
        diagnostics::warn("actions", format!("couldn't set up the actions folder: {reason}"));
    }
    // One inbox for the dictation: the shortcut, the pill's stop button and `likh ctl` all send to it.
    let (dictation, inputs) = mpsc::channel();
    // One inbox for the tray's thread: `likh ctl open`, update news, a dictation landing, the pill's Pick mic.
    let (ui, ui_requests) = iced::futures::channel::mpsc::unbounded();
    let pill = pill::Pill::spawn(dictation.clone(), ui.clone());
    let fake = transcriber::Transcriber::fake_from_env();
    let updates = update::Updates::new(Some(ui.clone()));
    let status =
        dictation::spawn(dictation.clone(), inputs, pill.clone(), fake, settings.clone(), updates.clone(), ui.clone());
    if settings.read().is_ok_and(|s| s.auto_update) {
        updates.check_if_due();
    }
    if let Err(error) = ctl::serve(dictation.clone(), status.clone(), ui, updates.clone()) {
        diagnostics::warn("ctl", format!("`likh ctl` won't work this time: {error}"));
    }
    let first_run = onboarded_marker().filter(|marker| !skip_onboarding && !marker.exists());
    match hotkey::spawn(dictation, triggers) {
        Ok(()) => {
            diagnostics::info("hotkey", format!("likh is running. Hold {shortcut} to talk; `likh ctl quit` stops it."))
        }
        Err(reason) => {
            diagnostics::error("hotkey", format!("{reason} `likh ctl start` still works."));
            // Holding the keys would do nothing and say nothing. Setup says so on its own
            // shortcut step; any other launch says so now, with the way to fix it.
            if first_run.is_none() {
                let text = format!("Another app has {shortcut}, so likh can't hear it.");
                pill.show(pill::Show::Problem { text, fix: pill::Fix::Shortcut });
            }
        }
    }

    // The tray owns the main thread until "Quit likh".
    match tray::run(status, settings, first_run, ui_requests, updates) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            diagnostics::error("app", error.to_string());
            ExitCode::FAILURE
        }
    }
}

/// Exists once onboarding has been finished, so it only opens the first time.
fn onboarded_marker() -> Option<PathBuf> {
    settings::folder().map(|dir| dir.join("onboarded"))
}
