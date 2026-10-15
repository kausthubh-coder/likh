//! What likh did and how long it took, kept on this computer so a problem
//! can be looked at after it happened. The installed likh has no console, so
//! this is the only place its warnings and errors go.
//!
//! Each step of a dictation (the mic opening, the transcriber, the cleanup,
//! the paste, the whole thing from key-up) is one span: one JSON line in
//! `logs/trace.ndjson` with its name, when it ended, how long it took and why
//! it failed, if it did. Warnings and errors are lines of their own. The file
//! rolls over at `ROLL_AT`, keeping `KEPT` old ones, so it never grows past a
//! few megabytes. Never the words someone said, never a token.
//!
//! `read` sums the files up for the Diagnostics tab and `likh ctl diagnostics`:
//! the busiest steps, the slowest, what failed and the latest warnings, like
//! T3 Code's Diagnostics page. Nothing here runs while likh is idle: lines are
//! written as things happen, and `Sampler` reads CPU and memory only while
//! someone is looking.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[cfg(test)]
mod tests;

const FILE: &str = "trace.ndjson";
/// A file rolls over past this; about a thousand dictations.
const ROLL_AT: u64 = 1024 * 1024;
/// Old files kept beside the current one: `trace.ndjson.1` is the newest of them.
const KEPT: usize = 3;
/// A step that took longer than this counts as slow.
pub const SLOW_MS: f64 = 2000.0;
/// How many lines each list in a `Report` keeps.
const LISTED: usize = 8;

/// The trace file the running app writes to. Unset in `likh ctl`, `likh status`
/// and tests, so they never write into a real likh's folder.
static TRACE: OnceLock<Trace> = OnceLock::new();

/// Starts writing to `folder/logs`. Called once, by the running app.
pub fn start(folder: &Path) {
    let _ = TRACE.set(Trace::new(folder.join("logs")));
}

/// The running app's logs folder, if it writes one.
pub fn folder() -> Option<PathBuf> {
    TRACE.get().map(|trace| trace.folder.clone())
}

/// Writes `record` to the running app's trace, if there is one.
fn write(record: &Record) {
    if let Some(trace) = TRACE.get() {
        trace.append(record);
    }
}

/// A warning: something didn't work, and likh carried on.
pub fn warn(source: &str, message: impl Into<String>) {
    log("warn", source, message.into());
}

/// An error: something the user wanted didn't happen.
pub fn error(source: &str, message: impl Into<String>) {
    log("error", source, message.into());
}

/// What likh is doing, worth a line: it started, which shortcut it took.
pub fn info(source: &str, message: impl Into<String>) {
    log("info", source, message.into());
}

fn log(level: &str, source: &str, message: String) {
    // A terminal running likh still sees it.
    eprintln!("likh: {message}");
    write(&Record::Log { level: level.into(), source: source.into(), message, at: now_ms() });
}

/// One step being timed. `end` writes it.
pub struct Span {
    name: &'static str,
    started: Instant,
    attributes: Map<String, Value>,
}

impl Span {
    pub fn start(name: &'static str) -> Self {
        Self { name, started: Instant::now(), attributes: Map::new() }
    }

    /// The same step, timed from `started` (a key-up seen earlier).
    pub fn since(name: &'static str, started: Instant) -> Self {
        Self { name, started, attributes: Map::new() }
    }

    /// Something worth knowing about this step: the engine, the seconds of audio. Never what was said.
    pub fn with(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Writes the step: done, or failed and why.
    pub fn end(self, outcome: Result<(), String>) {
        write(&self.record(outcome, now_ms()));
    }

    fn record(self, outcome: Result<(), String>, at: u64) -> Record {
        Record::Span {
            name: self.name.into(),
            at,
            duration_ms: (self.started.elapsed().as_secs_f64() * 1000.0 * 10.0).round() / 10.0,
            cause: outcome.err(),
            attributes: self.attributes,
        }
    }
}

/// One line of the trace file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Record {
    Span {
        name: String,
        /// When it ended, in milliseconds since 1970.
        at: u64,
        duration_ms: f64,
        /// Why it failed. None when it worked.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cause: Option<String>,
        #[serde(default, skip_serializing_if = "Map::is_empty")]
        attributes: Map<String, Value>,
    },
    Log {
        level: String,
        source: String,
        message: String,
        at: u64,
    },
}

/// A trace folder: `trace.ndjson` and the old files rolled over from it.
pub struct Trace {
    folder: PathBuf,
    max_bytes: u64,
    /// One writer at a time, so two threads' lines never interleave or roll over twice.
    writing: Mutex<()>,
}

impl Trace {
    pub fn new(folder: PathBuf) -> Self {
        Self::rolling_at(folder, ROLL_AT)
    }

    fn rolling_at(folder: PathBuf, max_bytes: u64) -> Self {
        Self { folder, max_bytes, writing: Mutex::new(()) }
    }

    /// The current file first, then the old ones, newest first.
    pub fn files(&self) -> Vec<PathBuf> {
        std::iter::once(self.folder.join(FILE))
            .chain((1..=KEPT).map(|n| self.folder.join(format!("{FILE}.{n}"))))
            .collect()
    }

    /// Adds one line. A trace that can't be written is given up on quietly:
    /// diagnostics must never get in the way of a dictation.
    pub fn append(&self, record: &Record) {
        let _writing = self.writing.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let Ok(line) = serde_json::to_string(record) else { return };
        if std::fs::create_dir_all(&self.folder).is_err() {
            return;
        }
        let files = self.files();
        if std::fs::metadata(&files[0]).is_ok_and(|meta| meta.len() >= self.max_bytes) {
            // The oldest falls off the end; each other moves one along.
            for n in (1..files.len()).rev() {
                let _ = std::fs::rename(&files[n - 1], &files[n]);
            }
        }
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&files[0]) {
            let _ = writeln!(file, "{line}");
        }
    }

    /// Everything in the folder, summed up.
    pub fn read(&self) -> Report {
        let files: Vec<PathBuf> = self.files().into_iter().filter(|file| file.exists()).collect();
        let mut lines = Vec::new();
        for file in &files {
            if let Ok(text) = std::fs::read_to_string(file) {
                lines.extend(text.lines().map(str::to_owned));
            }
        }
        let mut report = summarize(lines.iter().map(String::as_str), now_ms());
        report.files = files;
        report
    }
}

/// What the trace files say, for the Diagnostics tab and `likh ctl diagnostics`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Report {
    pub files: Vec<PathBuf>,
    pub read_at: u64,
    pub records: usize,
    /// Lines that aren't a record: cut off by a crash mid-write, or from a newer likh.
    pub unreadable: usize,
    pub first_at: Option<u64>,
    pub last_at: Option<u64>,
    pub failures: usize,
    pub slow_ms: f64,
    pub slow: usize,
    /// Warnings, errors and info lines, by level.
    pub levels: BTreeMap<String, usize>,
    /// The steps that ran most often.
    pub busiest: Vec<StepSummary>,
    pub slowest: Vec<Occurrence>,
    /// Failures with the same step and cause, together, the most common first.
    pub common_failures: Vec<FailureSummary>,
    pub latest_failures: Vec<Occurrence>,
    pub latest_logs: Vec<LogLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepSummary {
    pub name: String,
    pub count: usize,
    pub failures: usize,
    pub total_ms: f64,
    pub average_ms: f64,
    pub max_ms: f64,
}

/// One run of a step.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Occurrence {
    pub name: String,
    pub duration_ms: f64,
    pub at: u64,
    pub cause: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FailureSummary {
    pub name: String,
    pub cause: String,
    pub count: usize,
    pub last_at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogLine {
    pub level: String,
    pub source: String,
    pub message: String,
    pub at: u64,
}

/// Sums up trace lines, oldest or newest first: every list is sorted here.
pub fn summarize<'a>(lines: impl Iterator<Item = &'a str>, read_at: u64) -> Report {
    let mut report = Report { read_at, slow_ms: SLOW_MS, ..Report::default() };
    let mut steps: BTreeMap<String, StepSummary> = BTreeMap::new();
    let mut failures: BTreeMap<(String, String), FailureSummary> = BTreeMap::new();
    let mut spans = Vec::new();
    for line in lines.filter(|line| !line.trim().is_empty()) {
        let Ok(record) = serde_json::from_str::<Record>(line) else {
            report.unreadable += 1;
            continue;
        };
        report.records += 1;
        let at = match &record {
            Record::Span { at, .. } | Record::Log { at, .. } => *at,
        };
        report.first_at = Some(report.first_at.map_or(at, |first| first.min(at)));
        report.last_at = Some(report.last_at.map_or(at, |last| last.max(at)));
        match record {
            Record::Span { name, at, duration_ms, cause, .. } => {
                let step = steps.entry(name.clone()).or_insert_with(|| StepSummary {
                    name: name.clone(),
                    count: 0,
                    failures: 0,
                    total_ms: 0.0,
                    average_ms: 0.0,
                    max_ms: 0.0,
                });
                step.count += 1;
                step.total_ms += duration_ms;
                step.max_ms = step.max_ms.max(duration_ms);
                if duration_ms > SLOW_MS {
                    report.slow += 1;
                }
                if let Some(cause) = &cause {
                    step.failures += 1;
                    report.failures += 1;
                    let failure = failures.entry((name.clone(), cause.clone())).or_insert_with(|| FailureSummary {
                        name: name.clone(),
                        cause: cause.clone(),
                        count: 0,
                        last_at: at,
                    });
                    failure.count += 1;
                    failure.last_at = failure.last_at.max(at);
                }
                spans.push(Occurrence { name, duration_ms, at, cause });
            }
            Record::Log { level, source, message, at } => {
                *report.levels.entry(level.clone()).or_default() += 1;
                if level != "info" {
                    report.latest_logs.push(LogLine { level, source, message, at });
                }
            }
        }
    }
    let mut busiest: Vec<StepSummary> = steps
        .into_values()
        .map(|step| StepSummary { average_ms: round(step.total_ms / step.count as f64), ..step })
        .collect();
    busiest.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    report.busiest = busiest.into_iter().take(LISTED).collect();

    let mut common: Vec<FailureSummary> = failures.into_values().collect();
    common.sort_by(|a, b| b.count.cmp(&a.count).then(b.last_at.cmp(&a.last_at)));
    report.common_failures = common.into_iter().take(LISTED).collect();

    let mut latest: Vec<Occurrence> = spans.iter().filter(|span| span.cause.is_some()).cloned().collect();
    latest.sort_by_key(|span| std::cmp::Reverse(span.at));
    report.latest_failures = latest.into_iter().take(LISTED).collect();

    spans.sort_by(|a, b| b.duration_ms.total_cmp(&a.duration_ms));
    report.slowest = spans.into_iter().take(LISTED).collect();

    report.latest_logs.sort_by_key(|line| std::cmp::Reverse(line.at));
    report.latest_logs.truncate(LISTED);
    report
}

fn round(ms: f64) -> f64 {
    (ms * 10.0).round() / 10.0
}

/// Seconds of audio for a span: 3.13, not the 3.130000114 an f32 widens to.
pub fn seconds(seconds: f32) -> f64 {
    (f64::from(seconds) * 100.0).round() / 100.0
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// likh's own memory and CPU right now.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Usage {
    /// Physical memory in use (the working set on Windows).
    pub memory_bytes: u64,
    /// Share of the whole computer since the last sample; None on the first one.
    pub cpu_percent: Option<f32>,
    pub running_seconds: u64,
}

/// Reads likh's own process. Held only while someone is looking at the
/// numbers: the Diagnostics tab, or one `likh ctl diagnostics`.
pub struct Sampler {
    system: sysinfo::System,
    pid: Option<sysinfo::Pid>,
    sampled: bool,
}

impl Default for Sampler {
    fn default() -> Self {
        Self { system: sysinfo::System::new(), pid: sysinfo::get_current_pid().ok(), sampled: false }
    }
}

impl Sampler {
    pub fn sample(&mut self) -> Option<Usage> {
        let pid = self.pid?;
        self.system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[pid]),
            false,
            sysinfo::ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let process = self.system.process(pid)?;
        // sysinfo counts each core as 100%; Task Manager shows a share of the whole computer.
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f32;
        let cpu_percent = self.sampled.then(|| process.cpu_usage() / cores);
        self.sampled = true;
        Some(Usage { memory_bytes: process.memory(), cpu_percent, running_seconds: process.run_time() })
    }
}
