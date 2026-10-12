mod codex_auth;
mod ctl;
mod dictation;
mod hotkey;
mod microphone;
mod onboarding;
mod paste;
mod pill;
mod shared;
mod transcriber;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

/// Native dictation that types with your Codex plan.
#[derive(Parser)]
#[command(name = "murmur", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run murmur: hold Ctrl+Win anywhere to dictate. The default.
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
        Command::Onboarding => match onboarding::run(None) {
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

/// The app: the pill, the dictation thread, the shortcut and `murmur ctl`.
/// Onboarding opens the first time. Then the main thread just waits; `murmur ctl quit` ends it.
fn run(skip_onboarding: bool) -> ExitCode {
    if ctl::is_running() {
        eprintln!("murmur is already running. `murmur ctl quit` stops it.");
        return ExitCode::SUCCESS;
    }
    let transcriber = transcriber::Transcriber::from_env();
    let (dictation, status) = dictation::spawn(pill::Pill::spawn(), transcriber);
    if let Err(error) = ctl::serve(dictation.clone(), status.clone()) {
        eprintln!("murmur: `murmur ctl` won't work this time: {error}");
    }
    match hotkey::spawn(dictation) {
        Ok(()) => eprintln!("murmur is running. Hold {} to talk; `murmur ctl quit` stops it.", hotkey::SHORTCUT),
        Err(reason) => {
            eprintln!(
                "murmur: the {} shortcut isn't available ({reason}). `murmur ctl start` still works.",
                hotkey::SHORTCUT
            )
        }
    }

    let marker = onboarded_marker();
    if !skip_onboarding && marker.as_ref().is_some_and(|m| !m.exists()) {
        if let Err(error) = onboarding::run(Some(status)) {
            eprintln!("murmur: onboarding didn't open: {error}");
        } else if let Some(marker) = &marker {
            let _ = marker.parent().map(std::fs::create_dir_all);
            let _ = std::fs::write(marker, "");
        }
    }
    loop {
        std::thread::park();
    }
}

/// Exists once onboarding has been finished, so it only opens the first time.
fn onboarded_marker() -> Option<PathBuf> {
    dirs::data_local_dir().map(|dir| dir.join("murmur").join("onboarded"))
}
