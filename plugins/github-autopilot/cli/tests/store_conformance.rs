//! L2 TaskStore conformance suite.
//!
//! Mirrors the scenarios in plans/github-autopilot/04-test-scenarios.md §4.
//! When a SqliteTaskStore is added, the same suite is executed against both
//! adapters via a macro to enforce LSP.

use std::path::PathBuf;
use std::sync::Arc;

use autopilot::domain::{
    Epic, EpicStatus, EventKind, TaskFailureOutcome, TaskGraph, TaskId, TaskSource, TaskStatus,
};
use autopilot::ports::task_store::{
    EpicPlan, EventFilter, NewTask, NewWatchTask, ReconciliationPlan, RemotePrState,
    RemoteTaskState, TaskStore, UpsertOutcome,
};
use autopilot::store::InMemoryTaskStore;
use chrono::{DateTime, Duration, TimeZone, Utc};

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 28, 9, 0, 0).unwrap()
}

fn epic(name: &str) -> Epic {
    Epic {
        name: name.to_string(),
        spec_path: PathBuf::from(format!("spec/{name}.md")),
        branch: format!("epic/{name}"),
        status: EpicStatus::Active,
        created_at: t0(),
        completed_at: None,
    }
}

fn nt(id: &str, title: &str) -> NewTask {
    NewTask {
        id: TaskId::from_raw(id),
        source: TaskSource::Decompose,
        fingerprint: None,
        title: title.to_string(),
        body: None,
    }
}

fn plan(name: &str, tasks: Vec<NewTask>, deps: Vec<(&str, &str)>) -> EpicPlan {
    EpicPlan {
        epic: epic(name),
        tasks,
        deps: deps
            .into_iter()
            .map(|(a, b)| (TaskId::from_raw(a), TaskId::from_raw(b)))
            .collect(),
    }
}

fn make_store() -> Arc<dyn TaskStore> {
    Arc::new(InMemoryTaskStore::new())
}

// 4.1 insert_epic_with_tasks ----------------------------------------------

#[test]
fn insert_creates_epic_tasks_deps_and_promotes_entry_points() {
    let store = make_store();
    store
        .insert_epic_with_tasks(
            plan(
                "e",
                vec![nt("A", "task A"), nt("B", "task B"), nt("C", "task C")],
                vec![("B", "A"), ("C", "A")],
            ),
            t0(),
        )
        .unwrap();

    assert!(store.get_epic("e").unwrap().is_some());

    let by_id = |id: &str| store.get_task(&TaskId::from_raw(id)).unwrap().unwrap();
    assert_eq!(by_id("A").status, TaskStatus::Ready);
    assert_eq!(by_id("B").status, TaskStatus::Pending);
    assert_eq!(by_id("C").status, TaskStatus::Pending);
    assert_eq!(store.list_deps(&TaskId::from_raw("B")).unwrap().len(), 1);

    let events = store.list_events(EventFilter::default()).unwrap();
    let kinds: Vec<EventKind> = events.iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&EventKind::EpicStarted));
    assert_eq!(
        kinds.iter().filter(|k| **k == EventKind::TaskInserted).count(),
        3
    );
}

#[test]
fn insert_rejects_dep_cycle() {
    let store = make_store();
    let err = store
        .insert_epic_with_tasks(
            plan(
                "e",
                vec![nt("A", "a"), nt("B", "b")],
                vec![("A", "B"), ("B", "A")],
            ),
            t0(),
        )
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("cycle"), "expected cycle error, got: {msg}");
    assert!(store.get_epic("e").unwrap().is_none());
}

#[test]
fn insert_is_atomic_on_existing_active_epic() {
    let store = make_store();
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    let before_tasks = store.list_tasks_by_epic("e", None).unwrap().len();

    let err = store
        .insert_epic_with_tasks(plan("e", vec![nt("X", "x"), nt("Y", "y")], vec![]), t0())
        .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("already exists"),
        "expected already-exists error, got: {msg}"
    );
    let after_tasks = store.list_tasks_by_epic("e", None).unwrap().len();
    assert_eq!(before_tasks, after_tasks);
}

// 4.2 claim_next_task ------------------------------------------------------

#[test]
fn claim_returns_none_when_no_ready_task() {
    let store = make_store();
    assert!(store.claim_next_task("e", t0()).unwrap().is_none());
}

#[test]
fn claim_returns_oldest_ready_task() {
    let store = make_store();
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    store
        .insert_epic_with_tasks(
            plan("f", vec![nt("Z", "z")], vec![]),
            t0() + Duration::seconds(10),
        )
        .unwrap();

    let claimed = store.claim_next_task("e", t0() + Duration::seconds(20)).unwrap().unwrap();
    assert_eq!(claimed.id.as_str(), "A");
    assert_eq!(claimed.status, TaskStatus::Wip);
    assert_eq!(claimed.attempts, 1);
}

#[test]
fn claim_skips_tasks_with_unsatisfied_deps() {
    let store = make_store();
    store
        .insert_epic_with_tasks(
            plan(
                "e",
                vec![nt("A", "a"), nt("B", "b")],
                vec![("B", "A")],
            ),
            t0(),
        )
        .unwrap();
    let claimed = store.claim_next_task("e", t0()).unwrap().unwrap();
    assert_eq!(claimed.id.as_str(), "A");
    // Only A should be claimable; B is gated.
    let claimed_again = store.claim_next_task("e", t0()).unwrap();
    assert!(claimed_again.is_none());
}

#[test]
fn claim_increments_attempts_on_each_call_after_revert() {
    let store = make_store();
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap();
    store.revert_to_ready(&TaskId::from_raw("A"), t0()).unwrap();
    let claimed = store.claim_next_task("e", t0()).unwrap().unwrap();
    assert_eq!(claimed.attempts, 2);
}

#[test]
fn claim_is_atomic_under_concurrent_callers() {
    use std::sync::Barrier;
    use std::thread;

    let store: Arc<dyn TaskStore> = make_store();
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();

    let barrier = Arc::new(Barrier::new(8));
    let mut handles = Vec::new();
    for _ in 0..8 {
        let s = Arc::clone(&store);
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            s.claim_next_task("e", t0()).unwrap()
        }));
    }
    let winners: usize = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .filter(|c| c.is_some())
        .count();
    assert_eq!(winners, 1);
    let task = store.get_task(&TaskId::from_raw("A")).unwrap().unwrap();
    assert_eq!(task.attempts, 1);
}

// 4.3 complete_task_and_unblock -------------------------------------------

#[test]
fn completing_a_task_unblocks_dependents_with_satisfied_deps() {
    let store = make_store();
    store
        .insert_epic_with_tasks(
            plan(
                "e",
                vec![nt("A", "a"), nt("B", "b"), nt("C", "c")],
                vec![("B", "A"), ("C", "A"), ("C", "B")],
            ),
            t0(),
        )
        .unwrap();
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap();
    let report = store
        .complete_task_and_unblock(&TaskId::from_raw("A"), 42, t0())
        .unwrap();
    assert_eq!(report.completed.as_str(), "A");
    let by_id = |id: &str| store.get_task(&TaskId::from_raw(id)).unwrap().unwrap();
    assert_eq!(by_id("A").status, TaskStatus::Done);
    assert_eq!(by_id("A").pr_number, Some(42));
    assert_eq!(by_id("B").status, TaskStatus::Ready);
    assert_eq!(by_id("C").status, TaskStatus::Pending);
    assert!(report.newly_ready.contains(&TaskId::from_raw("B")));
}

#[test]
fn complete_rejects_when_status_not_wip() {
    let store = make_store();
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    let err = store
        .complete_task_and_unblock(&TaskId::from_raw("A"), 1, t0())
        .unwrap_err();
    assert!(format!("{err}").contains("illegal status transition"));
}

// 4.4 mark_task_failed -----------------------------------------------------

#[test]
fn failure_below_max_returns_to_ready() {
    let store = make_store();
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap(); // attempts=1
    let outcome = store
        .mark_task_failed(&TaskId::from_raw("A"), 3, t0())
        .unwrap();
    assert_eq!(outcome, TaskFailureOutcome::Retried { attempts: 1 });
    assert_eq!(
        store.get_task(&TaskId::from_raw("A")).unwrap().unwrap().status,
        TaskStatus::Ready
    );
}

#[test]
fn failure_at_max_escalates_and_blocks_dependents() {
    let store = make_store();
    store
        .insert_epic_with_tasks(
            plan(
                "e",
                vec![nt("A", "a"), nt("B", "b")],
                vec![("B", "A")],
            ),
            t0(),
        )
        .unwrap();
    // Drive A to attempts=3
    for _ in 0..3 {
        let _ = store.claim_next_task("e", t0()).unwrap().unwrap();
        let _ = store.mark_task_failed(&TaskId::from_raw("A"), 3, t0()).unwrap();
    }
    let by_id = |id: &str| store.get_task(&TaskId::from_raw(id)).unwrap().unwrap();
    assert_eq!(by_id("A").status, TaskStatus::Escalated);
    assert_eq!(by_id("B").status, TaskStatus::Blocked);
}

// 4.5 apply_reconciliation -------------------------------------------------

#[test]
fn reconcile_is_idempotent() {
    let store = make_store();
    let plan = ReconciliationPlan {
        epic: epic("e"),
        tasks: vec![nt("A", "a"), nt("B", "b")],
        deps: vec![(TaskId::from_raw("B"), TaskId::from_raw("A"))],
        remote_state: vec![RemoteTaskState {
            task_id: TaskId::from_raw("A"),
            branch_exists: true,
            pr: Some(RemotePrState {
                number: 10,
                merged: true,
                closed: true,
            }),
        }],
        orphan_branches: vec![],
    };
    store.apply_reconciliation(plan.clone(), t0()).unwrap();
    let snapshot1 = store.list_tasks_by_epic("e", None).unwrap();
    store
        .apply_reconciliation(plan, t0() + Duration::seconds(1))
        .unwrap();
    let snapshot2 = store.list_tasks_by_epic("e", None).unwrap();
    let key = |t: &autopilot::domain::Task| {
        (
            t.id.clone(),
            t.status.as_str().to_string(),
            t.pr_number,
            t.attempts,
        )
    };
    let mut s1: Vec<_> = snapshot1.iter().map(key).collect();
    let mut s2: Vec<_> = snapshot2.iter().map(key).collect();
    s1.sort();
    s2.sort();
    assert_eq!(s1, s2);
}

#[test]
fn reconcile_overrides_status_from_remote_truth() {
    let store = make_store();
    let plan = ReconciliationPlan {
        epic: epic("e"),
        tasks: vec![nt("A", "a")],
        deps: vec![],
        remote_state: vec![RemoteTaskState {
            task_id: TaskId::from_raw("A"),
            branch_exists: true,
            pr: Some(RemotePrState {
                number: 7,
                merged: true,
                closed: true,
            }),
        }],
        orphan_branches: vec![],
    };
    store.apply_reconciliation(plan, t0()).unwrap();
    let a = store.get_task(&TaskId::from_raw("A")).unwrap().unwrap();
    assert_eq!(a.status, TaskStatus::Done);
    assert_eq!(a.pr_number, Some(7));
}

#[test]
fn reconcile_preserves_attempts_counter() {
    let store = make_store();
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap();
    let _ = store.mark_task_failed(&TaskId::from_raw("A"), 3, t0()).unwrap();
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap();
    let plan = ReconciliationPlan {
        epic: epic("e"),
        tasks: vec![nt("A", "a")],
        deps: vec![],
        remote_state: vec![RemoteTaskState {
            task_id: TaskId::from_raw("A"),
            branch_exists: true,
            pr: None,
        }],
        orphan_branches: vec![],
    };
    store.apply_reconciliation(plan, t0()).unwrap();
    let a = store.get_task(&TaskId::from_raw("A")).unwrap().unwrap();
    assert_eq!(a.attempts, 2);
    assert_eq!(a.status, TaskStatus::Wip);
}

// 4.6 fingerprint upsert ---------------------------------------------------

#[test]
fn upsert_watch_task_inserts_new_when_no_fingerprint_match() {
    let store = make_store();
    store
        .insert_epic_with_tasks(plan("e", vec![], vec![]), t0())
        .unwrap();
    let outcome = store
        .upsert_watch_task(
            NewWatchTask {
                id: TaskId::from_raw("watch1"),
                epic_name: "e".to_string(),
                source: TaskSource::GapWatch,
                fingerprint: "fp-1".to_string(),
                title: "watch finding".to_string(),
                body: None,
            },
            t0(),
        )
        .unwrap();
    assert!(matches!(outcome, UpsertOutcome::Inserted(_)));
}

#[test]
fn upsert_watch_task_returns_duplicate_on_existing_fingerprint() {
    let store = make_store();
    store
        .insert_epic_with_tasks(plan("e", vec![], vec![]), t0())
        .unwrap();
    let _ = store
        .upsert_watch_task(
            NewWatchTask {
                id: TaskId::from_raw("watch1"),
                epic_name: "e".to_string(),
                source: TaskSource::GapWatch,
                fingerprint: "fp-1".to_string(),
                title: "first".to_string(),
                body: None,
            },
            t0(),
        )
        .unwrap();
    let outcome = store
        .upsert_watch_task(
            NewWatchTask {
                id: TaskId::from_raw("watch2"),
                epic_name: "e".to_string(),
                source: TaskSource::GapWatch,
                fingerprint: "fp-1".to_string(),
                title: "duplicate".to_string(),
                body: None,
            },
            t0(),
        )
        .unwrap();
    match outcome {
        UpsertOutcome::DuplicateFingerprint(id) => assert_eq!(id.as_str(), "watch1"),
        other => panic!("expected duplicate, got {other:?}"),
    }
    let tasks = store.list_tasks_by_epic("e", None).unwrap();
    assert_eq!(tasks.len(), 1);
}

// Domain helpers: TaskGraph cycle detection sanity (cross-checks 4.x).
#[test]
fn graph_cycle_detection_works_through_domain() {
    let g = TaskGraph::build([
        (TaskId::from_raw("A"), TaskId::from_raw("B")),
        (TaskId::from_raw("B"), TaskId::from_raw("A")),
    ]);
    assert!(g.detect_cycle().is_some());
}
