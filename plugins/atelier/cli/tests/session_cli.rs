//! End-to-end black-box tests for `atelier session ...`. The session surface
//! backs Stop hooks, where exit 2 means "block on stderr" — so its one hard
//! contract is that nothing handed to it makes the binary exit non-zero, not
//! even an argv an older binary would not recognise. That guarantee is what
//! lets the shims stay a plain `exec`.

use assert_cmd::Command;
use predicates::prelude::*;

fn atelier() -> Command {
    Command::cargo_bin("atelier").expect("locate `atelier` cargo binary")
}

#[test]
fn session_unknown_subcommand_exits_zero() {
    // A shim shipped with a newer plugin invoking an older binary: clap's own
    // exit 2 would read as "block" and wedge every session end.
    atelier()
        .args(["session", "no-such-command"])
        .write_stdin("")
        .assert()
        .success();
}

#[test]
fn session_help_exits_zero() {
    atelier()
        .args(["session", "--help"])
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::str::contains("push-check"));
}
