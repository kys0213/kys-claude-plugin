//! Shared builders for the `kanban board` black-box tests.
//!
//! `#![allow(dead_code)]` because each test binary uses only part of them
//! (Cargo compiles this module into every test crate that declares it).
#![allow(dead_code)]

use assert_cmd::Command;
use serde_json::{json, Value};

pub const NOW: u64 = 1_800_000_000_000;
pub const SEC: u64 = 1_000;
pub const MIN: u64 = 60 * SEC;

pub fn atelier() -> Command {
    Command::cargo_bin("atelier").expect("locate `atelier` cargo binary")
}

pub fn board_with(args: &[&str], stdin: &str) -> Value {
    let output = atelier()
        .args(["kanban", "board"])
        .args(args)
        .write_stdin(stdin)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("valid JSON on stdout")
}

pub fn board(sessions: Value) -> Value {
    let now = NOW.to_string();
    board_with(
        &[
            "--now",
            &now,
            "--stale-after",
            "2m",
            "--expire-after",
            "24h",
            "--self",
            "self-id",
            "--width",
            "100",
        ],
        &json!({ "sessions": sessions }).to_string(),
    )
}

pub fn session(id: &str, events: Value) -> Value {
    json!({
        "v": 1,
        "session_id": id,
        "cwd": "/work/project-a",
        "repo": null,
        "title": null,
        "first_prompt": null,
        "started_at": NOW - 30 * MIN,
        "heartbeat_at": NOW - 5 * SEC,
        "ended_reason": null,
        "events": events,
    })
}

pub fn event(name: &str, detail: Option<&str>, ago: u64) -> Value {
    json!({ "name": name, "detail": detail, "at": NOW - ago })
}

pub fn session_with(id: &str, events: Value, extra: Value) -> Value {
    let mut record = session(id, events);
    for (key, value) in extra.as_object().expect("extra fields") {
        record[key] = value.clone();
    }
    record
}
