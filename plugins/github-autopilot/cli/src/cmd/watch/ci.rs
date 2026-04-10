use crate::cmd::watch::WatchEvent;
use crate::github::CompletedRun;
use std::collections::HashSet;

/// Branch filter mode for CI events.
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum BranchFilter {
    /// Only default branch + autopilot branches (feature/issue-*, draft/issue-*)
    Autopilot,
    /// All branches
    All,
}

fn is_autopilot_branch(branch: &str, default_branch: &str) -> bool {
    branch == default_branch
        || branch.starts_with("feature/issue-")
        || branch.starts_with("draft/issue-")
}

/// Detect new CI completions from a list of runs.
///
/// Returns events for runs not in `seen_ids`, filtered by branch mode.
pub fn detect_ci(
    runs: &[CompletedRun],
    seen_ids: &HashSet<u64>,
    default_branch: &str,
    filter: &BranchFilter,
) -> Vec<WatchEvent> {
    runs.iter()
        .filter(|r| !seen_ids.contains(&r.id))
        .filter(|r| match filter {
            BranchFilter::All => true,
            BranchFilter::Autopilot => is_autopilot_branch(&r.branch, default_branch),
        })
        .filter_map(|r| match r.conclusion.as_str() {
            "failure" => Some(WatchEvent::CiFailure {
                run_id: r.id,
                workflow: r.name.clone(),
                branch: r.branch.clone(),
            }),
            "success" => Some(WatchEvent::CiSuccess {
                run_id: r.id,
                workflow: r.name.clone(),
                branch: r.branch.clone(),
            }),
            _ => None,
        })
        .collect()
}
