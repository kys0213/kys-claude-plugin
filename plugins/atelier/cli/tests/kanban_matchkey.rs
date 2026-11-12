mod kanban_support;

use kanban_support::{
    asking, asking_bash, asking_permission_at, asking_with_lengths, blocked, color_and_size,
    decision_waiting, question, question_lengths, wait_of, REPO,
};
use serde_json::{json, Value};

fn question_key(text: &str, labels: &[&str]) -> Value {
    wait_of(asking(json!([question(text, labels, false)])))["questions"][0]["key"].clone()
}

fn permission_key(repo: &str, command: &str) -> Value {
    wait_of(asking_permission_at(
        repo,
        repo,
        "Bash",
        json!({ "command": command }),
    ))["key"]
        .clone()
}

fn hex_after(key: &Value, prefix: &str) -> String {
    let key = key.as_str().expect("a key");
    let rest = key.strip_prefix(prefix).expect("kind and rule version");
    assert_eq!(rest.len(), 32, "{key}");
    assert!(rest
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    rest.to_string()
}

#[test]
fn a_question_key_is_its_kind_version_and_128_bits_of_hash() {
    hex_after(&question_key("Which color?", &["Red", "Blue"]), "q1:");
}

#[test]
fn the_same_normalized_question_has_the_same_key() {
    let base = question_key("Which color?", &["Red", "Blue"]);
    assert_eq!(question_key("  which   COLOR ", &["Red", "Blue"]), base);
    assert_eq!(question_key("Which color?!", &["red", " Blue "]), base);
}

#[test]
fn option_order_does_not_change_the_key() {
    assert_eq!(
        question_key("Which color?", &["Blue", "Red"]),
        question_key("Which color?", &["Red", "Blue"])
    );
}

#[test]
fn different_options_or_selection_mode_change_the_key() {
    let base = question_key("Which color?", &["Red", "Blue"]);
    assert_ne!(question_key("Which color?", &["Red", "Green"]), base);
    assert_ne!(question_key("Which colour?", &["Red", "Blue"]), base);
    let multi = wait_of(asking(json!([question(
        "Which color?",
        &["Red", "Blue"],
        true
    )])))["questions"][0]["key"]
        .clone();
    assert_ne!(multi, base);
}

#[test]
fn each_question_of_a_call_has_its_own_key() {
    let wait = wait_of(asking(color_and_size()));
    assert_ne!(wait["questions"][0]["key"], wait["questions"][1]["key"]);
    assert_eq!(
        wait["questions"][0]["key"],
        question_key("Which color?", &["Red", "Blue"])
    );
}

#[test]
fn a_cut_question_has_no_key() {
    let questions = json!([question("Which color?", &["Red", "Blue"], false)]);
    let mut lengths = question_lengths(&questions);
    lengths["question"][0] = json!(5000);
    let wait = wait_of(asking_with_lengths(questions, lengths));
    assert_eq!(wait["questions"][0]["key"], Value::Null);
    assert!(blocked(&wait).contains(&"truncated".to_string()));
    assert_eq!(wait["proxyable"], false);
}

#[test]
fn a_cut_option_label_has_no_key() {
    let questions = json!([question("Which color?", &["Red", "Blue"], false)]);
    let mut lengths = question_lengths(&questions);
    lengths["labels"][0][1] = json!(300);
    let wait = wait_of(asking_with_lengths(questions, lengths));
    assert_eq!(wait["questions"][0]["key"], Value::Null);
}

#[test]
fn a_shell_permission_key_reads_the_first_two_words() {
    let base = permission_key(REPO, "git status");
    hex_after(&base, "p1:");
    assert_eq!(permission_key(REPO, "git status -s"), base);
    assert_eq!(permission_key(REPO, "git  'status'  --short"), base);
    assert_ne!(permission_key(REPO, "git log"), base);
    assert_ne!(permission_key("/work/project-b", "git status"), base);
}

#[test]
fn a_chained_shell_command_has_no_key() {
    let wait = wait_of(asking_bash("git status && rm -rf x"));
    assert_eq!(wait["key"], Value::Null);
    assert!(blocked(&wait).contains(&"no_key".to_string()));
}

#[test]
fn a_permission_key_label_names_the_tool_and_the_target() {
    assert_eq!(
        wait_of(asking_bash("git status -s"))["key_label"],
        "Bash git status"
    );
}

#[test]
fn a_decision_key_ignores_option_order_and_is_never_proxyable() {
    let a = wait_of(decision_waiting(
        "Ship it?",
        Some("done"),
        &["Ship", "Wait"],
    ));
    let b = wait_of(decision_waiting("ship it", Some("done"), &["Wait", "Ship"]));
    hex_after(&a["key"], "d1:");
    assert_eq!(a["key"], b["key"]);
    assert_eq!(a["proxyable"], false);
    assert!(blocked(&a).contains(&"decision".to_string()));
}

#[test]
fn a_key_is_computed_from_the_text_before_the_marker_is_defused() {
    let marked = wait_of(asking(json!([question(
        "[kanban-wait s1 w1] Which color?",
        &["Red", "Blue"],
        false
    )])));
    let defused = wait_of(asking(json!([question(
        "［kanban-wait s1 w1] Which color?",
        &["Red", "Blue"],
        false
    )])));
    assert_eq!(
        marked["questions"][0]["question"],
        "［kanban-wait s1 w1] Which color?"
    );
    assert_ne!(
        marked["questions"][0]["key"],
        defused["questions"][0]["key"]
    );
}
