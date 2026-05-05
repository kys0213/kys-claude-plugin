//! End-to-end black-box tests for the `autopilot` binary.
//!
//! Unlike the in-process tests under `tests/*_tests.rs` (which construct
//! `TaskService` directly and exercise pure Rust APIs), these tests **spawn
//! the actual binary** via `assert_cmd`. They cover the `clap` argparse
//! layer, the routing in `main.rs`, and the real `stdout` / `stderr` /
//! exit-code surface that operators see — the gap C1 of `cli-ux-hardening`
//! is filling.
//!
//! Each test owns a `TempDir` workspace and points the binary's SQLite
//! store there via `AUTOPILOT_DB_PATH` (the env var honored by
//! `task_store_db_path` in `main.rs`). The temp dir is also used as the
//! process `current_dir` so that `Config::load` does not accidentally pick
//! up an `autopilot.toml` from the developer's checkout.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// Isolated workspace for a single test invocation.
///
/// Holds a `TempDir` (kept alive for the duration of the test) and hands
/// out fresh `Command` builders preconfigured with:
/// - `AUTOPILOT_DB_PATH` -> `<tempdir>/state.db` so the SQLite store is
///   private to this test.
/// - `current_dir` set to the tempdir so the binary does not pick up the
///   repository's `autopilot.toml` via the default lookup in
///   `load_config`.
///
/// Designed so that follow-up tasks (C2/C3/C4) can write 1- to 2-line
/// scenarios on top: `Workspace::new().cmd().args([...]).assert()...`.
pub struct Workspace {
    dir: TempDir,
}

impl Workspace {
    /// Create a fresh isolated workspace.
    pub fn new() -> Self {
        let dir = TempDir::new().expect("create tempdir for autopilot e2e workspace");
        Self { dir }
    }

    /// Path to the SQLite store this workspace will use. The binary
    /// honors `AUTOPILOT_DB_PATH` (see `main.rs::task_store_db_path`) and
    /// will create the file lazily on first command.
    pub fn db_path(&self) -> std::path::PathBuf {
        self.dir.path().join("state.db")
    }

    /// Build a fresh `Command` for the `autopilot` binary, scoped to this
    /// workspace. Each call returns a new builder so callers may chain
    /// `.args(...)` without leaking state between invocations.
    pub fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("autopilot").expect("locate `autopilot` cargo binary");
        cmd.current_dir(self.dir.path())
            .env("AUTOPILOT_DB_PATH", self.db_path());
        cmd
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

#[test]
fn e2e_help_subcommand_exits_zero() {
    // Sanity: the binary boots, clap renders its help, exit code is 0,
    // and stdout mentions the binary name. If this fails the harness
    // itself is broken — every other e2e scenario depends on it.
    let ws = Workspace::new();
    ws.cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("autopilot"));
}

#[test]
fn e2e_task_add_then_list_shows_task() {
    // Happy-path round-trip across three commands sharing one SQLite
    // store: `epic create` bootstraps the epic, `task add` inserts a
    // watch-style task, `task list` renders it. Validates that argparse
    // routing, the env-var-driven store path, and the task table format
    // line up end-to-end. Subsequent C2/C3/C4 scenarios layer assertions
    // about UX edges on top of this same shape.
    let ws = Workspace::new();
    let task_id = "abc123def456"; // 12 hex chars, matches deterministic id format
    let title = "c1 demo task";

    ws.cmd()
        .args([
            "epic",
            "create",
            "--name",
            "demo",
            "--spec",
            "specs/demo.md",
        ])
        .assert()
        .success();

    ws.cmd()
        .args(["task", "add", task_id, "--epic", "demo", "--title", title])
        .assert()
        .success();

    ws.cmd()
        .args(["task", "list", "--epic", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains(task_id).and(predicate::str::contains(title)));
}
