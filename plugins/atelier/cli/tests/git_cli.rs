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
        .stderr(predicate::str::contains("Usage"))
        .stderr(predicate::str::contains("pr"));
}

#[test]
fn git_guard_pr_non_matching_command_passes() {
    // A non-`gh pr create` command passes the PR guard before any gh lookup,
    // so this is deterministic without a gh binary or network.
    atelier()
        .args(["git", "guard", "pr"])
        .write_stdin(r#"{"tool_input":{"command":"echo hello"}}"#)
        .assert()
        .code(0);
}

#[test]
fn git_pr_guard_alias_non_matching_command_passes() {
    // Legacy alias of `guard pr` — must keep the same contract for hooks
    // registered before the unified guard surface.
    atelier()
        .args(["git", "pr-guard"])
        .write_stdin(r#"{"tool_input":{"command":"echo hello"}}"#)
        .assert()
        .code(0);
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

/// Bare remote + local clone on `main` with `origin/HEAD` set, so the CLI can
/// resolve a default branch. Mirrors `git_core_git.rs`'s setup; kept inline
/// (2 call sites, below the rust-test.md factory-extraction threshold).
fn repo_with_default_branch() -> (tempfile::TempDir, tempfile::TempDir) {
    use std::process::Command as Proc;
    fn run(args: &[&str], cwd: &std::path::Path) {
        let ok = Proc::new(args[0])
            .args(&args[1..])
            .current_dir(cwd)
            .status()
            .unwrap()
            .success();
        assert!(ok, "command failed: {args:?}");
    }
    let remote = tempfile::TempDir::new().unwrap();
    run(&["git", "init", "--bare"], remote.path());
    let local = tempfile::TempDir::new().unwrap();
    run(&["git", "init", "-b", "main"], local.path());
    run(&["git", "config", "user.email", "t@t.com"], local.path());
    run(&["git", "config", "user.name", "t"], local.path());
    run(&["git", "config", "commit.gpgsign", "false"], local.path());
    std::fs::write(local.path().join("README.md"), "x").unwrap();
    run(&["git", "add", "."], local.path());
    run(&["git", "commit", "-m", "init"], local.path());
    run(
        &[
            "git",
            "remote",
            "add",
            "origin",
            remote.path().to_str().unwrap(),
        ],
        local.path(),
    );
    run(&["git", "push", "-u", "origin", "main"], local.path());
    run(
        &[
            "git",
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
        local.path(),
    );
    (remote, local)
}

#[test]
fn git_default_branch_prints_plain_name() {
    // Locks the deliberate contract: a bare scalar (`main\n`), NOT the JSON the
    // other subcommands emit — exact stdout match proves no braces/quotes.
    let (_remote, local) = repo_with_default_branch();
    atelier()
        .args([
            "git",
            "default-branch",
            "--project-dir",
            local.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout("main\n");
}

#[test]
fn git_default_branch_no_remote_errors() {
    // No remote → detection exhausts all methods → handled error on stderr, exit 1
    // (so setup omits `--default-branch` and falls back to the guard's runtime detection).
    let tmp = tempfile::TempDir::new().unwrap();
    atelier()
        .args([
            "git",
            "default-branch",
            "--project-dir",
            tmp.path().to_str().unwrap(),
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("Error:"));
}
