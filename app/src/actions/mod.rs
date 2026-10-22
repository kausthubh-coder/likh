//! Custom actions: hold the action key (Ctrl+Shift+Space unless changed), say
//! what to do, and likh does it. Each action is one TOML file in
//! `LIKH_HOME/actions/`, so people and agents write them the same way, and the
//! folder is read again on every press: no watcher, nothing running while idle.
//!
//! - `mod.rs`: the file format, loading and checking a folder of them.
//! - `matcher.rs`: what you said against each action's phrases, locally.
//! - `router.rs`: ChatGPT picks an action when no phrase matched.
//! - `steps.rs`: carrying out an action's steps (paste, keys, open, wait, ask, run).
//! - `trust.rs`: a `run` step only runs once you approved its files.
//! - `runner.rs`: one action at a time, from what was heard to its result.
//!
//! `starters/` holds the README, the schema and the actions likh writes into a
//! new folder; `likh actions check` and `likh actions test` use this module
//! without a running likh.

pub mod matcher;
pub mod router;
pub mod runner;
pub mod steps;
pub mod trust;

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(test)]
mod tests;

/// How long a `run` step may take unless its file says otherwise.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// No file may ask for more: an action is a moment, not a background job.
pub const LONGEST_TIMEOUT: Duration = Duration::from_secs(600);

pub const README: &str = include_str!("starters/README.md");
pub const SCHEMA: &str = include_str!("starters/actions.schema.json");
/// What a new actions folder starts with: file name, contents.
/// Ones anyone can use, and examples (ignored until renamed) for tools not everyone has.
pub const STARTERS: [(&str, &str); 5] = [
    ("make-this.toml", include_str!("starters/make-this.toml")),
    ("search-the-web.toml", include_str!("starters/search-the-web.toml")),
    ("open-site.toml", include_str!("starters/open-site.toml")),
    ("new-t3-thread.example.toml", include_str!("starters/new-t3-thread.example.toml")),
    ("run-tests.example.toml", include_str!("starters/run-tests.example.toml")),
];

/// What an action reads besides what you said. Each costs something (the
/// selection takes a Ctrl+C), so only actions that ask for it get it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Context {
    Selection,
    Clipboard,
    /// The app in front, by its program name (`chrome.exe`).
    App,
    /// The window in front's title.
    Title,
}

impl Context {
    pub fn slot(self) -> &'static str {
        match self {
            Context::Selection => "selection",
            Context::Clipboard => "clipboard",
            Context::App => "app",
            Context::Title => "title",
        }
    }
}

/// What happens to the last step's result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Output {
    /// Pasted at the cursor, like a dictation.
    Paste,
    /// Pasted over the selection the action read.
    Replace,
    /// Shown in the pill for a moment.
    #[default]
    Notify,
    None,
}

impl Output {
    pub fn name(self) -> &'static str {
        match self {
            Output::Paste => "paste",
            Output::Replace => "replace the selection",
            Output::Notify => "show in the pill",
            Output::None => "nothing",
        }
    }
}

/// One step of an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Pastes this text (slots filled in) at the cursor.
    Paste(String),
    /// Presses keys: chords split by spaces, keys in a chord by `+` (`ctrl+shift+t enter`).
    Keys(Vec<steps::Chord>),
    /// Opens a link, file or app. Slots you speak are URL-encoded.
    Open(String),
    Wait(Duration),
    /// Asks ChatGPT this prompt; its answer is the result.
    Ask(String),
    /// Runs a program. Never through a shell, never with slots in its arguments:
    /// what you said arrives in environment variables and as JSON on stdin.
    Run(Vec<String>),
}

impl Step {
    /// The step's kind and what it does, in a few words for the Actions tab.
    pub fn describe(&self) -> String {
        match self {
            Step::Paste(text) => format!("paste {}", shorten(text, 32)),
            Step::Keys(chords) => {
                format!("press {}", chords.iter().map(steps::Chord::name).collect::<Vec<_>>().join(" "))
            }
            Step::Open(target) => format!("open {}", shorten(target.split('?').next().unwrap_or(target), 32)),
            Step::Wait(time) => format!("wait {} ms", time.as_millis()),
            Step::Ask(_) => "ask ChatGPT".into(),
            Step::Run(argv) => format!("run {}", argv.join(" ")),
        }
    }
}

fn shorten(text: &str, most: usize) -> String {
    match text.char_indices().nth(most) {
        Some((end, _)) => format!("{}\u{2026}", &text[..end]),
        None => text.to_owned(),
    }
}

/// One word or slot of a phrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Word(String),
    /// Matches one name from the action's list of this name.
    List(String),
    /// Catches the rest of what was said. Only at the end of a phrase.
    Free(String),
}

/// An action, as its file describes it, checked.
#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    /// The file's name without `.toml`; trust and the Actions tab know it by this.
    pub id: String,
    pub file: PathBuf,
    pub name: String,
    pub description: String,
    /// The phrases as written, for showing.
    pub say: Vec<String>,
    pub patterns: Vec<Vec<Token>>,
    /// Each list: the spoken name (normalized words) and the value it stands for.
    pub lists: BTreeMap<String, Vec<(Vec<String>, String)>>,
    pub input: Vec<Context>,
    pub output: Output,
    pub steps: Vec<Step>,
    pub confirm: bool,
    pub router: bool,
    pub enabled: bool,
    pub timeout: Duration,
}

impl Action {
    pub fn runs_programs(&self) -> bool {
        self.steps.iter().any(|step| matches!(step, Step::Run(_)))
    }

    /// The files a `run` step uses besides the action's own: arguments that
    /// name a file in the actions folder. Trust covers them too.
    pub fn script_files(&self) -> Vec<PathBuf> {
        let Some(dir) = self.file.parent() else { return Vec::new() };
        let mut files = Vec::new();
        for step in &self.steps {
            if let Step::Run(argv) = step {
                for arg in argv {
                    let candidate = dir.join(arg);
                    if !arg.contains("..") && candidate.is_file() && !files.contains(&candidate) {
                        files.push(candidate);
                    }
                }
            }
        }
        files
    }
}

/// Something wrong with one file, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub file: String,
    pub line: Option<usize>,
    pub message: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "{}:{line} {}", self.file, self.message),
            None => write!(f, "{} {}", self.file, self.message),
        }
    }
}

/// Every action in a folder, and every file that couldn't be one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    pub actions: Vec<Action>,
    pub problems: Vec<Problem>,
}

impl Catalog {
    /// Reads every `*.toml` in `dir` (not `*.example.toml`), in name order.
    /// Two actions with the same phrase are an error on the later file, never a silent first-wins.
    pub fn load(dir: &Path) -> Self {
        let mut catalog = Catalog::default();
        let Ok(entries) = std::fs::read_dir(dir) else { return catalog };
        let mut files: Vec<PathBuf> =
            entries.filter_map(Result::ok).map(|entry| entry.path()).filter(|path| is_action_file(path)).collect();
        files.sort();
        for file in files {
            match std::fs::read_to_string(&file) {
                Ok(source) => match parse(&file, &source) {
                    Ok(action) => catalog.add(action, &source),
                    Err(problem) => catalog.problems.push(problem),
                },
                Err(error) => catalog.problems.push(Problem {
                    file: file_name(&file),
                    line: None,
                    message: format!("can't be read: {error}"),
                }),
            }
        }
        catalog
    }

    fn add(&mut self, action: Action, source: &str) {
        for (index, pattern) in action.patterns.iter().enumerate() {
            let shape = matcher::shape(pattern);
            let clash = self.actions.iter().find(|other| {
                other.enabled && action.enabled && other.patterns.iter().any(|theirs| matcher::shape(theirs) == shape)
            });
            if let Some(other) = clash {
                self.problems.push(Problem {
                    file: file_name(&action.file),
                    line: line_of(source, &action.say[index]),
                    message: format!("\"{}\" is already a phrase of {}", action.say[index], file_name(&other.file)),
                });
                return;
            }
        }
        self.actions.push(action);
    }

    pub fn find(&self, id: &str) -> Option<&Action> {
        self.actions.iter().find(|action| action.id == id)
    }
}

/// The actions folder: `LIKH_HOME/actions`, or `%LOCALAPPDATA%\likh\actions`.
pub fn folder() -> Option<PathBuf> {
    crate::settings::folder().map(|dir| dir.join("actions"))
}

fn is_action_file(path: &Path) -> bool {
    let name = file_name(path);
    path.is_file() && name.ends_with(".toml") && !name.ends_with(".example.toml")
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Makes the folder the first time, with the starters in it, and keeps the
/// README and schema (likh's own files) up to date. Never touches a file you made or changed.
pub fn prepare(dir: &Path) -> Result<(), String> {
    let new = !dir.exists();
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    for (name, contents) in [("README.md", README), ("actions.schema.json", SCHEMA)] {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(contents) {
            std::fs::write(&path, contents).map_err(|error| format!("can't write {name}: {error}"))?;
        }
    }
    if new {
        for (name, contents) in STARTERS {
            std::fs::write(dir.join(name), contents).map_err(|error| format!("can't write {name}: {error}"))?;
        }
    }
    Ok(())
}

/// The file as written. Unknown keys are errors, so a typo never silently does nothing.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    name: String,
    #[serde(default)]
    description: Option<String>,
    say: Vec<String>,
    #[serde(default)]
    lists: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default)]
    input: Vec<Context>,
    #[serde(default)]
    output: Output,
    #[serde(rename = "do")]
    steps: Vec<StepFile>,
    #[serde(default)]
    confirm: bool,
    #[serde(default = "yes")]
    router: bool,
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default)]
    timeout: Option<u64>,
}

fn yes() -> bool {
    true
}

/// One `[[do]]` table: exactly one of these keys.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StepFile {
    paste: Option<String>,
    keys: Option<String>,
    open: Option<String>,
    wait: Option<u64>,
    ask: Option<String>,
    run: Option<Vec<String>>,
}

/// The slots every action can use in its steps, besides its phrases' own.
const ALWAYS: [&str; 2] = ["said", "result"];

/// Reads one action file, or says what's wrong with it and on which line.
pub fn parse(file: &Path, source: &str) -> Result<Action, Problem> {
    let problem = |line: Option<usize>, message: String| Problem { file: file_name(file), line, message };
    let raw: File = toml::from_str(source).map_err(|error| {
        let line = error.span().map(|span| source[..span.start.min(source.len())].matches('\n').count() + 1);
        problem(line, error.message().trim().to_owned())
    })?;
    let id = file_name(file).trim_end_matches(".toml").to_owned();
    if raw.name.trim().is_empty() {
        return Err(problem(line_of(source, "name"), "has no name".into()));
    }
    if raw.say.is_empty() {
        return Err(problem(line_of(source, "say"), "needs at least one phrase in `say`".into()));
    }
    let mut lists = BTreeMap::new();
    for (list, names) in &raw.lists {
        let mut spoken = Vec::new();
        for (name, value) in names {
            let words = matcher::words(name);
            if words.is_empty() {
                return Err(problem(line_of(source, name), format!("list `{list}` has a name with no words")));
            }
            spoken.push((words, value.clone()));
        }
        lists.insert(list.clone(), spoken);
    }
    let mut patterns = Vec::new();
    let mut slots: Vec<String> = Vec::new();
    for phrase in &raw.say {
        let pattern = matcher::pattern(phrase, &lists).map_err(|why| problem(line_of(source, phrase), why))?;
        for token in &pattern {
            if let Token::List(name) | Token::Free(name) = token
                && !slots.contains(name)
            {
                slots.push(name.clone());
            }
        }
        patterns.push(pattern);
    }
    let mut known: Vec<&str> = slots.iter().map(String::as_str).collect();
    known.extend(ALWAYS);
    known.extend(raw.input.iter().map(|context| context.slot()));
    if raw.output == Output::Replace && !raw.input.contains(&Context::Selection) {
        return Err(problem(
            line_of(source, "output"),
            "replaces the selection, so `input` needs \"selection\"".into(),
        ));
    }
    if raw.steps.is_empty() {
        return Err(problem(line_of(source, "[[do]]"), "has no steps: add a [[do]] table".into()));
    }
    let mut steps = Vec::new();
    for (index, step) in raw.steps.into_iter().enumerate() {
        let line = nth_line_of(source, "[[do]]", index);
        let step = step_from(step).map_err(|why| problem(line, format!("step {}: {why}", index + 1)))?;
        let template = match &step {
            Step::Paste(text) | Step::Open(text) | Step::Ask(text) => Some(text),
            _ => None,
        };
        if let Some(template) = template {
            for slot in
                steps::placeholders(template).map_err(|why| problem(line, format!("step {}: {why}", index + 1)))?
            {
                if !known.contains(&slot.as_str()) {
                    let hint = match slot.as_str() {
                        "selection" | "clipboard" | "app" | "title" => format!(" (add \"{slot}\" to `input`)"),
                        _ => String::new(),
                    };
                    return Err(problem(line, format!("step {}: unknown slot {{{slot}}}{hint}", index + 1)));
                }
            }
        }
        steps.push(step);
    }
    let timeout = match raw.timeout {
        None => DEFAULT_TIMEOUT,
        Some(seconds) if seconds == 0 || Duration::from_secs(seconds) > LONGEST_TIMEOUT => {
            return Err(problem(
                line_of(source, "timeout"),
                format!("timeout must be 1 to {} seconds", LONGEST_TIMEOUT.as_secs()),
            ));
        }
        Some(seconds) => Duration::from_secs(seconds),
    };
    let mut input = raw.input;
    input.sort();
    input.dedup();
    Ok(Action {
        id,
        file: file.to_path_buf(),
        description: raw.description.filter(|d| !d.trim().is_empty()).unwrap_or_else(|| raw.name.clone()),
        name: raw.name,
        say: raw.say,
        patterns,
        lists,
        input,
        output: raw.output,
        steps,
        confirm: raw.confirm,
        router: raw.router,
        enabled: raw.enabled,
        timeout,
    })
}

fn step_from(raw: StepFile) -> Result<Step, String> {
    let mut found = Vec::new();
    if let Some(text) = raw.paste {
        found.push(Step::Paste(text));
    }
    if let Some(keys) = raw.keys {
        found.push(Step::Keys(steps::parse_keys(&keys)?));
    }
    if let Some(target) = raw.open {
        if target.trim().is_empty() {
            return Err("`open` is empty".into());
        }
        found.push(Step::Open(target));
    }
    if let Some(ms) = raw.wait {
        if ms > 10_000 {
            return Err("`wait` is at most 10000 ms".into());
        }
        found.push(Step::Wait(Duration::from_millis(ms)));
    }
    if let Some(prompt) = raw.ask {
        if prompt.trim().is_empty() {
            return Err("`ask` is empty".into());
        }
        found.push(Step::Ask(prompt));
    }
    if let Some(argv) = raw.run {
        if argv.first().is_none_or(|program| program.trim().is_empty()) {
            return Err("`run` needs a program first, like [\"pwsh\", \"-File\", \"x.ps1\"]".into());
        }
        if argv.iter().any(|arg| has_slot(arg)) {
            return Err("`run` takes no slots: read LIKH_TEXT and the rest from the environment or stdin".into());
        }
        found.push(Step::Run(argv));
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err("needs one of paste, keys, open, wait, ask or run".into()),
        _ => Err("has more than one of paste, keys, open, wait, ask or run: give each its own [[do]]".into()),
    }
}

/// `{text}`-style slot in a program's argument. A script's own braces (`{ 'done' }`) are fine.
fn has_slot(arg: &str) -> bool {
    arg.split('{').skip(1).any(|after| {
        after.split_once('}').is_some_and(|(name, _)| {
            !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
    })
}

/// The first line that mentions `needle`, counting from 1.
fn line_of(source: &str, needle: &str) -> Option<usize> {
    source.lines().position(|line| line.contains(needle)).map(|index| index + 1)
}

/// The line of the `nth` (from 0) line that mentions `needle`.
fn nth_line_of(source: &str, needle: &str, nth: usize) -> Option<usize> {
    source.lines().enumerate().filter(|(_, line)| line.trim_start().starts_with(needle)).nth(nth).map(|(i, _)| i + 1)
}

/// Turns an action on or off by its file's own `enabled` line, leaving the rest of the file as written.
pub fn set_enabled(file: &Path, on: bool) -> Result<(), String> {
    let source = std::fs::read_to_string(file).map_err(|error| error.to_string())?;
    let line = format!("enabled = {on}");
    let mut lines: Vec<String> = source.lines().map(str::to_owned).collect();
    // Top-level keys come before the first table header.
    let top = lines.iter().position(|l| l.trim_start().starts_with('[')).unwrap_or(lines.len());
    match lines[..top].iter().position(|l| l.trim_start().starts_with("enabled")) {
        Some(index) => lines[index] = line,
        None => {
            let after_name = lines[..top].iter().position(|l| l.trim_start().starts_with("name")).map_or(0, |i| i + 1);
            lines.insert(after_name, line);
        }
    }
    let mut text = lines.join("\n");
    if source.ends_with('\n') {
        text.push('\n');
    }
    std::fs::write(file, text).map_err(|error| error.to_string())
}

/// A simple action the Actions tab's form makes: one step, written as a file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Draft {
    pub name: String,
    pub say: Vec<String>,
    pub kind: DraftKind,
    /// The text to paste, the link to open, the keys, or the prompt.
    pub value: String,
    pub input: Vec<Context>,
    pub output: Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DraftKind {
    #[default]
    Paste,
    Open,
    Keys,
    Ask,
}

impl DraftKind {
    pub const ALL: [DraftKind; 4] = [DraftKind::Paste, DraftKind::Open, DraftKind::Keys, DraftKind::Ask];

    pub fn name(self) -> &'static str {
        match self {
            DraftKind::Paste => "Paste text",
            DraftKind::Open => "Open",
            DraftKind::Keys => "Press keys",
            DraftKind::Ask => "Ask ChatGPT",
        }
    }

    fn key(self) -> &'static str {
        match self {
            DraftKind::Paste => "paste",
            DraftKind::Open => "open",
            DraftKind::Keys => "keys",
            DraftKind::Ask => "ask",
        }
    }
}

impl Draft {
    /// The file's text. Every value goes through TOML's own quoting.
    pub fn to_toml(&self) -> String {
        let quote = |text: &str| toml::Value::String(text.to_owned()).to_string();
        let list = |items: Vec<String>| format!("[{}]", items.join(", "));
        let mut text = format!("name = {}\n", quote(self.name.trim()));
        text.push_str(&format!("say = {}\n", list(self.say.iter().map(|s| quote(s.trim())).collect())));
        if !self.input.is_empty() {
            text.push_str(&format!("input = {}\n", list(self.input.iter().map(|c| quote(c.slot())).collect())));
        }
        let output = match self.output {
            Output::Paste => "paste",
            Output::Replace => "replace",
            Output::Notify => "notify",
            Output::None => "none",
        };
        text.push_str(&format!("output = {}\n\n[[do]]\n{} = {}\n", quote(output), self.kind.key(), quote(&self.value)));
        text
    }

    /// A file name from the action's name: `make-this-formal.toml`, then `-2`, `-3` if taken.
    pub fn file_in(&self, dir: &Path) -> PathBuf {
        let mut stem: String = self
            .name
            .trim()
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        if stem.is_empty() {
            stem = "action".into();
        }
        let mut path = dir.join(format!("{stem}.toml"));
        let mut n = 2;
        while path.exists() {
            path = dir.join(format!("{stem}-{n}.toml"));
            n += 1;
        }
        path
    }

    /// Writes the file after checking it the same way loading would. Err says what to fix.
    pub fn save(&self, dir: &Path) -> Result<PathBuf, String> {
        let path = self.file_in(dir);
        let text = self.to_toml();
        parse(&path, &text).map_err(|problem| problem.message)?;
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        std::fs::write(&path, text).map_err(|error| error.to_string())?;
        Ok(path)
    }
}

/// `likh actions check`: every problem in the folder, one per line. Ok when there are none.
pub fn check(dir: &Path) -> Result<String, String> {
    let catalog = Catalog::load(dir);
    let mut report = String::new();
    for action in &catalog.actions {
        let off = if action.enabled { "" } else { " (off)" };
        report.push_str(&format!("ok    {}{off}: {}\n", file_name(&action.file), action.say.join(" | ")));
    }
    for problem in &catalog.problems {
        report.push_str(&format!("error {problem}\n"));
    }
    if catalog.actions.is_empty() && catalog.problems.is_empty() {
        report.push_str(&format!("no actions in {}\n", dir.display()));
    }
    if catalog.problems.is_empty() { Ok(report) } else { Err(report) }
}

/// `likh actions test "<phrase>"`: which action that phrase runs, with what,
/// and the steps it would take. Never runs anything.
pub fn test(dir: &Path, said: &str) -> String {
    let catalog = Catalog::load(dir);
    let mut report = String::new();
    for problem in &catalog.problems {
        report.push_str(&format!("error {problem}\n"));
    }
    match matcher::find(&catalog.actions, said) {
        None => report.push_str(&format!(
            "no phrase matches \"{said}\"; likh would ask ChatGPT to pick from {} action(s)\n",
            catalog.actions.iter().filter(|a| a.enabled && a.router).count()
        )),
        Some(found) => {
            let action = &catalog.actions[found.action];
            if let Some(near) = found.near_miss.map(|index| &catalog.actions[index]) {
                report.push_str(&format!(
                    "{} ({}) nearly matched: likh would ask ChatGPT first, and run this if it picks nothing\n",
                    near.name,
                    file_name(&near.file)
                ));
            }
            report.push_str(&format!("matches {} ({})\n", action.name, file_name(&action.file)));
            for (slot, value) in &found.slots {
                report.push_str(&format!("  {{{slot}}} = \"{value}\"\n"));
            }
            for (i, step) in action.steps.iter().enumerate() {
                report.push_str(&format!("  {}. {}\n", i + 1, step.describe()));
            }
            report.push_str(&format!("  then: {}\n", action.output.name()));
            if action.confirm {
                report.push_str("  asks first: confirm = true\n");
            }
            if action.runs_programs() {
                let approved = trust::state(action) == trust::Trust::Approved;
                report.push_str(if approved {
                    "  its scripts are approved\n"
                } else {
                    "  won't run until you approve it on likh's Commands tab\n"
                });
            }
        }
    }
    report
}
