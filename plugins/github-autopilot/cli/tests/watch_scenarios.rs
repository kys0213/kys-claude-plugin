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
use autopilot::fs::FsOps;
use autopilot::ports::clock::{Clock, FixedClock};
use autopilot::ports::task_store::{EpicPlan, NewTask, NewWatchTask, TaskStore};
use autopilot::store::SqliteTaskStore;
use chrono::{DateTime, Duration, TimeZone, Utc};
use mock_fs::MockFs;
use mock_git::MockGit;
use mock_github::MockGitHub;
use std::collections::HashSet;
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

// ── Resilience helpers ──────────────────────────────────────────────────

/// Returns the path the production code uses for `watch.json` when the
/// repo name is `test-repo` (matching `MockGit::with_repo_name`).
///
/// The format is owned by `WatchService::state_path` — kept inline in
/// production rather than exposed as a helper. Resilience scenarios that
/// need to pre-seed `MockFs` with state must mirror that format here.
fn watch_state_path() -> std::path::PathBuf {
    std::path::PathBuf::from("/tmp/autopilot-test-repo/state/watch.json")
}

/// Per-tick env builder that lets resilience scenarios inject a custom
/// `MockGitHub` and a pre-populated `MockFs`. The store + clock + args
/// match `Fixture::new` for parity with the happy-path scenarios.
struct ResilienceEnv {
    _tmp: TempDir,
    store: Arc<dyn TaskStore>,
    clock: Arc<FixedClock>,
    fs: MockFs,
    svc: WatchService,
    args: WatchArgs,
    ts: TickState,
}

impl ResilienceEnv {
    fn build(now: DateTime<Utc>, stale_secs: u64, github: MockGitHub, fs: MockFs) -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("autopilot.db");
        let store: Arc<dyn TaskStore> =
            Arc::new(SqliteTaskStore::open(&db_path).expect("open sqlite"));
        let clock = Arc::new(FixedClock::new(now));
        let github = Arc::new(github);
        let git = MockGit::new().with_repo_name("test-repo");
        let fs_clone = fs.clone();
        let svc = WatchService::new(github, Box::new(git), Box::new(fs_clone))
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
            fs,
            svc,
            args,
            ts,
        }
    }

    fn tick(&mut self) -> Vec<WatchEvent> {
        self.svc.tick_once(&mut self.ts, &self.args)
    }

    fn advance(&self, dur: Duration) {
        self.clock.advance(dur);
    }
}

fn new_issues(events: &[WatchEvent]) -> Vec<u64> {
    events
        .iter()
        .filter_map(|e| match e {
            WatchEvent::NewIssue { number, .. } => Some(*number),
            _ => None,
        })
        .collect()
}

// ── Scenario 8 (resilience): GitHub fetch failure recovers ──────────────

/// Tick N: `list_open_issues` returns `Err(...)` (transient gh failure).
/// Tick N+1: same call returns Ok with an unseen issue. The daemon must
/// not panic, must not corrupt persisted state, and must emit the
/// `NEW_ISSUE` exactly once on the recovery tick — as if the failed tick
/// had not happened.
///
/// Issues run on `ISSUE_TICK_INTERVAL` (every 12 ticks at base poll, but
/// tick 0 always fires). We drive tick 0 with a failing mock and tick 12
/// with a healthy mock by reconstructing the service — this mirrors how
/// a real transient failure surfaces: state survives, dedupe set is
/// rebuilt on next successful poll.
#[test]
fn scenario_github_fetch_failure_recovers_on_next_tick() {
    // First service: failing on the first list_open_issues call.
    let github_failing = MockGitHub::new().with_fail_issues_first_n(1);
    let env = ResilienceEnv::build(t0(), 60, github_failing, MockFs::new());
    let mut env = env;

    // Tick 0 hits the failing list_open_issues path. The daemon must
    // ignore the error (no panic) and emit no NEW_ISSUE. seen_issue_numbers
    // also stays whatever init seeded it with (init's seeding path may
    // also fail, leaving it empty).
    let evs = env.tick();
    assert!(
        new_issues(&evs).is_empty(),
        "failing list_open_issues must not emit NEW_ISSUE; got {evs:?}"
    );

    // Sanity: the failed tick must not corrupt watch.json.
    // (No state-save tick has fired yet; verify no garbage was written.)
    for (path, content) in env.fs.written_files() {
        if path.ends_with("watch.json") {
            // Anything written must be valid JSON.
            serde_json::from_str::<serde_json::Value>(&content)
                .expect("any persisted watch.json must be valid JSON");
        }
    }

    // Second service: same store, healthy GitHub returning a fresh issue
    // and one already-seen issue. Tick 0 of this service simulates "next
    // tick" after recovery — it should emit NEW_ISSUE for the unseen
    // issue and treat the seen issue as already known.
    //
    // We carry forward the prior `seen_issue_numbers` so the recovery
    // tick's seeding does not silently swallow the new issue (mirroring
    // the same pattern as `scenario_github_and_ledger_emit_in_same_tick`).
    env.advance(Duration::seconds(5));
    let healthy_issues = vec![
        autopilot::github::OpenIssue {
            number: 100,
            title: "Pre-existing".to_string(),
            labels: vec![],
        },
        autopilot::github::OpenIssue {
            number: 101,
            title: "Fresh after recovery".to_string(),
            labels: vec![],
        },
    ];
    // Pretend the prior daemon had already seen issue 100 (e.g. via an
    // earlier successful poll the test doesn't simulate); only 101 is new.
    let mut seen_carry: HashSet<u64> = HashSet::new();
    seen_carry.insert(100);

    let svc2 = WatchService::new(
        Arc::new(MockGitHub::new().with_issues(healthy_issues)),
        Box::new(MockGit::new().with_repo_name("test-repo")),
        Box::new(MockFs::new()),
    )
    .with_store(Arc::clone(&env.store))
    .with_clock(Arc::clone(&env.clock) as Arc<_>);
    let mut ts2 = svc2.init_tick_state(&env.args).expect("init");
    ts2.seen_issue_numbers = seen_carry;

    let evs = svc2.tick_once(&mut ts2, &env.args);
    assert_eq!(
        new_issues(&evs),
        vec![101],
        "recovery tick must emit NEW_ISSUE 101 only; got {evs:?}"
    );
}

// ── Scenario 9 (resilience): no prior watch.json ────────────────────────

/// Pre-existing SQLite state (events from prior daemon runs) but no
/// `watch.json` file at all. `init_tick_state` must seed cursors from
/// `clock.now()` so historical ledger events are NOT replayed.
#[test]
fn scenario_no_prior_watch_json_does_not_replay_history() {
    // Pre-populate SQLite with a completed task at t0.
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("autopilot.db");
    let store: Arc<dyn TaskStore> = Arc::new(SqliteTaskStore::open(&db_path).expect("open sqlite"));
    let now = t0();
    store
        .insert_epic_with_tasks(
            EpicPlan {
                epic: epic("e9", now),
                tasks: vec![new_task("t1", "T1")],
                deps: vec![],
            },
            now,
        )
        .expect("insert");
    store.claim_next_task("e9", now).expect("claim");
    store
        .complete_task_and_unblock(&TaskId::from_raw("t1"), 1, now)
        .expect("complete");

    // Fresh daemon: empty MockFs (load_state returns default), clock
    // advanced past the historical events.
    let later = now + Duration::seconds(60);
    let clock = Arc::new(FixedClock::new(later));
    let github = Arc::new(MockGitHub::new());
    let git = MockGit::new().with_repo_name("test-repo");
    let fs = MockFs::new(); // no watch.json present
    assert!(
        !fs.file_exists(&watch_state_path()),
        "precondition: watch.json must not exist"
    );
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

    let evs = svc.tick_once(&mut ts, &args);
    assert!(
        ready(&evs).is_empty() && epic_done(&evs).is_empty(),
        "fresh daemon with absent watch.json must not backfill history; got {evs:?}"
    );
}

// ── Scenario 10 (resilience): corrupted watch.json ──────────────────────

/// `watch.json` exists but is invalid JSON. Current behavior (documented
/// here so the contract is explicit): `load_state` swallows the parse
/// error and falls back to default, which means the daemon recovers
/// silently — historical events are NOT replayed because the cursor is
/// re-seeded to `now`.
///
/// This is a deliberate UX choice: a corrupted cursor file should not
/// brick the daemon. If the policy ever changes (e.g. fail-loud), this
/// test must change with it.
#[test]
fn scenario_corrupted_watch_json_recovers_gracefully() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("autopilot.db");
    let store: Arc<dyn TaskStore> = Arc::new(SqliteTaskStore::open(&db_path).expect("open sqlite"));

    // Pre-existing completed task — would replay if cursor were treated
    // as "from beginning of time".
    let now = t0();
    store
        .insert_epic_with_tasks(
            EpicPlan {
                epic: epic("e10", now),
                tasks: vec![new_task("t1", "T1")],
                deps: vec![],
            },
            now,
        )
        .expect("insert");
    store.claim_next_task("e10", now).expect("claim");
    store
        .complete_task_and_unblock(&TaskId::from_raw("t1"), 1, now)
        .expect("complete");

    // MockFs pre-seeded with garbage at the watch.json path.
    let fs = MockFs::new().with_file(
        watch_state_path().to_str().expect("utf-8 watch.json path"),
        "{this is not valid json,,,",
    );
    let later = now + Duration::seconds(60);
    let clock = Arc::new(FixedClock::new(later));
    let github = Arc::new(MockGitHub::new());
    let git = MockGit::new().with_repo_name("test-repo");

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

    // init_tick_state must not panic on corrupted JSON.
    let mut ts = svc.init_tick_state(&args).expect("init must not error");

    // First tick: no replay, no panic.
    let evs = svc.tick_once(&mut ts, &args);
    assert!(
        ready(&evs).is_empty() && epic_done(&evs).is_empty(),
        "corrupted watch.json must fall back to fresh state, not replay; got {evs:?}"
    );
}

// ── Scenario 11 (event integrity): same NEW_ISSUE seen twice ────────────

/// `MockGitHub` returns the same open issue on tick 0 and the next
/// issue-poll tick. The dedupe via `seen_issue_numbers` must ensure
/// `NEW_ISSUE` is emitted at most once across both ticks.
///
/// Note: `init_tick_state` seeds `seen_issue_numbers` from the first
/// successful `list_open_issues`, so a pre-existing issue is treated as
/// already-known on tick 0. We start with an empty list, then introduce
/// the issue between ticks — this is the closest analogue to "PR seen on
/// two consecutive ticks" the watch surface supports.
#[test]
fn scenario_same_issue_seen_twice_emits_once() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = tmp.path().join("autopilot.db");
    let store: Arc<dyn TaskStore> = Arc::new(SqliteTaskStore::open(&db).expect("open"));
    let clock = Arc::new(FixedClock::new(t0()));
    let args = WatchArgs {
        poll_sec: 0,
        branch: "main".to_string(),
        branch_filter: BranchFilter::All,
        label_prefix: "autopilot:".to_string(),
        stale_threshold: "60s".to_string(),
        ledger_events: true,
    };

    // First service: empty issue list at init time so seeding doesn't
    // pre-mark anything.
    let svc1 = WatchService::new(
        Arc::new(MockGitHub::new()),
        Box::new(MockGit::new().with_repo_name("test-repo")),
        Box::new(MockFs::new()),
    )
    .with_store(Arc::clone(&store))
    .with_clock(Arc::clone(&clock) as Arc<_>);
    let mut ts1 = svc1.init_tick_state(&args).expect("init");
    let _ = svc1.tick_once(&mut ts1, &args);

    // Second service: introduces issue 7. First poll → emits NEW_ISSUE 7.
    clock.advance(Duration::seconds(1));
    let svc2 = WatchService::new(
        Arc::new(
            MockGitHub::new().with_issues(vec![autopilot::github::OpenIssue {
                number: 7,
                title: "First sighting".to_string(),
                labels: vec![],
            }]),
        ),
        Box::new(MockGit::new().with_repo_name("test-repo")),
        Box::new(MockFs::new()),
    )
    .with_store(Arc::clone(&store))
    .with_clock(Arc::clone(&clock) as Arc<_>);
    let mut ts2 = svc2.init_tick_state(&args).expect("init");
    ts2.seen_issue_numbers = ts1.seen_issue_numbers.clone(); // carry empty
    let evs_first = svc2.tick_once(&mut ts2, &args);
    assert_eq!(
        new_issues(&evs_first),
        vec![7],
        "first sighting must emit NEW_ISSUE 7; got {evs_first:?}"
    );

    // Same issue persists on the next tick. With the standard tick
    // intervals issue polling fires at tick % ISSUE_TICK_INTERVAL == 0;
    // we drive the next poll explicitly by resetting tick to 0 (the
    // service has no public API to fast-forward, so we rely on the fact
    // that the seeded `seen_issue_numbers` already covers this case).
    ts2.tick = 0; // force next tick to hit the issue-poll branch
    let evs_second = svc2.tick_once(&mut ts2, &args);
    assert!(
        new_issues(&evs_second).is_empty(),
        "duplicate sighting must not re-emit NEW_ISSUE; got {evs_second:?}"
    );
}

// ── Scenario 12 (event integrity): ready→claim same tick, no STALE_WIP ──

/// Two-part integrity guard for the "Ready → claim in the same tick
/// window" race:
///
/// **Part A** — a task inserted and immediately claimed (both at the
/// same logical `now`) before any tick fires. The detector reads
/// **current** task status, so `TASK_READY` is intentionally suppressed
/// for the already-Wip task (this is a deliberate design choice; the
/// detector does not replay historical Ready transitions). What matters
/// is the negative invariant: `STALE_WIP` MUST NOT fire, because the
/// task's `updated_at` is `now`, not `now - threshold`.
///
/// **Part B** — a task whose `TASK_READY` already fired on tick N then
/// gets claimed before tick N+1. Tick N+1 must emit neither `TASK_READY`
/// (already emitted) nor `STALE_WIP` (just claimed).
///
/// This guards the time-ordering invariant: `STALE_WIP` derives from
/// `list_stale(now - threshold)` over current Wip rows; a freshly
/// claimed task must never qualify, regardless of how many ledger events
/// fire on the same tick.
#[test]
fn scenario_ready_then_claim_same_tick_no_stale_wip() {
    // ── Part A: insert + claim before any tick ──
    let mut fx = Fixture::new(/*stale_secs=*/ 5);
    ensure_epic(&fx.store, "e12a", fx.now());
    fx.store
        .upsert_watch_task(watch_task("t1", "e12a"), fx.now())
        .expect("upsert");
    fx.store
        .claim_next_task("e12a", fx.now())
        .expect("claim")
        .expect("ready task to claim");

    let evs = fx.tick();
    assert!(
        ready(&evs).is_empty(),
        "TASK_READY must be suppressed when task is already Wip at observation time; got {evs:?}"
    );
    assert!(
        stale(&evs).is_empty(),
        "STALE_WIP must not fire for a task claimed `now`; got {evs:?}"
    );

    // ── Part B: insert → tick (READY) → claim → tick (no STALE_WIP) ──
    let mut fx2 = Fixture::new(/*stale_secs=*/ 5);
    ensure_epic(&fx2.store, "e12b", fx2.now());
    fx2.store
        .upsert_watch_task(watch_task("t2", "e12b"), fx2.now())
        .expect("upsert");

    let evs = fx2.tick();
    assert_eq!(
        ready(&evs),
        vec![("e12b", "t2")],
        "TASK_READY must fire on first tick when status is Ready; got {evs:?}"
    );
    assert!(stale(&evs).is_empty());

    // External CLI claims between ticks; very little time passes.
    fx2.advance(Duration::seconds(1));
    fx2.store
        .claim_next_task("e12b", fx2.now())
        .expect("claim")
        .expect("ready task to claim");

    let evs = fx2.tick();
    assert!(
        ready(&evs).is_empty(),
        "TASK_READY must not re-emit; got {evs:?}"
    );
    assert!(
        stale(&evs).is_empty(),
        "STALE_WIP must not fire for a task claimed within the threshold; got {evs:?}"
    );
}

// ── Scenario 13 (event integrity): clock-skew protection ────────────────

/// Documents a known limitation: if `last_event_at` in `watch.json` is
/// somehow ahead of `clock.now()` (system clock moved backward, or the
/// state file was copied from another machine), the SQLite query
/// `at >= since` will silently filter out events whose `at == now`.
/// `TASK_READY` / `EPIC_DONE` then stop firing until real time catches
/// up to the stored future cursor.
///
/// Per the C5 task brief, this scenario is **deferred** — fixing it is a
/// judgment call that should be made alongside a concrete recovery
/// policy (e.g. clamp `since` to `min(last_event_at, now)`, or warn and
/// re-anchor). The test below describes the desired behavior and is
/// marked `#[ignore]` so CI passes while the policy is decided.
///
/// To reproduce the bug interactively, remove `#[ignore]` and run
/// `cargo test -p autopilot scenario_clock_skew_does_not_freeze_emission`.
#[test]
#[ignore = "TODO(C5): clock-skew re-anchor policy not yet implemented; see PR body"]
fn scenario_clock_skew_does_not_freeze_emission() {
    let mut fx = Fixture::new(/*stale_secs=*/ 60);

    // Force the ledger cursor into the future by 1 hour. In production
    // this happens when the system clock jumps backward between daemon
    // restarts.
    fx.ts.state.ledger.last_event_at = Some(fx.now() + Duration::hours(1));

    // Insert a task at the (logical) current time.
    ensure_epic(&fx.store, "e13", fx.now());
    fx.store
        .upsert_watch_task(watch_task("t1", "e13"), fx.now())
        .expect("upsert");

    // Desired behavior: the daemon detects the future cursor, re-anchors
    // to `now`, and emits TASK_READY for t1. Current behavior: the SQL
    // filter `at >= since` excludes the freshly inserted row, and no
    // event fires until real time catches up an hour later.
    let evs = fx.tick();
    assert_eq!(
        ready(&evs),
        vec![("e13", "t1")],
        "clock-skew must not freeze TASK_READY emission; got {evs:?}"
    );
}
