//! Shared builders for the `kanban` black-box tests.
//!
//! `#![allow(dead_code)]` because each test binary uses only part of them
//! (Cargo compiles this module into every test crate that declares it).
#![allow(dead_code)]

use assert_cmd::Command;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

pub const NOW: u64 = 1_800_000_000_000;
pub const SEC: u64 = 1_000;
pub const MIN: u64 = 60 * SEC;
pub const HOUR: u64 = 60 * MIN;
pub const DAY: u64 = 24 * HOUR;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_millis() as u64
}

pub fn atelier() -> Command {
    Command::cargo_bin("atelier").expect("locate `atelier` cargo binary")
}

pub fn stdout_json(command: &mut Command) -> Value {
    let output = command.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).expect("valid JSON on stdout")
}

/// Implements `Deref<Target = Home>` for a test world holding a `home` field.
#[allow(unused_macros)]
macro_rules! home_deref {
    ($world:ty) => {
        impl std::ops::Deref for $world {
            type Target = $crate::kanban_support::Home;

            fn deref(&self) -> &Self::Target {
                &self.home
            }
        }
    };
}
#[allow(unused_imports)]
pub(crate) use home_deref;

pub fn kanban_command(config: &Path) -> Command {
    let mut command = atelier();
    command.env("CLAUDE_CONFIG_DIR", config).arg("kanban");
    command
}

pub fn spawn_kanban(config: &Path, args: &[&str]) -> std::process::Child {
    std::process::Command::new(assert_cmd::cargo::cargo_bin("atelier"))
        .env("CLAUDE_CONFIG_DIR", config)
        .arg("kanban")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn atelier")
}

pub fn refused(out: &Value) -> bool {
    out["error"].is_string()
}

pub fn with(mut base: Value, extra: Value) -> Value {
    for (key, value) in extra.as_object().expect("fields") {
        base[key] = value.clone();
    }
    base
}

pub fn ledger_dir(config: &Path) -> PathBuf {
    config.join("atelier/kanban/ledger")
}

pub fn run_record(config: &Path, facts: &Value) -> Value {
    stdout_json(
        kanban_command(config)
            .args(["ledger", "record"])
            .write_stdin(facts.to_string()),
    )
}

pub fn record_facts(config: &Path, facts: &Value) -> Vec<String> {
    let out = run_record(config, facts);
    out["recorded"]
        .as_array()
        .unwrap_or_else(|| panic!("recorded ids, got {out}"))
        .iter()
        .map(|id| id.as_str().expect("id").to_string())
        .collect()
}

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

    pub fn command(&self) -> Command {
        kanban_command(self.config())
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

    pub fn ledger_dir(&self) -> PathBuf {
        ledger_dir(self.config())
    }

    pub fn leave_raw(&self, session: &str, file: &str, contents: &str) {
        let path = self.inbox(session).join(file);
        fs::create_dir_all(path.parent().expect("inbox file has a directory")).expect("inbox");
        fs::write(path, contents).expect("write");
    }

    pub fn leave(&self, session: &str, name: &str, request: &Value) {
        self.leave_raw(session, &format!("{name}.json"), &request.to_string());
    }

    pub fn leave_permit(&self, session: &str, request_id: &str, request: &Value) {
        self.leave_raw(
            session,
            &format!("permits/{request_id}.json"),
            &request.to_string(),
        );
    }

    pub fn ledger_match(&self, key: &str, at: u64) -> Value {
        self.kanban(&["ledger", "match", "--key", key, "--now", &at.to_string()])
    }

    pub fn status_at(&self, at: u64, extra: &[&str]) -> Value {
        let mut args = vec!["status", "--now"];
        let now = at.to_string();
        args.push(&now);
        args.extend(extra);
        self.kanban(&args)
    }

    pub fn record_all(&self, facts: Value) -> Vec<String> {
        record_facts(self.config(), &facts)
    }

    pub fn record(&self, facts: Value) -> String {
        let ids = self.record_all(facts);
        assert_eq!(ids.len(), 1, "one entry: {ids:?}");
        ids[0].clone()
    }

    pub fn entry(&self, id: &str) -> Value {
        let out = self.kanban(&["ledger", "list", "--limit", "500"]);
        out["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .find(|e| e["id"] == id)
            .unwrap_or_else(|| panic!("entry {id} in {out}"))
            .clone()
    }

    pub fn rewrite_lines(&self, change: impl Fn(Value) -> Value) {
        for file in fs::read_dir(self.ledger_dir()).expect("ledger dir") {
            let path = file.expect("file").path();
            if path.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            let lines: Vec<String> = fs::read_to_string(&path)
                .expect("read")
                .lines()
                .map(|line| change(serde_json::from_str(line).expect("line")).to_string())
                .collect();
            fs::write(&path, lines.join("\n") + "\n").expect("write");
        }
    }

    pub fn add_ledger_line(&self, line: Value) {
        fs::create_dir_all(self.ledger_dir()).expect("ledger dir");
        let path = self.ledger_dir().join("2026-01.jsonl");
        let mut contents = fs::read_to_string(&path).unwrap_or_default();
        contents.push_str(&format!("{line}\n"));
        fs::write(path, contents).expect("write");
    }

    /// `inbox take` for `session` at `at`; with `waits` the open waits go in on stdin.
    pub fn take(
        &self,
        session: &str,
        at: u64,
        ttl: &str,
        turn: Option<&str>,
        waits: Option<&Value>,
    ) -> Value {
        let now = at.to_string();
        let mut args = vec![
            "inbox",
            "take",
            "--session",
            session,
            "--now",
            &now,
            "--ttl",
            ttl,
        ];
        if let Some(turn) = turn {
            args.extend(["--turn", turn]);
        }
        match waits {
            Some(open) => {
                args.push("--waits-stdin");
                self.run(&args, &open.to_string())
            }
            None => self.kanban(&args),
        }
    }

    /// `inbox wait-permit` that does not wait; `facts` is the flag that takes the stdin and what goes in.
    pub fn wait_permit(
        &self,
        session: &str,
        request: &str,
        at: u64,
        ttl: &str,
        facts: Option<(&str, &Value)>,
    ) -> Value {
        let now = at.to_string();
        let mut args = vec![
            "inbox",
            "wait-permit",
            "--session",
            session,
            "--request",
            request,
            "--now",
            &now,
            "--ttl",
            ttl,
            "--for",
            "0ms",
        ];
        match facts {
            Some((flag, stdin)) => {
                args.push(flag);
                self.run(&args, &stdin.to_string())
            }
            None => self.kanban(&args),
        }
    }
}

pub fn board_with(args: &[&str], stdin: &str) -> Value {
    stdout_json(
        atelier()
            .args(["kanban", "board"])
            .args(args)
            .write_stdin(stdin),
    )
}

pub fn board_args(width: &str, extra: &[&str]) -> Vec<String> {
    let mut args: Vec<String> = [
        "--now",
        &NOW.to_string(),
        "--stale-after",
        "2m",
        "--expire-after",
        "24h",
        "--width",
        width,
    ]
    .map(String::from)
    .to_vec();
    args.extend(extra.iter().map(|arg| arg.to_string()));
    args
}

pub fn board_at(width: &str, extra: &[&str], stdin: &str) -> Value {
    let args = board_args(width, extra);
    board_with(&args.iter().map(String::as_str).collect::<Vec<_>>(), stdin)
}

pub fn board(sessions: Value) -> Value {
    board_at(
        "100",
        &["--self", "self-id"],
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
    with(session(id, events), extra)
}

pub fn kanban_in(config: &Path, args: &[&str]) -> Value {
    stdout_json(kanban_command(config).args(args))
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

pub fn color_and_size() -> Value {
    json!([
        question("Which color?", &["Red", "Blue"], false),
        question("Which size?", &["S", "M"], false),
    ])
}

pub fn color() -> Value {
    json!([question("Which color?", &["Red", "Blue"], false)])
}

/// A question dialog the person answered in the session: its answers in the
/// call's question order, and how the call ended.
pub fn chose(answers: Value) -> Value {
    json!({
        "path": "session",
        "answers": answers,
        "afk_timeout_ms": null,
        "is_error": false,
        "denied": false,
    })
}

pub fn place_json(repo: &str) -> Value {
    json!({ "repo_root": repo, "cwd": repo, "permission_mode": "default" })
}

pub fn place_at(at: u64, repo: &str) -> Value {
    with(place_json(repo), json!({ "at": at, "session": "w" }))
}

pub fn question_facts_with(tool_use_id: &str, at: u64, questions: Value, answer: Value) -> Value {
    let orig_len = question_lengths(&questions);
    with(
        place_at(at, REPO),
        json!({
            "question": { "tool_use_id": tool_use_id, "questions": questions, "at": at - 10 * SEC, "orig_len": orig_len },
            "answer": answer,
        }),
    )
}

pub fn question_facts(at: u64, questions: Value, answer: Value) -> Value {
    question_facts_with(&format!("tu-{at}"), at, questions, answer)
}

pub fn permission_facts_with(id: &str, repo: &str, at: u64, command: &str, answer: Value) -> Value {
    with(
        place_at(at, repo),
        json!({
            "permission": {
                "id": id,
                "tool": "Bash",
                "input": { "command": command },
                "input_len": { "command": js_len(command) },
                "at": at - 10 * SEC,
            },
            "answer": answer,
        }),
    )
}

pub fn permission_facts(repo: &str, at: u64, command: &str, answer: Value) -> Value {
    permission_facts_with(&format!("t-{at}"), repo, at, command, answer)
}

pub fn decision_facts(at: u64, answer: Value) -> Value {
    let ask = "초안대로 진행할까요?";
    let options = ["초안대로 진행", "다시 쓰기"];
    with(
        place_at(at, REPO),
        json!({
            "decision": {
                "at": at - 10 * SEC,
                "ask": ask,
                "recap": "초안을 정리했어요.",
                "options": options,
                "orig_len": { "ask": js_len(ask), "options": options.map(js_len) },
            },
            "answer": answer,
        }),
    )
}

pub fn typed_decision_from(text: &str, origin: &str) -> Value {
    json!({ "path": "session", "text": text, "origin": origin })
}

pub fn typed_decision(text: &str) -> Value {
    typed_decision_from(text, "composer")
}

pub fn inbox_answer(via: &str, label: &str) -> Value {
    json!({ "path": "inbox", "via": via, "answers": [label] })
}

pub fn inbox_verdict(via: &str, decision: &str) -> Value {
    json!({ "path": "inbox", "via": via, "decision": decision })
}

pub fn session_post(post: Value, is_error: bool) -> Value {
    json!({ "path": "session", "post": post, "is_error": is_error })
}

pub fn ran() -> Value {
    session_post(json!("PostToolUse"), false)
}

pub fn errored() -> Value {
    session_post(Value::Null, true)
}

pub fn decision_waiting_with(
    ask: &str,
    recap: Option<&str>,
    options: &[&str],
    orig_len: Value,
) -> Value {
    session_with(
        "w",
        json!([event("turn.complete", None, 10 * SEC)]),
        json!({
            "cwd": REPO,
            "repo_root": REPO,
            "decision": {
                "ask": ask,
                "recap": recap,
                "options": options,
                "at": NOW - 10 * SEC,
                "orig_len": orig_len,
            },
        }),
    )
}

pub fn decision_waiting(ask: &str, recap: Option<&str>, options: &[&str]) -> Value {
    let orig_len = json!({
        "ask": js_len(ask),
        "options": options.iter().map(|o| js_len(o)).collect::<Vec<_>>(),
    });
    decision_waiting_with(ask, recap, options, orig_len)
}
