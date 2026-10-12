use super::*;

#[test]
fn the_paste_log_gets_one_json_line_per_paste_or_key() {
    let dir = tempfile::tempdir().expect("temp dir");
    let log = dir.path().join("pasted.jsonl");
    log_line(&log, &serde_json::json!({ "paste": "Meet at \"three\".\nSee you." })).expect("paste");
    log_line(&log, &serde_json::json!({ "press": Press::Tab.name() })).expect("press");
    let lines: Vec<serde_json::Value> = std::fs::read_to_string(&log)
        .expect("read")
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON value per line"))
        .collect();
    assert_eq!(
        lines,
        vec![serde_json::json!({ "paste": "Meet at \"three\".\nSee you." }), serde_json::json!({ "press": "Tab" })]
    );
}

#[test]
fn a_log_that_cant_be_written_says_where() {
    let dir = tempfile::tempdir().expect("temp dir");
    let log = dir.path().join("no-such-folder").join("pasted.jsonl");
    let error = log_line(&log, &serde_json::json!({ "paste": "x" })).expect_err("the folder is missing");
    assert!(error.starts_with(&format!("can't write {}", log.display())), "{error}");
}
