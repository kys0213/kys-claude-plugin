//! Shared builders for the `kanban` black-box tests.
//!
//! `#![allow(dead_code)]` because each test binary uses only part of them
//! (Cargo compiles this module into every test crate that declares it).
#![allow(dead_code)]

use assert_cmd::Command;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

pub const NOW: u64 = 1_800_000_000_000;
pub const SEC: u64 = 1_000;
pub const MIN: u64 = 60 * SEC;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_millis() as u64
}

pub fn atelier() -> Command {
    Command::cargo_bin("atelier").expect("locate `atelier` cargo binary")
}

/// The JSON a command that succeeds prints.
pub fn stdout_json(command: &mut Command) -> Value {
    let output = command.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).expect("valid JSON on stdout")
}

/// A config directory of its own for `atelier kanban` runs.
pub struct Home {
    config: TempDir,
}

impl Default for Home {
    fn default() -> Home {
        Home {
            config: TempDir::new().expect("config dir"),
        }
    }
}

impl Home {
    pub fn config(&self) -> &Path {
        self.config.path()
    }

    /// `atelier kanban`, its config directory set, before its arguments.
    pub fn command(&self) -> Command {
        let mut command = atelier();
        command
            .env("CLAUDE_CONFIG_DIR", self.config())
            .arg("kanban");
        command
    }

    pub fn run(&self, args: &[&str], stdin: &str) -> Value {
        stdout_json(self.command().args(args).write_stdin(stdin))
    }

    pub fn kanban(&self, args: &[&str]) -> Value {
        self.run(args, "")
    }

    pub fn inbox(&self, session: &str) -> PathBuf {
        self.config().join("atelier/kanban/inbox").join(session)
    }
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

/// An event a subagent's or teammate's loop raised in the session.
pub fn agent_event(name: &str, detail: Option<&str>, ago: u64) -> Value {
    json!({ "name": name, "detail": detail, "at": NOW - ago, "agent": true })
}

pub fn session_with(id: &str, events: Value, extra: Value) -> Value {
    let mut record = session(id, events);
    for (key, value) in extra.as_object().expect("extra fields") {
        record[key] = value.clone();
    }
    record
}

/// Runs `atelier kanban <args>` with its config directory at `config`.
pub fn kanban_in(config: &Path, args: &[&str]) -> Value {
    let output = atelier()
        .env("CLAUDE_CONFIG_DIR", config)
        .arg("kanban")
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("valid JSON on stdout")
}

pub fn record_path(config: &Path, id: &str) -> PathBuf {
    let out = kanban_in(config, &["record-path", "--session", id]);
    PathBuf::from(out["path"].as_str().expect("record-path names a path"))
}

/// Leaves `contents` where the mod leaves the session's record copy.
pub fn put_copy(config: &Path, id: &str, contents: &str) {
    let path = record_path(config, id);
    std::fs::create_dir_all(path.parent().expect("copy has a directory")).expect("make dir");
    std::fs::write(path, contents).expect("write copy");
}

pub fn put_record(config: &Path, record: &Value) {
    let id = record["session_id"].as_str().expect("id");
    put_copy(config, id, &record.to_string());
}

pub fn status_in(config: &Path, args: &[&str]) -> Value {
    let mut full = vec!["status"];
    full.extend_from_slice(args);
    kanban_in(config, &full)
}

pub const REPO: &str = "/work/project-a";

/// The UTF-16 length the mod records, as JavaScript's `.length` counts.
pub fn js_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// A session at `repo` waiting on one permission request for `tool`, its
/// input recorded whole with the length facts the mod writes.
pub fn asking_permission_at(repo: &str, cwd: &str, tool: &str, input: Value) -> Value {
    let lengths: serde_json::Map<String, Value> = input
        .as_object()
        .expect("input fields")
        .iter()
        .map(|(k, v)| (k.clone(), json!(js_len(v.as_str().expect("string field")))))
        .collect();
    session_with(
        "w",
        json!([event("classic.PermissionRequest", Some(tool), 10 * SEC)]),
        json!({
            "cwd": cwd,
            "repo_root": repo,
            "permission_mode": "default",
            "permission": {
                "id": "t-1-7",
                "tool": tool,
                "input": input,
                "input_len": lengths,
                "at": NOW - 10 * SEC,
            },
        }),
    )
}

pub fn asking_permission(tool: &str, input: Value) -> Value {
    asking_permission_at(REPO, REPO, tool, input)
}

pub fn asking_bash(command: &str) -> Value {
    asking_permission("Bash", json!({ "command": command }))
}

/// The `wait` of the only session on the board.
pub fn wait_of(record: Value) -> Value {
    let b = board(json!([record]));
    b["repos"][0]["sessions"][0]["wait"].clone()
}

pub fn blocked(wait: &Value) -> Vec<String> {
    wait["blocked"]
        .as_array()
        .expect("blocked list")
        .iter()
        .map(|r| r.as_str().expect("reason").to_string())
        .collect()
}

pub fn question(text: &str, labels: &[&str], multi: bool) -> Value {
    json!({
        "question": text,
        "header": "Pick",
        "multi_select": multi,
        "options": labels
            .iter()
            .map(|l| json!({ "label": l, "description": null }))
            .collect::<Vec<_>>(),
    })
}

/// The lengths the mod records for `questions` when nothing was cut.
pub fn question_lengths(questions: &Value) -> Value {
    let list = questions.as_array().expect("questions");
    let labels = |q: &Value| -> Vec<usize> {
        q["options"]
            .as_array()
            .expect("options")
            .iter()
            .map(|o| js_len(o["label"].as_str().expect("label")))
            .collect()
    };
    json!({
        "question": list
            .iter()
            .map(|q| js_len(q["question"].as_str().expect("text")))
            .collect::<Vec<_>>(),
        "labels": list.iter().map(labels).collect::<Vec<_>>(),
    })
}

/// A session at `REPO` waiting on one AskUserQuestion call with these
/// recorded questions and length facts.
pub fn asking_with_lengths(questions: Value, orig_len: Value) -> Value {
    session_with(
        "w",
        json!([event(
            "classic.PermissionRequest",
            Some("AskUserQuestion"),
            10 * SEC
        )]),
        json!({
            "cwd": REPO,
            "repo_root": REPO,
            "asks": { "tu-1": { "questions": questions, "at": NOW - 10 * SEC, "orig_len": orig_len } },
        }),
    )
}

pub fn asking(questions: Value) -> Value {
    let orig_len = question_lengths(&questions);
    asking_with_lengths(questions, orig_len)
}
