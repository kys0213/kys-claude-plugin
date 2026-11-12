use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

mod kanban_support;
use kanban_support::{
    atelier, chose, color, decision_facts, kanban_in, ledger_dir, permission_facts_with, place_at,
    question, question_facts_with, record_facts, run_record, spawn_kanban, stdout_json, with, DAY,
    NOW, REPO,
};

fn list(config: &Path, args: &[&str]) -> Vec<Value> {
    let mut full = vec!["ledger", "list"];
    full.extend_from_slice(args);
    let out = kanban_in(config, &full);
    out["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("entries, got {out}"))
        .clone()
}

fn only(config: &Path) -> Value {
    let entries = list(config, &[]);
    assert_eq!(entries.len(), 1, "one entry: {entries:?}");
    entries[0].clone()
}

fn question_facts(at: u64, questions: Value, answer: Value) -> Value {
    question_facts_with("tu-1", at, questions, answer)
}

fn permission_facts(at: u64, command: &str, answer: Value) -> Value {
    permission_facts_with("t-1-7", REPO, at, command, answer)
}

fn source(entry: &Value) -> (Value, Value) {
    (entry["decided_by"].clone(), entry["via"].clone())
}

#[test]
fn a_question_call_that_ended_in_an_error_or_a_deny_leaves_nothing() {
    for ended in [json!({ "is_error": true }), json!({ "denied": true })] {
        let dir = TempDir::new().unwrap();
        let ids = record_facts(
            dir.path(),
            &question_facts(NOW, color(), with(chose(json!(["Red"])), ended.clone())),
        );

        assert!(ids.is_empty(), "{ended}");
        assert!(list(dir.path(), &[]).is_empty(), "{ended}");
    }
}

#[test]
fn a_dialog_answer_that_is_not_text_is_no_answer() {
    let dir = TempDir::new().unwrap();
    let questions = json!([
        question("Which color?", &["Red", "Blue"], false),
        question("Which size?", &["S", "M"], false),
    ]);
    let ids = record_facts(
        dir.path(),
        &question_facts(NOW, questions, chose(json!([{ "picked": "Red" }, "M"]))),
    );

    assert_eq!(ids.len(), 1);
    assert_eq!(only(dir.path())["q_index"], 1);
}

#[test]
fn only_a_numeric_afk_timeout_marks_the_dialog_as_resolved_while_away() {
    for (afk, decided_by) in [
        (json!(60_000), Value::Null),
        (Value::Null, json!("human")),
        (json!("soon"), json!("human")),
    ] {
        let dir = TempDir::new().unwrap();
        record_facts(
            dir.path(),
            &question_facts(
                NOW,
                color(),
                with(chose(json!(["Red"])), json!({ "afk_timeout_ms": afk })),
            ),
        );

        assert_eq!(only(dir.path())["decided_by"], decided_by, "{afk}");
    }
}

#[test]
fn entries_for_different_waits_at_the_same_moment_get_different_ids() {
    let dir = TempDir::new().unwrap();
    let ran = json!({ "path": "session", "post": "PostToolUse", "is_error": false });
    let first = record_facts(
        dir.path(),
        &permission_facts(NOW, "git status", ran.clone()),
    );
    let second = record_facts(dir.path(), &permission_facts(NOW, "git diff", ran));

    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_ne!(first[0], second[0]);
}

fn bind(stdin: &Value) -> Value {
    stdout_json(
        atelier()
            .args(["kanban", "ledger", "bind"])
            .write_stdin(stdin.to_string()),
    )
}

fn open_call(id: &str, tool: &str, permission: bool) -> Value {
    json!({ "tool_use_id": id, "tool": tool, "permission": permission })
}

#[test]
fn a_permission_request_binds_to_the_one_call_of_its_tool_without_one() {
    let out = bind(&json!({
        "tool": "Bash",
        "calls": [
            open_call("tu-1", "Bash", true),
            open_call("tu-2", "Bash", false),
            open_call("tu-3", "Read", false),
        ],
    }));

    assert_eq!(out, json!({ "call": "tu-2" }));
}

#[test]
fn a_permission_request_with_two_candidate_calls_or_none_binds_to_nothing() {
    for calls in [
        json!([
            open_call("tu-1", "Bash", false),
            open_call("tu-2", "Bash", false)
        ]),
        json!([
            open_call("tu-1", "Bash", true),
            open_call("tu-2", "Read", false)
        ]),
        json!([]),
    ] {
        let out = bind(&json!({ "tool": "Bash", "calls": calls }));

        assert_eq!(out, json!({ "call": null }), "{calls}");
    }
}

#[test]
fn bind_refuses_facts_it_cannot_read() {
    for stdin in [
        json!({}),
        json!({ "tool": "Bash" }),
        json!({ "tool": "Bash", "calls": [{ "tool_use_id": "tu-1", "tool": "Bash" }] }),
    ] {
        assert!(bind(&stdin)["error"].is_string(), "{stdin}");
    }
}

#[test]
fn a_question_answered_in_its_own_dialog_is_a_persons_decision() {
    let dir = TempDir::new().unwrap();
    record_facts(
        dir.path(),
        &question_facts(NOW, color(), chose(json!(["Red"]))),
    );

    let entry = only(dir.path());
    assert_eq!(source(&entry), (json!("human"), json!("session")));
    assert_eq!(entry["kind"], "question");
    assert_eq!(entry["answer"], "Red");
    assert_eq!(entry["inferred"], false);
    assert_eq!(entry["free_text"], false);
    assert_eq!(entry["wait_id"], "tu-1");
    assert_eq!(entry["session"], "w");
    assert_eq!(entry["repo"], REPO);
    assert_eq!(entry["outcome"], "used");
}

#[test]
fn a_dialog_that_resolved_itself_while_the_person_was_away_is_no_persons_decision() {
    let dir = TempDir::new().unwrap();
    record_facts(
        dir.path(),
        &question_facts(
            NOW,
            color(),
            with(chose(json!(["Red"])), json!({ "afk_timeout_ms": 60_000 })),
        ),
    );

    assert_eq!(source(&only(dir.path())), (Value::Null, json!("session")));
}

#[test]
fn a_typed_answer_outside_the_options_is_free_text() {
    let dir = TempDir::new().unwrap();
    record_facts(
        dir.path(),
        &question_facts(NOW, color(), chose(json!(["Green"]))),
    );

    assert_eq!(only(dir.path())["free_text"], true);
}

#[test]
fn an_inbox_answer_is_judged_by_the_way_the_cli_said_it_came() {
    for (via, decided_by) in [
        ("pane", json!("human")),
        ("terminal", Value::Null),
        ("agent", json!("proxy")),
    ] {
        let dir = TempDir::new().unwrap();
        record_facts(
            dir.path(),
            &question_facts(
                NOW,
                color(),
                json!({ "path": "inbox", "via": via, "answers": ["Blue"] }),
            ),
        );
        record_facts(
            dir.path(),
            &permission_facts(
                NOW + 1,
                "git status",
                json!({ "path": "inbox", "via": via, "decision": "allow" }),
            ),
        );
        record_facts(
            dir.path(),
            &decision_facts(
                NOW + 2,
                json!({ "path": "inbox", "via": via, "text": "초안대로 진행" }),
            ),
        );

        let entries = list(dir.path(), &[]);
        assert_eq!(entries.len(), 3, "{via}");
        for entry in entries {
            assert_eq!(source(&entry), (decided_by.clone(), json!(via)), "{via}");
        }
    }
}

#[test]
fn a_way_the_cli_did_not_name_proves_nothing() {
    let dir = TempDir::new().unwrap();
    record_facts(
        dir.path(),
        &question_facts(
            NOW,
            color(),
            json!({ "path": "inbox", "via": "somewhere", "answers": ["Blue"] }),
        ),
    );

    assert_eq!(source(&only(dir.path())), (Value::Null, json!("terminal")));
}

#[test]
fn who_decided_is_never_taken_from_the_facts() {
    let dir = TempDir::new().unwrap();
    let mut facts = question_facts(
        NOW,
        color(),
        json!({ "path": "inbox", "via": "terminal", "answers": ["Blue"], "decided_by": "human" }),
    );
    facts["decided_by"] = json!("human");
    record_facts(dir.path(), &facts);

    assert_eq!(only(dir.path())["decided_by"], Value::Null);
}

#[test]
fn a_permission_dialog_whose_tool_then_ran_was_allowed_by_the_person() {
    for post in ["PostToolUse", "PostToolUseFailure"] {
        let dir = TempDir::new().unwrap();
        record_facts(
            dir.path(),
            &permission_facts(
                NOW,
                "git status",
                json!({ "path": "session", "post": post, "is_error": post == "PostToolUseFailure" }),
            ),
        );

        let entry = only(dir.path());
        assert_eq!(source(&entry), (json!("human"), json!("session")), "{post}");
        assert_eq!(entry["answer"], "allow", "{post}");
        assert_eq!(entry["inferred"], false, "{post}");
        assert_eq!(entry["wait_id"], "t-1-7");
        assert_eq!(
            entry["asked"],
            json!({ "tool": "Bash", "target": "Bash: git status" })
        );
    }
}

#[test]
fn a_permission_dialog_that_ended_in_an_error_without_the_tool_running_is_an_inferred_deny() {
    let dir = TempDir::new().unwrap();
    record_facts(
        dir.path(),
        &permission_facts(
            NOW,
            "git status",
            json!({ "path": "session", "post": null, "is_error": true }),
        ),
    );

    let entry = only(dir.path());
    assert_eq!(source(&entry), (json!("human"), json!("session")));
    assert_eq!(entry["answer"], "deny");
    assert_eq!(entry["inferred"], true);
}

#[test]
fn a_permission_dialog_with_neither_a_run_nor_an_error_leaves_nothing() {
    let dir = TempDir::new().unwrap();
    let ids = record_facts(
        dir.path(),
        &permission_facts(
            NOW,
            "git status",
            json!({ "path": "session", "post": null, "is_error": false }),
        ),
    );

    assert!(ids.is_empty());
    assert!(list(dir.path(), &[]).is_empty());
}

#[test]
fn a_prompt_typed_or_sent_by_a_person_while_a_decision_is_open_is_their_decision() {
    for origin in ["composer", "bridge"] {
        let dir = TempDir::new().unwrap();
        record_facts(
            dir.path(),
            &decision_facts(
                NOW,
                json!({ "path": "session", "text": "초안대로 진행", "origin": origin }),
            ),
        );

        let entry = only(dir.path());
        assert_eq!(
            source(&entry),
            (json!("human"), json!("session")),
            "{origin}"
        );
        assert_eq!(entry["origin"], origin);
        assert_eq!(entry["free_text"], false);
        assert_eq!(entry["asked"]["ask"], "초안대로 진행할까요?");
    }
}

#[test]
fn a_prompt_from_anyone_but_a_person_while_a_decision_is_open_is_no_persons_decision() {
    for origin in [
        "sdk",
        "plugin",
        "peer",
        "scheduled-trigger",
        "task-notification",
        "unclassified",
        "unknown",
    ] {
        let dir = TempDir::new().unwrap();
        record_facts(
            dir.path(),
            &decision_facts(
                NOW,
                json!({ "path": "session", "text": "초안대로 진행", "origin": origin }),
            ),
        );

        assert_eq!(only(dir.path())["decided_by"], Value::Null, "{origin}");
    }
}

#[test]
fn every_kind_carries_an_answer_fingerprint_so_opposite_permission_answers_show() {
    let dir = TempDir::new().unwrap();
    let session_dialog = |post: Value, is_error: bool| json!({ "path": "session", "post": post, "is_error": is_error });
    record_facts(
        dir.path(),
        &permission_facts(
            NOW,
            "git status",
            session_dialog(json!("PostToolUse"), false),
        ),
    );
    record_facts(
        dir.path(),
        &permission_facts(
            NOW + 1,
            "git status",
            session_dialog(json!("PostToolUse"), false),
        ),
    );
    record_facts(
        dir.path(),
        &permission_facts(NOW + 2, "git status", session_dialog(Value::Null, true)),
    );
    record_facts(
        dir.path(),
        &permission_facts(
            NOW + 3,
            "git status",
            json!({ "path": "inbox", "via": "pane", "decision": "deny" }),
        ),
    );

    let entries = list(dir.path(), &[]);
    let key = entries[0]["key"].as_str().expect("a key").to_string();
    let same_key = list(dir.path(), &["--key", &key]);
    assert_eq!(same_key.len(), 4);
    let fp = |answer: &str| {
        same_key
            .iter()
            .filter(|e| e["answer"] == answer)
            .map(|e| e["answer_fp"].as_str().expect("fingerprint").to_string())
            .collect::<Vec<_>>()
    };
    let allows = fp("allow");
    let denies = fp("deny");
    assert_eq!(allows.len(), 2);
    assert_eq!(denies.len(), 2);
    assert_eq!(allows[0], allows[1]);
    assert_eq!(denies[0], denies[1]);
    assert_ne!(allows[0], denies[0]);
    assert_eq!(allows[0].len(), 32);
}

#[test]
fn a_question_fingerprint_is_the_normalized_label() {
    let dir = TempDir::new().unwrap();
    record_facts(
        dir.path(),
        &question_facts(NOW, color(), chose(json!(["Red"]))),
    );
    record_facts(
        dir.path(),
        &question_facts(NOW + 1, color(), chose(json!(["  red. "]))),
    );
    record_facts(
        dir.path(),
        &question_facts(NOW + 2, color(), chose(json!(["Blue"]))),
    );

    let fps: Vec<Value> = list(dir.path(), &[])
        .iter()
        .map(|e| e["answer_fp"].clone())
        .collect();
    assert_eq!(fps[1], fps[2]);
    assert_ne!(fps[0], fps[1]);
}

#[test]
fn a_decision_fingerprint_is_the_normalized_reply() {
    let dir = TempDir::new().unwrap();
    let reply = |at: u64, text: &str| {
        decision_facts(
            at,
            json!({ "path": "session", "text": text, "origin": "composer" }),
        )
    };
    record_facts(dir.path(), &reply(NOW, "초안대로 진행"));
    record_facts(dir.path(), &reply(NOW + 1, "초안대로  진행"));
    record_facts(dir.path(), &reply(NOW + 2, "다시 쓰기"));

    let entries = list(dir.path(), &[]);
    assert_eq!(entries[1]["answer_fp"], entries[2]["answer_fp"]);
    assert_ne!(entries[0]["answer_fp"], entries[1]["answer_fp"]);
    assert_eq!(entries[0]["key"], entries[1]["key"]);
    assert!(entries[0]["key"].as_str().expect("key").starts_with("d1:"));
}

#[test]
fn a_wait_of_two_questions_leaves_one_entry_per_question() {
    let dir = TempDir::new().unwrap();
    let questions = json!([
        question("Which color?", &["Red", "Blue"], false),
        question("Which size?", &["S", "M"], false),
    ]);
    let ids = record_facts(
        dir.path(),
        &question_facts(NOW, questions, chose(json!(["Red", "M"]))),
    );
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);

    let mut entries = list(dir.path(), &[]);
    entries.sort_by_key(|e| e["q_index"].as_u64());
    assert_eq!(entries.len(), 2);
    for (index, entry) in entries.iter().enumerate() {
        assert_eq!(entry["q_index"], index);
        assert_eq!(entry["q_count"], 2);
        assert_eq!(entry["wait_id"], "tu-1");
    }
    assert_eq!(entries[0]["answer"], "Red");
    assert_eq!(entries[1]["answer"], "M");
    assert_ne!(entries[0]["key"], entries[1]["key"]);
    assert_eq!(entries[1]["asked"]["question"], "Which size?");

    for entry in &entries {
        let key = entry["key"].as_str().expect("key");
        let found = list(dir.path(), &["--key", key]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0]["answer"], entry["answer"]);
    }
}

#[test]
fn a_question_left_unanswered_leaves_no_entry_for_it() {
    let dir = TempDir::new().unwrap();
    let questions = json!([
        question("Which color?", &["Red", "Blue"], false),
        question("Which size?", &["S", "M"], false),
    ]);
    let ids = record_facts(
        dir.path(),
        &question_facts(NOW, questions, chose(json!([null, "M"]))),
    );

    assert_eq!(ids.len(), 1);
    let entry = only(dir.path());
    assert_eq!(entry["q_index"], 1);
    assert_eq!(entry["q_count"], 2);
}

#[test]
fn an_entry_keeps_whether_its_wait_could_then_be_answered_by_a_precedent() {
    let dir = TempDir::new().unwrap();
    record_facts(
        dir.path(),
        &question_facts(NOW, color(), chose(json!(["Red"]))),
    );
    record_facts(
        dir.path(),
        &question_facts(
            NOW + 1,
            json!([question("Which pets?", &["Cat", "Dog"], true)]),
            chose(json!(["Cat, Dog"])),
        ),
    );
    record_facts(
        dir.path(),
        &permission_facts(
            NOW + 2,
            "rm -rf build",
            json!({ "path": "session", "post": "PostToolUse", "is_error": false }),
        ),
    );

    let entries = list(dir.path(), &[]);
    assert_eq!(entries[2]["proxyable"], true);
    assert_eq!(entries[2]["blocked"], json!([]));
    assert_eq!(entries[1]["proxyable"], false);
    assert_eq!(entries[1]["blocked"], json!(["multi_select"]));
    assert_eq!(entries[0]["proxyable"], false);
    assert!(entries[0]["blocked"]
        .as_array()
        .expect("reasons")
        .contains(&json!("not_allowlisted:rm")));
}

#[test]
fn a_question_cut_when_recorded_has_no_key() {
    let dir = TempDir::new().unwrap();
    let questions = color();
    let mut facts = question_facts(NOW, questions, chose(json!(["Red"])));
    facts["question"]["orig_len"]["question"] = json!([500]);
    record_facts(dir.path(), &facts);

    let entry = only(dir.path());
    assert_eq!(entry["key"], Value::Null);
    assert_eq!(entry["proxyable"], false);
}

#[test]
fn a_secret_is_masked_before_it_is_kept_and_marks_the_answer_it_changed() {
    let hex40 = "0123456789abcdef0123456789abcdef01234567";
    let b64 = "QWxhZGRpbjpvcGVuIHNlc2FtZQ9xY2hhbmdlbWVfX19z";
    let cases = [
        ("api_key=abc123", "api_key=[masked]"),
        ("TOKEN: abc123", "TOKEN: [masked]"),
        ("password=hunter2 then", "password=[masked] then"),
        ("Authorization: Bearer abc.def", "Authorization: [masked]"),
        ("use Bearer abc.def now", "use Bearer [masked] now"),
        ("key sk-abcdefghijklmnop1234", "key [masked]"),
        ("ghp_abcdefghijklmnopqrstuvwxyz0123", "[masked]"),
        ("AKIAABCDEFGHIJKLMNOP", "[masked]"),
        ("xoxb-123-456-abc", "[masked]"),
        (hex40, "[masked]"),
        (b64, "[masked]"),
    ];
    for (said, kept) in cases {
        let dir = TempDir::new().unwrap();
        record_facts(
            dir.path(),
            &decision_facts(
                NOW,
                json!({ "path": "session", "text": said, "origin": "composer" }),
            ),
        );

        let entry = only(dir.path());
        assert_eq!(entry["answer"], kept, "{said}");
        assert_eq!(entry["answer_masked"], true, "{said}");
    }
}

#[test]
fn a_secret_under_a_prefixed_or_quoted_name_in_a_url_or_a_mysql_flag_is_masked() {
    let cases = [
        ("GITHUB_TOKEN=ghx_notreal123", "GITHUB_TOKEN=[masked]"),
        ("DB_PASSWORD=hunter2", "DB_PASSWORD=[masked]"),
        ("client_secret=s3cr3tval", "client_secret=[masked]"),
        ("access_token=abc123def", "access_token=[masked]"),
        (
            "export DB_PASSWORD=hunter2 && make",
            "export DB_PASSWORD=[masked] && make",
        ),
        (r#"{"password": "hunter2"}"#, r#"{"password": "[masked]"}"#),
        (
            r#"{"api_key":"abc def","user":"bob"}"#,
            r#"{"api_key":"[masked]","user":"bob"}"#,
        ),
        (
            "git clone https://alice:hunter2@example.com/r.git",
            "git clone https://alice:[masked]@example.com/r.git",
        ),
        (
            "mysql -u root -phunter2 app",
            "mysql -u root -p[masked] app",
        ),
        ("mysqldump -phunter2 app", "mysqldump -p[masked] app"),
    ];
    for (said, kept) in cases {
        let dir = TempDir::new().unwrap();
        record_facts(
            dir.path(),
            &decision_facts(
                NOW,
                json!({ "path": "session", "text": said, "origin": "composer" }),
            ),
        );

        let entry = only(dir.path());
        assert_eq!(entry["answer"], kept, "{said}");
        assert_eq!(entry["answer_masked"], true, "{said}");
    }
}

#[test]
fn no_kept_field_of_a_permission_entry_carries_the_secret_in_its_target() {
    for command in [
        "export DB_PASSWORD=hunter2",
        "curl -H 'X-Api-Key: hunter2' https://example.com",
        "git clone https://alice:hunter2@example.com/r.git",
        "mysql -h db -phunter2 app",
        r#"echo '{"password": "hunter2"}' > cfg.json"#,
    ] {
        let dir = TempDir::new().unwrap();
        record_facts(
            dir.path(),
            &permission_facts(
                NOW,
                command,
                json!({ "path": "session", "post": "PostToolUse", "is_error": false }),
            ),
        );

        let entry = only(dir.path());
        assert!(!entry.to_string().contains("hunter2"), "{command}: {entry}");
        assert!(entry["asked"]["target"]
            .as_str()
            .expect("target")
            .contains("[masked]"));
    }
}

#[test]
fn a_command_refused_for_a_secret_word_keeps_the_reason_masked() {
    for (command, secret) in [
        ("PGPASSWORD=hunter2 psql -h db", "hunter2"),
        (
            "GITHUB_TOKEN=ghp_aB3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3z gh pr list",
            "ghp_aB3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3z",
        ),
        ("https://alice:hunter2@example.com/repo.git", "hunter2"),
    ] {
        let dir = TempDir::new().unwrap();
        record_facts(
            dir.path(),
            &permission_facts(
                NOW,
                command,
                json!({ "path": "session", "post": "PostToolUse", "is_error": false }),
            ),
        );

        let line = fs::read_dir(ledger_dir(dir.path()))
            .unwrap()
            .map(|file| fs::read_to_string(file.unwrap().path()).unwrap())
            .collect::<String>();
        let entry = only(dir.path());
        assert!(
            entry["blocked"]
                .as_array()
                .expect("blocked")
                .iter()
                .any(|reason| reason.as_str().unwrap().starts_with("not_allowlisted:")),
            "{command}: {entry}"
        );
        assert!(!line.contains(secret), "{command}: {line}");
    }
}

#[test]
fn no_kept_field_of_a_question_or_decision_entry_carries_the_secret() {
    let dir = TempDir::new().unwrap();
    record_facts(
        dir.path(),
        &question_facts(
            NOW,
            json!([question(
                "Use DB_PASSWORD=hunter2?",
                &["Yes, client_secret=hunter2", "No"],
                false
            )]),
            chose(json!(["Yes, client_secret=hunter2"])),
        ),
    );
    record_facts(
        dir.path(),
        &decision_facts(
            NOW + 1,
            json!({ "path": "session", "text": "use https://alice:hunter2@example.com", "origin": "composer" }),
        ),
    );

    let entries = list(dir.path(), &[]);
    assert_eq!(entries.len(), 2);
    for entry in entries {
        assert!(!entry.to_string().contains("hunter2"), "{entry}");
    }
}

#[test]
fn words_that_only_look_long_are_kept_as_said() {
    for said in [
        "초안대로 진행",
        "/home/user/kys-claude-plugin/plugins/atelier/cli/src/kanban",
        "tokens are fine",
        "mkdir -p build && cp -pr a b",
        "mysql -P3306 -p app",
        "open https://example.com:8080/a@b",
        "my_tokens: 3",
    ] {
        let dir = TempDir::new().unwrap();
        record_facts(
            dir.path(),
            &decision_facts(
                NOW,
                json!({ "path": "session", "text": said, "origin": "composer" }),
            ),
        );

        let entry = only(dir.path());
        assert_eq!(entry["answer"], said);
        assert_eq!(entry["answer_masked"], false);
    }
}

#[test]
fn what_was_asked_is_masked_while_the_key_comes_from_the_text_as_asked() {
    let dir = TempDir::new().unwrap();
    let secret = "deploy with token=s3cr3t?";
    let plain = "deploy with token=other?";
    let ask = |at: u64, text: &str| {
        question_facts(
            at,
            json!([question(text, &["Yes", "No"], false)]),
            chose(json!(["Yes"])),
        )
    };
    record_facts(dir.path(), &ask(NOW, secret));
    record_facts(dir.path(), &ask(NOW + 1, plain));

    let entries = list(dir.path(), &[]);
    assert_eq!(
        entries[1]["asked"]["question"],
        "deploy with token=[masked]"
    );
    assert!(!entries[1]["key_label"]
        .as_str()
        .expect("label")
        .contains("s3cr3t"));
    assert_ne!(entries[0]["key"], entries[1]["key"]);
    assert_eq!(entries[1]["answer_masked"], false);
}

#[test]
fn a_long_answer_is_cut() {
    let dir = TempDir::new().unwrap();
    let long = "가".repeat(800);
    record_facts(
        dir.path(),
        &decision_facts(
            NOW,
            json!({ "path": "session", "text": long, "origin": "composer" }),
        ),
    );

    assert_eq!(
        only(dir.path())["answer"]
            .as_str()
            .expect("answer")
            .chars()
            .count(),
        500
    );
}

#[test]
fn list_is_newest_first_and_skips_lines_it_cannot_read() {
    let dir = TempDir::new().unwrap();
    record_facts(
        dir.path(),
        &question_facts(NOW, color(), chose(json!(["Red"]))),
    );
    let file = fs::read_dir(ledger_dir(dir.path()))
        .expect("ledger dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .expect("a month file");
    let mut handle = fs::OpenOptions::new().append(true).open(&file).unwrap();
    writeln!(handle, "{{not json").unwrap();
    writeln!(handle, "{{\"v\":99}}").unwrap();
    record_facts(
        dir.path(),
        &question_facts(NOW + 1, color(), chose(json!(["Blue"]))),
    );

    let answers: Vec<Value> = list(dir.path(), &[])
        .iter()
        .map(|e| e["answer"].clone())
        .collect();
    assert_eq!(answers, vec![json!("Blue"), json!("Red")]);
}

#[test]
fn list_filters_by_what_it_is_given() {
    let dir = TempDir::new().unwrap();
    record_facts(
        dir.path(),
        &question_facts(NOW, color(), chose(json!(["Red"]))),
    );
    record_facts(
        dir.path(),
        &permission_facts(
            NOW + 1,
            "git status",
            json!({ "path": "inbox", "via": "terminal", "decision": "allow" }),
        ),
    );
    let mut other = decision_facts(
        NOW + 2,
        json!({ "path": "session", "text": "다시 쓰기", "origin": "composer" }),
    );
    other["session"] = json!("x");
    record_facts(dir.path(), &other);

    let count = |args: &[&str]| list(dir.path(), args).len();
    assert_eq!(count(&[]), 3);
    assert_eq!(count(&["--kind", "question", "--kind", "decision"]), 2);
    assert_eq!(count(&["--session", "x"]), 1);
    assert_eq!(count(&["--decided-by", "human"]), 2);
    assert_eq!(count(&["--via", "terminal"]), 1);
    assert_eq!(count(&["--repo", REPO]), 3);
    assert_eq!(count(&["--repo", "/elsewhere"]), 0);
    assert_eq!(count(&["--search", "WHICH COLOR"]), 1);
    assert_eq!(count(&["--since", &(NOW + 1).to_string()]), 2);
    assert_eq!(count(&["--until", &NOW.to_string()]), 1);
    assert_eq!(count(&["--limit", "1"]), 1);
    assert_eq!(count(&["--target-session", "w"]), 0);
    assert_eq!(count(&["--target-wait", "tu-1"]), 0);
}

#[test]
fn list_refuses_what_it_does_not_know() {
    let dir = TempDir::new().unwrap();
    for args in [
        &["ledger", "list", "--kind", "nope"][..],
        &["ledger", "list", "--decided-by", "someone"][..],
        &["ledger", "list", "--via", "mail"][..],
        &["ledger", "list", "--limit", "501"][..],
    ] {
        let out = kanban_in(dir.path(), args);
        assert!(out["error"].is_string(), "{args:?}: {out}");
    }
}

#[test]
fn record_refuses_facts_it_cannot_read() {
    let dir = TempDir::new().unwrap();
    for facts in [
        json!({}),
        with(
            place_at(NOW, REPO),
            json!({ "answer": { "path": "session", "answers": ["Red"] } }),
        ),
        with(
            question_facts(NOW, color(), chose(json!(["Red"]))),
            json!({ "session": "bad id!" }),
        ),
        question_facts(
            NOW,
            color(),
            json!({ "path": "nowhere", "answers": ["Red"] }),
        ),
        question_facts(NOW, color(), json!({ "path": "session", "text": "Red" })),
    ] {
        let out = run_record(dir.path(), &facts);
        assert!(out["error"].is_string(), "{facts}: {out}");
    }
    assert!(list(dir.path(), &[]).is_empty());
}

fn write_month(config: &Path, month: &str, lines: &[String]) {
    let dir = ledger_dir(config);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{month}.jsonl")), lines.join("\n") + "\n").unwrap();
}

fn kept_entry(at: u64, padding: usize) -> String {
    let dir = TempDir::new().unwrap();
    record_facts(
        dir.path(),
        &decision_facts(
            at,
            json!({ "path": "session", "text": "x".repeat(padding.min(400)), "origin": "composer" }),
        ),
    );
    let mut entry = only(dir.path());
    entry["asked"]["pad"] = json!(" ".repeat(padding));
    entry.to_string()
}

fn prune(config: &Path, now: u64) -> Value {
    kanban_in(config, &["ledger", "prune", "--now", &now.to_string()])
}

#[test]
fn prune_drops_entries_older_than_180_days_and_the_files_left_empty() {
    let dir = TempDir::new().unwrap();
    write_month(dir.path(), "2026-06", &[kept_entry(NOW - 181 * DAY, 0)]);
    write_month(
        dir.path(),
        "2026-07",
        &[
            kept_entry(NOW - 181 * DAY, 0),
            kept_entry(NOW - 179 * DAY, 0),
        ],
    );

    let out = prune(dir.path(), NOW);

    assert_eq!(out["pruned"], json!({ "entries": 2, "files": 1 }));
    let left = list(dir.path(), &[]);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0]["at"], NOW - 179 * DAY);
    assert!(!ledger_dir(dir.path()).join("2026-06.jsonl").exists());
}

#[test]
fn prune_drops_the_oldest_entries_until_the_ledger_fits_in_50_mb() {
    let dir = TempDir::new().unwrap();
    let line = kept_entry(NOW - 3 * DAY, 100_000);
    let per = line.len() as u64 + 1;
    let fits = 50 * 1024 * 1024 / per;
    let old: Vec<String> = (0..10)
        .map(|_| kept_entry(NOW - 9 * DAY, 100_000))
        .collect();
    let new: Vec<String> = (0..fits).map(|_| line.clone()).collect();
    write_month(dir.path(), "2026-01", &old);
    write_month(dir.path(), "2026-02", &new);

    let out = prune(dir.path(), NOW);

    assert_eq!(out["pruned"], json!({ "entries": 10, "files": 1 }));
    let total: u64 = fs::read_dir(ledger_dir(dir.path()))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .map(|e| e.metadata().unwrap().len())
        .sum();
    assert!(total <= 50 * 1024 * 1024);
    assert!(!ledger_dir(dir.path()).join("2026-01.jsonl").exists());
}

#[test]
fn prune_of_an_empty_ledger_drops_nothing() {
    let dir = TempDir::new().unwrap();

    assert_eq!(
        prune(dir.path(), NOW)["pruned"],
        json!({ "entries": 0, "files": 0 })
    );
}

fn lock_file(config: &Path) -> fs::File {
    let dir = ledger_dir(config);
    fs::create_dir_all(&dir).unwrap();
    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(".lock"))
        .unwrap()
}

fn spawn_record(config: &Path, facts: &Value) -> std::process::Child {
    let mut child = spawn_kanban(config, &["ledger", "record"]);
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(facts.to_string().as_bytes())
        .expect("write facts");
    child
}

#[test]
fn a_record_waits_for_the_ledger_lock_and_then_adds_its_whole_line() {
    let dir = TempDir::new().unwrap();
    let lock = lock_file(dir.path());
    lock.lock().expect("hold the ledger lock");

    let mut child = spawn_record(
        dir.path(),
        &question_facts(NOW, color(), chose(json!(["Red"]))),
    );
    std::thread::sleep(Duration::from_millis(300));
    assert!(child.try_wait().expect("poll").is_none());
    lock.unlock().expect("let it go");
    let status = child.wait().expect("record finishes");

    assert!(status.success());
    assert_eq!(only(dir.path())["answer"], "Red");
}

#[test]
#[ignore = "timing-dependent stress run; the lock test above is the deterministic check"]
fn many_records_at_once_never_mix_their_lines() {
    let dir = TempDir::new().unwrap();
    let children: Vec<_> = (0..16)
        .map(|i| {
            spawn_record(
                dir.path(),
                &question_facts(NOW + i, color(), chose(json!(["Red"]))),
            )
        })
        .collect();
    for mut child in children {
        assert!(child.wait().expect("wait").success());
    }

    assert_eq!(list(dir.path(), &[]).len(), 16);
}
