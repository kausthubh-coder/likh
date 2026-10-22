//! Voice editing: select text, hold the shortcut, say what to change ("make it
//! shorter", "more formal", "in Hindi", "translate this to French", "fix the
//! grammar"), let go. ChatGPT rewrites the selection and likh pastes the
//! result over it. Translation is one more instruction.
//!
//! It's the ordinary shortcut, not a key of its own, so it has to tell an
//! instruction from words meant to be typed. Only a short sentence that starts
//! like an edit counts (`instruction`), and only then is the selection read,
//! with a Ctrl+C. With nothing selected the words are pasted as said, so
//! dictating "Make it shorter." into an empty box still types it.
//!
//! In a terminal on Windows a Ctrl+C with nothing selected stops the program
//! running there, so likh never reads the selection in one (`is_terminal`).
//!
//! It is one ChatGPT model call, so on ChatGPT Free it stops once the plan's
//! allowance runs out (`allowance/`) and says so; the selection stays as it was.
//! `LIKH_ASK=<text>` answers it, as it does an action's `ask`.

use crate::transcriber::language;

#[cfg(test)]
mod tests;

/// Longer than this, it's a sentence to type, not an instruction.
const MOST_WORDS: usize = 12;

/// How a sentence that's an instruction starts, as bare lowercase words. Verbs
/// and "make it", never a bare adjective: "Longer meetings mean less focus." is a sentence.
const STARTS: [&str; 31] = [
    "make it",
    "make this",
    "make that",
    "rewrite",
    "rephrase",
    "reword",
    "translate",
    "turn it into",
    "turn this into",
    "fix the grammar",
    "fix grammar",
    "fix the spelling",
    "fix spelling",
    "fix the typos",
    "fix typos",
    "fix it",
    "fix this",
    "proofread",
    "shorten",
    "summarize",
    "summarise",
    "simplify",
    "expand",
    "more formal",
    "more casual",
    "more friendly",
    "less formal",
    "as bullet points",
    "as a list",
    "change it to",
    "change this to",
];

const ROLE: &str = "You edit text the user selected in another app, following their spoken instruction. \
Return only the edited text, ready to replace the selection: no quotes, no preamble, no explanation. \
Keep its meaning, and keep its language unless the instruction asks for another one. Keep line breaks \
and lists unless the instruction changes them.";

/// How long an edit may take. A long selection's answer is long too.
fn patience(selection: &str) -> std::time::Duration {
    std::time::Duration::from_secs(8) + std::time::Duration::from_millis(4 * selection.chars().count() as u64)
}

/// The instruction in what was said, if it is one: short, and starting the
/// way an edit does, or "in <language>" alone. Trailing punctuation goes.
pub fn instruction(said: &str) -> Option<String> {
    let bare: Vec<String> = said
        .split_whitespace()
        .map(|word| word.chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect::<String>().to_lowercase())
        .filter(|word| !word.is_empty())
        .collect();
    if bare.is_empty() || bare.len() > MOST_WORDS {
        return None;
    }
    let joined = format!("{} ", bare.join(" "));
    let starts = STARTS.iter().any(|start| joined.starts_with(&format!("{start} ")));
    // "In Hindi", "in Spanish please": a language on its own is a translation.
    let polite = |rest: &[String]| rest.is_empty() || rest == ["please"];
    let in_language = bare.len() >= 2
        && bare[0] == "in"
        && polite(&bare[2..])
        && language::ALL.iter().any(|l| l.name.eq_ignore_ascii_case(&bare[1]));
    (starts || in_language).then(|| said.trim().trim_end_matches(['.', '!', '?', ',']).trim().to_owned())
}

/// Programs where Ctrl+C with nothing selected stops what's running, by file name.
pub fn is_terminal(app: &str) -> bool {
    const TERMINALS: [&str; 12] = [
        "windowsterminal.exe",
        "wt.exe",
        "cmd.exe",
        "powershell.exe",
        "pwsh.exe",
        "conhost.exe",
        "openconsole.exe",
        "wezterm-gui.exe",
        "alacritty.exe",
        "mintty.exe",
        "tabby.exe",
        "hyper.exe",
    ];
    TERMINALS.contains(&app.to_ascii_lowercase().as_str())
}

/// What goes to ChatGPT besides the role: the instruction, then the selection.
pub fn request(instruction: &str, selection: &str) -> String {
    format!("Instruction: {instruction}\n\nSelected text:\n{selection}")
}

/// The selection as the instruction wants it, from ChatGPT.
pub fn edit(instruction: &str, selection: &str) -> Result<String, String> {
    let input = request(instruction, selection);
    if let Some(fake) = std::env::var_os("LIKH_ASK") {
        return Ok(fake.to_string_lossy().replace("{prompt}", &input));
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let credentials = crate::codex_auth::codex_home()
        .and_then(|home| crate::codex_auth::credentials(&home, now).ok())
        .ok_or("Editing needs Codex signed in.")?;
    let span = crate::diagnostics::Span::start("edit").with("characters", selection.chars().count());
    let answer = crate::cleanup::ask(&credentials, crate::cleanup::body(ROLE, &input, None), patience(selection))
        .and_then(|stream| crate::cleanup::read_answer(&stream).ok_or_else(|| "ChatGPT sent back nothing.".into()));
    span.end(answer.as_ref().map(|_| ()).map_err(Clone::clone));
    answer
}
