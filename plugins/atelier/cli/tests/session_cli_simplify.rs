//! End-to-end wiring test for `atelier session simplify-check`: what reaches
//! stdout must be the Stop `additionalContext` document, because a Stop hook's
//! plain stdout never reaches the model.

use assert_cmd::Command;
use std::path::Path;
use std::process::Command as Process;

const SESSION_ID: &str = "wiring-test-01";

fn atelier(state_dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("atelier").expect("locate `atelier` cargo binary");
    cmd.env("TMPDIR", state_dir);
    cmd
}

fn git(repo: &Path, args: &[&str]) {
    let status = Process::new("git")
        .args(args)
        .current_dir(repo)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

#[test]
fn simplify_check_prints_stop_additional_context_document() {
    let repo = tempfile::TempDir::new().unwrap();
    let state = tempfile::TempDir::new().unwrap();
    let repo_path = repo.path();
    let project_dir = repo_path.to_str().unwrap();
    let payload = format!(r#"{{"session_id":"{SESSION_ID}","cwd":"{project_dir}"}}"#);

    git(repo_path, &["init", "-q"]);

    atelier(state.path())
        .args(["session", "baseline", "--project-dir", project_dir])
        .write_stdin(payload.clone())
        .assert()
        .success();

    // Staged, not untracked: `status.showUntrackedFiles=no` in a user's git
    // config would otherwise hide the file from the dirty set.
    std::fs::write(repo_path.join("a.rs"), "fn main() {}\n").unwrap();
    git(repo_path, &["add", "a.rs"]);

    let output = atelier(state.path())
        .args(["session", "simplify-check", "--project-dir", project_dir])
        .write_stdin(payload)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let doc: serde_json::Value =
        serde_json::from_slice(&output).expect("stdout is a single JSON document");
    assert_eq!(doc["hookSpecificOutput"]["hookEventName"], "Stop");
    let context = doc["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext is a string");
    assert!(context.contains("a.rs"));
}
