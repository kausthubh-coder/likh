#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod autostart;
mod codex_auth;
mod commands;
mod ctl;
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
    /// Drive the running app: status, start, stop, cancel or quit. Prints JSON.
    Ctl {
        #[arg(value_parser = ctl::COMMANDS)]
        command: String,
    },
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
    if let Some(data) = dirs::data_local_dir().filter(|_| own_home) {
        match settings::adopt_murmur_folder(&data) {
            Ok(true) => eprintln!("likh: moved your settings and history over from murmur."),
            Ok(false) => {}
            Err(reason) => eprintln!("likh: couldn't move murmur's settings over: {reason}"),
        }
    }
    if own_home && let Err(reason) = autostart::adopt_murmur_value() {
        eprintln!("likh: couldn't move murmur's start-with-Windows entry over: {reason}");
    }
    let settings = Arc::new(RwLock::new(settings::Settings::load()));
    let (shortcut, mouse) = settings.read().map(|s| (s.shortcut, s.mouse)).unwrap_or_default();
    // One inbox for the dictation: the shortcut, the pill's stop button and `likh ctl` all send to it.
    let (dictation, inputs) = mpsc::channel();
    let pill = pill::Pill::spawn(dictation.clone());
    let fake = transcriber::Transcriber::fake_from_env();
    let (ui, ui_requests) = iced::futures::channel::mpsc::unbounded();
    let updates = update::Updates::new(Some(ui.clone()));
    let status = dictation::spawn(dictation.clone(), inputs, pill, fake, settings.clone(), updates.clone());
    if settings.read().is_ok_and(|s| s.auto_update) {
        updates.check_if_due();
    }
    if let Err(error) = ctl::serve(dictation.clone(), status.clone(), ui, updates.clone()) {
        eprintln!("likh: `likh ctl` won't work this time: {error}");
    }
    match hotkey::spawn(dictation, shortcut, mouse) {
        Ok(()) => eprintln!("likh is running. Hold {shortcut} to talk; `likh ctl quit` stops it."),
        Err(reason) => eprintln!("likh: {reason} `likh ctl start` still works."),
    }

    let first_run = onboarded_marker().filter(|marker| !skip_onboarding && !marker.exists());
    // The tray owns the main thread until "Quit likh".
    match tray::run(status, settings, first_run, ui_requests, updates) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("likh: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Exists once onboarding has been finished, so it only opens the first time.
fn onboarded_marker() -> Option<PathBuf> {
    settings::folder().map(|dir| dir.join("onboarded"))
}
