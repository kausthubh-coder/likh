//! `likh ctl`: drive the running app from a terminal or an agent, without
//! pressing keys. One JSON line in each direction over a local socket (a
//! named pipe on Windows), private to the signed-in user's name.
//!
//! `likh ctl start` / `stop` are the shortcut going down and up,
//! `action-start` / `action-stop` the same for the action key, `cancel` is Esc,
//! `retry` sends the last failed dictation again (the pill's Retry), `status` reports what likh is doing (and where updating stands),
//! `update` checks for a newer likh now, `install-update` is "Restart to
//! update", `diagnostics` sums up the trace (what failed, what was slow) with
//! likh's memory, `quit` stops it.

use crate::diagnostics::{self, Trace};
use crate::dictation::{Input, Status};
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Name, Stream};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

#[cfg(test)]
mod tests;

pub const COMMANDS: [&str; 14] = [
    "status",
    "start",
    "stop",
    "action-start",
    "action-stop",
    "cancel",
    "paste-last",
    "retry",
    "copy-last",
    "quit",
    "open",
    "update",
    "install-update",
    "diagnostics",
];

fn name() -> std::io::Result<Name<'static>> {
    let user = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default();
    // A test instance with its own `LIKH_HOME` gets its own pipe, named after that folder.
    let instance = std::env::var_os("LIKH_HOME")
        .and_then(|home| PathBuf::from(home).file_name().map(|name| format!("-{}", name.to_string_lossy())))
        .unwrap_or_default();
    format!("likh-{user}{instance}.sock").to_ns_name::<GenericNamespaced>()
}

/// Sends one command to the running app and returns its JSON answer.
/// `Err` means nothing is listening: likh isn't running.
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
pub fn serve(
    dictation: Sender<Input>,
    status: Arc<Mutex<Status>>,
    ui: iced::futures::channel::mpsc::UnboundedSender<crate::tray::Pick>,
    updates: crate::update::Updates,
) -> std::io::Result<()> {
    // On macOS the name is a socket file, and quitting leaves it behind: the
    // next launch failed with "Address already in use" and `likh ctl` never
    // answered (found 2026-10-22). `run` hands off to a likh that's still
    // running before it gets here, so a file left now is stale.
    let listener = ListenerOptions::new().name(name()?).try_overwrite(true).create_sync()?;
    std::thread::Builder::new().name("likh-ctl".into()).spawn(move || {
        for connection in listener.incoming().filter_map(Result::ok) {
            let mut connection = BufReader::new(connection);
            let mut line = String::new();
            if connection.read_line(&mut line).is_err() {
                continue;
            }
            let snapshot = status.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
            if line.trim() == "update" {
                updates.check_now();
            }
            let (answer, input) = answer(line.trim(), &snapshot, &updates.state());
            // These two go to the tray's thread, like the menu lines they stand in for.
            let to_tray = match line.trim() {
                "open" => Some(crate::tray::Pick::Open),
                "copy-last" => Some(crate::tray::Pick::CopyLast),
                _ => None,
            };
            let answer = if to_tray.is_some_and(|pick| ui.unbounded_send(pick).is_err()) {
                json!({ "ok": false, "error": "likh's window isn't ready. Try again." })
            } else {
                answer
            };
            let answer = match line.trim() {
                // Answer first: on success likh quits for the installer and can't reply after.
                "install-update" if matches!(updates.state(), crate::update::State::Ready { .. }) => {
                    let _ = connection.get_mut().write_all(format!("{}\n", json!({ "ok": true })).as_bytes());
                    let failed = updates.apply().err().unwrap_or_default();
                    json!({ "ok": false, "error": failed })
                }
                "install-update" => {
                    json!({ "ok": false, "error": "No update is ready yet. Run `likh ctl update` first." })
                }
                "diagnostics" => {
                    // CPU is measured between two samples.
                    let mut sampler = diagnostics::Sampler::default();
                    let _ = sampler.sample();
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    json!({
                        "ok": true,
                        "logs": diagnostics::folder(),
                        "process": sampler.sample(),
                        "report": diagnostics::folder().map(|logs| Trace::new(logs).read()),
                    })
                }
                _ => answer,
            };
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
/// `update` is where updating stands (it has already been asked to check).
pub fn answer(command: &str, status: &Status, update: &crate::update::State) -> (Value, Option<Input>) {
    let ok = json!({ "ok": true });
    match command {
        "status" => (
            json!({
                "ok": true,
                "version": env!("CARGO_PKG_VERSION"),
                "dictation": status,
                "codex": crate::onboarding::check_codex_now(),
                "update": update,
            }),
            None,
        ),
        "start" => (ok, Some(Input::Hold)),
        "stop" => (ok, Some(Input::Stop)),
        "action-start" => (ok, Some(Input::ActionHold)),
        // Ends the recording either way; the action key's own release does the same.
        "action-stop" => (ok, Some(Input::Stop)),
        "cancel" => (ok, Some(Input::Cancel)),
        "retry" => (ok, Some(Input::Retry)),
        "paste-last" => (ok, Some(Input::PasteLast)),
        "quit" | "open" | "copy-last" | "install-update" | "diagnostics" => (ok, None),
        "update" => (json!({ "ok": true, "update": update }), None),
        other => (
            json!({ "ok": false, "error": format!("unknown command {other:?}; try one of {}", COMMANDS.join(", ")) }),
            None,
        ),
    }
}
