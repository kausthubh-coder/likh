//! Carrying out an action's steps, one after another. Each step can use the
//! slots (`{text}`, `{selection}`, the result of the step before as
//! `{result}`) and leaves a result for the next: `ask` and `run` replace it
//! with their answer, the others pass it on.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Keys a `keys` step can name, besides letters, digits and F1 to F12.
pub const NAMED_KEYS: [&str; 20] = [
    "enter",
    "tab",
    "esc",
    "space",
    "backspace",
    "delete",
    "up",
    "down",
    "left",
    "right",
    "home",
    "end",
    "pageup",
    "pagedown",
    "play_pause",
    "next_track",
    "previous_track",
    "volume_up",
    "volume_down",
    "mute",
];

/// One key press with its modifiers: `ctrl+shift+t`, `enter`, `play_pause`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    /// Lowercase: a letter, a digit, `f1`..`f12` or one of `NAMED_KEYS`.
    pub key: String,
}

impl Chord {
    pub fn name(&self) -> String {
        let mut parts: Vec<&str> = [(self.ctrl, "ctrl"), (self.alt, "alt"), (self.shift, "shift"), (self.win, "win")]
            .into_iter()
            .filter_map(|(on, name)| on.then_some(name))
            .collect();
        parts.push(&self.key);
        parts.join("+")
    }
}

fn is_key(name: &str) -> bool {
    let single = name.chars().count() == 1 && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let function = name.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| (1..=12).contains(&n));
    single || function || NAMED_KEYS.contains(&name)
}

/// `ctrl+shift+t enter`: chords split by spaces, keys in a chord by `+`.
pub fn parse_keys(text: &str) -> Result<Vec<Chord>, String> {
    let mut chords = Vec::new();
    for chord in text.split_whitespace() {
        let mut parsed = Chord { ctrl: false, alt: false, shift: false, win: false, key: String::new() };
        for part in chord.split('+').map(|part| part.trim().to_lowercase()) {
            match part.as_str() {
                "ctrl" | "control" => parsed.ctrl = true,
                "alt" => parsed.alt = true,
                "shift" => parsed.shift = true,
                "win" => parsed.win = true,
                "escape" => parsed.key = "esc".into(),
                "return" => parsed.key = "enter".into(),
                key if is_key(key) && parsed.key.is_empty() => parsed.key = key.to_owned(),
                key if is_key(key) => return Err(format!("\"{chord}\" has two keys; press them as two chords")),
                other => return Err(format!("\"{other}\" isn't a key likh knows")),
            }
        }
        if parsed.key.is_empty() {
            return Err(format!("\"{chord}\" is only modifiers; add the key they go with"));
        }
        chords.push(parsed);
    }
    if chords.is_empty() {
        return Err("`keys` is empty".into());
    }
    Ok(chords)
}

/// The slot names in a template. `{{` and `}}` stand for braces.
pub fn placeholders(template: &str) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
            }
            '{' => {
                let mut name = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' => name.push(c),
                        _ => return Err("a { without its } (write {{ for a brace)".into()),
                    }
                }
                if name.is_empty() {
                    return Err("{} names no slot".into());
                }
                found.push(name);
            }
            '}' => return Err("a } without its { (write }} for a brace)".into()),
            _ => {}
        }
    }
    Ok(found)
}

/// The template with each slot's value in. In an `open` step, `encode` is
/// true: what you said and read is URL-encoded, but a list's own value (a
/// link you wrote in the file) goes in as written.
pub fn fill(template: &str, values: &BTreeMap<String, String>, encode: bool, raw: &[String]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push('}');
            }
            '{' => {
                let name: String = chars.by_ref().take_while(|c| *c != '}').collect();
                let value = values.get(&name).map(String::as_str).unwrap_or_default();
                if encode && !raw.contains(&name) {
                    out.push_str(&percent_encode(value));
                } else {
                    out.push_str(value);
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// Everything but letters, digits and `-_.~` as `%XX` bytes (RFC 3986).
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// What an action has to work with: what you said, its slots, what it read,
/// and the last step's result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Values {
    pub values: BTreeMap<String, String>,
    /// List slots: their values are links from the file, never URL-encoded.
    pub raw: Vec<String>,
    /// The slot that caught the rest of what you said, if the phrase had one.
    pub free: Option<String>,
}

impl Values {
    pub fn result(&self) -> &str {
        self.values.get("result").map(String::as_str).unwrap_or_default()
    }

    /// `LIKH_SAID`, `LIKH_TEXT` (the slot that caught the rest), `LIKH_SLOT_<NAME>` for each slot,
    /// and `LIKH_SELECTION`, `LIKH_CLIPBOARD`, `LIKH_APP`, `LIKH_TITLE`, `LIKH_RESULT` when there are any.
    pub fn environment(&self) -> Vec<(String, String)> {
        let mut env = Vec::new();
        for (name, value) in &self.values {
            let key = match name.as_str() {
                "said" | "selection" | "clipboard" | "app" | "title" | "result" => {
                    format!("LIKH_{}", name.to_uppercase())
                }
                slot => format!("LIKH_SLOT_{}", slot.to_uppercase()),
            };
            env.push((key, value.clone()));
        }
        let text = self.free.as_ref().and_then(|slot| self.values.get(slot)).cloned().unwrap_or_default();
        env.push(("LIKH_TEXT".into(), text));
        env
    }
}

/// Stops a running step when Esc is pressed.
pub type Cancel = Arc<AtomicBool>;

/// Runs one step. Ok(Some) is a new result; Ok(None) leaves the last one.
pub fn run(
    step: &super::Step,
    values: &Values,
    dir: &Path,
    timeout: Duration,
    cancel: &Cancel,
) -> Result<Option<String>, String> {
    use super::Step;
    match step {
        Step::Paste(template) => {
            let text = fill(template, &values.values, false, &values.raw);
            crate::paste::paste(&text)?.finish();
            Ok(None)
        }
        Step::Keys(chords) => {
            for chord in chords {
                crate::paste::chord(chord)?;
            }
            Ok(None)
        }
        Step::Open(template) => {
            let target = fill(template, &values.values, true, &values.raw);
            crate::paste::open(&target)?;
            Ok(None)
        }
        Step::Wait(time) => {
            let until = Instant::now() + *time;
            while Instant::now() < until {
                if cancel.load(Ordering::SeqCst) {
                    return Err("Stopped".into());
                }
                std::thread::sleep(Duration::from_millis(20).min(until - Instant::now()));
            }
            Ok(None)
        }
        Step::Ask(template) => ask(&fill(template, &values.values, false, &values.raw)).map(Some),
        Step::Run(argv) => program(argv, values, dir, timeout, cancel).map(Some),
    }
}

const ASK_ROLE: &str = "You carry out one small task for a voice command in a desktop app. The task is below. \
Return only the text it asks for, ready to paste: no preamble, no quotes around it, no explanation. Keep the \
language of any text you are given unless the task says otherwise.";

/// How long an `ask` may take: longer than cleanup's pass, since this is the action itself.
const ASK_PATIENCE: Duration = Duration::from_secs(20);

/// One ChatGPT answer through Codex's sign-in, as text.
pub fn ask(prompt: &str) -> Result<String, String> {
    if let Some(fake) = std::env::var_os("LIKH_ASK") {
        // Tests and agents: a fixed answer, with the prompt in it so they can check what was asked.
        return Ok(fake.to_string_lossy().replace("{prompt}", prompt));
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let credentials = crate::codex_auth::codex_home()
        .and_then(|home| crate::codex_auth::credentials(&home, now).ok())
        .ok_or("Asking ChatGPT needs Codex signed in.")?;
    let body = crate::cleanup::body(ASK_ROLE, prompt, None);
    let stream = crate::cleanup::ask(&credentials, body, ASK_PATIENCE)?;
    crate::cleanup::read_answer(&stream).ok_or_else(|| "ChatGPT sent back nothing.".to_string())
}

/// Runs a program from the actions folder and returns what it printed.
/// Never through a shell. It gets what the action knows as environment
/// variables and as one JSON object on stdin, and is stopped by its own
/// handle (its PID) at the timeout or on Esc.
pub fn program(
    argv: &[String],
    values: &Values,
    dir: &Path,
    timeout: Duration,
    cancel: &Cancel,
) -> Result<String, String> {
    let (program, args) = argv.split_first().ok_or("`run` has no program")?;
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(dir)
        .envs(values.environment())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashes up over what you're doing.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|error| format!("couldn't start {program}: {error}"))?;
    let input = serde_json::to_string(&values.values).unwrap_or_default();
    if let Some(mut stdin) = child.stdin.take() {
        std::thread::spawn(move || {
            let _ = stdin.write_all(input.as_bytes());
        });
    }
    let reader = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_string(&mut text);
            }
            text
        })
    };
    let stdout = reader(child.stdout.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>));
    let stderr = reader(child.stderr.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        let stop = if cancel.load(Ordering::SeqCst) {
            Some("Stopped".to_string())
        } else if started.elapsed() >= timeout {
            Some(format!("{program} took longer than {} s, so likh stopped it", timeout.as_secs()))
        } else {
            None
        };
        if let Some(why) = stop {
            let _ = child.kill();
            let _ = child.wait();
            return Err(why);
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let out = stdout.join().unwrap_or_default();
    let err = stderr.join().unwrap_or_default();
    if status.success() {
        Ok(out.trim().to_owned())
    } else {
        let last = err.lines().rev().find(|line| !line.trim().is_empty()).map(str::trim);
        Err(match (last, status.code()) {
            (Some(line), _) => line.to_owned(),
            (None, Some(code)) => format!("{program} stopped with code {code}"),
            (None, None) => format!("{program} stopped"),
        })
    }
}
