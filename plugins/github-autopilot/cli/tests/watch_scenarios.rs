//! Scenario-driven tests for the `autopilot watch` daemon.
//!
//! Where `watch_tests.rs` exercises **detection functions** in isolation
//! (push / CI / issues / `detect_ledger_events`), this suite drives the
//! full [`WatchService::tick_once`] orchestration with real SQLite (via
//! `tempfile`) and a mocked GitHub client. The point is to verify the
//! per-tick wiring — what the daemon actually emits over its lifecycle —
//! rather than re-test pure functions already covered upstream.
//!
//! Mocking strategy:
//! - `MockGitHub` (in-process)        — only external dep that needs faking
//! - `SqliteTaskStore` on `TempDir`   — real SQLite, wiped per test
//! - `MockGit`, `MockFs`              — unrelated to the assertions; satisfy
//!   the constructor and absorb `watch.json` writes harmlessly
//! - `FixedClock`                     — drives stale-threshold scenarios
//!
//! Per `CLAUDE.md` "책임 경계", these tests treat the daemon as a black
//! box: inject scenario state, call `tick_once`, assert on the returned
//! `Vec<WatchEvent>`. Internal refactors must keep these green.

mod mock_fs;
mod mock_git;
mod mock_github;

use std::path::PathBuf;
use std::sync::Arc;

use autopilot::cmd::watch::ci::BranchFilter;
use autopilot::cmd::watch::{TickState, WatchArgs, WatchEvent, WatchService};
use autopilot::domain::{Epic, EpicStatus, TaskId, TaskSource, TaskStatus};
use autopilot::ports::clock::{Clock, FixedClock};
use autopilot::ports::task_store::{EpicPlan, NewTask, NewWatchTask, TaskStore};
use autopilot::store::SqliteTaskStore;
use chrono::{DateTime, Duration, TimeZone, Utc};
use mock_fs::MockFs;
use mock_git::MockGit;
use mock_github::MockGitHub;
use tempfile::TempDir;

// ── Fixture ─────────────────────────────────────────────────────────────

/// Reference time anchoring every scenario. Keeps `Utc::now()` out of
/// assertions so failures are reproducible across machines.
fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap()
}

/// One self-contained per-tick test environment. The `_tmp` field keeps
/// the SQLite DB file alive for the test duration; dropping the fixture
/// wipes it.
struct Fixture {
    _tmp: TempDir,
    store: Arc<dyn TaskStore>,
    clock: Arc<FixedClock>,
    svc: WatchService,
    args: WatchArgs,
    ts: TickState,
}

impl Fixture {
    /// Builds a fixture seeded at [`t0()`] with a stale threshold of
    /// `stale_secs` seconds. The daemon has just started: no historical
    /// state on disk, no GitHub events, no SQLite events.
    fn new(stale_secs: u64) -> Self {
        Self::new_at(t0(), stale_secs)
    }

    fn new_at(now: DateTime<Utc>, stale_secs: u64) -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("autopilot.db");
        let store: Arc<dyn TaskStore> =
            Arc::new(SqliteTaskStore::open(&db_path).expect("open sqlite"));

        let clock = Arc::new(FixedClock::new(now));
        let github = Arc::new(MockGitHub::new());
        // MockGit returns no remote ref → push detection is a no-op,
        // which is exactly what we want for ledger-focused scenarios.
        let git = MockGit::new().with_repo_name("test-repo");
        let fs = MockFs::new();

        let svc = WatchService::new(github, Box::new(git), Box::new(fs))
            .with_store(Arc::clone(&store))
            .with_clock(Arc::clone(&clock) as Arc<_>);

        let args = WatchArgs {
            poll_sec: 0,
            branch: "main".to_string(),
            branch_filter: BranchFilter::All,
            label_prefix: "autopilot:".to_string(),
            stale_threshold: format!("{stale_secs}s"),
            ledger_events: true,
        };

        let ts = svc.init_tick_state(&args).expect("init tick state");

        Self {
            _tmp: tmp,
            store,
            clock,
            svc,
            args,
            ts,
        }
    }

    /// Runs one tick and returns the events the daemon would have
    /// printed. Mutates internal state for the next call.
    fn tick(&mut self) -> Vec<WatchEvent> {
        self.svc.tick_once(&mut self.ts, &self.args)
    }

    /// Advances the injected clock by `dur`, mirroring real elapsed time
    /// without `thread::sleep`.
    fn advance(&self, dur: Duration) {
        self.clock.advance(dur);
    }

    fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// Re-creates the `WatchService` against the same DB, simulating a
    /// daemon restart. State is reloaded via `MockFs`-backed
    /// `load_state` (empty in tests), but the SQLite events table
    /// persists, so dedupe is what's under test.
    ///
    /// Returns `(new_svc, new_ts)` rather than mutating `self` so tests
    /// can keep referencing the original store handle.
    fn restart(&self) -> (WatchService, TickState) {
        let github = Arc::new(MockGitHub::new());
        let git = MockGit::new().with_repo_name("test-repo");
        let fs = MockFs::new();
        let svc = WatchService::new(github, Box::new(git), Box::new(fs))
            .with_store(Arc::clone(&self.store))
            .with_clock(Arc::clone(&self.clock) as Arc<_>);
        let ts = svc.init_tick_state(&self.args).expect("init tick state");
        (svc, ts)
    }
}

// ── Test data builders ──────────────────────────────────────────────────

fn epic(name: &str, created_at: DateTime<Utc>) -> Epic {
    Epic {
        name: name.to_string(),
        spec_path: PathBuf::from(format!("specs/{name}.md")),
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

fn watch_task(id: &str, epic: &str) -> NewWatchTask {
    NewWatchTask {
        id: TaskId::from_raw(id),
        epic_name: epic.to_string(),
        source: TaskSource::Human,
        fingerprint: format!("fp-{id}"),
        title: format!("Task {id}"),
        body: None,
    }
}

/// Ensures `epic_name` exists in the store. `upsert_watch_task` enforces
/// a foreign-key relationship to `epics`, so the epic row must be in
/// place before any task can be inserted via the watch path.
fn ensure_epic(store: &Arc<dyn TaskStore>, epic_name: &str, at: DateTime<Utc>) {
    store
        .upsert_epic(&epic(epic_name, at))
        .expect("upsert epic");
}

/// Filters events for the matching variant. Lets each scenario assert on
/// a specific event kind without coupling to ordering of unrelated events.
fn ready(events: &[WatchEvent]) -> Vec<(&str, &str)> {
    events
        .iter()
        .filter_map(|e| match e {
            WatchEvent::TaskReady { epic, task_id } => Some((epic.as_str(), task_id.as_str())),
            _ => None,
        })
        .collect()
}

fn epic_done(events: &[WatchEvent]) -> Vec<(&str, u64)> {
    events
        .iter()
        .filter_map(|e| match e {
            WatchEvent::EpicDone { epic, total } => Some((epic.as_str(), *total)),
            _ => None,
        })
        .collect()
}

fn stale(events: &[WatchEvent]) -> Vec<(String, Vec<String>)> {
    events
        .iter()
        .filter_map(|e| match e {
            WatchEvent::StaleWip { epic, candidates } => Some((epic.clone(), candidates.clone())),
            _ => None,
        })
        .collect()
}

// ── Scenario 1: Full lifecycle ──────────────────────────────────────────

/// task add → TASK_READY → claim (silent) → wait past stale → STALE_WIP
/// → complete → EPIC_DONE. One end-to-end pass through every emission
/// type in the order Monitor's dispatcher would observe.
#[test]
fn scenario_full_task_lifecycle() {
    let mut fx = Fixture::new(/*stale_secs=*/ 5);

    // 1. Insert a watch task — should produce TASK_READY on next tick.
    ensure_epic(&fx.store, "e1", fx.now());
    fx.store
        .upsert_watch_task(watch_task("t1", "e1"), fx.now())
        .expect("upsert");

    let evs = fx.tick();
    assert_eq!(
        ready(&evs),
        vec![("e1", "t1")],
        "expected single TASK_READY, got {evs:?}"
    );
    assert!(epic_done(&evs).is_empty());
    assert!(stale(&evs).is_empty());

    // 2. Worker claims the task — no event is emitted (claim is silent
    //    by design; only state-machine *outputs* go to stdout).
    fx.advance(Duration::seconds(1));
    fx.store
        .claim_next_task("e1", fx.now())
        .expect("claim")
        .expect("ready task to claim");
    let evs = fx.tick();
    assert!(
        ready(&evs).is_empty() && epic_done(&evs).is_empty() && stale(&evs).is_empty(),
        "claim must not emit watch events; got {evs:?}"
    );

    // 3. Wait past the stale threshold → STALE_WIP for t1.
    fx.advance(Duration::seconds(10));
    let evs = fx.tick();
    assert_eq!(
        stale(&evs),
        vec![("e1".to_string(), vec!["t1".to_string()])],
        "expected STALE_WIP for t1, got {evs:?}"
    );

    // 4. Complete the task → EPIC_DONE (single-task epic).
    fx.advance(Duration::seconds(1));
    fx.store
        .complete_task_and_unblock(&TaskId::from_raw("t1"), 99, fx.now())
        .expect("complete");
    let evs = fx.tick();
    assert_eq!(
        epic_done(&evs),
        vec![("e1", 1)],
        "expected EPIC_DONE for e1, got {evs:?}"
    );
}

// ── Scenario 2: Idempotency across daemon restart ───────────────────────

/// State persists in SQLite (events table). After emitting once and
/// restarting the service, the new service must NOT replay events that
/// have already been emitted in this lifetime.
///
/// Note: this checks the *intra-process* idempotency contract — the
/// `LedgerState` round-trip across restart is covered by the unit test
/// `ledger_state_round_trips_through_json` in `watch_tests.rs`. Here we
/// verify the orchestration also re-seeds correctly.
#[test]
fn scenario_restart_with_seeded_ledger_does_not_replay_past_events() {
    let mut fx = Fixture::new(/*stale_secs=*/ 60);

    // Lifetime A: complete a task and emit TASK_READY + EPIC_DONE.
    let plan = EpicPlan {
        epic: epic("e2", fx.now()),
        tasks: vec![new_task("only", "Only")],
        deps: vec![],
    };
    fx.store
        .insert_epic_with_tasks(plan, fx.now())
        .expect("insert");
    fx.advance(Duration::seconds(1));
    fx.store.claim_next_task("e2", fx.now()).expect("claim");
    fx.store
        .complete_task_and_unblock(&TaskId::from_raw("only"), 1, fx.now())
        .expect("complete");
    let evs = fx.tick();
    assert_eq!(epic_done(&evs), vec![("e2", 1)]);

    // Lifetime B: a fresh service against the same DB. Ledger cursor is
    // re-seeded to `now` (no prior `watch.json` on MockFs), so historical
    // events are NOT backfilled.
    fx.advance(Duration::seconds(5));
    let (svc2, mut ts2) = fx.restart();
    let evs2 = svc2.tick_once(&mut ts2, &fx.args);
    assert!(
        ready(&evs2).is_empty() && epic_done(&evs2).is_empty(),
        "fresh daemon must not backfill history; got {evs2:?}"
    );
}

// ── Scenario 3: Concurrent CLI access ───────────────────────────────────

/// A CLI process (simulated via direct `TaskStore` call on the same DB)
/// inserts a task between ticks. The watch daemon must pick it up on the
/// next tick — i.e. SQLite is the source of truth, not in-memory state.
#[test]
fn scenario_cli_insert_visible_on_next_tick() {
    let mut fx = Fixture::new(/*stale_secs=*/ 60);

    // Seed: nothing happens.
    let evs = fx.tick();
    assert!(ready(&evs).is_empty());

    // Out-of-band insert (simulating `autopilot task add` from another
    // process sharing the SQLite file).
    fx.advance(Duration::seconds(1));
    ensure_epic(&fx.store, "e3", fx.now());
    fx.store
        .upsert_watch_task(watch_task("cli-task", "e3"), fx.now())
        .expect("upsert");

    // Daemon's next tick sees the new event.
    let evs = fx.tick();
    assert_eq!(ready(&evs), vec![("e3", "cli-task")]);
}

// ── Scenario 4: GitHub + ledger interleaving ────────────────────────────

/// Both GitHub-driven (issues) and ledger-driven (TASK_READY) events
/// arrive in the same tick window. Both emission paths must fire
/// independently — neither suppresses the other.
#[test]
fn scenario_github_and_ledger_emit_in_same_tick() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = tmp.path().join("autopilot.db");
    let store: Arc<dyn TaskStore> = Arc::new(SqliteTaskStore::open(&db).expect("open"));
    let clock = Arc::new(FixedClock::new(t0()));

    // GitHub mock returns one fresh open issue → triggers NEW_ISSUE.
    let github = Arc::new(
        MockGitHub::new().with_issues(vec![autopilot::github::OpenIssue {
            number: 42,
            title: "From GitHub".to_string(),
            labels: vec![],
        }]),
    );
    let git = MockGit::new().with_repo_name("test-repo");
    let fs = MockFs::new();

    let svc = WatchService::new(github, Box::new(git), Box::new(fs))
        .with_store(Arc::clone(&store))
        .with_clock(Arc::clone(&clock) as Arc<_>);
    let args = WatchArgs {
        poll_sec: 0,
        branch: "main".to_string(),
        branch_filter: BranchFilter::All,
        label_prefix: "autopilot:".to_string(),
        stale_threshold: "60s".to_string(),
        ledger_events: true,
    };
    let mut ts = svc.init_tick_state(&args).expect("init");

    // Seed: GitHub seen-set is populated on first run (so existing
    // issues aren't backfilled). After this tick, issue 42 is "seen".
    let _ = svc.tick_once(&mut ts, &args);

    // Now insert a fresh GitHub issue + a SQLite task between ticks.
    // The new issue (43) is not in seen_issue_numbers, so NEW_ISSUE
    // fires. The task triggers TASK_READY in the same tick.
    clock.advance(Duration::seconds(1));
    ensure_epic(&store, "eX", clock.now());
    store
        .upsert_watch_task(watch_task("t1", "eX"), clock.now())
        .expect("upsert");
    // Replace the GitHub issue list to include a new one.
    let svc2 = WatchService::new(
        Arc::new(MockGitHub::new().with_issues(vec![
            autopilot::github::OpenIssue {
                number: 42, // already seen
                title: "Old".to_string(),
                labels: vec![],
            },
            autopilot::github::OpenIssue {
                number: 43, // new
                title: "Fresh".to_string(),
                labels: vec![],
            },
        ])),
        Box::new(MockGit::new().with_repo_name("test-repo")),
        Box::new(MockFs::new()),
    )
    .with_store(Arc::clone(&store))
    .with_clock(Arc::clone(&clock) as Arc<_>);
    // Carry over `seen_issue_numbers` from the first svc so re-seeding
    // doesn't suppress issue 43.
    let mut ts2 = svc2.init_tick_state(&args).expect("init");
    ts2.seen_issue_numbers = ts.seen_issue_numbers.clone();

    let evs = svc2.tick_once(&mut ts2, &args);
    let new_issues: Vec<u64> = evs
        .iter()
        .filter_map(|e| match e {
            WatchEvent::NewIssue { number, .. } => Some(*number),
            _ => None,
        })
        .collect();
    assert_eq!(new_issues, vec![43], "expected NEW_ISSUE 43, got {evs:?}");
    assert_eq!(
        ready(&evs),
        vec![("eX", "t1")],
        "expected TASK_READY t1 in same tick, got {evs:?}"
    );
}

// ── Scenario 5: Empty epic / pending deps ───────────────────────────────

/// A task whose dependency is unmet must NOT produce TASK_READY (the
/// store leaves it in `Blocked`). Once the dep completes and the
/// `task_unblocked` event lands, TASK_READY fires.
#[test]
fn scenario_blocked_task_only_emits_after_unblock() {
    let mut fx = Fixture::new(/*stale_secs=*/ 60);

    // a depends on b. b is the entry-point.
    let plan = EpicPlan {
        epic: epic("e5", fx.now()),
        tasks: vec![new_task("a", "A"), new_task("b", "B")],
        deps: vec![(TaskId::from_raw("a"), TaskId::from_raw("b"))],
    };
    fx.store
        .insert_epic_with_tasks(plan, fx.now())
        .expect("insert");

    // First tick: only b is Ready. a is Blocked and must not emit.
    let evs = fx.tick();
    let r = ready(&evs);
    assert_eq!(
        r,
        vec![("e5", "b")],
        "blocked task `a` must not appear; got {evs:?}"
    );

    // Worker completes b → task_unblocked event for a.
    fx.advance(Duration::seconds(1));
    fx.store.claim_next_task("e5", fx.now()).expect("claim");
    fx.store
        .complete_task_and_unblock(&TaskId::from_raw("b"), 1, fx.now())
        .expect("complete");

    // Next tick: TASK_READY for a (now unblocked). EPIC_DONE must NOT
    // fire because a is still open.
    let evs = fx.tick();
    assert_eq!(
        ready(&evs),
        vec![("e5", "a")],
        "expected TASK_READY for unblocked task, got {evs:?}"
    );
    assert!(
        epic_done(&evs).is_empty(),
        "EPIC_DONE must not fire while tasks remain; got {evs:?}"
    );
}

// ── Scenario 6: Stale recovery dedup ────────────────────────────────────

/// After STALE_WIP fires for a task, an agent calls `release_claim` to
/// revert it to Ready. The watch daemon must NOT re-emit STALE_WIP for
/// that task on subsequent ticks, even if the *new* claim later goes
/// stale — `stale_seen` is a per-task dedupe.
///
/// This covers the agent-driven recovery flow where Monitor dispatches
/// the release on STALE_WIP and the daemon must not loop.
#[test]
fn scenario_stale_wip_does_not_re_fire_after_release() {
    let mut fx = Fixture::new(/*stale_secs=*/ 5);

    let plan = EpicPlan {
        epic: epic("e6", fx.now()),
        tasks: vec![new_task("a", "A")],
        deps: vec![],
    };
    fx.store
        .insert_epic_with_tasks(plan, fx.now())
        .expect("insert");
    fx.advance(Duration::seconds(1));
    fx.store.claim_next_task("e6", fx.now()).expect("claim");

    // Wait past threshold → STALE_WIP fires once.
    fx.advance(Duration::seconds(10));
    let evs = fx.tick();
    assert_eq!(
        stale(&evs),
        vec![("e6".to_string(), vec!["a".to_string()])],
        "first STALE_WIP must fire; got {evs:?}"
    );

    // Agent releases the stale claim → task back to Ready, leaving the
    // stale set entirely (no longer Wip).
    fx.advance(Duration::seconds(1));
    fx.store
        .release_claim(&TaskId::from_raw("a"), fx.now())
        .expect("release");

    // Verify state-machine effect: task is Ready, not Wip.
    let task = fx
        .store
        .get_task(&TaskId::from_raw("a"))
        .expect("get_task")
        .expect("task exists");
    assert_eq!(task.status, TaskStatus::Ready);

    // Next tick: stale list is empty. TASK_READY may or may not fire
    // depending on whether `task_unblocked` was emitted by `release_claim`,
    // but STALE_WIP definitely must not.
    let evs = fx.tick();
    assert!(
        stale(&evs).is_empty(),
        "STALE_WIP must not re-emit after release; got {evs:?}"
    );
}

// ── Scenario 7: Multiple epics ──────────────────────────────────────────

/// Events from independent epics must be tagged correctly and never
/// cross-contaminate (e.g. EPIC_DONE for one epic when another's task
/// completes).
#[test]
fn scenario_two_epics_emit_independent_events() {
    let mut fx = Fixture::new(/*stale_secs=*/ 60);

    // Epic A: one task. Epic B: two tasks (so B can't go Done yet).
    fx.store
        .insert_epic_with_tasks(
            EpicPlan {
                epic: epic("A", fx.now()),
                tasks: vec![new_task("a1", "A1")],
                deps: vec![],
            },
            fx.now(),
        )
        .expect("insert A");
    fx.store
        .insert_epic_with_tasks(
            EpicPlan {
                epic: epic("B", fx.now()),
                tasks: vec![new_task("b1", "B1"), new_task("b2", "B2")],
                deps: vec![],
            },
            fx.now(),
        )
        .expect("insert B");

    // First tick: TASK_READY for a1, b1, b2 (one per task).
    let evs = fx.tick();
    let mut r = ready(&evs);
    r.sort();
    assert_eq!(
        r,
        vec![("A", "a1"), ("B", "b1"), ("B", "b2")],
        "expected three TASK_READY tagged per epic; got {evs:?}"
    );

    // Complete A's task and B's b1. Only A should emit EPIC_DONE.
    fx.advance(Duration::seconds(1));
    fx.store.claim_next_task("A", fx.now()).expect("claim a1");
    fx.store
        .complete_task_and_unblock(&TaskId::from_raw("a1"), 11, fx.now())
        .expect("complete a1");
    fx.store.claim_next_task("B", fx.now()).expect("claim b1");
    fx.store
        .complete_task_and_unblock(&TaskId::from_raw("b1"), 22, fx.now())
        .expect("complete b1");

    let evs = fx.tick();
    let dones = epic_done(&evs);
    assert_eq!(
        dones,
        vec![("A", 1)],
        "only A is fully done; B has b2 open. got {evs:?}"
    );
}
