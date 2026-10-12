//! `murmur ctl`: drive the running app from a terminal or an agent, without
//! pressing keys. One JSON line in each direction over a local socket (a
//! named pipe on Windows), private to the signed-in user's name.
//!
//! `murmur ctl start` / `stop` are the shortcut going down and up, `cancel`
//! is Esc, `status` reports what murmur is doing, `quit` stops it.

use crate::dictation::{Input, Status};
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Name, Stream};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

#[cfg(test)]
mod tests;

pub const COMMANDS: [&str; 5] = ["status", "start", "stop", "cancel", "quit"];

fn name() -> std::io::Result<Name<'static>> {
    let user = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default();
    format!("murmur-{user}.sock").to_ns_name::<GenericNamespaced>()
}

/// Sends one command to the running app and returns its JSON answer.
/// `Err` means nothing is listening: murmur isn't running.
pub fn send(command: &str) -> std::io::Result<Value> {
    let mut connection = BufReader::new(Stream::connect(name()?)?);
    connection.get_mut().write_all(format!("{command}\n").as_bytes())?;
    let mut line = String::new();
    connection.read_line(&mut line)?;
    serde_json::from_str(&line).map_err(std::io::Error::other)
}

pub fn is_running() -> bool {
    send("status").is_ok()
}

/// Answers commands on a background thread for as long as the app runs.
pub fn serve(dictation: Sender<Input>, status: Arc<Mutex<Status>>) -> std::io::Result<()> {
    let listener = ListenerOptions::new().name(name()?).create_sync()?;
    std::thread::Builder::new().name("murmur-ctl".into()).spawn(move || {
        for connection in listener.incoming().filter_map(Result::ok) {
            let mut connection = BufReader::new(connection);
            let mut line = String::new();
            if connection.read_line(&mut line).is_err() {
                continue;
            }
            let snapshot = status.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
            let (answer, input) = answer(line.trim(), &snapshot);
            let quitting = line.trim() == "quit";
            if let Some(input) = input {
                let _ = dictation.send(input);
            }
            let _ = connection.get_mut().write_all(format!("{answer}\n").as_bytes());
            drop(connection);
            if quitting {
                std::process::exit(0);
            }
        }
    })?;
    Ok(())
}

/// What to reply to a command and what, if anything, to tell the dictation.
pub fn answer(command: &str, status: &Status) -> (Value, Option<Input>) {
    let ok = json!({ "ok": true });
    match command {
        "status" => (
            json!({
                "ok": true,
                "version": env!("CARGO_PKG_VERSION"),
                "dictation": status,
                "codex": crate::onboarding::check_codex_now(),
            }),
            None,
        ),
        "start" => (ok, Some(Input::Hold)),
        "stop" => (ok, Some(Input::Stop)),
        "cancel" => (ok, Some(Input::Cancel)),
        "quit" => (ok, None),
        other => (
            json!({ "ok": false, "error": format!("unknown command {other:?}; try one of {}", COMMANDS.join(", ")) }),
            None,
        ),
    }
}
