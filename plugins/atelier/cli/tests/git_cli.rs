//! End-to-end black-box tests for `atelier git ...` — the routing layer,
//! analogous to the dispatch portion of `git-utils/tests/cli.test.ts`. The TS
//! `parseArgs` unit tests are not ported because clap replaces that bespoke
//! parser; these scenarios lock the observable binary surface instead.

use assert_cmd::Command;
use predicates::prelude::*;

fn atelier() -> Command {
    Command::cargo_bin("atelier").expect("locate `atelier` cargo binary")
}

#[test]
fn git_help_exits_zero() {
    atelier()
        .args(["git", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("commit").and(predicate::str::contains("branch")));
}

#[test]
fn git_no_subcommand_prints_usage_and_exits_zero() {
    // Matches the standalone git-utils CLI: no args prints usage + exit 0,
    // not clap's default missing-subcommand error (exit 2).
    atelier()
        .arg("git")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

#[test]
fn git_version_exits_zero() {
    atelier()
        .args(["git", "--version"])
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"\d+\.\d+\.\d+").unwrap());
}

#[test]
fn git_unknown_command_errors() {
    atelier().args(["git", "unknown-cmd"]).assert().failure();
}

#[test]
fn git_commit_dispatches_to_handler() {
    // commit with empty description triggers validation before any git op,
    // exits 1 with the handled-error message on stderr.
    atelier()
        .args(["git", "commit", "feat", ""])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("Description is required"));
}

#[test]
fn git_commit_invalid_type_dispatches() {
    atelier()
        .args(["git", "commit", "bogus", "some description"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("Invalid commit type"));
}

#[test]
fn git_guard_missing_target_usage() {
    atelier()
        .args(["git", "guard"])
        .write_stdin("{}")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn git_hook_list_empty_in_temp_project() {
    // Point project-dir at a fresh temp dir so there is no settings.json; list
    // returns an empty object and exits 0.
    let tmp = tempfile::TempDir::new().unwrap();
    atelier()
        .args([
            "git",
            "hook",
            "list",
            "--project-dir",
            tmp.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("{}"));
}

#[test]
fn git_hook_register_then_list_roundtrip() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().to_str().unwrap();
    atelier()
        .args([
            "git",
            "hook",
            "register",
            "Stop",
            "*",
            "bash hook.sh",
            "--project-dir",
            dir,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("created"));

    atelier()
        .args(["git", "hook", "list", "--project-dir", dir])
        .assert()
        .success()
        .stdout(predicate::str::contains("Stop").and(predicate::str::contains("bash hook.sh")));
}
