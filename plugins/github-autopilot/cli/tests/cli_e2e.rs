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

use std::io::Write;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::{NamedTempFile, TempDir};

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

// ---------- C2: command-surface lifecycle scenarios ----------
//
// The scenarios below exercise multi-command flows end-to-end, asserting that
// adjacent subcommands compose naturally and that state transitions are
// observable through the public CLI surface (no peeking at the SQLite file).
// They are deliberately written against **current** behavior — sharp edges
// (e.g. lax id validation, silent path acceptance) are flagged with
// `TODO(C3)` so the validation pass can flip the assertion.

/// 1. Full task lifecycle: epic create → task add → list (ready) → claim
///    (Ready→Wip) → list (wip) → complete (Wip→Done) → list (done). Exercises
///    every state transition the watch-style task takes through its life.
#[test]
fn e2e_task_lifecycle_full_flow() {
    let ws = Workspace::new();
    let task_id = "abc123def456";

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
        .args([
            "task",
            "add",
            task_id,
            "--epic",
            "demo",
            "--title",
            "lifecycle task",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("inserted task"));

    // Fresh watch task starts Ready (no deps).
    ws.cmd()
        .args(["task", "list", "--epic", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains(task_id).and(predicate::str::contains("ready")));

    // claim flips Ready -> Wip and renders the claimed task.
    ws.cmd()
        .args(["task", "claim", "--epic", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains(task_id).and(predicate::str::contains("status:")));

    ws.cmd()
        .args(["task", "list", "--epic", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("wip"));

    // complete flips Wip -> Done and reports unblocked dependents (none here).
    ws.cmd()
        .args(["task", "complete", task_id, "--pr", "42"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("completed task")
                .and(predicate::str::contains("newly ready: (none)")),
        );

    ws.cmd()
        .args(["task", "list", "--epic", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("done"));

    // Once Done, no more ready work — claim signals "no ready tasks" via exit 1.
    ws.cmd()
        .args(["task", "claim", "--epic", "demo"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("no ready tasks"));
}

/// 2. `task add` is fingerprint-idempotent: re-adding the same fingerprint
///    (with a different task id, simulating a watcher firing twice) does not
///    create a duplicate row, exits 0, and reports "duplicate of <id>". The
///    final `task list` shows one task, not two.
///
/// TODO(C3): exit code is 0 for "duplicate fingerprint, different id" but 1
/// for "duplicate id" — both are arguably "already present". C3 should
/// decide whether to unify these and/or surface the distinction explicitly.
#[test]
fn e2e_task_add_same_fingerprint_is_idempotent() {
    let ws = Workspace::new();

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

    let fingerprint = "0xDEADBEEFCAFEBABE";

    ws.cmd()
        .args([
            "task",
            "add",
            "aaa111aaa111",
            "--epic",
            "demo",
            "--title",
            "first watcher hit",
            "--fingerprint",
            fingerprint,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("inserted task aaa111aaa111"));

    // Same fingerprint, fresh id — store recognizes the dedup and reports it
    // as a duplicate of the original. Exit 0: this is the happy "watcher fired
    // again, no new work" path, not a user error.
    ws.cmd()
        .args([
            "task",
            "add",
            "bbb222bbb222",
            "--epic",
            "demo",
            "--title",
            "second watcher hit (same fingerprint)",
            "--fingerprint",
            fingerprint,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("duplicate of task aaa111aaa111"));

    // Only one task should be visible in the list — the second add was deduped.
    ws.cmd()
        .args(["task", "list", "--epic", "demo"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("aaa111aaa111")
                .and(predicate::str::contains("bbb222bbb222").not()),
        );
}

/// 3. Epic lifecycle: create → get (active) → status (zeroed counts) →
///    complete → get (completed, with completed_at) → list filtered by
///    `--status active` (empty) and `--status completed` (shows it). The
///    binary has no separate `activate` step — `epic create` produces an
///    Active epic directly, so the natural lifecycle is create → use →
///    complete (or abandon).
#[test]
fn e2e_epic_create_status_complete_flow() {
    let ws = Workspace::new();

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
        .success()
        .stdout(predicate::str::contains("epic 'demo' created"));

    // TODO(C3): `epic create --spec` accepts a path that does not exist on
    // disk. C3 should decide whether to validate or document the behavior.

    ws.cmd()
        .args(["epic", "get", "demo"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("name:")
                .and(predicate::str::contains("demo"))
                .and(predicate::str::contains("status:"))
                .and(predicate::str::contains("active")),
        );

    // Empty epic — every status bucket should be 0.
    ws.cmd()
        .args(["epic", "status", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("demo").and(predicate::str::contains("active")));

    ws.cmd()
        .args(["epic", "complete", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("epic 'demo' completed"));

    // After completion, `get` reflects the new status and surfaces completed_at.
    ws.cmd()
        .args(["epic", "get", "demo"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("completed").and(predicate::str::contains("completed_at:")),
        );

    // Filtering by status: completed epic disappears from `--status active`
    // and reappears under `--status completed`.
    ws.cmd()
        .args(["epic", "list", "--status", "active"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(no epics)"));

    ws.cmd()
        .args(["epic", "list", "--status", "completed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("demo"));
}

/// 4. Block / unblock: a child task that depends on a parent starts Pending
///    and is **not** claimable; only the parent (Ready) gets claimed. Once
///    the parent is `complete`d, the store auto-promotes the child to Ready
///    (reported on the completion line as "newly ready: <child>") and the
///    next `claim` picks it up.
///
///    Note: the binary surfaces deps only via `epic reconcile <plan.jsonl>`
///    — `task add` itself has no `--blocked-by` flag (deps come from spec
///    decomposition, never from watcher inserts). This test uses the JSONL
///    plan that `reconcile` consumes to set up the dependency.
#[test]
fn e2e_blocked_task_becomes_claimable_after_parent_completes() {
    let ws = Workspace::new();

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

    // Reconcile plan: parent (aaaa) and child (bbbb) where bbbb depends on aaaa.
    let mut plan_file = NamedTempFile::new().expect("create reconcile plan tempfile");
    writeln!(
        plan_file,
        r#"{{"kind":"task","id":"aaaaaaaaaaaa","title":"parent","source":"decompose"}}"#
    )
    .unwrap();
    writeln!(
        plan_file,
        r#"{{"kind":"task","id":"bbbbbbbbbbbb","title":"child","source":"decompose"}}"#
    )
    .unwrap();
    writeln!(
        plan_file,
        r#"{{"kind":"dep","task":"bbbbbbbbbbbb","depends_on":"aaaaaaaaaaaa"}}"#
    )
    .unwrap();
    plan_file.flush().unwrap();

    ws.cmd()
        .args([
            "epic",
            "reconcile",
            "--name",
            "demo",
            "--plan",
            plan_file.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    // Parent starts Ready, child starts Pending (blocked on parent).
    ws.cmd()
        .args(["task", "list", "--epic", "demo", "--status", "ready"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("aaaaaaaaaaaa")
                .and(predicate::str::contains("bbbbbbbbbbbb").not()),
        );
    ws.cmd()
        .args(["task", "list", "--epic", "demo", "--status", "pending"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("bbbbbbbbbbbb")
                .and(predicate::str::contains("aaaaaaaaaaaa").not()),
        );

    // First claim picks up only the parent.
    ws.cmd()
        .args(["task", "claim", "--epic", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("aaaaaaaaaaaa"));

    // Second claim — child is still Pending, so claim signals "no ready" via exit 1.
    ws.cmd()
        .args(["task", "claim", "--epic", "demo"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("no ready tasks"));

    // Completing the parent unblocks the child. The completion line itself
    // surfaces the newly-ready set — agents rely on this to chain work.
    ws.cmd()
        .args(["task", "complete", "aaaaaaaaaaaa", "--pr", "7"])
        .assert()
        .success()
        .stdout(predicate::str::contains("newly ready: bbbbbbbbbbbb"));

    // Now the child is claimable.
    ws.cmd()
        .args(["task", "claim", "--epic", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("bbbbbbbbbbbb"));
}

/// 5. `task list` filter sanity: `--epic` scopes to one epic, `--status`
///    narrows further. Two epics with mixed-status tasks make the filter
///    semantics observable in a single workspace.
#[test]
fn e2e_task_list_filters_by_epic_and_status() {
    let ws = Workspace::new();

    for epic in ["alpha", "beta"] {
        ws.cmd()
            .args([
                "epic",
                "create",
                "--name",
                epic,
                "--spec",
                &format!("specs/{epic}.md"),
            ])
            .assert()
            .success();
    }

    // Two tasks on alpha (one will be claimed -> Wip), one on beta (stays Ready).
    ws.cmd()
        .args([
            "task",
            "add",
            "aaa000000001",
            "--epic",
            "alpha",
            "--title",
            "alpha-1",
        ])
        .assert()
        .success();
    ws.cmd()
        .args([
            "task",
            "add",
            "aaa000000002",
            "--epic",
            "alpha",
            "--title",
            "alpha-2",
        ])
        .assert()
        .success();
    ws.cmd()
        .args([
            "task",
            "add",
            "bbb000000001",
            "--epic",
            "beta",
            "--title",
            "beta-1",
        ])
        .assert()
        .success();

    // Claim one task on alpha so we have at least one Wip and one Ready under alpha.
    ws.cmd()
        .args(["task", "claim", "--epic", "alpha"])
        .assert()
        .success();

    // --epic alpha must not leak the beta task.
    ws.cmd()
        .args(["task", "list", "--epic", "alpha"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("aaa000000001")
                .and(predicate::str::contains("aaa000000002"))
                .and(predicate::str::contains("bbb000000001").not()),
        );

    // --status ready under alpha shows exactly the un-claimed alpha task.
    ws.cmd()
        .args(["task", "list", "--epic", "alpha", "--status", "ready"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ready"));

    // --status wip under alpha shows exactly the claimed task.
    ws.cmd()
        .args(["task", "list", "--epic", "alpha", "--status", "wip"])
        .assert()
        .success()
        .stdout(predicate::str::contains("wip"));

    // --epic beta is independent — only the beta task surfaces.
    ws.cmd()
        .args(["task", "list", "--epic", "beta"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("bbb000000001")
                .and(predicate::str::contains("aaa000000001").not()),
        );
}
