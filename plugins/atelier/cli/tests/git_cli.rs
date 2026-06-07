//! E2e routing tests for `atelier git <...>`. Replaces git-utils
//! `tests/cli.test.ts` (which tested the now-obsolete custom `parseArgs`);
//! clap owns parsing, so these cover the routing surface instead.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn atelier() -> Command {
    Command::cargo_bin("atelier").expect("locate `atelier` cargo binary")
}

#[test]
fn git_help_lists_commands() {
    atelier()
        .args(["git", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("branch").and(predicate::str::contains("commit")));
}

#[test]
fn git_dispatches_to_commit_validation() {
    // `commit feat ""` fails validation (empty description) before any git
    // side effects — proves routing reaches the command handler.
    atelier()
        .args(["git", "commit", "feat", ""])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("Description is required"));
}

#[test]
fn git_invalid_commit_type_errors() {
    atelier()
        .args(["git", "commit", "bogus", "some description"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("Invalid commit type"));
}

#[test]
fn git_unknown_subcommand_errors() {
    // clap rejects unknown subcommands (exit 2).
    atelier().args(["git", "unknown-cmd"]).assert().failure();
}

// ---------- guard (reads hook payload on stdin) ----------

#[test]
fn git_guard_allows_non_commit_command() {
    // A non-`git commit` Bash command is allowed (exit 0), proving the guard
    // reads the stdin payload and routes correctly. `--project-dir` is a temp
    // dir (not a repo), so no real git side effects.
    let t = TempDir::new().unwrap();
    atelier()
        .args([
            "git",
            "guard",
            "--target",
            "commit",
            "--project-dir",
            t.path().to_str().unwrap(),
        ])
        .write_stdin(r#"{"tool_input":{"command":"ls -la"}}"#)
        .assert()
        .success();
}

#[test]
fn git_guard_empty_stdin_allows() {
    // No payload → nothing to guard → allowed.
    let t = TempDir::new().unwrap();
    atelier()
        .args([
            "git",
            "guard",
            "--target",
            "write",
            "--project-dir",
            t.path().to_str().unwrap(),
        ])
        .write_stdin("")
        .assert()
        .success();
}

// ---------- hook (registers into .claude/settings.json) ----------

#[test]
fn git_hook_register_writes_settings() {
    let t = TempDir::new().unwrap();
    let dir = t.path().to_str().unwrap();
    atelier()
        .args([
            "git",
            "hook",
            "register",
            "--type",
            "PreToolUse",
            "--matcher",
            "Bash",
            "--command",
            "atelier git guard --target commit",
            "--project-dir",
            dir,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("created hook"));

    let settings = std::fs::read_to_string(t.path().join(".claude").join("settings.json")).unwrap();
    assert!(settings.contains("atelier git guard --target commit"));
}

#[test]
fn git_hook_list_reports_registered() {
    let t = TempDir::new().unwrap();
    let dir = t.path().to_str().unwrap();
    atelier()
        .args([
            "git",
            "hook",
            "register",
            "--type",
            "PreToolUse",
            "--matcher",
            "Bash",
            "--command",
            "x.sh",
            "--project-dir",
            dir,
        ])
        .assert()
        .success();
    atelier()
        .args(["git", "hook", "list", "--project-dir", dir])
        .assert()
        .success()
        .stdout(predicate::str::contains("PreToolUse"));
}
