mod kanban_support;

use kanban_support::{event, kanban_in, put_record, refused, session, status_in, MIN, NOW};
use serde_json::{json, Value};
use std::path::Path;
use tempfile::TempDir;

fn opened(config: &Path, args: &[&str]) -> Value {
    let mut full = vec!["opened", "--session", "s1"];
    full.extend_from_slice(args);
    kanban_in(config, &full)
}

fn status_opened(config: &Path) -> Value {
    let now = NOW.to_string();
    status_in(config, &["--now", &now])["sessions"][0]["opened"].clone()
}

fn with_session() -> TempDir {
    let config = TempDir::new().unwrap();
    put_record(
        config.path(),
        &session("s1", json!([event("session.start", None, MIN)])),
    );
    config
}

#[test]
fn a_session_no_board_opened_has_no_entry() {
    let config = with_session();

    assert_eq!(status_opened(config.path()), Value::Null);
}

#[test]
fn the_board_sees_a_tab_opening_then_the_terminal_it_reported() {
    let config = with_session();

    let claimed = opened(
        config.path(),
        &["--opening", "--seen", "none", "--now", "100"],
    );
    assert_eq!(
        claimed["opened"],
        json!({ "session_id": "s1", "at": 100, "terminal": null })
    );
    assert_eq!(
        status_opened(config.path()),
        json!({ "at": 100, "terminal": null })
    );

    opened(config.path(), &["--terminal", "t31", "--now", "200"]);
    assert_eq!(
        status_opened(config.path()),
        json!({ "at": 200, "terminal": "t31" })
    );
}

#[test]
fn two_boards_that_saw_the_same_entry_open_one_tab() {
    let config = with_session();

    let first = opened(
        config.path(),
        &["--opening", "--seen", "none", "--now", "100"],
    );
    let second = opened(
        config.path(),
        &["--opening", "--seen", "none", "--now", "101"],
    );

    assert!(!refused(&first), "{first}");
    assert!(refused(&second), "{second}");
    assert_eq!(status_opened(config.path())["at"], 100);
}

#[test]
fn a_board_reopens_over_the_closed_terminal_it_saw() {
    let config = with_session();
    opened(config.path(), &["--terminal", "t2y", "--now", "100"]);

    let stale = opened(
        config.path(),
        &["--opening", "--seen", "none", "--now", "300"],
    );
    let seen = opened(
        config.path(),
        &["--opening", "--seen", "100", "--now", "300"],
    );

    assert!(refused(&stale), "{stale}");
    assert_eq!(seen["opened"]["terminal"], Value::Null);
}

#[test]
fn a_clear_with_seen_keeps_an_entry_written_since() {
    let config = with_session();
    opened(
        config.path(),
        &["--opening", "--seen", "none", "--now", "100"],
    );
    opened(config.path(), &["--terminal", "t31", "--now", "150"]);

    let late = opened(config.path(), &["--clear", "--seen", "100"]);
    assert!(refused(&late), "{late}");
    assert_eq!(status_opened(config.path())["terminal"], "t31");

    assert_eq!(
        opened(config.path(), &["--clear"]),
        json!({ "cleared": "s1" })
    );
    assert_eq!(status_opened(config.path()), Value::Null);
}

#[test]
fn forget_removes_the_entry_too() {
    let config = with_session();
    opened(config.path(), &["--terminal", "t31", "--now", "100"]);

    kanban_in(config.path(), &["forget", "--session", "s1"]);
    put_record(
        config.path(),
        &session("s1", json!([event("session.start", None, MIN)])),
    );

    assert_eq!(status_opened(config.path()), Value::Null);
}

#[test]
fn a_bad_terminal_or_seen_is_refused() {
    let config = with_session();

    assert!(refused(&opened(config.path(), &["--terminal", "t 1"])));
    assert!(refused(&opened(
        config.path(),
        &["--opening", "--seen", "soon"]
    )));
}
