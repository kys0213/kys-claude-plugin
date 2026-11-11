mod kanban_support;

use kanban_support::{
    atelier, event, kanban_in, put_copy, record_path, session, session_with, status_in, MIN, NOW,
    SEC,
};
use serde_json::{json, Value};
use tempfile::TempDir;

fn now_arg() -> String {
    NOW.to_string()
}

fn working(id: &str) -> Value {
    session(id, json!([event("turn.start", None, 10 * SEC)]))
}

fn asking(id: &str) -> Value {
    session(
        id,
        json!([
            event("turn.start", None, 20 * SEC),
            event("classic.PermissionRequest", Some("Bash"), 10 * SEC)
        ]),
    )
}

fn idle(id: &str) -> Value {
    session(id, json!([event("session.start", None, MIN)]))
}

fn put(config: &TempDir, record: &Value) {
    put_copy(
        config.path(),
        record["session_id"].as_str().expect("id"),
        &record.to_string(),
    );
}

fn ids(out: &Value) -> Vec<&str> {
    out["sessions"]
        .as_array()
        .expect("sessions array")
        .iter()
        .map(|s| s["session_id"].as_str().expect("id"))
        .collect()
}

#[test]
fn lists_sessions_from_the_copies_in_board_order() {
    let config = TempDir::new().unwrap();
    put(&config, &working("w1"));
    put(&config, &idle("i1"));
    put(&config, &asking("a1"));

    let out = status_in(config.path(), &["--now", &now_arg()]);

    assert_eq!(out["now"], json!(NOW));
    assert_eq!(ids(&out), vec!["a1", "w1", "i1"]);
    assert_eq!(out["sessions"][0]["column"], "needs_input");
    assert_eq!(out["sessions"][0]["cwd"], "/work/project-a");
    assert!(out["sessions"][0]["actions"].is_array());
}

#[test]
fn a_broken_copy_drops_only_its_session() {
    let config = TempDir::new().unwrap();
    put(&config, &working("w1"));
    put_copy(config.path(), "broken", "{not json");
    put_copy(
        config.path(),
        "wrongver",
        r#"{"v":9,"session_id":"wrongver"}"#,
    );
    put(&config, &asking("a1"));

    let out = status_in(config.path(), &["--now", &now_arg()]);

    assert_eq!(ids(&out), vec!["a1", "w1"]);
}

#[test]
fn column_may_repeat_to_pick_several() {
    let config = TempDir::new().unwrap();
    put(&config, &working("w1"));
    put(&config, &idle("i1"));
    put(&config, &asking("a1"));

    let one = status_in(config.path(), &["--now", &now_arg(), "--column", "working"]);
    let two = status_in(
        config.path(),
        &[
            "--now",
            &now_arg(),
            "--column",
            "working",
            "--column",
            "needs_input",
        ],
    );

    assert_eq!(ids(&one), vec!["w1"]);
    assert_eq!(ids(&two), vec!["a1", "w1"]);
}

#[test]
fn an_unknown_column_is_refused() {
    let config = TempDir::new().unwrap();
    put(&config, &working("w1"));

    let out = status_in(config.path(), &["--now", &now_arg(), "--column", "done"]);

    assert!(out["error"].as_str().unwrap().contains("done"));
    assert!(out.get("sessions").is_none());
}

#[test]
fn a_fixed_now_gives_the_same_output() {
    let config = TempDir::new().unwrap();
    put(&config, &working("w1"));
    put(&config, &asking("a1"));

    let first = status_in(config.path(), &["--now", &now_arg()]);
    let second = status_in(config.path(), &["--now", &now_arg()]);
    let later = status_in(config.path(), &["--now", &(NOW + 10 * MIN).to_string()]);

    assert_eq!(first, second);
    assert_ne!(first["sessions"][0]["age"], later["sessions"][0]["age"]);
}

#[test]
fn a_copy_silent_for_over_a_day_is_hidden() {
    let config = TempDir::new().unwrap();
    put(&config, &working("fresh"));
    let old = session_with(
        "old",
        json!([event("turn.start", None, 25 * 60 * MIN)]),
        json!({ "heartbeat_at": NOW - 25 * 60 * MIN }),
    );
    put(&config, &old);

    let out = status_in(config.path(), &["--now", &now_arg()]);

    assert_eq!(ids(&out), vec!["fresh"]);
}

#[test]
fn a_silent_copy_shows_as_stale() {
    let config = TempDir::new().unwrap();
    let quiet = session_with(
        "quiet",
        json!([event("turn.start", None, 10 * MIN)]),
        json!({ "heartbeat_at": NOW - 10 * MIN }),
    );
    put(&config, &quiet);

    let out = status_in(config.path(), &["--now", &now_arg()]);

    assert_eq!(out["sessions"][0]["column"], "stale");
}

#[test]
fn session_picks_one_session() {
    let config = TempDir::new().unwrap();
    put(&config, &working("w1"));
    put(&config, &asking("a1"));

    let out = status_in(config.path(), &["--now", &now_arg(), "--session", "w1"]);

    assert_eq!(ids(&out), vec!["w1"]);
}

#[test]
fn text_format_prints_a_heading_per_column_and_a_line_per_session() {
    let config = TempDir::new().unwrap();
    put(&config, &asking("abcdefgh-1234"));
    put(&config, &working("w1"));

    let output = atelier()
        .env("CLAUDE_CONFIG_DIR", config.path())
        .args(["kanban", "status", "--now", &now_arg(), "--format", "text"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();

    assert!(text.contains("입력 대기 1"));
    assert!(text.contains("작업 중 1"));
    assert!(text.contains("abcdefgh"));
    assert!(!text.contains("abcdefgh-1234"));
    assert!(text.contains("권한 요청: Bash"));
}

#[test]
fn without_a_records_directory_status_is_refused() {
    let config = TempDir::new().unwrap();

    let out = status_in(config.path(), &["--now", &now_arg()]);

    assert!(out["error"].is_string());
}

#[test]
fn an_unknown_format_is_refused() {
    let config = TempDir::new().unwrap();
    put(&config, &working("w1"));

    let out = status_in(config.path(), &["--now", &now_arg(), "--format", "yaml"]);

    assert!(out["error"].as_str().unwrap().contains("yaml"));
}

#[test]
fn record_path_names_one_file_per_session_under_the_config_dir() {
    let config = TempDir::new().unwrap();

    let a = record_path(config.path(), "s-1");
    let b = record_path(config.path(), "s-2");

    assert!(a.starts_with(config.path()));
    assert_ne!(a, b);
    assert_eq!(a, record_path(config.path(), "s-1"));
}

#[test]
fn record_path_refuses_an_id_that_leaves_the_directory() {
    let config = TempDir::new().unwrap();

    let out = kanban_in(config.path(), &["record-path", "--session", "../x"]);

    assert!(out["error"].is_string());
}

#[test]
fn forget_removes_the_copy_and_the_inbox() {
    let config = TempDir::new().unwrap();
    put(&config, &working("w1"));
    put(&config, &working("w2"));
    kanban_in(
        config.path(),
        &["send", "--session", "w1", "--from", "pane", "--text", "hi"],
    );
    let inbox = kanban_in(config.path(), &["inbox", "dir", "--session", "w1"]);
    let inbox = std::path::PathBuf::from(inbox["dir"].as_str().unwrap());
    assert!(inbox.exists());

    let out = kanban_in(config.path(), &["forget", "--session", "w1"]);

    assert_eq!(out["forgot"], "w1");
    assert!(!record_path(config.path(), "w1").exists());
    assert!(!inbox.exists());
    assert!(record_path(config.path(), "w2").exists());
}

#[test]
fn forgetting_an_unknown_session_succeeds() {
    let config = TempDir::new().unwrap();

    let out = kanban_in(config.path(), &["forget", "--session", "nobody"]);

    assert_eq!(out["forgot"], "nobody");
}

#[test]
fn forget_refuses_an_id_that_leaves_the_directory() {
    let config = TempDir::new().unwrap();

    let out = kanban_in(config.path(), &["forget", "--session", "../x"]);

    assert!(out["error"].is_string());
}
