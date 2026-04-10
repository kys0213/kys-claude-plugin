use crate::github::{EventPayload, EventType, EventsResponse};
use clap::ValueEnum;
use std::collections::HashSet;
use std::fmt;

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

/// Branch filter mode for CI events.
#[derive(Debug, Clone, ValueEnum)]
pub enum BranchMode {
    /// Only default branch + autopilot branches (feature/issue-*, draft/issue-*)
    Autopilot,
    /// All branches
    All,
}

/// Filter configuration for event detection.
#[derive(Debug, Clone)]
pub struct EventFilter {
    pub default_branch: String,
    pub branch_mode: BranchMode,
}

fn is_autopilot_branch(branch: &str, default_branch: &str) -> bool {
    branch == default_branch
        || branch.starts_with("feature/issue-")
        || branch.starts_with("draft/issue-")
}

/// Pure function: convert raw EventsResponse into filtered WatchEvents.
///
/// Skips events whose id is in `seen_ids`. Returns new event IDs to add to the seen set.
pub fn detect_events(
    response: &EventsResponse,
    filter: &EventFilter,
    seen_ids: &HashSet<String>,
) -> Vec<WatchEvent> {
    response
        .events
        .iter()
        .filter(|e| !seen_ids.contains(&e.id))
        .filter_map(|e| match (&e.event_type, &e.payload) {
            (
                EventType::Push,
                EventPayload::Push {
                    branch,
                    before,
                    after,
                    size,
                },
            ) => {
                if branch == &filter.default_branch {
                    Some(WatchEvent::MainUpdated {
                        before: before.clone(),
                        after: after.clone(),
                        count: *size,
                    })
                } else {
                    None
                }
            }
            (
                EventType::WorkflowRun,
                EventPayload::WorkflowRun {
                    run_id,
                    name,
                    branch,
                    conclusion,
                },
            ) => {
                let pass_filter = match &filter.branch_mode {
                    BranchMode::All => true,
                    BranchMode::Autopilot => is_autopilot_branch(branch, &filter.default_branch),
                };
                if !pass_filter {
                    return None;
                }
                match conclusion.as_str() {
                    "failure" => Some(WatchEvent::CiFailure {
                        run_id: *run_id,
                        workflow: name.clone(),
                        branch: branch.clone(),
                    }),
                    "success" => Some(WatchEvent::CiSuccess {
                        run_id: *run_id,
                        workflow: name.clone(),
                        branch: branch.clone(),
                    }),
                    _ => None,
                }
            }
            (
                EventType::Issues,
                EventPayload::Issues {
                    action,
                    number,
                    title,
                },
            ) => {
                if action == "opened" {
                    Some(WatchEvent::NewIssue {
                        number: *number,
                        title: title.clone(),
                    })
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect()
}

/// Collect all event IDs from a response into a set.
pub fn collect_ids(response: &EventsResponse) -> HashSet<String> {
    response.events.iter().map(|e| e.id.clone()).collect()
}
