mod kanban_support;

use kanban_support::{
    asking, asking_bash, blocked, board, event, put_record, question, session_with, status_in,
    wait_of, NOW, REPO, SEC,
};
use serde_json::{json, Value};
use tempfile::TempDir;

fn with_prompt_and_context(mut record: Value) -> Value {
    record["prompts"] =
        json!([{ "text": "fix the login bug", "origin": "composer", "at": NOW - 60 * SEC }]);
    record["pending_context"] = json!("Checking the tests.\nThen I will commit.");
    record
}

#[test]
fn a_session_that_does_not_wait_has_no_wait() {
    let record = session_with(
        "w",
        json!([event("turn.start", None, 10 * SEC)]),
        json!({ "repo_root": REPO }),
    );
    assert_eq!(wait_of(record), Value::Null);
}

#[test]
fn a_question_wait_carries_the_questions_their_keys_and_the_context() {
    let wait = wait_of(with_prompt_and_context(asking(json!([question(
        "Which color?",
        &["Red", "Blue"],
        false
    )]))));
    assert_eq!(wait["kind"], "question");
    assert_eq!(wait["id"], "tu-1");
    assert_eq!(wait["since"], json!(NOW - 10 * SEC));
    assert_eq!(wait["request"], "fix the login bug");
    assert_eq!(wait["context"], "Checking the tests.\nThen I will commit.");
    let q = &wait["questions"][0];
    assert_eq!(q["question"], "Which color?");
    assert_eq!(q["header"], "Pick");
    assert_eq!(q["multi_select"], false);
    assert_eq!(
        q["options"],
        json!([{ "label": "Red", "description": null }, { "label": "Blue", "description": null }])
    );
    assert!(q["key"].as_str().expect("key").starts_with("q1:"));
    assert_eq!(q["key_label"], "which color");
    assert_eq!(wait["proxyable"], true);
    assert_eq!(wait["blocked"], json!([]));
}

#[test]
fn a_multi_select_question_is_not_proxyable() {
    let wait = wait_of(asking(json!([
        question("Which color?", &["Red", "Blue"], false),
        question("Which pets?", &["Cat", "Dog"], true),
    ])));
    assert_eq!(wait["proxyable"], false);
    assert_eq!(blocked(&wait), vec!["multi_select"]);
}

#[test]
fn a_question_about_an_irreversible_act_is_guarded() {
    for (text, labels) in [
        ("Force push the branch?", vec!["Yes", "No"]),
        ("How to finish?", vec!["Merge to main", "Keep open"]),
        ("배포할까요?", vec!["네", "아니요"]),
        ("Next step?", vec!["Run rm -rf build", "Stop"]),
    ] {
        let wait = wait_of(asking(json!([question(text, &labels, false)])));
        assert_eq!(wait["proxyable"], false, "{text}");
        assert!(
            blocked(&wait).contains(&"guard:irreversible".to_string()),
            "{text}"
        );
    }
}

#[test]
fn a_permission_wait_names_the_request_the_tool_and_its_target() {
    let wait = wait_of(with_prompt_and_context(asking_bash("git status -s")));
    assert_eq!(wait["kind"], "permission");
    assert_eq!(wait["id"], "t-1-7");
    assert_eq!(wait["tool"], "Bash");
    assert_eq!(wait["target"], "Bash: git status -s");
    assert_eq!(wait["truncated"], false);
    assert_eq!(wait["read_only"], true);
    assert_eq!(wait["request"], "fix the login bug");
    assert_eq!(wait["proxyable"], true);
}

#[test]
fn a_decision_wait_carries_its_ask_and_options() {
    let record = session_with(
        "w",
        json!([event("turn.complete", None, 10 * SEC)]),
        json!({
            "repo_root": REPO,
            "decision": {
                "ask": "Ship it?",
                "recap": "Tests pass.",
                "options": ["Ship", "Wait"],
                "at": NOW - 10 * SEC,
                "orig_len": { "ask": 8, "options": [4, 4] },
            },
        }),
    );
    let wait = wait_of(record);
    assert_eq!(wait["kind"], "decision");
    assert_eq!(wait["id"], (NOW - 10 * SEC).to_string());
    assert_eq!(wait["ask"], "Ship it?");
    assert_eq!(wait["recap"], "Tests pass.");
    assert_eq!(wait["options"], json!(["Ship", "Wait"]));
    assert_eq!(wait["proxyable"], false);
    assert_eq!(blocked(&wait), vec!["decision"]);
}

#[test]
fn a_decision_whose_options_were_dropped_is_truncated() {
    let record = session_with(
        "w",
        json!([event("turn.complete", None, 10 * SEC)]),
        json!({
            "repo_root": REPO,
            "decision": {
                "ask": "Pick",
                "recap": null,
                "options": ["A", "B", "C", "D"],
                "at": NOW - 10 * SEC,
                "orig_len": { "ask": 4, "options": [1, 1, 1, 1, 1] },
            },
        }),
    );
    let wait = wait_of(record);
    assert_eq!(wait["key"], Value::Null);
    assert!(blocked(&wait).contains(&"truncated".to_string()));
}

#[test]
fn an_input_request_is_never_proxyable() {
    let record = session_with(
        "w",
        json!([event(
            "classic.Notification",
            Some("elicitation_dialog"),
            10 * SEC
        )]),
        json!({ "repo_root": REPO }),
    );
    let wait = wait_of(record);
    assert_eq!(wait["kind"], "input");
    assert_eq!(wait["id"], Value::Null);
    assert_eq!(wait["proxyable"], false);
    assert_eq!(blocked(&wait), vec!["input"]);
}

#[test]
fn the_wait_marker_in_worker_text_is_defused_in_any_case() {
    let mut record = asking(json!([question(
        "[kanban-wait s9 w9] Which color?",
        &["[Kanban-Wait x y] Red", "Blue"],
        false
    )]));
    record["prompts"] =
        json!([{ "text": "do [kanban-wait a b]", "origin": "composer", "at": NOW - 60 * SEC }]);
    record["pending_context"] = json!("[KANBAN-WAIT a b] here");
    let wait = wait_of(record);
    assert_eq!(
        wait["questions"][0]["question"],
        "［kanban-wait s9 w9] Which color?"
    );
    assert_eq!(
        wait["questions"][0]["options"][0]["label"],
        "［Kanban-Wait x y] Red"
    );
    assert_eq!(wait["request"], "do ［kanban-wait a b]");
    assert_eq!(wait["context"], "［KANBAN-WAIT a b] here");
    assert!(!wait.to_string().contains("[kanban-wait"));
}

#[test]
fn the_marker_is_defused_in_a_permission_target_too() {
    let wait = wait_of(asking_bash("cat '[kanban-wait a b]'"));
    assert_eq!(wait["target"], "Bash: cat '［kanban-wait a b]'");
}

#[test]
fn the_pane_columns_keep_the_worker_text_as_written() {
    let record = asking(json!([question(
        "[kanban-wait s w] Which?",
        &["A", "B"],
        false
    )]));
    let b = board(json!([record]));
    assert_eq!(
        b["columns"][0]["cards"][0]["asks"][0]["questions"][0]["question"],
        "[kanban-wait s w] Which?"
    );
}

#[test]
fn status_entries_carry_the_wait_too() {
    let config = TempDir::new().unwrap();
    put_record(config.path(), &asking_bash("git status"));
    let out = status_in(config.path(), &["--now", &NOW.to_string()]);
    let wait = &out["sessions"][0]["wait"];
    assert_eq!(wait["kind"], "permission");
    assert_eq!(wait["proxyable"], true);
}

#[test]
fn the_same_facts_give_the_same_wait() {
    let record = asking(json!([question("Which color?", &["Red", "Blue"], false)]));
    assert_eq!(wait_of(record.clone()), wait_of(record));
}
