mod codex_auth;
mod onboarding;
mod shared;

use clap::{Parser, Subcommand};
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
    /// Open the first-run window (the default until the tray app lands).
    Onboarding,
    /// Print the Codex sign-in state. Agents: use --json.
    Status {
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command.unwrap_or(Command::Onboarding) {
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
    }
}
