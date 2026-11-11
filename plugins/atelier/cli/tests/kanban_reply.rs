use assert_cmd::Command;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tempfile::TempDir;

mod kanban_support;
use kanban_support::now_ms;

struct World {
    config: TempDir,
}

impl World {
    fn new() -> World {
        World {
            config: TempDir::new().expect("config dir"),
        }
    }

    fn run(&self, args: &[&str], stdin: &str) -> Value {
        let output = Command::cargo_bin("atelier")
            .expect("locate `atelier` cargo binary")
            .arg("kanban")
            .args(args)
            .env("CLAUDE_CONFIG_DIR", self.config.path())
            .write_stdin(stdin)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        serde_json::from_slice(&output).expect("valid JSON on stdout")
    }

    fn kanban(&self, args: &[&str]) -> Value {
        self.run(args, "")
    }

    fn inbox(&self, session: &str) -> PathBuf {
        self.config
            .path()
            .join("atelier/kanban/inbox")
            .join(session)
    }

    fn take(&self, session: &str, turn: Option<&str>, open: Value) -> Value {
        let now = now_ms().to_string();
        let mut args = vec![
            "inbox",
            "take",
            "--session",
            session,
            "--now",
            &now,
            "--ttl",
            "60s",
            "--waits-stdin",
        ];
        if let Some(turn) = turn {
            args.extend(["--turn", turn]);
        }
        self.run(&args, &open.to_string())
    }

    fn wait_permit(&self, session: &str, request: &str) -> Value {
        self.wait_permit_with(session, request, None)
    }

    fn wait_permit_with(&self, session: &str, request: &str, nonces: Option<Value>) -> Value {
        let now = now_ms().to_string();
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
            "60s",
            "--for",
            "0ms",
        ];
        let stdin = nonces.map_or(String::new(), |nonces| {
            args.push("--nonces-stdin");
            json!({ "nonces": nonces }).to_string()
        });
        self.run(&args, &stdin)
    }
}

fn open_ask(id: &str, questions: usize) -> Value {
    json!({
        "asks": [{ "id": id, "questions": vec![json!({ "question": "Which?" }); questions] }],
        "decision": null,
    })
}

fn open_decision(at: u64) -> Value {
    json!({ "asks": [], "decision": { "at": at, "ask": "진행할까요?", "options": ["네"] } })
}

fn with_nonces(mut open: Value, nonces: Value) -> Value {
    open["nonces"] = nonces;
    open
}

fn left(kind: &str, nonce: &str) -> Value {
    json!({ "kind": kind, "nonce": nonce })
}

#[test]
fn a_pane_reply_reaches_the_open_question_it_names() {
    let w = World::new();
    let out = w.kanban(&[
        "reply",
        "--session",
        "s-1",
        "--ask",
        "tu-1",
        "--answer",
        "Blue",
        "--answer",
        "Cat, Fish",
        "--from",
        "pane",
    ]);
    assert_eq!(out["queued"]["kind"], "answer");
    assert_eq!(out["queued"]["session_id"], "s-1");

    let taken = w.take("s-1", Some("t-1"), open_ask("tu-1", 2));
    assert_eq!(
        taken["actions"],
        json!([{ "kind": "answer", "tool_use_id": "tu-1", "answers": ["Blue", "Cat, Fish"], "from": "pane", "via": "terminal" }])
    );
    assert_eq!(taken["results"][0]["kind"], "answer");
    assert_eq!(taken["results"][0]["status"], "delivered");
}

#[test]
fn an_answer_to_a_question_no_longer_open_is_stale() {
    let w = World::new();
    w.kanban(&[
        "reply",
        "--session",
        "s-1",
        "--ask",
        "tu-1",
        "--answer",
        "Blue",
        "--from",
        "pane",
    ]);
    let taken = w.take("s-1", None, open_ask("tu-2", 1));
    assert_eq!(taken["actions"], json!([]));
    assert_eq!(taken["results"][0]["status"], "stale");
}

#[test]
fn an_answer_whose_count_differs_from_the_questions_is_invalid() {
    let w = World::new();
    w.kanban(&[
        "reply",
        "--session",
        "s-1",
        "--ask",
        "tu-1",
        "--answer",
        "Blue",
        "--from",
        "pane",
    ]);
    let taken = w.take("s-1", None, open_ask("tu-1", 2));
    assert_eq!(taken["actions"], json!([]));
    assert_eq!(taken["results"][0]["status"], "invalid");
}

#[test]
fn a_take_without_waits_stdin_leaves_stdin_unread_and_finds_every_answer_stale() {
    let w = World::new();
    w.kanban(&[
        "reply",
        "--session",
        "s-1",
        "--ask",
        "tu-1",
        "--answer",
        "Blue",
        "--from",
        "pane",
    ]);
    let taken = w.run(
        &[
            "inbox",
            "take",
            "--session",
            "s-1",
            "--now",
            &now_ms().to_string(),
            "--ttl",
            "60s",
        ],
        &open_ask("tu-1", 1).to_string(),
    );
    assert_eq!(taken["results"][0]["status"], "stale");
}

#[test]
fn a_take_without_waits_stdin_ends_while_stdin_stays_open() {
    let w = World::new();
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("atelier"))
        .args([
            "kanban",
            "inbox",
            "take",
            "--session",
            "s-1",
            "--now",
            &now_ms().to_string(),
            "--ttl",
            "60s",
        ])
        .env("CLAUDE_CONFIG_DIR", w.config.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn atelier");
    let stdin = child.stdin.take();
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll atelier") {
            break Some(status);
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    drop(stdin);
    assert!(status.is_some_and(|s| s.success()), "take waited on stdin");
}

#[test]
fn a_pane_decide_answers_the_decision_it_names_as_typed() {
    let w = World::new();
    let out = w.kanban(&[
        "decide",
        "--session",
        "s-1",
        "--at",
        "1700",
        "--text",
        "초안대로 진행",
        "--from",
        "pane",
    ]);
    assert_eq!(out["queued"]["kind"], "decide");

    let taken = w.take("s-1", None, open_decision(1700));
    assert_eq!(
        taken["actions"],
        json!([{ "kind": "decide", "decision_at": "1700", "text": "초안대로 진행", "from": "pane", "via": "terminal" }])
    );
    assert_eq!(taken["results"][0]["status"], "delivered");
}

#[test]
fn a_decide_for_another_decision_time_is_stale() {
    let w = World::new();
    w.kanban(&[
        "decide",
        "--session",
        "s-1",
        "--at",
        "1700",
        "--text",
        "네",
        "--from",
        "pane",
    ]);
    let taken = w.take("s-1", None, open_decision(1800));
    assert_eq!(taken["actions"], json!([]));
    assert_eq!(taken["results"][0]["status"], "stale");
}

#[test]
fn a_decide_arriving_while_a_turn_runs_is_busy() {
    let w = World::new();
    w.kanban(&[
        "decide",
        "--session",
        "s-1",
        "--at",
        "1700",
        "--text",
        "네",
        "--from",
        "pane",
    ]);
    let taken = w.take("s-1", Some("t-1"), open_decision(1700));
    assert_eq!(taken["actions"], json!([]));
    assert_eq!(taken["results"][0]["status"], "busy");
}

#[test]
fn agent_answers_are_refused_until_the_ledger_exists() {
    let w = World::new();
    for args in [
        vec![
            "reply",
            "--session",
            "s-1",
            "--ask",
            "tu-1",
            "--answer",
            "Blue",
            "--from",
            "agent",
        ],
        vec![
            "permit",
            "--session",
            "s-1",
            "--request",
            "p-1",
            "--decision",
            "allow",
            "--from",
            "agent",
        ],
        vec![
            "decide",
            "--session",
            "s-1",
            "--at",
            "1700",
            "--text",
            "네",
            "--from",
            "agent",
        ],
    ] {
        let out = w.kanban(&args);
        let error = out["error"].as_str().unwrap_or_default();
        assert!(error.contains("장부가 필요해요"), "{args:?} → {out}");
    }
    assert!(!w.config.path().join("atelier").exists());
}

#[test]
fn an_agent_answer_left_in_the_inbox_is_refused_by_the_session_that_takes_it() {
    let w = World::new();
    fs::create_dir_all(w.inbox("s-1")).expect("inbox");
    fs::write(
        w.inbox("s-1").join("1-1.json"),
        json!({ "v": 1, "kind": "answer", "tool_use_id": "tu-1", "answers": ["Blue"], "from": "agent" })
            .to_string(),
    )
    .expect("write");
    fs::write(
        w.inbox("s-1").join("1-2.json"),
        json!({ "v": 1, "kind": "decide", "decision_at": "1700", "text": "네", "from": "agent" })
            .to_string(),
    )
    .expect("write");
    let open = json!({ "asks": [{ "id": "tu-1", "questions": [{}] }], "decision": { "at": 1700 } });
    let taken = w.take("s-1", None, open);
    assert_eq!(taken["actions"], json!([]));
    assert_eq!(taken["results"][0]["status"], "refused");
    assert_eq!(taken["results"][1]["status"], "refused");
}

#[test]
fn a_pane_permit_is_taken_with_where_it_came_from() {
    let w = World::new();
    w.kanban(&[
        "permit",
        "--session",
        "s-1",
        "--request",
        "p-1",
        "--decision",
        "allow",
        "--from",
        "pane",
    ]);
    assert_eq!(
        w.wait_permit("s-1", "p-1"),
        json!({ "decision": "allow", "from": "pane", "via": "terminal", "used_nonces": [] })
    );
}

#[test]
fn a_permit_written_before_senders_were_named_is_read_as_the_pane() {
    let w = World::new();
    let permits = w.inbox("s-1").join("permits");
    fs::create_dir_all(&permits).expect("permits");
    fs::write(
        permits.join("p-1.json"),
        json!({ "v": 1, "kind": "permit", "request_id": "p-1", "decision": "deny" }).to_string(),
    )
    .expect("write");
    assert_eq!(
        w.wait_permit("s-1", "p-1"),
        json!({ "decision": "deny", "from": "pane", "via": "terminal", "used_nonces": [] })
    );
}

#[test]
fn an_agent_permit_left_in_the_inbox_is_refused_by_the_waiting_session() {
    let w = World::new();
    let permits = w.inbox("s-1").join("permits");
    fs::create_dir_all(&permits).expect("permits");
    fs::write(
        permits.join("p-1.json"),
        json!({ "v": 1, "kind": "permit", "request_id": "p-1", "decision": "allow", "from": "agent" })
            .to_string(),
    )
    .expect("write");
    assert_eq!(
        w.wait_permit("s-1", "p-1"),
        json!({ "decision": null, "status": "refused", "via": "agent" })
    );
}

#[test]
fn answers_that_cannot_be_read_are_refused_before_anything_is_written() {
    let w = World::new();
    for args in [
        vec![
            "reply",
            "--session",
            "s-1",
            "--ask",
            "tu-1",
            "--from",
            "pane",
        ],
        vec![
            "reply",
            "--session",
            "s-1",
            "--ask",
            "tu-1",
            "--answer",
            "  ",
            "--from",
            "pane",
        ],
        vec![
            "reply",
            "--session",
            "s-1",
            "--ask",
            "../x",
            "--answer",
            "a",
            "--from",
            "pane",
        ],
        vec![
            "decide",
            "--session",
            "s-1",
            "--at",
            "17 00",
            "--text",
            "네",
            "--from",
            "pane",
        ],
        vec![
            "decide",
            "--session",
            "s-1",
            "--at",
            "1700",
            "--text",
            " ",
            "--from",
            "pane",
        ],
    ] {
        let out = w.kanban(&args);
        assert!(out["error"].is_string(), "{args:?} → {out}");
    }
    assert!(!w.config.path().join("atelier").exists());
}

#[test]
fn an_answer_may_start_with_a_dash() {
    let w = World::new();
    w.kanban(&[
        "reply",
        "--session",
        "s-1",
        "--ask",
        "tu-1",
        "--answer",
        "-1",
        "--from",
        "pane",
    ]);
    let taken = w.take("s-1", None, open_ask("tu-1", 1));
    assert_eq!(taken["actions"][0]["answers"], json!(["-1"]));
}

#[test]
fn unreadable_open_waits_are_refused_and_the_inbox_is_left_alone() {
    let w = World::new();
    w.kanban(&[
        "reply",
        "--session",
        "s-1",
        "--ask",
        "tu-1",
        "--answer",
        "Blue",
        "--from",
        "pane",
    ]);
    let out = w.run(
        &[
            "inbox",
            "take",
            "--session",
            "s-1",
            "--now",
            &now_ms().to_string(),
            "--ttl",
            "60s",
            "--waits-stdin",
        ],
        "not json",
    );
    assert!(out["error"].is_string(), "{out}");
    assert_eq!(fs::read_dir(w.inbox("s-1")).expect("inbox").count(), 1);
}

fn reply_with_nonce(w: &World, ask: &str, nonce: &str) {
    w.kanban(&[
        "reply",
        "--session",
        "s-1",
        "--ask",
        ask,
        "--answer",
        "Blue",
        "--from",
        "pane",
        "--nonce",
        nonce,
    ]);
}

#[test]
fn a_pane_reply_with_the_nonce_left_for_it_comes_through_the_pane() {
    let w = World::new();
    reply_with_nonce(&w, "tu-1", "n-1");
    let taken = w.take(
        "s-1",
        None,
        with_nonces(open_ask("tu-1", 1), json!([left("answer", "n-1")])),
    );
    assert_eq!(
        taken["actions"],
        json!([{ "kind": "answer", "tool_use_id": "tu-1", "answers": ["Blue"], "from": "pane", "via": "pane" }])
    );
    assert_eq!(taken["results"][0]["via"], "pane");
    assert_eq!(taken["used_nonces"], json!([left("answer", "n-1")]));
}

#[test]
fn a_pane_reply_without_a_nonce_comes_through_the_terminal() {
    let w = World::new();
    w.kanban(&[
        "reply",
        "--session",
        "s-1",
        "--ask",
        "tu-1",
        "--answer",
        "Blue",
        "--from",
        "pane",
    ]);
    let taken = w.take(
        "s-1",
        None,
        with_nonces(open_ask("tu-1", 1), json!([left("answer", "n-1")])),
    );
    assert_eq!(taken["actions"][0]["via"], "terminal");
    assert_eq!(taken["used_nonces"], json!([]));
}

#[test]
fn a_nonce_left_for_another_kind_or_a_different_nonce_comes_through_the_terminal() {
    let w = World::new();
    reply_with_nonce(&w, "tu-1", "n-1");
    let taken = w.take(
        "s-1",
        None,
        with_nonces(
            open_ask("tu-1", 1),
            json!([
                left("decide", "n-1"),
                left("permit", "n-1"),
                left("answer", "n-2")
            ]),
        ),
    );
    assert_eq!(taken["actions"][0]["via"], "terminal");
    assert_eq!(taken["used_nonces"], json!([]));
}

#[test]
fn nonce_entries_that_do_not_read_are_skipped() {
    let w = World::new();
    reply_with_nonce(&w, "tu-1", "n-1");
    let taken = w.take(
        "s-1",
        None,
        with_nonces(
            open_ask("tu-1", 1),
            json!([
                { "kind": "answer", "nonce": 7 },
                { "kind": "reply", "nonce": "n-1" },
                "n-1",
                left("answer", "n-1")
            ]),
        ),
    );
    assert_eq!(taken["actions"][0]["via"], "pane");
}

#[test]
fn the_same_nonce_counts_as_the_pane_only_once_in_one_take() {
    let w = World::new();
    reply_with_nonce(&w, "tu-1", "n-1");
    reply_with_nonce(&w, "tu-2", "n-1");
    let open = json!({
        "asks": [{ "id": "tu-1", "questions": [{}] }, { "id": "tu-2", "questions": [{}] }],
        "decision": null,
        "nonces": [left("answer", "n-1")],
    });
    let taken = w.take("s-1", None, open);
    let vias: Vec<&Value> = taken["actions"]
        .as_array()
        .expect("actions")
        .iter()
        .map(|a| &a["via"])
        .collect();
    assert_eq!(vias, [&json!("pane"), &json!("terminal")]);
    assert_eq!(taken["used_nonces"], json!([left("answer", "n-1")]));
}

#[test]
fn a_nonce_the_session_already_used_up_comes_through_the_terminal() {
    let w = World::new();
    reply_with_nonce(&w, "tu-1", "n-1");
    let first = w.take(
        "s-1",
        None,
        with_nonces(open_ask("tu-1", 1), json!([left("answer", "n-1")])),
    );
    assert_eq!(first["actions"][0]["via"], "pane");

    reply_with_nonce(&w, "tu-2", "n-1");
    let second = w.take("s-1", None, with_nonces(open_ask("tu-2", 1), json!([])));
    assert_eq!(second["actions"][0]["via"], "terminal");
}

#[test]
fn an_agent_answer_with_the_nonce_left_for_the_pane_stays_the_agents() {
    let w = World::new();
    fs::create_dir_all(w.inbox("s-1").join("permits")).expect("inbox");
    fs::write(
        w.inbox("s-1").join("1-1.json"),
        json!({ "v": 1, "kind": "answer", "tool_use_id": "tu-1", "answers": ["Blue"], "from": "agent", "nonce": "n-1" })
            .to_string(),
    )
    .expect("write");
    fs::write(
        w.inbox("s-1").join("1-2.json"),
        json!({ "v": 1, "kind": "decide", "decision_at": "1700", "text": "네", "from": "agent", "nonce": "n-2" })
            .to_string(),
    )
    .expect("write");
    fs::write(
        w.inbox("s-1").join("permits/p-1.json"),
        json!({ "v": 1, "kind": "permit", "request_id": "p-1", "decision": "allow", "from": "agent", "nonce": "n-3" })
            .to_string(),
    )
    .expect("write");
    let open = json!({
        "asks": [{ "id": "tu-1", "questions": [{}] }],
        "decision": { "at": 1700 },
        "nonces": [left("answer", "n-1"), left("decide", "n-2")],
    });
    let taken = w.take("s-1", None, open);
    assert_eq!(taken["actions"], json!([]));
    assert_eq!(taken["results"][0]["via"], "agent");
    assert_eq!(taken["results"][1]["via"], "agent");
    assert_eq!(taken["used_nonces"], json!([]));
    assert_eq!(
        w.wait_permit_with("s-1", "p-1", Some(json!([left("permit", "n-3")]))),
        json!({ "decision": null, "status": "refused", "via": "agent" })
    );
}

#[test]
fn a_pane_decide_with_the_nonce_left_for_it_comes_through_the_pane() {
    let w = World::new();
    w.kanban(&[
        "decide",
        "--session",
        "s-1",
        "--at",
        "1700",
        "--text",
        "네",
        "--from",
        "pane",
        "--nonce",
        "n-2",
    ]);
    let taken = w.take(
        "s-1",
        None,
        with_nonces(open_decision(1700), json!([left("decide", "n-2")])),
    );
    assert_eq!(
        taken["actions"],
        json!([{ "kind": "decide", "decision_at": "1700", "text": "네", "from": "pane", "via": "pane" }])
    );
    assert_eq!(taken["used_nonces"], json!([left("decide", "n-2")]));
}

fn permit_with_nonce(w: &World, request: &str, nonce: &str) {
    w.kanban(&[
        "permit",
        "--session",
        "s-1",
        "--request",
        request,
        "--decision",
        "allow",
        "--from",
        "pane",
        "--nonce",
        nonce,
    ]);
}

#[test]
fn a_pane_permit_with_the_nonce_left_for_it_comes_through_the_pane() {
    let w = World::new();
    permit_with_nonce(&w, "p-1", "n-3");
    assert_eq!(
        w.wait_permit_with("s-1", "p-1", Some(json!([left("permit", "n-3")]))),
        json!({ "decision": "allow", "from": "pane", "via": "pane", "used_nonces": [left("permit", "n-3")] })
    );
}

#[test]
fn a_permit_wait_without_nonces_stdin_finds_every_permit_through_the_terminal() {
    let w = World::new();
    permit_with_nonce(&w, "p-1", "n-3");
    assert_eq!(
        w.wait_permit("s-1", "p-1"),
        json!({ "decision": "allow", "from": "pane", "via": "terminal", "used_nonces": [] })
    );
}

#[test]
fn unreadable_nonce_facts_are_refused_and_the_inbox_is_left_alone() {
    let w = World::new();
    reply_with_nonce(&w, "tu-1", "n-1");
    let out = w.take("s-1", None, with_nonces(open_ask("tu-1", 1), json!("n-1")));
    assert!(out["error"].is_string(), "{out}");
    assert_eq!(fs::read_dir(w.inbox("s-1")).expect("inbox").count(), 1);

    permit_with_nonce(&w, "p-1", "n-3");
    let out = w.wait_permit_with("s-1", "p-1", Some(json!({ "kind": "permit" })));
    assert!(out["error"].is_string(), "{out}");
    assert!(w.inbox("s-1").join("permits/p-1.json").exists());
}

#[test]
fn a_nonce_that_is_not_an_id_is_refused_before_anything_is_written() {
    let w = World::new();
    for args in [
        vec![
            "reply",
            "--session",
            "s-1",
            "--ask",
            "tu-1",
            "--answer",
            "a",
            "--from",
            "pane",
            "--nonce",
            "a b",
        ],
        vec![
            "permit",
            "--session",
            "s-1",
            "--request",
            "p-1",
            "--decision",
            "allow",
            "--from",
            "pane",
            "--nonce",
            "",
        ],
        vec![
            "decide",
            "--session",
            "s-1",
            "--at",
            "1700",
            "--text",
            "네",
            "--from",
            "pane",
            "--nonce",
            "../n",
        ],
    ] {
        let out = w.kanban(&args);
        assert!(out["error"].is_string(), "{args:?} → {out}");
    }
    assert!(!w.config.path().join("atelier").exists());
}

#[test]
fn an_answer_left_in_the_inbox_with_an_unreadable_nonce_is_invalid() {
    let w = World::new();
    fs::create_dir_all(w.inbox("s-1")).expect("inbox");
    fs::write(
        w.inbox("s-1").join("1-1.json"),
        json!({ "v": 1, "kind": "answer", "tool_use_id": "tu-1", "answers": ["Blue"], "from": "pane", "nonce": 7 })
            .to_string(),
    )
    .expect("write");
    let taken = w.take("s-1", None, open_ask("tu-1", 1));
    assert_eq!(taken["actions"], json!([]));
    assert_eq!(taken["results"][0]["status"], "invalid");
}
