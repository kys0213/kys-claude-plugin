pub mod ci;
pub mod issues;
pub mod ledger;
pub mod push;

use crate::cmd::task::parse_duration_seconds;
use crate::fs::FsOps;
use crate::git::GitOps;
use crate::github::GitHub;
use crate::ports::clock::{Clock, StdClock};
use crate::ports::task_store::TaskStore;
use anyhow::{Context, Result};
use ci::BranchFilter;
use clap::Args;
use ledger::LedgerState;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// ── Watch Events ──

/// Filtered watch event emitted to stdout for Monitor consumption.
#[derive(Debug, Clone)]
pub enum WatchEvent {
    MainUpdated {
        before: String,
        after: String,
        count: u64,
    },
    CiFailure {
        run_id: u64,
        workflow: String,
        branch: String,
    },
    CiSuccess {
        run_id: u64,
        workflow: String,
        branch: String,
    },
    NewIssue {
        number: u64,
        title: String,
    },
    /// Ledger: a task is now Ready with no unmet deps.
    TaskReady {
        epic: String,
        task_id: String,
    },
    /// Ledger: every task in `epic` is Done.
    EpicDone {
        epic: String,
        total: u64,
    },
    /// Ledger: Wip tasks past the stale threshold (one event per epic).
    StaleWip {
        epic: String,
        candidates: Vec<String>,
    },
}

impl fmt::Display for WatchEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WatchEvent::MainUpdated {
                before,
                after,
                count,
            } => write!(
                f,
                "MAIN_UPDATED before={before} after={after} count={count}"
            ),
            WatchEvent::CiFailure {
                run_id,
                workflow,
                branch,
            } => write!(
                f,
                "CI_FAILURE run_id={run_id} workflow={workflow} branch={branch}"
            ),
            WatchEvent::CiSuccess {
                run_id,
                workflow,
                branch,
            } => write!(
                f,
                "CI_SUCCESS run_id={run_id} workflow={workflow} branch={branch}"
            ),
            WatchEvent::NewIssue { number, title } => {
                write!(f, "NEW_ISSUE number={number} title={title}")
            }
            WatchEvent::TaskReady { epic, task_id } => {
                write!(f, "TASK_READY epic={epic} task_id={task_id}")
            }
            WatchEvent::EpicDone { epic, total } => {
                write!(f, "EPIC_DONE epic={epic} total={total}")
            }
            WatchEvent::StaleWip { epic, candidates } => {
                let json = serde_json::to_string(candidates).unwrap_or_else(|_| "[]".to_string());
                write!(f, "STALE_WIP candidates={json} epic={epic}")
            }
        }
    }
}

// ── Persisted State ──

#[derive(Serialize, Deserialize, Default)]
pub struct WatchState {
    #[serde(default)]
    pub last_push_sha: String,
    #[serde(default)]
    pub seen_run_ids: Vec<u64>,
    #[serde(default)]
    pub seen_issue_numbers: Vec<u64>,
    #[serde(default)]
    pub ledger: LedgerState,
}

// ── CLI ──

#[derive(Args)]
pub struct WatchArgs {
    /// Base poll interval in seconds (push detection runs every tick)
    #[arg(long, default_value = "5")]
    pub poll_sec: u64,
    /// Branch to watch for pushes
    #[arg(long, default_value = "main")]
    pub branch: String,
    /// Branch filter mode for CI events
    #[arg(long, value_enum, default_value = "autopilot")]
    pub branch_filter: BranchFilter,
    /// Label prefix for issue filtering
    #[arg(long, default_value = "autopilot:")]
    pub label_prefix: String,
    /// Stale threshold for STALE_WIP detection (Go-style duration: `30s`,
    /// `5m`, `1h`, `1d`). Wip tasks with `updated_at` older than `now -
    /// threshold` are reported once per tick.
    #[arg(long, default_value = "1h")]
    pub stale_threshold: String,
    /// Emit ledger events (TASK_READY / EPIC_DONE / STALE_WIP) on each
    /// tick by polling the SQLite events table. Disable with
    /// `--no-ledger-events` for back-compat with pre-ledger Monitor
    /// dispatchers.
    #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
    pub ledger_events: bool,
}

// ── Service ──

/// Tick multipliers for different detectors.
const CI_TICK_INTERVAL: u64 = 6; // every 6 ticks (30s at 5s base)
const ISSUE_TICK_INTERVAL: u64 = 12; // every 12 ticks (60s at 5s base)
const STATE_SAVE_INTERVAL: u64 = 60; // every 60 ticks (5min at 5s base)

/// Per-tick mutable state for [`WatchService::tick_once`]. Held across
/// ticks (and persisted via `save_state` periodically) so the daemon
/// remembers cursors / dedupe sets between iterations.
///
/// Exposed publicly so blackbox tests in `tests/watch_scenarios.rs` can
/// drive the loop one tick at a time without `thread::sleep`.
pub struct TickState {
    pub stale_threshold_secs: i64,
    pub default_branch: String,
    pub last_sha: String,
    pub seen_run_ids: HashSet<u64>,
    pub seen_issue_numbers: HashSet<u64>,
    pub state: WatchState,
    pub tick: u64,
}

pub struct WatchService {
    github: Arc<dyn GitHub>,
    git: Box<dyn GitOps>,
    fs: Box<dyn FsOps>,
    /// Optional ledger store. `None` means ledger emission is unavailable
    /// (e.g. SQLite open failed); the loop will still emit GitHub events.
    store: Option<Arc<dyn TaskStore>>,
    clock: Arc<dyn Clock>,
}

impl WatchService {
    pub fn new(github: Arc<dyn GitHub>, git: Box<dyn GitOps>, fs: Box<dyn FsOps>) -> Self {
        Self {
            github,
            git,
            fs,
            store: None,
            clock: Arc::new(StdClock),
        }
    }

    /// Attaches a ledger store so this service can emit `TASK_READY`,
    /// `EPIC_DONE`, and `STALE_WIP` events alongside the existing GitHub
    /// events.
    pub fn with_store(mut self, store: Arc<dyn TaskStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// Replaces the default `StdClock`. Used by tests that need
    /// deterministic timestamps via `FixedClock`.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    pub fn run(&self, args: &WatchArgs) -> Result<i32> {
        let mut ts = match self.init_tick_state(args) {
            Ok(ts) => ts,
            Err(code) => return Ok(code),
        };

        loop {
            for event in self.tick_once(&mut ts, args) {
                println!("{event}");
            }
            thread::sleep(Duration::from_secs(args.poll_sec));
        }
    }

    /// Builds a fresh [`TickState`] from persisted state + initial seeding.
    /// Mirrors what `run` does before its first iteration. On invalid args
    /// returns the exit code so `run` can return it cleanly.
    pub fn init_tick_state(&self, args: &WatchArgs) -> std::result::Result<TickState, i32> {
        let stale_threshold_secs = match parse_duration_seconds(&args.stale_threshold) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("invalid --stale-threshold: {e}");
                return Err(2);
            }
        };
        let default_branch = self
            .github
            .default_branch()
            .unwrap_or(args.branch.to_string());
        let mut state = self.load_state();
        // Seed ledger cursor on first run so we don't backfill historical events.
        state.ledger.seed(self.clock.now());

        let last_sha = if state.last_push_sha.is_empty() {
            // Initialize with current remote SHA
            let _ = self.git.fetch_remote("origin", &args.branch);
            let refname = format!("origin/{}", args.branch);
            self.git.rev_parse_ref(&refname).unwrap_or_default()
        } else {
            state.last_push_sha.clone()
        };

        let mut seen_run_ids: HashSet<u64> = state.seen_run_ids.iter().copied().collect();
        let mut seen_issue_numbers: HashSet<u64> =
            state.seen_issue_numbers.iter().copied().collect();

        // Seed seen sets on first run to avoid emitting all existing items
        if seen_run_ids.is_empty() {
            if let Ok(runs) = self.github.list_completed_runs(20) {
                seen_run_ids = runs.iter().map(|r| r.id).collect();
            }
        }
        if seen_issue_numbers.is_empty() {
            if let Ok(issues) = self.github.list_open_issues(50) {
                seen_issue_numbers = issues.iter().map(|i| i.number).collect();
            }
        }

        Ok(TickState {
            stale_threshold_secs,
            default_branch,
            last_sha,
            seen_run_ids,
            seen_issue_numbers,
            state,
            tick: 0,
        })
    }

    /// Runs one iteration of the watch loop and returns the events that
    /// would have been printed. `ts` is mutated for the next tick.
    ///
    /// Pure with respect to time (`self.clock`) and external systems
    /// (`self.github`, `self.store`, `self.fs`), so blackbox tests drive
    /// it without sleeping.
    pub fn tick_once(&self, ts: &mut TickState, args: &WatchArgs) -> Vec<WatchEvent> {
        let mut out: Vec<WatchEvent> = Vec::new();

        // Push: every tick
        if let Some(event) = push::detect_push(&*self.git, "origin", &args.branch, &ts.last_sha) {
            if let WatchEvent::MainUpdated { ref after, .. } = event {
                ts.last_sha = after.clone();
            }
            out.push(event);
        }

        // CI: every CI_TICK_INTERVAL ticks
        if ts.tick.is_multiple_of(CI_TICK_INTERVAL) {
            if let Ok(runs) = self.github.list_completed_runs(20) {
                let events = ci::detect_ci(
                    &runs,
                    &ts.seen_run_ids,
                    &ts.default_branch,
                    &args.branch_filter,
                );
                ts.seen_run_ids = runs.iter().map(|r| r.id).collect();
                out.extend(events);
            }
        }

        // Issues: every ISSUE_TICK_INTERVAL ticks
        if ts.tick.is_multiple_of(ISSUE_TICK_INTERVAL) {
            if let Ok(issues) = self.github.list_open_issues(50) {
                let events =
                    issues::detect_issues(&issues, &ts.seen_issue_numbers, &args.label_prefix);
                ts.seen_issue_numbers = issues.iter().map(|i| i.number).collect();
                out.extend(events);
            }
        }

        // Ledger: every tick (when enabled and a store is attached)
        if args.ledger_events {
            if let Some(store) = self.store.as_ref() {
                let events = ledger::detect_ledger_events(
                    store.as_ref(),
                    &mut ts.state.ledger,
                    self.clock.now(),
                    ts.stale_threshold_secs,
                );
                out.extend(events);
            }
        }

        // Save state periodically
        if ts.tick > 0 && ts.tick.is_multiple_of(STATE_SAVE_INTERVAL) {
            ts.state.last_push_sha = ts.last_sha.clone();
            ts.state.seen_run_ids = ts.seen_run_ids.iter().copied().collect();
            ts.state.seen_issue_numbers = ts.seen_issue_numbers.iter().copied().collect();
            let _ = self.save_state(&ts.state);
        }

        ts.tick += 1;
        out
    }

    fn state_path(&self) -> std::path::PathBuf {
        let repo = self.git.repo_name().unwrap_or("unknown".to_string());
        std::path::PathBuf::from(format!("/tmp/autopilot-{repo}/state/watch.json"))
    }

    fn load_state(&self) -> WatchState {
        let path = self.state_path();
        self.fs
            .read_file(&path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    fn save_state(&self, state: &WatchState) -> Result<()> {
        let path = self.state_path();
        let content = serde_json::to_string_pretty(state).context("failed to serialize state")?;
        self.fs.write_file(&path, &content)
    }
}
