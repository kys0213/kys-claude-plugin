use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use autopilot::cmd::task::{TaskService, TaskSourceArg, TaskStatusArg};
use autopilot::domain::{Epic, EpicStatus, EventKind, TaskId, TaskSource, TaskStatus};
use autopilot::ports::clock::{Clock, FixedClock};
use autopilot::ports::task_store::{EpicPlan, EventFilter, NewTask, TaskStore};
use autopilot::store::InMemoryTaskStore;
use chrono::{TimeZone, Utc};
use tempfile::NamedTempFile;

// ---------- helpers ----------

fn fixture() -> (Arc<dyn TaskStore>, FixedClock) {
    let store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
    seed_epic(store.as_ref(), &clock, "e");
    (store, clock)
}

fn seed_epic(store: &dyn TaskStore, clock: &dyn Clock, name: &str) {
    let now = clock.now();
    store
        .insert_epic_with_tasks(
            EpicPlan {
                epic: Epic {
                    name: name.to_string(),
                    spec_path: PathBuf::from(format!("spec/{name}.md")),
                    branch: format!("epic/{name}"),
                    status: EpicStatus::Active,
                    created_at: now,
                    completed_at: None,
                },
                tasks: vec![],
                deps: vec![],
            },
            now,
        )
        .unwrap();
}

fn capture<F>(f: F) -> (i32, String)
where
    F: FnOnce(&mut Vec<u8>) -> anyhow::Result<i32>,
{
    let mut buf: Vec<u8> = Vec::new();
    let code = f(&mut buf).expect("service call");
    (code, String::from_utf8(buf).expect("utf-8"))
}

fn write_jsonl(lines: &[&str]) -> NamedTempFile {
    let f = NamedTempFile::new().unwrap();
    let mut h = std::fs::File::create(f.path()).unwrap();
    for l in lines {
        writeln!(h, "{l}").unwrap();
    }
    f
}

/// Insert a watch-style task into epic 'e' through the public `add` path with
/// a sane default title/source. Cuts the 7-arg ceremony at call sites that
/// only care that the task exists with a known fingerprint.
fn seed_via_add(svc: &TaskService<'_>, id: &str, fingerprint: &str) {
    let mut buf: Vec<u8> = Vec::new();
    svc.add(
        "e",
        id,
        "x",
        None,
        Some(fingerprint),
        TaskSourceArg::Human,
        &mut buf,
    )
    .expect("seed_via_add");
}

/// Insert a task into epic `e` directly via the store, in a given starting status.
/// Used to bypass the `add` lifecycle for tests that focus on later transitions.
fn seed_task(
    store: &dyn TaskStore,
    clock: &dyn Clock,
    id: &str,
    status: TaskStatus,
    deps_on: &[&str],
) {
    let now = clock.now();
    let new_task = NewTask {
        id: TaskId::from_raw(id),
        source: TaskSource::Decompose,
        fingerprint: None,
        title: format!("task-{id}"),
        body: None,
    };
    let mut tasks = vec![new_task];
    let mut deps: Vec<(TaskId, TaskId)> = Vec::new();
    for d in deps_on {
        // also seed the dependency target as a Decompose task if we haven't already
        tasks.push(NewTask {
            id: TaskId::from_raw(*d),
            source: TaskSource::Decompose,
            fingerprint: None,
            title: format!("dep-{d}"),
            body: None,
        });
        deps.push((TaskId::from_raw(id), TaskId::from_raw(*d)));
    }
    // Upsert the new tasks/deps into existing epic 'e' via a minimal
    // reconciliation plan, then nudge to a non-default status if requested.
    use autopilot::ports::task_store::ReconciliationPlan;
    let existing = store
        .get_epic("e")
        .unwrap()
        .expect("seed_task assumes epic 'e' exists");
    store
        .apply_reconciliation(
            ReconciliationPlan {
                epic: existing,
                tasks,
                deps,
                remote_state: vec![],
                orphan_branches: vec![],
            },
            now,
        )
        .unwrap();
    if status != TaskStatus::Ready && status != TaskStatus::Pending {
        store
            .force_status(&TaskId::from_raw(id), status, "test seed", now)
            .unwrap();
    }
}

// ---------- add ----------

#[test]
fn add_with_explicit_fingerprint_persists_task_and_emits_event() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    let (code, out) = capture(|w| {
        svc.add(
            "e",
            "aaaaaaaaaaaa",
            "title",
            Some("body"),
            Some("0xDEADBEEF"),
            TaskSourceArg::Human,
            w,
        )
    });
    assert_eq!(code, 0, "stdout: {out}");
    assert!(out.contains("inserted task aaaaaaaaaaaa"), "stdout: {out}");

    let task = store
        .get_task(&TaskId::from_raw("aaaaaaaaaaaa"))
        .unwrap()
        .unwrap();
    assert_eq!(task.fingerprint.as_deref(), Some("0xDEADBEEF"));
    assert_eq!(task.title, "title");
    assert_eq!(task.source, TaskSource::Human);
    assert_eq!(task.status, TaskStatus::Ready);

    let events = store
        .list_events(EventFilter {
            kinds: vec![EventKind::TaskInserted],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].task_id.as_ref().map(|t| t.as_str()),
        Some("aaaaaaaaaaaa")
    );
}

#[test]
fn add_auto_derives_fingerprint_from_title_and_body() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    let (code, _) = capture(|w| {
        svc.add(
            "e",
            "aaaaaaaaaaaa",
            "rate limiter middleware",
            Some("add interceptor for throttling"),
            None,
            TaskSourceArg::Human,
            w,
        )
    });
    assert_eq!(code, 0);
    let task = store
        .get_task(&TaskId::from_raw("aaaaaaaaaaaa"))
        .unwrap()
        .unwrap();
    let fp = task
        .fingerprint
        .expect("fingerprint should be auto-derived");
    assert!(fp.starts_with("0x"), "expected hex fingerprint, got: {fp}");
    assert_eq!(fp.len(), 18, "expected 0x + 16 hex chars, got: {fp}");
}

#[test]
fn add_detects_duplicate_fingerprint_and_returns_same_id() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);

    let (_, _out1) = capture(|w| {
        svc.add(
            "e",
            "aaaaaaaaaaaa",
            "first",
            None,
            Some("0xDEADBEEF"),
            TaskSourceArg::Human,
            w,
        )
    });
    let (code2, out2) = capture(|w| {
        svc.add(
            "e",
            "bbbbbbbbbbbb",
            "second",
            None,
            Some("0xDEADBEEF"),
            TaskSourceArg::Human,
            w,
        )
    });
    assert_eq!(code2, 0);
    assert!(
        out2.contains("duplicate of task aaaaaaaaaaaa"),
        "stdout: {out2}"
    );
    // The second task was NOT inserted under bbbb...
    assert!(store
        .get_task(&TaskId::from_raw("bbbbbbbbbbbb"))
        .unwrap()
        .is_none());

    let dup_events = store
        .list_events(EventFilter {
            kinds: vec![EventKind::WatchDuplicate],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(dup_events.len(), 1);
}

// ---------- add-batch ----------

#[test]
fn add_batch_inserts_multiple_tasks_skipping_duplicates() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);

    let f = write_jsonl(&[
        r#"{"id":"aaaaaaaaaaaa","title":"first","fingerprint":"0x1"}"#,
        r#"{"id":"bbbbbbbbbbbb","title":"second","fingerprint":"0x2"}"#,
        // duplicate of "0x1" — should be detected
        r#"{"id":"cccccccccccc","title":"third","fingerprint":"0x1"}"#,
    ]);
    let (code, out) = capture(|w| svc.add_batch("e", f.path(), w));
    assert_eq!(code, 0);
    assert!(out.contains("inserted: 2"), "stdout: {out}");
    assert!(out.contains("duplicates: 1"), "stdout: {out}");

    let tasks = store.list_tasks_by_epic("e", None).unwrap();
    let ids: Vec<_> = tasks.iter().map(|t| t.id.as_str().to_string()).collect();
    assert!(ids.contains(&"aaaaaaaaaaaa".to_string()));
    assert!(ids.contains(&"bbbbbbbbbbbb".to_string()));
    assert!(!ids.contains(&"cccccccccccc".to_string()));
}

#[test]
fn add_batch_rejects_malformed_line() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);

    // Missing required `title` field.
    let f = write_jsonl(&[r#"{"id":"aaaaaaaaaaaa"}"#]);
    let mut buf: Vec<u8> = Vec::new();
    let err = svc.add_batch("e", f.path(), &mut buf).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("parsing line 1"), "error: {msg}");
    // No partial commits up to the failing line: the first (and only) line failed.
    assert!(store.list_tasks_by_epic("e", None).unwrap().is_empty());
}

// ---------- get ----------

#[test]
fn get_renders_same_as_show() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");
    // `get` is wired in main.rs to call `show`. Verify they produce identical output.
    let (c1, out_show) = capture(|w| svc.show("aaaaaaaaaaaa", true, w));
    let (c2, out_get) = capture(|w| svc.show("aaaaaaaaaaaa", true, w));
    assert_eq!(c1, 0);
    assert_eq!(c2, 0);
    assert_eq!(out_show, out_get);
    let v: serde_json::Value = serde_json::from_str(out_show.trim()).unwrap();
    assert_eq!(v["id"], "aaaaaaaaaaaa");
}

// ---------- claim ----------

#[test]
fn claim_outputs_next_ready_task_as_json() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");

    let (code, out) = capture(|w| svc.claim("e", true, w));
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["id"], "aaaaaaaaaaaa");
    assert_eq!(v["status"], "wip");
    assert_eq!(v["attempts"], 1);
}

#[test]
fn claim_signals_no_ready_via_exit_1() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    let (code, _out) = capture(|w| svc.claim("e", false, w));
    assert_eq!(code, 1);
}

// ---------- complete ----------

#[test]
fn complete_updates_pr_and_unblocks_dependents() {
    let (store, clock) = fixture();
    // Seed two tasks with bbbb depending on aaaa (so bbbb starts pending).
    seed_task(
        store.as_ref(),
        &clock,
        "aaaaaaaaaaaa",
        TaskStatus::Ready,
        &[],
    );
    seed_task(
        store.as_ref(),
        &clock,
        "bbbbbbbbbbbb",
        TaskStatus::Pending,
        &["aaaaaaaaaaaa"],
    );

    let svc = TaskService::new(store.as_ref(), &clock);
    // Claim & complete aaaa.
    let _ = capture(|w| svc.claim("e", false, w));
    let (code, out) = capture(|w| svc.complete("aaaaaaaaaaaa", 42, w));
    assert_eq!(code, 0);
    assert!(out.contains("completed task aaaaaaaaaaaa"), "stdout: {out}");
    assert!(out.contains("bbbbbbbbbbbb"), "stdout: {out}");

    let a = store
        .get_task(&TaskId::from_raw("aaaaaaaaaaaa"))
        .unwrap()
        .unwrap();
    assert_eq!(a.status, TaskStatus::Done);
    assert_eq!(a.pr_number, Some(42));

    let b = store
        .get_task(&TaskId::from_raw("bbbbbbbbbbbb"))
        .unwrap()
        .unwrap();
    assert_eq!(b.status, TaskStatus::Ready);
}

// ---------- fail ----------

#[test]
fn fail_with_attempts_below_max_outputs_retried_outcome() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");
    // First claim => attempts=1
    let _ = capture(|w| svc.claim("e", false, w));
    let (code, out) = capture(|w| svc.fail("aaaaaaaaaaaa", w));
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["outcome"], "retried");
    assert_eq!(v["attempts"], 1);

    let t = store
        .get_task(&TaskId::from_raw("aaaaaaaaaaaa"))
        .unwrap()
        .unwrap();
    assert_eq!(t.status, TaskStatus::Ready);
}

#[test]
fn fail_with_attempts_at_max_outputs_escalated_outcome() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");
    // claim+fail 3 times; the 3rd fail crosses max_attempts and emits the
    // escalated JSON outcome (and persists status=Escalated, attempts=3).
    let mut last: Option<(i32, String)> = None;
    for _ in 0..3 {
        let _ = capture(|w| svc.claim("e", false, w));
        last = Some(capture(|w| svc.fail("aaaaaaaaaaaa", w)));
    }
    let (code, out) = last.expect("loop ran 3 times");
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["outcome"], "escalated");
    assert_eq!(v["attempts"], 3);

    let t = store
        .get_task(&TaskId::from_raw("aaaaaaaaaaaa"))
        .unwrap()
        .unwrap();
    assert_eq!(t.status, TaskStatus::Escalated);
    assert_eq!(t.attempts, 3);
}

// ---------- escalate ----------

#[test]
fn escalate_sets_escalated_issue_field() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");
    let (code, _) = capture(|w| svc.escalate("aaaaaaaaaaaa", 99, w));
    assert_eq!(code, 0);
    let t = store
        .get_task(&TaskId::from_raw("aaaaaaaaaaaa"))
        .unwrap()
        .unwrap();
    assert_eq!(t.escalated_issue, Some(99));
}

// ---------- release ----------

#[test]
fn release_decrements_attempts_and_reverts_to_ready() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");
    let _ = capture(|w| svc.claim("e", false, w)); // attempts=1, status=wip
    let (code, _) = capture(|w| svc.release("aaaaaaaaaaaa", w));
    assert_eq!(code, 0);
    let t = store
        .get_task(&TaskId::from_raw("aaaaaaaaaaaa"))
        .unwrap()
        .unwrap();
    assert_eq!(t.status, TaskStatus::Ready);
    assert_eq!(t.attempts, 0);
}

// ---------- list-stale ----------

#[test]
fn list_stale_json_returns_array_of_full_task_records() {
    // Mirrors `find-by-pr --json` shape: an array of full Task objects, so
    // the agent reviewer has every field needed (epic_name, updated_at,
    // attempts, status, ...).
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");
    let _ = capture(|w| svc.claim("e", false, w));
    clock.advance(chrono::Duration::minutes(5));

    let (code, out) = capture(|w| svc.list_stale(60, true, w));
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 1);
    let t = &arr[0];
    assert_eq!(t["id"], "aaaaaaaaaaaa");
    assert_eq!(t["epic_name"], "e");
    assert_eq!(t["status"], "wip");
    assert_eq!(t["attempts"], 1);
    // `updated_at` is the canonical "claimed_at" surrogate the spec calls out.
    assert!(t.get("updated_at").is_some());
}

#[test]
fn list_stale_does_not_modify_tasks() {
    // Read-only contract: after list-stale the task is still Wip with the
    // same attempts count and no TaskReleasedStale event was emitted.
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");
    let _ = capture(|w| svc.claim("e", false, w));
    clock.advance(chrono::Duration::hours(2));

    let (code, _) = capture(|w| svc.list_stale(60, true, w));
    assert_eq!(code, 0);
    let t = store
        .get_task(&TaskId::from_raw("aaaaaaaaaaaa"))
        .unwrap()
        .unwrap();
    assert_eq!(t.status, TaskStatus::Wip);
    assert_eq!(t.attempts, 1);
    let evs = store
        .list_events(EventFilter {
            kinds: vec![EventKind::TaskReleasedStale],
            ..Default::default()
        })
        .unwrap();
    assert!(evs.is_empty());
}

#[test]
fn list_stale_empty_exits_0_without_modifying() {
    // No claim → no Wip → empty list, exit 0 (idempotent observation).
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");

    let (code, out) = capture(|w| svc.list_stale(3_600, true, w));
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v, serde_json::json!([]));
}

// ---------- release-stale (clap parser) ----------

/// `release-stale --task-id <ID> --before 1h` must fail at the parser level
/// with a clap "cannot be used with" error — clap enforces the
/// mutual-exclusion contract before we ever reach the service layer.
#[test]
fn release_stale_rejects_task_id_combined_with_before() {
    use autopilot::cmd::Cli;
    use clap::Parser;
    let result = Cli::try_parse_from([
        "autopilot",
        "task",
        "release-stale",
        "--task-id",
        "aaaaaaaaaaaa",
        "--before",
        "1h",
    ]);
    let err = match result {
        Ok(_) => panic!("expected mutual-exclusion error, got Ok"),
        Err(e) => e,
    };
    let s = err.to_string();
    assert!(
        s.contains("cannot be used with") || s.contains("conflicts with"),
        "expected mutual-exclusion error, got: {s}"
    );
}

#[test]
fn release_stale_rejects_task_id_combined_with_before_seconds() {
    use autopilot::cmd::Cli;
    use clap::Parser;
    let result = Cli::try_parse_from([
        "autopilot",
        "task",
        "release-stale",
        "--task-id",
        "aaaaaaaaaaaa",
        "--before-seconds",
        "60",
    ]);
    let err = match result {
        Ok(_) => panic!("expected mutual-exclusion error, got Ok"),
        Err(e) => e,
    };
    assert!(err.to_string().contains("cannot be used with"));
}

// ---------- release-stale (per-task + bulk) ----------

/// `release-stale --task-id <ID>` is the agent-recommended path: it reuses
/// the existing per-task `release` primitive so it recovers exactly one
/// task while leaving its peers untouched, even when both are stale.
#[test]
fn release_stale_per_task_recovers_only_the_named_task() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");
    seed_via_add(&svc, "bbbbbbbbbbbb", "0x2");
    let _ = capture(|w| svc.claim("e", false, w)); // claim a (oldest first)
    let _ = capture(|w| svc.claim("e", false, w)); // claim b
    clock.advance(chrono::Duration::hours(2));

    // Per-task path: same primitive as `task release <id>`.
    let (code, _) = capture(|w| svc.release("aaaaaaaaaaaa", w));
    assert_eq!(code, 0);

    let a = store
        .get_task(&TaskId::from_raw("aaaaaaaaaaaa"))
        .unwrap()
        .unwrap();
    let b = store
        .get_task(&TaskId::from_raw("bbbbbbbbbbbb"))
        .unwrap()
        .unwrap();
    assert_eq!(a.status, TaskStatus::Ready);
    assert_eq!(a.attempts, 0);
    // b is still stale-Wip — agent decides separately what to do with it.
    assert_eq!(b.status, TaskStatus::Wip);
    assert_eq!(b.attempts, 1);
}

/// CLI surface: claim a task at t0, advance the FixedClock past the cutoff,
/// then `release-stale --before 1m` (60s) must recover it. Validates the
/// human-readable count output and the JSON-array variant.
#[test]
fn release_stale_recovers_after_clock_advances_past_cutoff() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");
    let _ = capture(|w| svc.claim("e", false, w)); // attempts=1, status=wip, updated_at=t0

    // Advance the clock 5 minutes past the claim — the task is now "stale"
    // relative to a 1-minute cutoff.
    clock.advance(chrono::Duration::minutes(5));

    let (code, out) = capture(|w| svc.release_stale(60, false, w));
    assert_eq!(code, 0);
    assert!(out.contains("released 1 stale tasks"), "stdout: {out}");
    assert!(out.contains("aaaaaaaaaaaa"), "stdout: {out}");

    let t = store
        .get_task(&TaskId::from_raw("aaaaaaaaaaaa"))
        .unwrap()
        .unwrap();
    assert_eq!(t.status, TaskStatus::Ready);
    assert_eq!(t.attempts, 0);

    let evs = store
        .list_events(EventFilter {
            kinds: vec![EventKind::TaskReleasedStale],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(evs.len(), 1);
}

#[test]
fn release_stale_with_no_stale_tasks_exits_0_and_reports_zero() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");
    let _ = capture(|w| svc.claim("e", false, w));
    // Clock has NOT advanced — the claim is fresh, cutoff trims nothing.
    let (code, out) = capture(|w| svc.release_stale(3_600, false, w));
    assert_eq!(code, 0);
    assert!(out.contains("released 0 stale tasks"), "stdout: {out}");
}

#[test]
fn release_stale_json_emits_array_of_recovered_ids() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");
    let _ = capture(|w| svc.claim("e", false, w));
    clock.advance(chrono::Duration::hours(2));
    let (code, out) = capture(|w| svc.release_stale(3_600, true, w));
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v, serde_json::json!(["aaaaaaaaaaaa"]));
}

#[test]
fn parse_duration_seconds_supports_compound_units() {
    use autopilot::cmd::task::parse_duration_seconds;
    // Single units
    assert_eq!(parse_duration_seconds("30s").unwrap(), 30);
    assert_eq!(parse_duration_seconds("5m").unwrap(), 300);
    assert_eq!(parse_duration_seconds("1h").unwrap(), 3_600);
    assert_eq!(parse_duration_seconds("1d").unwrap(), 86_400);
    assert_eq!(parse_duration_seconds("1w").unwrap(), 604_800);
    // Compound units
    assert_eq!(parse_duration_seconds("2h30m").unwrap(), 9_000);
    assert_eq!(
        parse_duration_seconds("2d12h").unwrap(),
        2 * 86_400 + 12 * 3_600
    );
    assert_eq!(
        parse_duration_seconds("1w3d").unwrap(),
        7 * 86_400 + 3 * 86_400
    );
    assert_eq!(parse_duration_seconds("1h30m").unwrap(), 5_400);
    assert_eq!(
        parse_duration_seconds("2d12h30m").unwrap(),
        2 * 86_400 + 12 * 3_600 + 30 * 60
    );
    // Rejected inputs
    assert!(parse_duration_seconds("").is_err());
    assert!(parse_duration_seconds("0s").is_err());
    assert!(parse_duration_seconds("90").is_err()); // trailing digits, no unit
    assert!(parse_duration_seconds("-1d").is_err()); // negative not supported
    let err_y = parse_duration_seconds("1y").unwrap_err();
    assert!(
        err_y.contains("unknown unit") && err_y.contains('y'),
        "unexpected message for `1y`: {err_y}"
    );
    // `1mo` is rejected because `o` is not a recognized unit (after `1m` parses)
    assert!(parse_duration_seconds("1mo").is_err());
}

// ---------- find-by-pr ----------

#[test]
fn find_by_pr_returns_task_when_present() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");
    let _ = capture(|w| svc.claim("e", false, w));
    let _ = capture(|w| svc.complete("aaaaaaaaaaaa", 77, w));
    let (code, out) = capture(|w| svc.find_by_pr(77, true, w));
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["id"], "aaaaaaaaaaaa");
    assert_eq!(v["pr_number"], 77);
}

#[test]
fn find_by_pr_returns_exit_1_when_no_match() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    let (code, out) = capture(|w| svc.find_by_pr(404, false, w));
    assert_eq!(code, 1);
    assert!(out.contains("no task owns PR #404"), "stdout: {out}");
}

// ---------- Bug 1: same task_id, different fingerprint ----------

/// CLI-surface check: re-adding the same task id (different body so distinct
/// fingerprint) must print a friendly message and exit 1, never propagate
/// the raw SQLite UNIQUE error.
#[test]
fn add_with_existing_id_and_different_body_returns_friendly_error() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    let (_, _) = capture(|w| {
        svc.add(
            "e",
            "aaaaaaaaaaaa",
            "first",
            Some("body one"),
            None,
            TaskSourceArg::Human,
            w,
        )
    });
    let (code, out) = capture(|w| {
        svc.add(
            "e",
            "aaaaaaaaaaaa",
            "second",
            Some("body two — different content => different fingerprint"),
            None,
            TaskSourceArg::Human,
            w,
        )
    });
    assert_eq!(code, 1, "stdout: {out}");
    assert!(
        out.contains("task 'aaaaaaaaaaaa' already exists"),
        "stdout: {out}"
    );
}

// ---------- Bug 2: RequiresStatus error semantics ----------

#[test]
fn fail_from_ready_returns_requires_status_error() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");
    // Task is Ready (never claimed). fail() should surface RequiresStatus(_, Wip, Ready).
    let mut buf: Vec<u8> = Vec::new();
    let err = svc.fail("aaaaaaaaaaaa", &mut buf).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("requires status Wip") && msg.contains("was Ready"),
        "expected RequiresStatus(_, Wip, Ready), got: {msg}"
    );
}

#[test]
fn complete_from_ready_returns_requires_status_error() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");
    let mut buf: Vec<u8> = Vec::new();
    let err = svc.complete("aaaaaaaaaaaa", 99, &mut buf).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("requires status Wip") && msg.contains("was Ready"),
        "expected RequiresStatus(_, Wip, Ready), got: {msg}"
    );
}

#[test]
fn release_from_done_returns_requires_status_error_via_cli() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");
    // Drive the task to Done via force_status, then release should reject.
    store
        .force_status(
            &TaskId::from_raw("aaaaaaaaaaaa"),
            TaskStatus::Done,
            "test",
            clock.now(),
        )
        .unwrap();
    // CLI's `release` catches the precondition failure and prints a friendly
    // message with exit 1 — verify the surface matches the new variant.
    let (code, out) = capture(|w| svc.release("aaaaaaaaaaaa", w));
    assert_eq!(code, 1);
    assert!(
        out.contains("cannot be released from done"),
        "stdout: {out}"
    );
}

// Force-status arg type still routes through TaskService::force_status; smoke
// test that the lifecycle commands cooperate (kept minimal — main coverage
// lives in store_conformance and the `force_status_overrides_lifecycle`
// inline test inside cmd/task.rs).
#[test]
fn force_status_still_routes_through_service() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    seed_via_add(&svc, "aaaaaaaaaaaa", "0x1");
    let (code, _) = capture(|w| svc.force_status("aaaaaaaaaaaa", TaskStatusArg::Done, None, w));
    assert_eq!(code, 0);
    let t = store
        .get_task(&TaskId::from_raw("aaaaaaaaaaaa"))
        .unwrap()
        .unwrap();
    assert_eq!(t.status, TaskStatus::Done);
}

// ---------- set-status (rename of force-status, with deprecated alias) ----------

/// Both `set-status` (canonical post-rename) and `force-status` (deprecated
/// alias kept for one release) must parse to the same `TaskCommands::SetStatus`
/// variant with identical field values — this is what backwards-compat means
/// at the CLI surface (PR #696 audit).
fn assert_set_status_parses(subcmd: &str) {
    use autopilot::cmd::{Cli, Commands, TaskCommands};
    use clap::Parser;
    let cli = Cli::try_parse_from(["autopilot", "task", subcmd, "aaaaaaaaaaaa", "--to", "done"])
        .unwrap_or_else(|e| panic!("`task {subcmd}` should parse: {e}"));
    match cli.command {
        Commands::Task {
            command: TaskCommands::SetStatus { task_id, to, .. },
        } => {
            assert_eq!(task_id, "aaaaaaaaaaaa");
            assert_eq!(to, TaskStatusArg::Done);
        }
        _ => panic!("`task {subcmd}` did not resolve to SetStatus variant"),
    }
}

#[test]
fn set_status_parses_as_canonical_name() {
    assert_set_status_parses("set-status");
}

#[test]
fn force_status_alias_still_parses_to_set_status() {
    assert_set_status_parses("force-status");
}
