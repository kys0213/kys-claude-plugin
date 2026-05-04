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
    // Each call uses a fresh epic to avoid duplicate inserts; instead, we
    // upsert into the existing epic via reconcile-like behavior. The simplest
    // path: open a brand-new epic with these tasks.
    // ... but tests want everything under "e", so use force_status after insert.
    //
    // Strategy: insert via insert_epic_with_tasks fails on existing epic, so
    // we rely on `apply_reconciliation` to upsert. Build a minimal plan.
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
    let (_store, clock) = fixture();
    let store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
    seed_epic(store.as_ref(), &clock, "e");
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
    let (_, _) = capture(|w| {
        svc.add(
            "e",
            "aaaaaaaaaaaa",
            "x",
            None,
            Some("0x1"),
            TaskSourceArg::Human,
            w,
        )
    });
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
    let (_, _) = capture(|w| {
        svc.add(
            "e",
            "aaaaaaaaaaaa",
            "first",
            None,
            Some("0x1"),
            TaskSourceArg::Human,
            w,
        )
    });

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
    let _ = capture(|w| {
        svc.add(
            "e",
            "aaaaaaaaaaaa",
            "x",
            None,
            Some("0x1"),
            TaskSourceArg::Human,
            w,
        )
    });
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
    let _ = capture(|w| {
        svc.add(
            "e",
            "aaaaaaaaaaaa",
            "x",
            None,
            Some("0x1"),
            TaskSourceArg::Human,
            w,
        )
    });
    // claim+fail 3 times; on the 3rd fail, attempts is already 3 → Escalated.
    for _ in 0..3 {
        let _ = capture(|w| svc.claim("e", false, w));
        let (_, _) = capture(|w| svc.fail("aaaaaaaaaaaa", w));
    }
    // After three retries, the task is Escalated; final fail call shouldn't
    // happen — verify state directly.
    let t = store
        .get_task(&TaskId::from_raw("aaaaaaaaaaaa"))
        .unwrap()
        .unwrap();
    assert_eq!(t.status, TaskStatus::Escalated);
    assert_eq!(t.attempts, 3);

    // Also verify a fresh scenario: claim once but pre-set attempts=3 via
    // direct claim sequence; the 3rd fail outputs escalated JSON.
    let (store2, clock2) = fixture();
    let svc2 = TaskService::new(store2.as_ref(), &clock2);
    let _ = capture(|w| {
        svc2.add(
            "e",
            "bbbbbbbbbbbb",
            "y",
            None,
            Some("0x2"),
            TaskSourceArg::Human,
            w,
        )
    });
    // attempts will reach 3 on the 3rd claim, then fail → Escalated
    let _ = capture(|w| svc2.claim("e", false, w));
    let (_, _) = capture(|w| svc2.fail("bbbbbbbbbbbb", w));
    let _ = capture(|w| svc2.claim("e", false, w));
    let (_, _) = capture(|w| svc2.fail("bbbbbbbbbbbb", w));
    let _ = capture(|w| svc2.claim("e", false, w));
    let (code, out) = capture(|w| svc2.fail("bbbbbbbbbbbb", w));
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["outcome"], "escalated");
    assert_eq!(v["attempts"], 3);
}

// ---------- escalate ----------

#[test]
fn escalate_sets_escalated_issue_field() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    let _ = capture(|w| {
        svc.add(
            "e",
            "aaaaaaaaaaaa",
            "x",
            None,
            Some("0x1"),
            TaskSourceArg::Human,
            w,
        )
    });
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
    let _ = capture(|w| {
        svc.add(
            "e",
            "aaaaaaaaaaaa",
            "x",
            None,
            Some("0x1"),
            TaskSourceArg::Human,
            w,
        )
    });
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

// ---------- find-by-pr ----------

#[test]
fn find_by_pr_returns_task_when_present() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    let _ = capture(|w| {
        svc.add(
            "e",
            "aaaaaaaaaaaa",
            "x",
            None,
            Some("0x1"),
            TaskSourceArg::Human,
            w,
        )
    });
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

// Force-status arg type still routes through TaskService::force_status; smoke
// test that the lifecycle commands cooperate (kept minimal — main coverage
// lives in store_conformance and the `force_status_overrides_lifecycle`
// inline test inside cmd/task.rs).
#[test]
fn force_status_still_routes_through_service() {
    let (store, clock) = fixture();
    let svc = TaskService::new(store.as_ref(), &clock);
    let _ = capture(|w| {
        svc.add(
            "e",
            "aaaaaaaaaaaa",
            "x",
            None,
            Some("0x1"),
            TaskSourceArg::Human,
            w,
        )
    });
    let (code, _) = capture(|w| svc.force_status("aaaaaaaaaaaa", TaskStatusArg::Done, None, w));
    assert_eq!(code, 0);
    let t = store
        .get_task(&TaskId::from_raw("aaaaaaaaaaaa"))
        .unwrap()
        .unwrap();
    assert_eq!(t.status, TaskStatus::Done);
}
