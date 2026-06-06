//! E2e routing tests for `atelier git <...>`. Replaces git-utils
//! `tests/cli.test.ts` (which tested the now-obsolete custom `parseArgs`);
//! clap owns parsing, so these cover the routing surface instead.

use assert_cmd::Command;
use predicates::prelude::*;

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
