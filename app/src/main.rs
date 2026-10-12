mod autostart;
mod codex_auth;
mod commands;
mod ctl;
mod dictation;
mod history;
mod hotkey;
mod microphone;
mod onboarding;
mod panel;
mod paste;
mod pill;
mod settings;
mod shared;
mod sounds;
mod transcriber;
mod tray;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc;
use std::sync::{Arc, RwLock};

/// Native dictation that types with your Codex plan.
#[derive(Parser)]
#[command(name = "murmur", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run murmur: hold Ctrl+Space anywhere to dictate. The default.
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
                eprintln!("murmur: {error}");
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
                println!("murmur {} · {} · {}", env!("CARGO_PKG_VERSION"), copy.title, copy.detail);
            }
            ExitCode::SUCCESS
        }
        Command::Ctl { command } => match ctl::send(&command) {
            Ok(answer) => {
                println!("{answer}");
                if answer["ok"] == true { ExitCode::SUCCESS } else { ExitCode::FAILURE }
            }
            Err(_) => {
                eprintln!("murmur isn't running. Start it with `murmur`.");
                ExitCode::FAILURE
            }
        },
    }
}

/// The app: the pill, the dictation thread, the shortcut, `murmur ctl`, and
/// the tray on the main thread. Onboarding opens the first time.
fn run(skip_onboarding: bool) -> ExitCode {
    if ctl::is_running() {
        eprintln!("murmur is already running. `murmur ctl quit` stops it.");
        return ExitCode::SUCCESS;
    }
    let settings = Arc::new(RwLock::new(settings::Settings::load()));
    let shortcut = settings.read().map(|s| s.shortcut).unwrap_or_default();
    // One inbox for the dictation: the shortcut, the pill's stop button and `murmur ctl` all send to it.
    let (dictation, inputs) = mpsc::channel();
    let pill = pill::Pill::spawn(dictation.clone());
    let status =
        dictation::spawn(dictation.clone(), inputs, pill, transcriber::Transcriber::from_env(), settings.clone());
    if let Err(error) = ctl::serve(dictation.clone(), status.clone()) {
        eprintln!("murmur: `murmur ctl` won't work this time: {error}");
    }
    match hotkey::spawn(dictation, shortcut) {
        Ok(()) => eprintln!("murmur is running. Hold {shortcut} to talk; `murmur ctl quit` stops it."),
        Err(reason) => eprintln!("murmur: {reason} `murmur ctl start` still works."),
    }

    let first_run = onboarded_marker().filter(|marker| !skip_onboarding && !marker.exists());
    // The tray owns the main thread until "Quit murmur".
    match tray::run(status, settings, first_run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("murmur: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Exists once onboarding has been finished, so it only opens the first time.
fn onboarded_marker() -> Option<PathBuf> {
    dirs::data_local_dir().map(|dir| dir.join("murmur").join("onboarded"))
}
