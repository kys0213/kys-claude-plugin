pub mod ci;
pub mod issues;
pub mod push;

use crate::fs::FsOps;
use crate::git::GitOps;
use crate::github::GitHub;
use anyhow::{Context, Result};
use ci::BranchFilter;
use clap::Args;
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
        }
    }
}

// ── Persisted State ──

#[derive(Serialize, Deserialize, Default)]
struct WatchState {
    #[serde(default)]
    last_push_sha: String,
    #[serde(default)]
    seen_run_ids: Vec<u64>,
    #[serde(default)]
    seen_issue_numbers: Vec<u64>,
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
}

// ── Service ──

/// Tick multipliers for different detectors.
const CI_TICK_INTERVAL: u64 = 6; // every 6 ticks (30s at 5s base)
const ISSUE_TICK_INTERVAL: u64 = 12; // every 12 ticks (60s at 5s base)
const STATE_SAVE_INTERVAL: u64 = 60; // every 60 ticks (5min at 5s base)

pub struct WatchService {
    github: Arc<dyn GitHub>,
    git: Box<dyn GitOps>,
    fs: Box<dyn FsOps>,
}

impl WatchService {
    pub fn new(github: Arc<dyn GitHub>, git: Box<dyn GitOps>, fs: Box<dyn FsOps>) -> Self {
        Self { github, git, fs }
    }

    pub fn run(
        &self,
        branch: &str,
        branch_filter: &BranchFilter,
        label_prefix: &str,
        poll_sec: u64,
    ) -> Result<i32> {
        let default_branch = self.github.default_branch().unwrap_or(branch.to_string());
        let state = self.load_state();

        let mut last_sha = if state.last_push_sha.is_empty() {
            // Initialize with current remote SHA
            let _ = self.git.fetch_remote("origin", branch);
            let refname = format!("origin/{branch}");
            self.git.rev_parse_ref(&refname).unwrap_or_default()
        } else {
            state.last_push_sha
        };

        let mut seen_run_ids: HashSet<u64> = state.seen_run_ids.into_iter().collect();
        let mut seen_issue_numbers: HashSet<u64> = state.seen_issue_numbers.into_iter().collect();

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

        let mut tick: u64 = 0;

        loop {
            // Push: every tick
            if let Some(event) = push::detect_push(&*self.git, "origin", branch, &last_sha) {
                if let WatchEvent::MainUpdated { ref after, .. } = event {
                    last_sha = after.clone();
                }
                println!("{event}");
            }

            // CI: every CI_TICK_INTERVAL ticks
            if tick.is_multiple_of(CI_TICK_INTERVAL) {
                if let Ok(runs) = self.github.list_completed_runs(20) {
                    let events =
                        ci::detect_ci(&runs, &seen_run_ids, &default_branch, branch_filter);
                    seen_run_ids = runs.iter().map(|r| r.id).collect();
                    for event in events {
                        println!("{event}");
                    }
                }
            }

            // Issues: every ISSUE_TICK_INTERVAL ticks
            if tick.is_multiple_of(ISSUE_TICK_INTERVAL) {
                if let Ok(issues) = self.github.list_open_issues(50) {
                    let events = issues::detect_issues(&issues, &seen_issue_numbers, label_prefix);
                    seen_issue_numbers = issues.iter().map(|i| i.number).collect();
                    for event in events {
                        println!("{event}");
                    }
                }
            }

            // Save state periodically
            if tick > 0 && tick.is_multiple_of(STATE_SAVE_INTERVAL) {
                let _ = self.save_state(&WatchState {
                    last_push_sha: last_sha.clone(),
                    seen_run_ids: seen_run_ids.iter().copied().collect(),
                    seen_issue_numbers: seen_issue_numbers.iter().copied().collect(),
                });
            }

            tick += 1;
            thread::sleep(Duration::from_secs(poll_sec));
        }
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
