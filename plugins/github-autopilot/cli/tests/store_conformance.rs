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
    DomainError, Epic, EpicStatus, EventKind, TaskFailureOutcome, TaskGraph, TaskId, TaskSource,
    TaskStatus,
};
use autopilot::ports::task_store::{
    EpicPlan, EventFilter, NewTask, NewWatchTask, ReconciliationPlan, RemotePrState,
    RemoteTaskState, TaskStore, TaskStoreError, UpsertOutcome,
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

fn body_release_claim_decrements_attempts(store: Arc<dyn TaskStore>) {
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap();
    store.release_claim(&TaskId::from_raw("A"), t0()).unwrap();
    let claimed = store.claim_next_task("e", t0()).unwrap().unwrap();
    assert_eq!(claimed.attempts, 1);
}

fn body_list_stale_returns_old_wip_tasks_without_modifying_them(store: Arc<dyn TaskStore>) {
    // Two old Wip tasks + one fresh Wip task. list_stale must return the
    // two old ones in deterministic (updated_at, id) order, and must NOT
    // mutate any task: status stays Wip, attempts unchanged, no
    // TaskReleasedStale event emitted (read-only contract).
    store
        .insert_epic_with_tasks(
            plan("e", vec![nt("A", "a"), nt("B", "b"), nt("C", "c")], vec![]),
            t0(),
        )
        .unwrap();
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap(); // A — old
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap(); // B — old
    let _ = store
        .claim_next_task("e", t0() + Duration::minutes(10))
        .unwrap()
        .unwrap(); // C — fresh

    let stale = store.list_stale(t0() + Duration::minutes(5)).unwrap();
    let ids: Vec<String> = stale.iter().map(|t| t.id.as_str().to_string()).collect();
    assert_eq!(ids, vec!["A".to_string(), "B".to_string()]);
    // Returned shape: full Task records — caller (agent) sees epic_name,
    // updated_at, attempts, status to make per-task decisions.
    assert!(stale.iter().all(|t| t.status == TaskStatus::Wip));
    assert!(stale.iter().all(|t| t.epic_name == "e"));
    assert!(stale.iter().all(|t| t.attempts == 1));

    // Read-only: nothing changed.
    let by_id = |id: &str| store.get_task(&TaskId::from_raw(id)).unwrap().unwrap();
    assert_eq!(by_id("A").status, TaskStatus::Wip);
    assert_eq!(by_id("B").status, TaskStatus::Wip);
    assert_eq!(by_id("C").status, TaskStatus::Wip);
    assert_eq!(by_id("A").attempts, 1);
    let evs = store
        .list_events(EventFilter {
            kinds: vec![EventKind::TaskReleasedStale],
            ..Default::default()
        })
        .unwrap();
    assert!(
        evs.is_empty(),
        "list_stale must not emit TaskReleasedStale: {evs:?}"
    );
}

fn body_list_stale_skips_recent_or_non_wip_tasks(store: Arc<dyn TaskStore>) {
    // A recent Wip + tasks in non-Wip statuses (Done, Ready, Escalated) with
    // stale `updated_at` must all be excluded. Same boundary contract as
    // release_stale, just observation-only.
    store
        .insert_epic_with_tasks(
            plan(
                "e",
                vec![nt("A", "a"), nt("B", "b"), nt("C", "c"), nt("D", "d")],
                vec![],
            ),
            t0(),
        )
        .unwrap();
    // A: claim+complete (Done). B: stays Ready. C: force to Escalated.
    // D: claim recently (Wip, but fresh — must be excluded).
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap();
    store
        .complete_task_and_unblock(&TaskId::from_raw("A"), 1, t0())
        .unwrap();
    store
        .force_status(&TaskId::from_raw("C"), TaskStatus::Escalated, "test", t0())
        .unwrap();
    let _ = store
        .claim_next_task("e", t0() + Duration::minutes(10))
        .unwrap()
        .unwrap(); // D — fresh

    let stale = store.list_stale(t0() + Duration::minutes(5)).unwrap();
    assert!(stale.is_empty(), "expected empty list, got {stale:?}");
}

fn body_release_stale_recovers_old_wip_tasks(store: Arc<dyn TaskStore>) {
    // Two tasks claimed at t0 (Wip), one task claimed at t0+10m. Cutoff at
    // t0+5m must recover only the older one. Validates: status -> Ready,
    // attempts decremented, TaskReleasedStale event emitted.
    store
        .insert_epic_with_tasks(
            plan("e", vec![nt("A", "a"), nt("B", "b"), nt("C", "c")], vec![]),
            t0(),
        )
        .unwrap();
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap(); // A
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap(); // B
    let _ = store
        .claim_next_task("e", t0() + Duration::minutes(10))
        .unwrap()
        .unwrap(); // C — fresh

    let recovered = store
        .release_stale(t0() + Duration::minutes(5), t0() + Duration::minutes(11))
        .unwrap();
    let mut ids: Vec<String> = recovered.iter().map(|i| i.as_str().to_string()).collect();
    ids.sort();
    assert_eq!(ids, vec!["A".to_string(), "B".to_string()]);

    let by_id = |id: &str| store.get_task(&TaskId::from_raw(id)).unwrap().unwrap();
    assert_eq!(by_id("A").status, TaskStatus::Ready);
    assert_eq!(by_id("A").attempts, 0);
    assert_eq!(by_id("B").status, TaskStatus::Ready);
    assert_eq!(by_id("B").attempts, 0);
    assert_eq!(by_id("C").status, TaskStatus::Wip); // untouched

    let evs = store
        .list_events(EventFilter {
            kinds: vec![EventKind::TaskReleasedStale],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(evs.len(), 2);
    let ev_ids: Vec<String> = evs
        .iter()
        .map(|e| e.task_id.as_ref().unwrap().as_str().to_string())
        .collect();
    assert!(ev_ids.contains(&"A".to_string()));
    assert!(ev_ids.contains(&"B".to_string()));
}

fn body_release_stale_skips_recent_wip_tasks(store: Arc<dyn TaskStore>) {
    // Single Wip task with updated_at younger than cutoff — must be skipped
    // and exit with empty list. Idempotent on the empty case.
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    let _ = store
        .claim_next_task("e", t0() + Duration::minutes(10))
        .unwrap()
        .unwrap();
    let recovered = store
        .release_stale(t0() + Duration::minutes(5), t0() + Duration::minutes(11))
        .unwrap();
    assert!(recovered.is_empty());
    let a = store.get_task(&TaskId::from_raw("A")).unwrap().unwrap();
    assert_eq!(a.status, TaskStatus::Wip);
    assert_eq!(a.attempts, 1);
}

fn body_release_stale_skips_non_wip_tasks(store: Arc<dyn TaskStore>) {
    // Tasks in non-Wip statuses (Done, Ready, Escalated) with stale
    // updated_at must be untouched even though their timestamp is older
    // than the cutoff.
    store
        .insert_epic_with_tasks(
            plan("e", vec![nt("A", "a"), nt("B", "b"), nt("C", "c")], vec![]),
            t0(),
        )
        .unwrap();
    // A: claim+complete (Done). B: stays Ready. C: force to Escalated.
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap();
    store
        .complete_task_and_unblock(&TaskId::from_raw("A"), 1, t0())
        .unwrap();
    store
        .force_status(&TaskId::from_raw("C"), TaskStatus::Escalated, "test", t0())
        .unwrap();

    let recovered = store
        .release_stale(t0() + Duration::hours(1), t0() + Duration::hours(2))
        .unwrap();
    assert!(recovered.is_empty());

    let by_id = |id: &str| store.get_task(&TaskId::from_raw(id)).unwrap().unwrap();
    assert_eq!(by_id("A").status, TaskStatus::Done);
    assert_eq!(by_id("B").status, TaskStatus::Ready);
    assert_eq!(by_id("C").status, TaskStatus::Escalated);
}

fn body_release_claim_rejects_non_wip(store: Arc<dyn TaskStore>) {
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    let err = store
        .release_claim(&TaskId::from_raw("A"), t0())
        .unwrap_err();
    // C3 reworded RequiresStatus to lowercase canonical statuses with
    // an actionable suffix (`task claim ...` / `task set-status ...`).
    let msg = format!("{err}");
    assert!(
        msg.contains("requires status 'wip'"),
        "expected RequiresStatus(_, Wip, _), got: {msg}"
    );
    assert!(
        msg.contains("task claim") || msg.contains("set-status"),
        "expected actionable hint in: {msg}"
    );
}

fn body_mark_failed_preserves_attempts_for_retry(store: Arc<dyn TaskStore>) {
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap();
    let _ = store
        .mark_task_failed(&TaskId::from_raw("A"), 3, t0())
        .unwrap();
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
    // Match the C3 lowercase canonical-status message.
    assert!(format!("{err}").contains("requires status 'wip'"));
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

fn body_upsert_watch_task_rejects_same_id_with_different_fingerprint(store: Arc<dyn TaskStore>) {
    store
        .insert_epic_with_tasks(plan("e", vec![], vec![]), t0())
        .unwrap();
    store
        .upsert_watch_task(
            NewWatchTask {
                id: TaskId::from_raw("watch1"),
                epic_name: "e".to_string(),
                source: TaskSource::Human,
                fingerprint: "fp-1".to_string(),
                title: "first".to_string(),
                body: None,
            },
            t0(),
        )
        .unwrap();
    let err = store
        .upsert_watch_task(
            NewWatchTask {
                id: TaskId::from_raw("watch1"),
                epic_name: "e".to_string(),
                source: TaskSource::Human,
                fingerprint: "fp-2".to_string(),
                title: "second".to_string(),
                body: Some("different body".to_string()),
            },
            t0(),
        )
        .unwrap_err();
    assert!(
        matches!(
            &err,
            TaskStoreError::Domain(DomainError::DuplicateTaskId(id)) if id.as_str() == "watch1"
        ),
        "expected DuplicateTaskId, got: {err:?}"
    );
}

fn body_find_task_by_pr_returns_owning_task(store: Arc<dyn TaskStore>) {
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a"), nt("B", "b")], vec![]), t0())
        .unwrap();
    let _ = store.claim_next_task("e", t0()).unwrap().unwrap();
    store
        .complete_task_and_unblock(&TaskId::from_raw("A"), 42, t0())
        .unwrap();
    let found = store.find_task_by_pr(42).unwrap().unwrap();
    assert_eq!(found.id.as_str(), "A");
}

fn body_find_task_by_pr_returns_none_when_unknown(store: Arc<dyn TaskStore>) {
    assert!(store.find_task_by_pr(9999).unwrap().is_none());
}

fn body_find_active_by_spec_path_matches_active_only(store: Arc<dyn TaskStore>) {
    store
        .insert_epic_with_tasks(plan("e1", vec![], vec![]), t0())
        .unwrap();
    store
        .insert_epic_with_tasks(plan("e2", vec![], vec![]), t0())
        .unwrap();
    store
        .set_epic_status("e2", EpicStatus::Abandoned, t0() + Duration::seconds(1))
        .unwrap();
    let path = std::path::PathBuf::from("spec/e1.md");
    let found = store.find_active_by_spec_path(&path).unwrap().unwrap();
    assert_eq!(found.name, "e1");
}

fn body_find_active_by_spec_path_returns_none_when_no_match(store: Arc<dyn TaskStore>) {
    store
        .insert_epic_with_tasks(plan("e1", vec![], vec![]), t0())
        .unwrap();
    let path = std::path::PathBuf::from("spec/elsewhere.md");
    assert!(store.find_active_by_spec_path(&path).unwrap().is_none());
}

fn body_find_active_by_spec_path_rejects_invariant_violation(store: Arc<dyn TaskStore>) {
    let shared = std::path::PathBuf::from("spec/shared.md");
    let mk = |name: &str| Epic {
        name: name.to_string(),
        spec_path: shared.clone(),
        branch: format!("epic/{name}"),
        status: EpicStatus::Active,
        created_at: t0(),
        completed_at: None,
    };
    store.upsert_epic(&mk("e1")).unwrap();
    store.upsert_epic(&mk("e2")).unwrap();
    let err = store.find_active_by_spec_path(&shared).unwrap_err();
    assert!(
        format!("{err}").contains("inconsistency"),
        "expected Inconsistency error, got: {err}"
    );
}

fn body_force_status_bypasses_normal_transition(store: Arc<dyn TaskStore>) {
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    for _ in 0..3 {
        let _ = store.claim_next_task("e", t0()).unwrap().unwrap();
        let _ = store
            .mark_task_failed(&TaskId::from_raw("A"), 3, t0())
            .unwrap();
    }
    store
        .force_status(
            &TaskId::from_raw("A"),
            TaskStatus::Pending,
            "manual reset",
            t0(),
        )
        .unwrap();
    let a = store.get_task(&TaskId::from_raw("A")).unwrap().unwrap();
    assert_eq!(a.status, TaskStatus::Pending);
}

fn body_force_status_does_not_unblock_dependents(store: Arc<dyn TaskStore>) {
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
    let b_before = store.get_task(&TaskId::from_raw("B")).unwrap().unwrap();
    assert_eq!(b_before.status, TaskStatus::Blocked);
    store
        .force_status(
            &TaskId::from_raw("A"),
            TaskStatus::Done,
            "human fixed",
            t0(),
        )
        .unwrap();
    let b_after = store.get_task(&TaskId::from_raw("B")).unwrap().unwrap();
    assert_eq!(b_after.status, TaskStatus::Blocked);
}

fn body_force_status_records_event_with_reason(store: Arc<dyn TaskStore>) {
    store
        .insert_epic_with_tasks(plan("e", vec![nt("A", "a")], vec![]), t0())
        .unwrap();
    store
        .force_status(
            &TaskId::from_raw("A"),
            TaskStatus::Pending,
            "rollback for repro",
            t0(),
        )
        .unwrap();
    let evs = store
        .list_events(EventFilter {
            kinds: vec![EventKind::TaskForceStatus],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(evs.len(), 1);
    assert_eq!(evs[0].payload["reason"], "rollback for repro");
    assert_eq!(evs[0].payload["to"], "pending");
}

fn body_suppression_blocks_until_window_expires(store: Arc<dyn TaskStore>) {
    let until = t0() + Duration::hours(1);
    store.suppress("fp-1", "unmatched_watch", until).unwrap();
    assert!(store
        .is_suppressed("fp-1", "unmatched_watch", t0() + Duration::minutes(30))
        .unwrap());
    assert!(!store
        .is_suppressed("fp-1", "unmatched_watch", t0() + Duration::hours(2))
        .unwrap());
}

fn body_suppression_is_scoped_by_reason(store: Arc<dyn TaskStore>) {
    store
        .suppress("fp-1", "unmatched_watch", t0() + Duration::hours(1))
        .unwrap();
    assert!(!store
        .is_suppressed("fp-1", "rejected_by_human", t0())
        .unwrap());
}

fn body_suppression_clear_unblocks_immediately(store: Arc<dyn TaskStore>) {
    let until = t0() + Duration::days(30);
    store.suppress("fp-1", "r", until).unwrap();
    store.clear("fp-1", "r").unwrap();
    assert!(!store.is_suppressed("fp-1", "r", t0()).unwrap());
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
    body_release_claim_decrements_attempts,
    body_release_claim_rejects_non_wip,
    body_list_stale_returns_old_wip_tasks_without_modifying_them,
    body_list_stale_skips_recent_or_non_wip_tasks,
    body_release_stale_recovers_old_wip_tasks,
    body_release_stale_skips_recent_wip_tasks,
    body_release_stale_skips_non_wip_tasks,
    body_mark_failed_preserves_attempts_for_retry,
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
    body_upsert_watch_task_rejects_same_id_with_different_fingerprint,
    body_find_task_by_pr_returns_owning_task,
    body_find_task_by_pr_returns_none_when_unknown,
    body_find_active_by_spec_path_matches_active_only,
    body_find_active_by_spec_path_returns_none_when_no_match,
    body_find_active_by_spec_path_rejects_invariant_violation,
    body_force_status_bypasses_normal_transition,
    body_force_status_does_not_unblock_dependents,
    body_force_status_records_event_with_reason,
    body_suppression_blocks_until_window_expires,
    body_suppression_is_scoped_by_reason,
    body_suppression_clear_unblocks_immediately,
);

#[test]
fn graph_cycle_detection_works_through_domain() {
    let g = TaskGraph::build([
        (TaskId::from_raw("A"), TaskId::from_raw("B")),
        (TaskId::from_raw("B"), TaskId::from_raw("A")),
    ]);
    assert!(g.detect_cycle().is_some());
}
