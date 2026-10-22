//! Voice editing: select text, hold the shortcut, say what to change ("make it
//! shorter", "more formal", "in Hindi", "translate this to French", "fix the
//! grammar"), let go. ChatGPT rewrites the selection and likh pastes the
//! result over it. Translation is one more instruction.
//!
//! It's the ordinary shortcut, not a key of its own, so it has to tell an
//! instruction from words meant to be typed. Only a short sentence shaped like
//! an edit counts (`instruction`), and only then is the selection read, with a
//! Ctrl+C. With nothing selected the words are pasted as said, so
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

/// Words that point at the selection.
const IT: [&str; 3] = ["it", "this", "that"];
/// What "make it" can become: "make it shorter", "make this sound friendlier".
const BECOMES: [&str; 22] = [
    "shorter",
    "longer",
    "more",
    "less",
    "formal",
    "casual",
    "friendly",
    "friendlier",
    "professional",
    "polite",
    "politer",
    "concise",
    "clear",
    "clearer",
    "simple",
    "simpler",
    "punchier",
    "warmer",
    "nicer",
    "better",
    "sound",
    "bullet",
];
/// A tone after "more" or "less": "more formal", "less casual".
const TONES: [&str; 12] = [
    "formal",
    "casual",
    "friendly",
    "professional",
    "polite",
    "concise",
    "direct",
    "confident",
    "detailed",
    "personal",
    "specific",
    "clear",
];
/// Verbs whose object is the selection, said as "it", "this" or "that": "rewrite it as …".
const VERBS: [&str; 13] = [
    "rewrite",
    "rephrase",
    "reword",
    "shorten",
    "summarize",
    "summarise",
    "simplify",
    "expand",
    "proofread",
    "translate",
    "polish",
    "tighten",
    "clean",
];
/// "turn it into …", "change it to …": verbs that need their object and a destination.
const INTO: [&str; 3] = ["turn", "change", "convert"];
/// Said alone, these are instructions: "Shorter.", "Proofread.", "Bullet points."
const ALONE: [&str; 9] =
    ["shorter", "longer", "simpler", "friendlier", "proofread", "summarize", "summarise", "bullets", "translate"];

const ROLE: &str = "You edit text the user selected in another app, following their spoken instruction. \
Return only the edited text, ready to replace the selection: no quotes, no preamble, no explanation. \
Keep its meaning, and keep its language unless the instruction asks for another one. Keep line breaks \
and lists unless the instruction changes them.";

/// How long an edit may take. A long selection's answer is long too.
fn patience(selection: &str) -> std::time::Duration {
    std::time::Duration::from_secs(8) + std::time::Duration::from_millis(4 * selection.chars().count() as u64)
}

/// The instruction in what was said, if it is one. Only the shape of an edit
/// counts, never just its first word: "make it shorter" is one, "Make it happen
/// by Friday." and "Expand the team next quarter." are sentences to type over
/// the selection (Devin, 2026-10-22). At most 12 words. Trailing punctuation goes.
pub fn instruction(said: &str) -> Option<String> {
    let words: Vec<String> = said
        .split_whitespace()
        .map(|word| word.chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect::<String>().to_lowercase())
        .filter(|word| !word.is_empty())
        .collect();
    let w: Vec<&str> = words.iter().map(String::as_str).collect();
    if w.is_empty() || w.len() > MOST_WORDS {
        return None;
    }
    let it = |i: usize| w.get(i).is_some_and(|word| IT.contains(word));
    let is_language =
        |i: usize| w.get(i).is_some_and(|word| language::ALL.iter().any(|l| l.name.eq_ignore_ascii_case(word)));
    // Nothing after, or only "please".
    let ends_at = |i: usize| w.len() == i || (w.len() == i + 1 && w[i] == "please");
    let edit = match w.as_slice() {
        // "make it shorter", "make this sound more professional", "make it bullet points"
        ["make", _, next, ..] => it(1) && BECOMES.contains(next),
        // "more formal", "less casual please"
        ["more" | "less", tone, ..] => TONES.contains(tone) && ends_at(2),
        // "in Hindi", "in Spanish please"
        ["in", ..] => is_language(1) && ends_at(2),
        // "translate to French", "translate this into Hindi"
        ["translate", "to" | "into", ..] => is_language(2) && ends_at(3),
        ["translate", _, "to" | "into", ..] => it(1) && is_language(3) && ends_at(4),
        // "fix the grammar", "fix spelling", "fix the typos"
        ["fix", "the", "grammar" | "spelling" | "typos" | "punctuation", ..]
        | ["fix", "grammar" | "spelling" | "typos" | "punctuation", ..] => true,
        // "turn this into a list", "change it to past tense"
        [verb, _, "into" | "to", ..] if INTO.contains(verb) => it(1),
        // "expand on this"
        ["expand", "on", ..] => it(2),
        // "rewrite it as bullet points", "summarize this in two sentences", "clean it up"
        [verb, ..] if VERBS.contains(verb) => it(1),
        // "as bullet points", "as a list"
        ["as", "bullet", "points" | "point"] | ["as", "a", "list" | "bulleted" | "numbered", ..] => true,
        ["bullet", "points"] => true,
        [word] | [word, "please"] => ALONE.contains(word),
        _ => false,
    };
    edit.then(|| said.trim().trim_end_matches(['.', '!', '?', ',']).trim().to_owned())
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

/// What VS Code, Cursor, JetBrains and Sublime copy on Ctrl+C with nothing
/// selected: the line the cursor is on, with its line break. A selection
/// rarely ends that way, so in a code editor it means nothing was selected.
pub fn is_a_copied_line(copied: &str) -> bool {
    copied.ends_with('\n') && copied.trim_end_matches(['\r', '\n']).lines().count() <= 1
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
