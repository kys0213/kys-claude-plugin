//! L2 TaskStore conformance suite.
//!
//! Mirrors the scenarios in plans/github-autopilot/04-test-scenarios.md §4.
//! Each scenario is a body fn taking `Arc<dyn TaskStore>`; the
//! `conformance_suite!` macro generates `#[test]` wrappers that run every
//! body against both `InMemoryTaskStore` and `SqliteTaskStore`, enforcing
//! LSP across the two adapters.

use std::path::PathBuf;
use std::sync::Arc;

use autopilot::domain::{
    Epic, EpicStatus, EventKind, TaskFailureOutcome, TaskGraph, TaskId, TaskSource, TaskStatus,
};
use autopilot::ports::task_store::{
    EpicPlan, EventFilter, NewTask, NewWatchTask, ReconciliationPlan, RemotePrState,
    RemoteTaskState, TaskStore, UpsertOutcome,
};
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

fn body_insert_creates_epic_tasks_deps_and_promotes_entry_points(store: Arc<dyn TaskStore>) {
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
        kinds
            .iter()
            .filter(|k| **k == EventKind::TaskInserted)
            .count(),
        3
    );
}

fn body_insert_rejects_dep_cycle(store: Arc<dyn TaskStore>) {
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

fn body_insert_is_atomic_on_existing_active_epic(store: Arc<dyn TaskStore>) {
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

fn body_claim_returns_none_when_no_ready_task(store: Arc<dyn TaskStore>) {
    assert!(store.claim_next_task("e", t0()).unwrap().is_none());
}

fn body_claim_returns_oldest_ready_task(store: Arc<dyn TaskStore>) {
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    store
        .insert_epic_with_tasks(
            plan("f", vec![nt("Z", "z")], vec![]),
            t0() + Duration::seconds(10),
        )
        .unwrap();
    let claimed = store
        .claim_next_task("e", t0() + Duration::seconds(20))
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id.as_str(), "A");
    assert_eq!(claimed.status, TaskStatus::Wip);
    assert_eq!(claimed.attempts, 1);
}

fn body_claim_skips_tasks_with_unsatisfied_deps(store: Arc<dyn TaskStore>) {
    store
        .insert_epic_with_tasks(
            plan("e", vec![nt("A", "a"), nt("B", "b")], vec![("B", "A")]),
            t0(),
        )
        .unwrap();
    let claimed = store.claim_next_task("e", t0()).unwrap().unwrap();
    assert_eq!(claimed.id.as_str(), "A");
    let claimed_again = store.claim_next_task("e", t0()).unwrap();
    assert!(claimed_again.is_none());
}

fn body_claim_increments_attempts_on_each_call_after_revert(store: Arc<dyn TaskStore>) {
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap();
    store.revert_to_ready(&TaskId::from_raw("A"), t0()).unwrap();
    let claimed = store.claim_next_task("e", t0()).unwrap().unwrap();
    assert_eq!(claimed.attempts, 2);
}

fn body_claim_is_atomic_under_concurrent_callers(store: Arc<dyn TaskStore>) {
    use std::sync::Barrier;
    use std::thread;

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

fn body_completing_a_task_unblocks_dependents_with_satisfied_deps(store: Arc<dyn TaskStore>) {
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

fn body_complete_rejects_when_status_not_wip(store: Arc<dyn TaskStore>) {
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    let err = store
        .complete_task_and_unblock(&TaskId::from_raw("A"), 1, t0())
        .unwrap_err();
    assert!(format!("{err}").contains("illegal status transition"));
}

fn body_failure_below_max_returns_to_ready(store: Arc<dyn TaskStore>) {
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap();
    let outcome = store
        .mark_task_failed(&TaskId::from_raw("A"), 3, t0())
        .unwrap();
    assert_eq!(outcome, TaskFailureOutcome::Retried { attempts: 1 });
    assert_eq!(
        store
            .get_task(&TaskId::from_raw("A"))
            .unwrap()
            .unwrap()
            .status,
        TaskStatus::Ready
    );
}

fn body_failure_at_max_escalates_and_blocks_dependents(store: Arc<dyn TaskStore>) {
    store
        .insert_epic_with_tasks(
            plan("e", vec![nt("A", "a"), nt("B", "b")], vec![("B", "A")]),
            t0(),
        )
        .unwrap();
    for _ in 0..3 {
        let _ = store.claim_next_task("e", t0()).unwrap().unwrap();
        let _ = store
            .mark_task_failed(&TaskId::from_raw("A"), 3, t0())
            .unwrap();
    }
    let by_id = |id: &str| store.get_task(&TaskId::from_raw(id)).unwrap().unwrap();
    assert_eq!(by_id("A").status, TaskStatus::Escalated);
    assert_eq!(by_id("B").status, TaskStatus::Blocked);
}

fn body_reconcile_is_idempotent(store: Arc<dyn TaskStore>) {
    let p = ReconciliationPlan {
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
    store.apply_reconciliation(p.clone(), t0()).unwrap();
    let snapshot1 = store.list_tasks_by_epic("e", None).unwrap();
    store
        .apply_reconciliation(p, t0() + Duration::seconds(1))
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

fn body_reconcile_overrides_status_from_remote_truth(store: Arc<dyn TaskStore>) {
    let p = ReconciliationPlan {
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
    store.apply_reconciliation(p, t0()).unwrap();
    let a = store.get_task(&TaskId::from_raw("A")).unwrap().unwrap();
    assert_eq!(a.status, TaskStatus::Done);
    assert_eq!(a.pr_number, Some(7));
}

fn body_reconcile_preserves_attempts_counter(store: Arc<dyn TaskStore>) {
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap();
    let _ = store
        .mark_task_failed(&TaskId::from_raw("A"), 3, t0())
        .unwrap();
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap();
    let p = ReconciliationPlan {
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
    store.apply_reconciliation(p, t0()).unwrap();
    let a = store.get_task(&TaskId::from_raw("A")).unwrap().unwrap();
    assert_eq!(a.attempts, 2);
    assert_eq!(a.status, TaskStatus::Wip);
}

fn body_upsert_watch_task_inserts_new_when_no_fingerprint_match(store: Arc<dyn TaskStore>) {
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

fn body_upsert_watch_task_returns_duplicate_on_existing_fingerprint(store: Arc<dyn TaskStore>) {
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

macro_rules! conformance_suite {
    ($($name:ident),* $(,)?) => {
        mod in_memory {
            use std::sync::Arc;
            use autopilot::ports::task_store::TaskStore;
            use autopilot::store::InMemoryTaskStore;
            $(
                #[test]
                fn $name() {
                    let store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
                    super::$name(store);
                }
            )*
        }
        mod sqlite {
            use std::sync::Arc;
            use autopilot::ports::task_store::TaskStore;
            use autopilot::store::SqliteTaskStore;
            $(
                #[test]
                fn $name() {
                    let store: Arc<dyn TaskStore> =
                        Arc::new(SqliteTaskStore::open_in_memory().unwrap());
                    super::$name(store);
                }
            )*
        }
    };
}

conformance_suite!(
    body_insert_creates_epic_tasks_deps_and_promotes_entry_points,
    body_insert_rejects_dep_cycle,
    body_insert_is_atomic_on_existing_active_epic,
    body_claim_returns_none_when_no_ready_task,
    body_claim_returns_oldest_ready_task,
    body_claim_skips_tasks_with_unsatisfied_deps,
    body_claim_increments_attempts_on_each_call_after_revert,
    body_claim_is_atomic_under_concurrent_callers,
    body_completing_a_task_unblocks_dependents_with_satisfied_deps,
    body_complete_rejects_when_status_not_wip,
    body_failure_below_max_returns_to_ready,
    body_failure_at_max_escalates_and_blocks_dependents,
    body_reconcile_is_idempotent,
    body_reconcile_overrides_status_from_remote_truth,
    body_reconcile_preserves_attempts_counter,
    body_upsert_watch_task_inserts_new_when_no_fingerprint_match,
    body_upsert_watch_task_returns_duplicate_on_existing_fingerprint,
);

#[test]
fn graph_cycle_detection_works_through_domain() {
    let g = TaskGraph::build([
        (TaskId::from_raw("A"), TaskId::from_raw("B")),
        (TaskId::from_raw("B"), TaskId::from_raw("A")),
    ]);
    assert!(g.detect_cycle().is_some());
}
