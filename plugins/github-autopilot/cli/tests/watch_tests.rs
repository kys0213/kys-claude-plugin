mod mock_git;
mod mock_github;

use autopilot::cmd::watch::ci::{detect_ci, BranchFilter};
use autopilot::cmd::watch::issues::detect_issues;
use autopilot::cmd::watch::ledger::{detect_ledger_events, LedgerState};
use autopilot::cmd::watch::push::detect_push;
use autopilot::cmd::watch::WatchEvent;
use autopilot::domain::{Epic, EpicStatus, TaskId, TaskSource};
use autopilot::github::{CompletedRun, OpenIssue};
use autopilot::ports::task_store::{EpicPlan, NewTask, NewWatchTask, TaskStore};
use autopilot::store::InMemoryTaskStore;
use chrono::{DateTime, Duration, TimeZone, Utc};
use mock_git::MockGit;
use std::collections::HashSet;
use std::sync::Arc;

// ── Push detection tests ──

#[test]
fn push_detects_new_commits() {
    let git = MockGit::new()
        .with_ref("origin/main", "new_sha")
        .with_rev_list_count("old_sha", "new_sha", 3);
    let result = detect_push(&git, "origin", "main", "old_sha");
    assert!(result.is_some());
    match result.unwrap() {
        WatchEvent::MainUpdated {
            before,
            after,
            count,
        } => {
            assert_eq!(before, "old_sha");
            assert_eq!(after, "new_sha");
            assert_eq!(count, 3);
        }
        other => panic!("expected MainUpdated, got {other}"),
    }
}

#[test]
fn push_no_event_when_unchanged() {
    let git = MockGit::new().with_ref("origin/main", "same_sha");
    let result = detect_push(&git, "origin", "main", "same_sha");
    assert!(result.is_none());
}

#[test]
fn push_returns_none_on_resolve_failure() {
    let git = MockGit::new(); // no ref configured → resolve fails
    let result = detect_push(&git, "origin", "main", "old_sha");
    assert!(result.is_none());
}

// ── CI detection tests ──

fn run(id: u64, name: &str, branch: &str, conclusion: &str) -> CompletedRun {
    CompletedRun {
        id,
        name: name.to_string(),
        branch: branch.to_string(),
        conclusion: conclusion.to_string(),
    }
}

#[test]
fn ci_detects_new_failure() {
    let runs = vec![run(100, "CI", "main", "failure")];
    let events = detect_ci(&runs, &HashSet::new(), "main", &BranchFilter::All);
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        WatchEvent::CiFailure { run_id: 100, .. }
    ));
}

#[test]
fn ci_detects_new_success() {
    let runs = vec![run(200, "Build", "feature/issue-1", "success")];
    let events = detect_ci(&runs, &HashSet::new(), "main", &BranchFilter::Autopilot);
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        WatchEvent::CiSuccess { run_id: 200, .. }
    ));
}

#[test]
fn ci_skips_seen_runs() {
    let runs = vec![run(100, "CI", "main", "failure")];
    let seen: HashSet<u64> = [100].into();
    let events = detect_ci(&runs, &seen, "main", &BranchFilter::All);
    assert!(events.is_empty());
}

#[test]
fn ci_filters_non_autopilot_branches() {
    let runs = vec![run(100, "CI", "user/feature", "failure")];
    let events = detect_ci(&runs, &HashSet::new(), "main", &BranchFilter::Autopilot);
    assert!(events.is_empty());
}

#[test]
fn ci_allows_all_branches_in_all_mode() {
    let runs = vec![run(100, "CI", "user/feature", "failure")];
    let events = detect_ci(&runs, &HashSet::new(), "main", &BranchFilter::All);
    assert_eq!(events.len(), 1);
}

#[test]
fn ci_autopilot_allows_default_branch() {
    let runs = vec![run(100, "CI", "main", "failure")];
    let events = detect_ci(&runs, &HashSet::new(), "main", &BranchFilter::Autopilot);
    assert_eq!(events.len(), 1);
}

#[test]
fn ci_autopilot_allows_draft_branches() {
    let runs = vec![run(100, "CI", "draft/issue-5", "success")];
    let events = detect_ci(&runs, &HashSet::new(), "main", &BranchFilter::Autopilot);
    assert_eq!(events.len(), 1);
}

#[test]
fn ci_ignores_cancelled_runs() {
    let runs = vec![run(100, "CI", "main", "cancelled")];
    let events = detect_ci(&runs, &HashSet::new(), "main", &BranchFilter::All);
    assert!(events.is_empty());
}

// ── Issue detection tests ──

fn issue(number: u64, title: &str, labels: &[&str]) -> OpenIssue {
    OpenIssue {
        number,
        title: title.to_string(),
        labels: labels.iter().map(|l| l.to_string()).collect(),
    }
}

#[test]
fn issues_detects_new_unlabeled() {
    let issues = vec![issue(55, "Add OAuth", &[])];
    let events = detect_issues(&issues, &HashSet::new(), "autopilot:");
    assert_eq!(events.len(), 1);
    match &events[0] {
        WatchEvent::NewIssue { number, title } => {
            assert_eq!(*number, 55);
            assert_eq!(title, "Add OAuth");
        }
        other => panic!("expected NewIssue, got {other}"),
    }
}

#[test]
fn issues_skips_labeled() {
    let issues = vec![issue(55, "Add OAuth", &["autopilot:ready"])];
    let events = detect_issues(&issues, &HashSet::new(), "autopilot:");
    assert!(events.is_empty());
}

#[test]
fn issues_skips_seen() {
    let issues = vec![issue(55, "Add OAuth", &[])];
    let seen: HashSet<u64> = [55].into();
    let events = detect_issues(&issues, &seen, "autopilot:");
    assert!(events.is_empty());
}

#[test]
fn issues_allows_non_autopilot_labels() {
    let issues = vec![issue(55, "Add OAuth", &["bug", "enhancement"])];
    let events = detect_issues(&issues, &HashSet::new(), "autopilot:");
    assert_eq!(events.len(), 1);
}

// ── Display format tests ──

#[test]
fn main_updated_display() {
    let e = WatchEvent::MainUpdated {
        before: "abc".to_string(),
        after: "def".to_string(),
        count: 3,
    };
    assert_eq!(e.to_string(), "MAIN_UPDATED before=abc after=def count=3");
}

#[test]
fn ci_failure_display() {
    let e = WatchEvent::CiFailure {
        run_id: 123,
        workflow: "validate.yml".to_string(),
        branch: "main".to_string(),
    };
    assert_eq!(
        e.to_string(),
        "CI_FAILURE run_id=123 workflow=validate.yml branch=main"
    );
}

#[test]
fn ci_success_display() {
    let e = WatchEvent::CiSuccess {
        run_id: 456,
        workflow: "build.yml".to_string(),
        branch: "feature/issue-1".to_string(),
    };
    assert_eq!(
        e.to_string(),
        "CI_SUCCESS run_id=456 workflow=build.yml branch=feature/issue-1"
    );
}

#[test]
fn new_issue_display() {
    let e = WatchEvent::NewIssue {
        number: 55,
        title: "Add OAuth support".to_string(),
    };
    assert_eq!(e.to_string(), "NEW_ISSUE number=55 title=Add OAuth support");
}

// ── Ledger detection tests ──
//
// These tests drive `detect_ledger_events` against an `InMemoryTaskStore`
// fixture. Time is injected (`now`) so each scenario controls when the
// daemon "wakes up" for a tick relative to event timestamps.

fn ledger_base_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap()
}

fn ledger_store() -> Arc<dyn TaskStore> {
    Arc::new(InMemoryTaskStore::new())
}

/// Builds an Epic with the given name, anchored at `created_at`.
fn make_epic(name: &str, created_at: DateTime<Utc>) -> Epic {
    Epic {
        name: name.to_string(),
        spec_path: std::path::PathBuf::from(format!("specs/{name}.md")),
        branch: format!("epic/{name}"),
        status: EpicStatus::Active,
        created_at,
        completed_at: None,
    }
}

fn new_task(id: &str, title: &str) -> NewTask {
    NewTask {
        id: TaskId::from_raw(id),
        source: TaskSource::Decompose,
        fingerprint: Some(format!("fp-{id}")),
        title: title.to_string(),
        body: None,
    }
}

/// Daemon at `seed_at` boots with an empty `LedgerState`. Seeds the
/// cursor (mirrors `WatchService::run`) so the first tick does not
/// backfill historical events.
fn fresh_state(seed_at: DateTime<Utc>) -> LedgerState {
    let mut s = LedgerState::default();
    s.seed(seed_at);
    s
}

// TASK_READY ─────────────────────────────────────────────────────────────

#[test]
fn ledger_emits_task_ready_when_watch_task_inserted() {
    let store = ledger_store();
    let t0 = ledger_base_time();
    let mut state = fresh_state(t0);

    // Daemon was already running (seeded at t0); a watch task lands at t0+1s.
    let now = t0 + Duration::seconds(1);
    store
        .upsert_watch_task(
            NewWatchTask {
                id: TaskId::from_raw("t1"),
                epic_name: "e1".to_string(),
                source: TaskSource::Human,
                fingerprint: "fp-t1".to_string(),
                title: "T1".to_string(),
                body: None,
            },
            now,
        )
        .expect("upsert");

    let events = detect_ledger_events(store.as_ref(), &mut state, now, 60);
    assert_eq!(events.len(), 1, "expected 1 event, got {events:?}");
    match &events[0] {
        WatchEvent::TaskReady { epic, task_id } => {
            assert_eq!(epic, "e1");
            assert_eq!(task_id, "t1");
        }
        other => panic!("expected TaskReady, got {other}"),
    }
}

#[test]
fn ledger_emits_task_ready_when_dep_unblocks() {
    let store = ledger_store();
    let t0 = ledger_base_time();
    // Seed *after* the epic was created, so initial TaskInserted events for
    // a/b at t0 are not re-emitted (we only care about the unblock).
    let mut state = fresh_state(t0 + Duration::seconds(1));

    let plan = EpicPlan {
        epic: make_epic("e2", t0),
        // a depends on b: b is the entry-point and goes Ready first.
        tasks: vec![new_task("a", "A"), new_task("b", "B")],
        deps: vec![(TaskId::from_raw("a"), TaskId::from_raw("b"))],
    };
    store.insert_epic_with_tasks(plan, t0).expect("insert");

    // Claim & complete b at t1 → a is unblocked → TaskUnblocked event lands.
    let t1 = t0 + Duration::seconds(2);
    let _claimed = store.claim_next_task("e2", t1).expect("claim");
    let _report = store
        .complete_task_and_unblock(&TaskId::from_raw("b"), 42, t1)
        .expect("complete");

    let events = detect_ledger_events(store.as_ref(), &mut state, t1, 60);
    // Only `a` is now Ready; b is Done. We must see exactly one TASK_READY for a.
    let ready: Vec<&WatchEvent> = events
        .iter()
        .filter(|e| matches!(e, WatchEvent::TaskReady { .. }))
        .collect();
    assert_eq!(ready.len(), 1, "expected 1 TaskReady, got {events:?}");
    match ready[0] {
        WatchEvent::TaskReady { epic, task_id } => {
            assert_eq!(epic, "e2");
            assert_eq!(task_id, "a");
        }
        _ => unreachable!(),
    }
}

#[test]
fn ledger_skips_task_ready_when_task_no_longer_ready() {
    // A task gets inserted then immediately claimed before the watch tick.
    // The TaskInserted event is still in the table, but get_task shows Wip,
    // so we must NOT emit TASK_READY (the consumer would dispatch a stale
    // claim).
    let store = ledger_store();
    let t0 = ledger_base_time();
    let mut state = fresh_state(t0);

    let now = t0 + Duration::seconds(1);
    store
        .upsert_watch_task(
            NewWatchTask {
                id: TaskId::from_raw("t1"),
                epic_name: "e1".to_string(),
                source: TaskSource::Human,
                fingerprint: "fp-t1".to_string(),
                title: "T1".to_string(),
                body: None,
            },
            now,
        )
        .expect("upsert");
    // Worker raced ahead and claimed before the watch tick fired.
    let _ = store.claim_next_task("e1", now).expect("claim");

    let events = detect_ledger_events(store.as_ref(), &mut state, now, 60);
    let ready: Vec<&WatchEvent> = events
        .iter()
        .filter(|e| matches!(e, WatchEvent::TaskReady { .. }))
        .collect();
    assert!(ready.is_empty(), "expected no TaskReady, got {events:?}");
}

// EPIC_DONE ──────────────────────────────────────────────────────────────

#[test]
fn ledger_emits_epic_done_when_last_task_completes() {
    let store = ledger_store();
    let t0 = ledger_base_time();
    let mut state = fresh_state(t0 + Duration::seconds(1));

    let plan = EpicPlan {
        epic: make_epic("e3", t0),
        tasks: vec![new_task("only", "Only")],
        deps: vec![],
    };
    store.insert_epic_with_tasks(plan, t0).expect("insert");

    let t1 = t0 + Duration::seconds(2);
    let _ = store.claim_next_task("e3", t1).expect("claim");
    let _ = store
        .complete_task_and_unblock(&TaskId::from_raw("only"), 7, t1)
        .expect("complete");

    let events = detect_ledger_events(store.as_ref(), &mut state, t1, 60);
    let epic_done: Vec<&WatchEvent> = events
        .iter()
        .filter(|e| matches!(e, WatchEvent::EpicDone { .. }))
        .collect();
    assert_eq!(epic_done.len(), 1, "expected 1 EpicDone, got {events:?}");
    match epic_done[0] {
        WatchEvent::EpicDone { epic, total } => {
            assert_eq!(epic, "e3");
            assert_eq!(*total, 1);
        }
        _ => unreachable!(),
    }
}

#[test]
fn ledger_skips_epic_done_when_other_tasks_still_open() {
    let store = ledger_store();
    let t0 = ledger_base_time();
    let mut state = fresh_state(t0 + Duration::seconds(1));

    let plan = EpicPlan {
        epic: make_epic("e4", t0),
        tasks: vec![new_task("a", "A"), new_task("b", "B")],
        deps: vec![],
    };
    store.insert_epic_with_tasks(plan, t0).expect("insert");

    // Complete only a; b is still Ready.
    let t1 = t0 + Duration::seconds(2);
    let _ = store.claim_next_task("e4", t1).expect("claim");
    let _ = store
        .complete_task_and_unblock(&TaskId::from_raw("a"), 1, t1)
        .expect("complete");

    let events = detect_ledger_events(store.as_ref(), &mut state, t1, 60);
    let epic_done: Vec<&WatchEvent> = events
        .iter()
        .filter(|e| matches!(e, WatchEvent::EpicDone { .. }))
        .collect();
    assert!(epic_done.is_empty(), "got {events:?}");
}

// STALE_WIP ──────────────────────────────────────────────────────────────

#[test]
fn ledger_emits_stale_wip_for_old_wip_tasks() {
    let store = ledger_store();
    let t0 = ledger_base_time();
    let plan = EpicPlan {
        epic: make_epic("e5", t0),
        tasks: vec![new_task("a", "A")],
        deps: vec![],
    };
    store.insert_epic_with_tasks(plan, t0).expect("insert");
    // Worker claimed the task at t0+1s but never completed.
    let claim_at = t0 + Duration::seconds(1);
    let _ = store.claim_next_task("e5", claim_at).expect("claim");

    // Daemon wakes up much later; the stale threshold is 5s.
    let mut state = fresh_state(claim_at + Duration::seconds(10));
    let now = claim_at + Duration::seconds(10);
    let events = detect_ledger_events(store.as_ref(), &mut state, now, 5);
    let stale: Vec<&WatchEvent> = events
        .iter()
        .filter(|e| matches!(e, WatchEvent::StaleWip { .. }))
        .collect();
    assert_eq!(stale.len(), 1, "expected 1 StaleWip, got {events:?}");
    match stale[0] {
        WatchEvent::StaleWip { epic, candidates } => {
            assert_eq!(epic, "e5");
            assert_eq!(candidates, &vec!["a".to_string()]);
        }
        _ => unreachable!(),
    }
}

#[test]
fn ledger_stale_wip_display_uses_json_array() {
    let e = WatchEvent::StaleWip {
        epic: "e".to_string(),
        candidates: vec!["a".to_string(), "b".to_string()],
    };
    assert_eq!(e.to_string(), r#"STALE_WIP candidates=["a","b"] epic=e"#);
}

// Idempotency ────────────────────────────────────────────────────────────

#[test]
fn ledger_does_not_re_emit_task_ready_on_second_tick() {
    let store = ledger_store();
    let t0 = ledger_base_time();
    let mut state = fresh_state(t0);

    let now = t0 + Duration::seconds(1);
    store
        .upsert_watch_task(
            NewWatchTask {
                id: TaskId::from_raw("t1"),
                epic_name: "e1".to_string(),
                source: TaskSource::Human,
                fingerprint: "fp-t1".to_string(),
                title: "T1".to_string(),
                body: None,
            },
            now,
        )
        .expect("upsert");

    let first = detect_ledger_events(store.as_ref(), &mut state, now, 60);
    assert_eq!(first.len(), 1);

    // Second tick (no new events) — must be silent.
    let later = now + Duration::seconds(1);
    let second = detect_ledger_events(store.as_ref(), &mut state, later, 60);
    assert!(
        second.is_empty(),
        "second tick should not re-emit; got {second:?}"
    );
}

#[test]
fn ledger_does_not_re_emit_epic_done_on_second_tick() {
    let store = ledger_store();
    let t0 = ledger_base_time();
    let mut state = fresh_state(t0 + Duration::seconds(1));

    let plan = EpicPlan {
        epic: make_epic("e6", t0),
        tasks: vec![new_task("only", "Only")],
        deps: vec![],
    };
    store.insert_epic_with_tasks(plan, t0).expect("insert");
    let t1 = t0 + Duration::seconds(2);
    let _ = store.claim_next_task("e6", t1).expect("claim");
    let _ = store
        .complete_task_and_unblock(&TaskId::from_raw("only"), 1, t1)
        .expect("complete");

    let first = detect_ledger_events(store.as_ref(), &mut state, t1, 60);
    let first_epic_done: Vec<_> = first
        .iter()
        .filter(|e| matches!(e, WatchEvent::EpicDone { .. }))
        .collect();
    assert_eq!(first_epic_done.len(), 1);

    // Second tick: completion event still present in the log; we must not
    // re-emit EPIC_DONE because state.epics_done remembers it.
    let second = detect_ledger_events(store.as_ref(), &mut state, t1 + Duration::seconds(1), 60);
    let second_epic_done: Vec<_> = second
        .iter()
        .filter(|e| matches!(e, WatchEvent::EpicDone { .. }))
        .collect();
    assert!(
        second_epic_done.is_empty(),
        "EpicDone must fire exactly once; got {second:?}"
    );
}

#[test]
fn ledger_does_not_re_emit_stale_wip_for_same_task() {
    let store = ledger_store();
    let t0 = ledger_base_time();
    let plan = EpicPlan {
        epic: make_epic("e7", t0),
        tasks: vec![new_task("a", "A")],
        deps: vec![],
    };
    store.insert_epic_with_tasks(plan, t0).expect("insert");
    let claim_at = t0 + Duration::seconds(1);
    let _ = store.claim_next_task("e7", claim_at).expect("claim");

    let mut state = fresh_state(claim_at + Duration::seconds(10));
    let tick1 = claim_at + Duration::seconds(10);
    let first = detect_ledger_events(store.as_ref(), &mut state, tick1, 5);
    let first_stale: Vec<_> = first
        .iter()
        .filter(|e| matches!(e, WatchEvent::StaleWip { .. }))
        .collect();
    assert_eq!(first_stale.len(), 1);

    // Tick2: still stale, but already reported.
    let tick2 = tick1 + Duration::seconds(1);
    let second = detect_ledger_events(store.as_ref(), &mut state, tick2, 5);
    let second_stale: Vec<_> = second
        .iter()
        .filter(|e| matches!(e, WatchEvent::StaleWip { .. }))
        .collect();
    assert!(
        second_stale.is_empty(),
        "STALE_WIP must dedupe per task; got {second:?}"
    );
}

#[test]
fn ledger_state_round_trips_through_json() {
    // The watch loop persists LedgerState as part of WatchState in
    // /tmp/.../watch.json. Make sure the cursor and dedupe sets survive
    // a serialize/deserialize round-trip — otherwise a daemon restart
    // would re-emit every prior event.
    let store = ledger_store();
    let t0 = ledger_base_time();
    let mut state = fresh_state(t0);

    let now = t0 + Duration::seconds(1);
    store
        .upsert_watch_task(
            NewWatchTask {
                id: TaskId::from_raw("t1"),
                epic_name: "e1".to_string(),
                source: TaskSource::Human,
                fingerprint: "fp-t1".to_string(),
                title: "T1".to_string(),
                body: None,
            },
            now,
        )
        .expect("upsert");
    let _ = detect_ledger_events(store.as_ref(), &mut state, now, 60);

    // Round-trip through JSON (mirrors load_state / save_state).
    let json = serde_json::to_string(&state).expect("serialize");
    let mut restored: LedgerState = serde_json::from_str(&json).expect("deserialize");

    // Restored daemon: same store, no new events. Must stay silent.
    let later = now + Duration::seconds(2);
    let after_restart = detect_ledger_events(store.as_ref(), &mut restored, later, 60);
    assert!(
        after_restart.is_empty(),
        "restart must not replay; got {after_restart:?}"
    );
}
