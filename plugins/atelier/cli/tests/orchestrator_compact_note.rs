//! End-to-end black-box tests for `atelier orchestrator compact-note`. The
//! output never varies with input (plan decision 1): stdin is ignored
//! entirely, the process always exits 0, and stdout is one line of
//! `{"instructions": "..."}` with a non-empty instruction string.

use assert_cmd::Command;
use serde_json::Value;

fn atelier() -> Command {
    Command::cargo_bin("atelier").expect("locate `atelier` cargo binary")
}

fn instructions_for(stdin: &str) -> String {
    let output = atelier()
        .args(["orchestrator", "compact-note"])
        .write_stdin(stdin)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).expect("stdout is UTF-8");
    assert_eq!(
        stdout.lines().count(),
        1,
        "expected exactly one line of stdout, got: {stdout:?}"
    );
    let value: Value = serde_json::from_str(stdout.trim_end()).expect("valid JSON on stdout");
    value["instructions"]
        .as_str()
        .expect("instructions is a string")
        .to_string()
}

#[test]
fn empty_stdin_yields_nonempty_instructions() {
    let instructions = instructions_for("");
    assert!(!instructions.trim().is_empty());
}

#[test]
fn instructions_mention_key_phrases() {
    let instructions = instructions_for("");
    assert!(instructions.contains("epic"));
    assert!(instructions.contains("log_dir"));
    assert!(instructions.contains("무시"));
}

#[test]
fn arbitrary_stdin_does_not_change_the_output() {
    let baseline = instructions_for("");
    assert_eq!(instructions_for(r#"{"foo":"bar"}"#), baseline);
    assert_eq!(instructions_for("not json"), baseline);
    assert_eq!(instructions_for("garbage \x00 bytes"), baseline);
}
