use super::*;

fn span(name: &str, at: u64, duration_ms: f64, cause: Option<&str>) -> String {
    let record =
        Record::Span { name: name.into(), at, duration_ms, cause: cause.map(str::to_owned), attributes: Map::new() };
    serde_json::to_string(&record).expect("a record serializes")
}

fn log(level: &str, message: &str, at: u64) -> String {
    let record = Record::Log { level: level.into(), source: "cleanup".into(), message: message.into(), at };
    serde_json::to_string(&record).expect("a record serializes")
}

#[test]
fn a_span_is_one_line_with_its_name_duration_and_cause() {
    let record = Span::start("transcribe")
        .with("engine", "codex")
        .with("audio_seconds", 4.5)
        .record(Err("Can't reach ChatGPT.".into()), 1_000);
    let line = serde_json::to_value(&record).expect("serializes");
    assert_eq!(line["type"], "span");
    assert_eq!(line["name"], "transcribe");
    assert_eq!(line["at"], 1_000);
    assert_eq!(line["cause"], "Can't reach ChatGPT.");
    assert_eq!(line["attributes"], serde_json::json!({ "engine": "codex", "audio_seconds": 4.5 }));
    assert!(line["duration_ms"].as_f64().is_some_and(|ms| ms < 1000.0), "{line}");
}

#[test]
fn a_step_that_worked_has_no_cause() {
    let line = serde_json::to_string(&Span::start("paste").record(Ok(()), 5)).expect("serializes");
    assert!(!line.contains("cause"), "{line}");
    assert!(!line.contains("attributes"), "{line}");
}

#[test]
fn summarize_counts_steps_failures_slow_ones_and_logs() {
    let lines = [
        span("transcribe", 1_000, 900.0, None),
        span("transcribe", 2_000, 3_100.0, None),
        span("transcribe", 3_000, 400.0, Some("Can't reach ChatGPT.")),
        span("transcribe", 4_000, 450.0, Some("Can't reach ChatGPT.")),
        span("paste", 5_000, 60.0, None),
        span("cleanup", 6_000, 4_000.0, Some("timed out")),
        log("warn", "the cleanup came back empty", 7_000),
        log("info", "likh is running", 500),
        "{\"type\":\"span\",\"name\":\"cut off mid-wri".to_string(),
    ];
    let report = summarize(lines.iter().map(String::as_str), 10_000);

    assert_eq!(report.records, 8);
    assert_eq!(report.unreadable, 1);
    assert_eq!((report.first_at, report.last_at), (Some(500), Some(7_000)));
    assert_eq!(report.failures, 3);
    assert_eq!(report.slow, 2, "3.1 s and 4 s are over {SLOW_MS} ms");

    let busiest: Vec<_> = report.busiest.iter().map(|s| (s.name.as_str(), s.count, s.failures)).collect();
    assert_eq!(busiest, [("transcribe", 4, 2), ("cleanup", 1, 1), ("paste", 1, 0)]);
    assert_eq!(report.busiest[0].average_ms, 1212.5);
    assert_eq!(report.busiest[0].max_ms, 3_100.0);

    let slowest: Vec<_> = report.slowest.iter().map(|s| s.duration_ms).collect();
    assert_eq!(slowest, [4_000.0, 3_100.0, 900.0, 450.0, 400.0, 60.0]);

    assert_eq!(
        report.common_failures,
        [
            FailureSummary {
                name: "transcribe".into(),
                cause: "Can't reach ChatGPT.".into(),
                count: 2,
                last_at: 4_000
            },
            FailureSummary { name: "cleanup".into(), cause: "timed out".into(), count: 1, last_at: 6_000 },
        ]
    );
    let latest: Vec<_> = report.latest_failures.iter().map(|f| f.at).collect();
    assert_eq!(latest, [6_000, 4_000, 3_000], "newest first");

    assert_eq!(report.levels, BTreeMap::from([("info".into(), 1), ("warn".into(), 1)]));
    // Info lines are counted but not listed: the list is for what went wrong.
    assert_eq!(report.latest_logs.len(), 1);
    assert_eq!(report.latest_logs[0].message, "the cleanup came back empty");
}

#[test]
fn an_empty_trace_reads_as_nothing_yet() {
    let report = summarize(std::iter::empty(), 10);
    assert_eq!(report.records, 0);
    assert_eq!(report.first_at, None);
    assert!(report.busiest.is_empty() && report.slowest.is_empty() && report.latest_logs.is_empty());
}

#[test]
fn the_trace_rolls_over_and_keeps_only_the_newest_files() {
    let dir = tempfile::tempdir().expect("temp dir");
    // Small enough that every line starts a new file.
    let trace = Trace::rolling_at(dir.path().join("logs"), 10);
    for at in 1..=6 {
        trace.append(&Record::Log { level: "warn".into(), source: "test".into(), message: format!("line {at}"), at });
    }
    let kept: Vec<_> = trace.files().into_iter().filter(|f| f.exists()).collect();
    assert_eq!(kept.len(), 1 + KEPT, "the current file and {KEPT} old ones");
    let report = trace.read();
    let messages: Vec<_> = report.latest_logs.iter().map(|l| l.message.as_str()).collect();
    assert_eq!(messages, ["line 6", "line 5", "line 4", "line 3"], "lines 1 and 2 rolled off the end");
    assert_eq!(report.files.len(), 1 + KEPT);
}

#[test]
fn appending_to_a_trace_keeps_every_line() {
    let dir = tempfile::tempdir().expect("temp dir");
    let trace = Trace::new(dir.path().join("logs"));
    trace.append(&Span::start("paste").with("characters", 12).record(Ok(()), 100));
    trace.append(&Span::start("transcribe").record(Err("Groq turned down your key.".into()), 200));
    let report = trace.read();
    assert_eq!(report.records, 2);
    assert_eq!(report.common_failures[0].cause, "Groq turned down your key.");
}

#[test]
fn nothing_is_written_until_the_app_starts_a_trace() {
    // Tests never call `start`, so a warning from any code under test goes nowhere on disk.
    assert_eq!(folder(), None);
    warn("test", "this goes to the terminal only");
    assert_eq!(folder(), None);
}

#[test]
fn the_sampler_reads_this_process() {
    let mut sampler = Sampler::default();
    let first = sampler.sample().expect("this process can read itself");
    assert!(first.memory_bytes > 1024 * 1024, "{first:?}");
    assert_eq!(first.cpu_percent, None, "CPU needs two samples to measure");
    let second = sampler.sample().expect("again");
    assert!(second.cpu_percent.is_some_and(|cpu| (0.0..=100.0).contains(&cpu)), "{second:?}");
}
