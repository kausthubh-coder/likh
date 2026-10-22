use super::*;

const NOW: u64 = 1_792_627_200;
/// What ChatGPT answered a Free account on 2026-10-21 once its allowance ran out.
const OUT: &str = r#"{"error":{"type":"usage_limit_reached","message":"The usage limit has been reached","plan_type":"free","resets_at":1795025828,"resets_in_seconds":2398628}}"#;

#[test]
fn chatgpts_out_of_allowance_answer_gives_its_reset() {
    assert_eq!(read_limit(429, OUT, NOW), Some(1_795_025_828));
}

#[test]
fn a_reset_given_only_in_seconds_counts_from_now() {
    let body = r#"{"error":{"type":"usage_limit_reached","resets_in_seconds":3600}}"#;
    assert_eq!(read_limit(429, body, NOW), Some(NOW + 3600));
}

#[test]
fn an_answer_that_names_no_reset_waits_an_hour() {
    assert_eq!(read_limit(429, "usage_limit_reached", NOW), Some(NOW + 3600));
    let past = r#"{"type":"usage_limit_reached","resets_at":1}"#;
    assert_eq!(read_limit(429, past, NOW), Some(NOW + 3600));
}

#[test]
fn a_passing_rate_limit_or_any_other_answer_is_not_the_allowance() {
    assert_eq!(read_limit(429, r#"{"error":{"type":"rate_limit_exceeded"}}"#, NOW), None);
    assert_eq!(read_limit(500, OUT, NOW), None);
    assert_eq!(read_limit(200, "data: {}", NOW), None);
}

#[test]
fn a_recorded_reset_pauses_until_then_and_tells_the_next_dictation_once() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("likh").join("allowance.json");
    assert_eq!(paused_until(&file, NOW), None);
    record(&file, NOW + 600).expect("saved");
    assert_eq!(paused_until(&file, NOW), Some(NOW + 600));
    assert_eq!(paused_until(&file, NOW + 600), None, "over once it resets");
    // Other tests may record too; this one's reset is what it finds, then nothing.
    assert!(take_news().is_some());
    assert_eq!(take_news(), None);
}

#[test]
fn a_broken_file_never_pauses_anything() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("allowance.json");
    std::fs::write(&file, "not json").expect("write");
    assert_eq!(paused_until(&file, NOW), None);
}

#[test]
fn a_reset_weeks_away_reads_as_a_day_and_month() {
    let day = when(NOW + 30 * 86_400, NOW);
    let (number, month) = day.split_once(' ').expect("a day and a month");
    assert!(number.parse::<u32>().is_ok_and(|n| (1..=31).contains(&n)), "{day}");
    assert_eq!(month.len(), 3, "{day}");
}

#[test]
fn the_fake_answer_reads_as_chatgpts_own() {
    let body = serde_json::json!({ "error": { "type": "usage_limit_reached", "resets_at": NOW + 60 } }).to_string();
    assert_eq!(read_limit(429, &body, NOW), Some(NOW + 60));
}
