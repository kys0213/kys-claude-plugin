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
use serde_json::Value;
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

    /// Create an empty file at `relative_path` inside the workspace,
    /// creating parent directories as needed. Useful for satisfying
    /// `epic create`'s "spec file must exist" precondition without
    /// hard-coding contents the ledger never reads.
    pub fn touch(&self, relative_path: &str) -> std::path::PathBuf {
        let abs = self.dir.path().join(relative_path);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).expect("create parent dirs for spec file");
        }
        std::fs::File::create(&abs).expect("create spec file");
        abs
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

// ---------------------------------------------------------------------------
// JSON output schema lock-in (C4)
// ---------------------------------------------------------------------------
//
// Agents that integrate with this CLI parse the `--json` payloads — every
// field rename, removal, or type change is a breaking change for them.
// The tests below assert each documented field explicitly so that any such
// shift forces an explicit update here, rather than silently slipping
// through. Snapshot libraries are deliberately avoided: the verbosity is
// the lock.
//
// Non-deterministic fields (timestamps, sub-second-derived ids) are
// asserted only by type / format, not by exact value.

/// Helper: capture stdout from `cmd` and parse it as JSON. Panics with
/// useful context if the binary did not exit 0 or the output was not JSON.
fn run_json(cmd: &mut Command) -> Value {
    let output = cmd.output().expect("spawn autopilot binary");
    assert!(
        output.status.success(),
        "expected exit 0, got {:?}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout was not JSON: {e}\nstdout:\n{stdout}"))
}

/// Bring up an epic + a single Ready task in `ws`, returning the task id.
/// Used by the schema tests as the minimal shared scaffolding.
fn seed_epic_with_task(ws: &Workspace, epic: &str, task_id: &str, title: &str) {
    let spec_rel = format!("specs/{epic}.md");
    // `epic create` requires the spec file to exist on disk; the ledger
    // never reads it, so an empty stub is enough.
    ws.touch(&spec_rel);
    ws.cmd()
        .args(["epic", "create", "--name", epic, "--spec", &spec_rel])
        .assert()
        .success();
    ws.cmd()
        .args([
            "task",
            "add",
            task_id,
            "--epic",
            epic,
            "--title",
            title,
            "--body",
            "demo body",
        ])
        .assert()
        .success();
}

/// Assert the field set of the canonical `Task` JSON shape (used by
/// `task list`, `task get`, `task list-stale`, `task claim`,
/// `task find-by-pr`). Centralized so a Task field rename forces *one*
/// explicit edit visible across every consumer test.
fn assert_task_shape(task: &Value, expected_id: &str, expected_epic: &str, expected_title: &str) {
    // ---- string fields with stable values ----
    assert_eq!(task["id"], expected_id, "task.id");
    assert_eq!(task["epic_name"], expected_epic, "task.epic_name");
    assert_eq!(task["title"], expected_title, "task.title");

    // ---- enum-ish strings (value range pinned at the assertion site) ----
    assert!(task["source"].is_string(), "task.source must be string");
    assert!(task["status"].is_string(), "task.status must be string");

    // ---- numbers ----
    assert!(
        task["attempts"].is_u64(),
        "task.attempts must be unsigned int, got {:?}",
        task["attempts"]
    );

    // ---- nullable strings / numbers — type lock only ----
    assert!(
        task["fingerprint"].is_string() || task["fingerprint"].is_null(),
        "task.fingerprint must be string-or-null"
    );
    assert!(
        task["body"].is_string() || task["body"].is_null(),
        "task.body must be string-or-null"
    );
    assert!(
        task["branch"].is_string() || task["branch"].is_null(),
        "task.branch must be string-or-null"
    );
    assert!(
        task["pr_number"].is_u64() || task["pr_number"].is_null(),
        "task.pr_number must be unsigned int-or-null"
    );
    assert!(
        task["escalated_issue"].is_u64() || task["escalated_issue"].is_null(),
        "task.escalated_issue must be unsigned int-or-null"
    );

    // ---- timestamps — RFC3339 strings, exact value not pinned ----
    let created_at = task["created_at"].as_str().expect("task.created_at string");
    assert!(
        chrono::DateTime::parse_from_rfc3339(created_at).is_ok(),
        "task.created_at not RFC3339: {created_at}"
    );
    let updated_at = task["updated_at"].as_str().expect("task.updated_at string");
    assert!(
        chrono::DateTime::parse_from_rfc3339(updated_at).is_ok(),
        "task.updated_at not RFC3339: {updated_at}"
    );
}

#[test]
fn json_schema_task_list() {
    // Lock: `task list --json` -> JSON array of Task objects with the
    // canonical field set. Initial `task add` produces a Ready task with
    // attempts=0 and source=human (per the `--source` default).
    let ws = Workspace::new();
    let task_id = "abc123def456";
    let title = "schema demo";
    seed_epic_with_task(&ws, "demo", task_id, title);

    let json = run_json(ws.cmd().args(["task", "list", "--epic", "demo", "--json"]));

    assert!(json.is_array(), "task list --json must emit a JSON array");
    let tasks = json.as_array().unwrap();
    assert_eq!(tasks.len(), 1, "exactly one task seeded");

    let task = &tasks[0];
    assert_task_shape(task, task_id, "demo", title);
    assert_eq!(task["status"], "ready");
    assert_eq!(task["source"], "human");
    assert_eq!(task["attempts"], 0);
    assert_eq!(task["body"], "demo body");
    assert!(task["branch"].is_null());
    assert!(task["pr_number"].is_null());
    assert!(task["escalated_issue"].is_null());
}

#[test]
fn json_schema_task_get() {
    // Lock: `task get --json` -> single Task object. Same field set as
    // `task list` elements (both go through `render_task`).
    let ws = Workspace::new();
    let task_id = "abc123def456";
    let title = "schema demo";
    seed_epic_with_task(&ws, "demo", task_id, title);

    let json = run_json(ws.cmd().args(["task", "get", task_id, "--json"]));

    assert!(json.is_object(), "task get --json must emit a JSON object");
    assert_task_shape(&json, task_id, "demo", title);
    assert_eq!(json["status"], "ready");
    assert_eq!(json["attempts"], 0);
}

#[test]
fn json_schema_task_claim() {
    // Lock: `task claim --json` -> single Task object, status flipped to
    // "wip" and attempts incremented to 1. Schema is identical to
    // `task get`, but we additionally pin the post-claim values.
    let ws = Workspace::new();
    let task_id = "abc123def456";
    let title = "claim demo";
    seed_epic_with_task(&ws, "demo", task_id, title);

    let json = run_json(ws.cmd().args(["task", "claim", "--epic", "demo", "--json"]));

    assert!(json.is_object());
    assert_task_shape(&json, task_id, "demo", title);
    assert_eq!(json["status"], "wip", "claim must transition to wip");
    assert_eq!(json["attempts"], 1, "claim must increment attempts");
}

#[test]
fn json_schema_task_list_stale() {
    // Lock: `task list-stale --json` -> JSON array of Task objects. Same
    // shape as `task list`, just filtered to stale Wip claims. Empty case
    // is `[]` (always-emitted, even when no candidates) — that's also
    // part of the contract agents rely on.
    let ws = Workspace::new();
    let task_id = "abc123def456";
    let title = "stale demo";
    seed_epic_with_task(&ws, "demo", task_id, title);

    // Empty case: nothing is Wip yet.
    let empty = run_json(
        ws.cmd()
            .args(["task", "list-stale", "--before", "1s", "--json"]),
    );
    assert!(empty.is_array(), "list-stale --json must emit an array");
    assert_eq!(empty.as_array().unwrap().len(), 0, "no Wip tasks => empty");

    // Populate: claim flips Ready -> Wip with updated_at = now.
    ws.cmd()
        .args(["task", "claim", "--epic", "demo"])
        .assert()
        .success();

    // Wait long enough that the Wip claim is older than the cutoff.
    std::thread::sleep(std::time::Duration::from_millis(1100));

    let json = run_json(
        ws.cmd()
            .args(["task", "list-stale", "--before", "1s", "--json"]),
    );
    assert!(json.is_array());
    let stale = json.as_array().unwrap();
    assert_eq!(stale.len(), 1, "one stale Wip task expected");
    assert_task_shape(&stale[0], task_id, "demo", title);
    assert_eq!(stale[0]["status"], "wip");
}

#[test]
fn json_schema_task_release_stale() {
    // Lock: `task release-stale --json` -> JSON **array of task id
    // strings**, NOT an array of Task objects. This is the one place the
    // CLI emits a "thin" id list rather than full Task records, so we
    // pin both shape and element type.
    let ws = Workspace::new();
    let task_id = "abc123def456";
    seed_epic_with_task(&ws, "demo", task_id, "release-stale demo");
    ws.cmd()
        .args(["task", "claim", "--epic", "demo"])
        .assert()
        .success();
    std::thread::sleep(std::time::Duration::from_millis(1100));

    let json = run_json(
        ws.cmd()
            .args(["task", "release-stale", "--before", "1s", "--json"]),
    );
    assert!(
        json.is_array(),
        "release-stale --json must emit an array of ids"
    );
    let ids = json.as_array().unwrap();
    assert_eq!(ids.len(), 1, "one task should be released");
    let id = ids[0]
        .as_str()
        .expect("release-stale --json elements must be strings");
    assert_eq!(id, task_id);
}

#[test]
fn json_schema_task_find_by_pr() {
    // Lock: `task find-by-pr --json` -> single Task object. After
    // `task complete`, the task carries pr_number == requested PR and
    // status == "done"; everything else stays canonical.
    let ws = Workspace::new();
    let task_id = "abc123def456";
    seed_epic_with_task(&ws, "demo", task_id, "pr demo");
    ws.cmd()
        .args(["task", "claim", "--epic", "demo"])
        .assert()
        .success();
    ws.cmd()
        .args(["task", "complete", task_id, "--pr", "42"])
        .assert()
        .success();

    let json = run_json(ws.cmd().args(["task", "find-by-pr", "42", "--json"]));
    assert!(json.is_object());
    assert_task_shape(&json, task_id, "demo", "pr demo");
    assert_eq!(json["status"], "done");
    assert_eq!(json["pr_number"], 42);
}

#[test]
fn json_schema_task_fail() {
    // Lock: `task fail` always emits JSON (no `--json` toggle): a
    // {outcome, attempts} record. `outcome` is one of "retried" /
    // "escalated"; `attempts` is u32. First failure on a fresh task is
    // always "retried".
    let ws = Workspace::new();
    let task_id = "abc123def456";
    seed_epic_with_task(&ws, "demo", task_id, "fail demo");
    ws.cmd()
        .args(["task", "claim", "--epic", "demo"])
        .assert()
        .success();

    let json = run_json(ws.cmd().args(["task", "fail", task_id]));
    assert!(json.is_object(), "task fail emits a JSON object");
    assert_eq!(json["outcome"], "retried");
    assert!(
        json["attempts"].is_u64(),
        "attempts must be unsigned int, got {:?}",
        json["attempts"]
    );
    assert_eq!(json["attempts"], 1);
}

/// Assert the canonical `Epic` JSON shape (used by `epic list`,
/// `epic get`, `epic find-by-spec-path`).
fn assert_epic_shape(epic: &Value, expected_name: &str) {
    assert_eq!(epic["name"], expected_name, "epic.name");
    // `spec_path` serializes via `PathBuf`'s Display, which on Unix is the
    // raw path string. We pin the type, not the absolute exact value, since
    // path normalization may differ across platforms.
    assert!(
        epic["spec_path"].is_string(),
        "epic.spec_path must be string"
    );
    assert!(epic["branch"].is_string(), "epic.branch must be string");
    assert!(epic["status"].is_string(), "epic.status must be string");
    let created_at = epic["created_at"].as_str().expect("epic.created_at string");
    assert!(
        chrono::DateTime::parse_from_rfc3339(created_at).is_ok(),
        "epic.created_at not RFC3339: {created_at}"
    );
    // `completed_at` is null while the epic is active; type lock only.
    assert!(
        epic["completed_at"].is_string() || epic["completed_at"].is_null(),
        "epic.completed_at must be RFC3339 string-or-null"
    );
}

#[test]
fn json_schema_epic_list() {
    // Lock: `epic list --json` -> array of Epic objects.
    let ws = Workspace::new();
    seed_epic_with_task(&ws, "demo", "abc123def456", "epic-list demo");

    let json = run_json(ws.cmd().args(["epic", "list", "--json"]));
    assert!(json.is_array(), "epic list --json must emit array");
    let epics = json.as_array().unwrap();
    assert_eq!(epics.len(), 1);

    let epic = &epics[0];
    assert_epic_shape(epic, "demo");
    assert_eq!(epic["status"], "active");
    assert_eq!(epic["branch"], "epic/demo");
    assert_eq!(epic["spec_path"], "specs/demo.md");
    assert!(epic["completed_at"].is_null());
}

#[test]
fn json_schema_epic_get() {
    // Lock: `epic get <name> --json` -> single Epic object.
    let ws = Workspace::new();
    seed_epic_with_task(&ws, "demo", "abc123def456", "epic-get demo");

    let json = run_json(ws.cmd().args(["epic", "get", "demo", "--json"]));
    assert!(json.is_object());
    assert_epic_shape(&json, "demo");
    assert_eq!(json["status"], "active");
    assert_eq!(json["branch"], "epic/demo");
    assert_eq!(json["spec_path"], "specs/demo.md");
}

#[test]
fn json_schema_epic_find_by_spec_path() {
    // Lock: `epic find-by-spec-path <path> --json` -> single Epic object,
    // identical schema to `epic get`.
    let ws = Workspace::new();
    seed_epic_with_task(&ws, "demo", "abc123def456", "find-by-spec demo");

    let json = run_json(
        ws.cmd()
            .args(["epic", "find-by-spec-path", "specs/demo.md", "--json"]),
    );
    assert!(json.is_object());
    assert_epic_shape(&json, "demo");
    assert_eq!(json["status"], "active");
}

#[test]
fn json_schema_epic_status() {
    // Lock: `epic status --json` -> array of EpicStatusReport. Each
    // element: { epic, status, total, counts: { pending, ready, wip,
    // blocked, done, escalated } }. The seeded epic has exactly one
    // Ready task, so total=1 and counts.ready=1; all others 0.
    let ws = Workspace::new();
    seed_epic_with_task(&ws, "demo", "abc123def456", "status demo");

    let json = run_json(ws.cmd().args(["epic", "status", "--json"]));
    assert!(json.is_array(), "epic status --json must emit array");
    let reports = json.as_array().unwrap();
    assert_eq!(reports.len(), 1);

    let report = &reports[0];
    assert_eq!(report["epic"], "demo");
    assert_eq!(report["status"], "active");
    // `total` is a usize on the Rust side; serializes as an unsigned int.
    assert!(
        report["total"].is_u64(),
        "report.total must be unsigned int"
    );
    assert_eq!(report["total"], 1);

    let counts = &report["counts"];
    assert!(counts.is_object(), "report.counts must be object");
    for field in ["pending", "ready", "wip", "blocked", "done", "escalated"] {
        assert!(
            counts[field].is_u64(),
            "report.counts.{field} must be unsigned int, got {:?}",
            counts[field]
        );
    }
    assert_eq!(counts["pending"], 0);
    assert_eq!(counts["ready"], 1);
    assert_eq!(counts["wip"], 0);
    assert_eq!(counts["blocked"], 0);
    assert_eq!(counts["done"], 0);
    assert_eq!(counts["escalated"], 0);
}

#[test]
fn json_schema_events_list() {
    // Lock: `events list --json` -> array of EventRecord objects.
    // Schema: { at: RFC3339 string, kind: snake_case string, epic:
    // string-or-null, task: string-or-null, payload: arbitrary JSON
    // (object/null/etc.) }. We seed one epic + one task, then claim it,
    // which should produce three deterministic event kinds in order:
    // epic_started, task_inserted, task_claimed.
    let ws = Workspace::new();
    let task_id = "abc123def456";
    seed_epic_with_task(&ws, "demo", task_id, "events demo");
    ws.cmd()
        .args(["task", "claim", "--epic", "demo"])
        .assert()
        .success();

    let json = run_json(ws.cmd().args(["events", "list", "--json"]));
    assert!(json.is_array(), "events list --json must emit array");
    let events = json.as_array().unwrap();
    assert_eq!(
        events.len(),
        3,
        "expected epic_started + task_inserted + task_claimed, got {events:?}"
    );

    for ev in events {
        // Per-event shape lock.
        let at = ev["at"].as_str().expect("event.at string");
        assert!(
            chrono::DateTime::parse_from_rfc3339(at).is_ok(),
            "event.at not RFC3339: {at}"
        );
        assert!(ev["kind"].is_string(), "event.kind must be string");
        assert!(
            ev["epic"].is_string() || ev["epic"].is_null(),
            "event.epic must be string-or-null"
        );
        assert!(
            ev["task"].is_string() || ev["task"].is_null(),
            "event.task must be string-or-null"
        );
        // payload is intentionally untyped JSON (per Event struct); the
        // contract is just "key present".
        assert!(
            ev.get("payload").is_some(),
            "event.payload key must be present"
        );
    }

    // Pin the canonical kinds + ordering. If a future refactor reorders or
    // renames events, the failure here points straight at the regression.
    assert_eq!(events[0]["kind"], "epic_started");
    assert_eq!(events[0]["epic"], "demo");
    assert!(events[0]["task"].is_null());

    assert_eq!(events[1]["kind"], "task_inserted");
    assert_eq!(events[1]["epic"], "demo");
    assert_eq!(events[1]["task"], task_id);

    assert_eq!(events[2]["kind"], "task_claimed");
    assert_eq!(events[2]["epic"], "demo");
    assert_eq!(events[2]["task"], task_id);
}

#[test]
fn json_schema_task_list_header_behavior() {
    // Lock current behavior: when `--json` is NOT passed, `task list`
    // always prints the human-readable column header
    // ("ID            STATUS     ATTEMPTS  TITLE"), regardless of
    // tty / piping. The C4 brief surfaced this as a sharp edge — we
    // pick option (b): keep current behavior, document it via this
    // test, defer any `--no-header` / tty-detection work to a future
    // epic. If the default ever flips, this test failure is the
    // explicit signal that downstream agents (which today swallow the
    // header line by string-matching) need to be revisited.
    let ws = Workspace::new();
    seed_epic_with_task(&ws, "demo", "abc123def456", "header demo");

    ws.cmd()
        .args(["task", "list", "--epic", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "ID            STATUS     ATTEMPTS  TITLE",
        ));
}
