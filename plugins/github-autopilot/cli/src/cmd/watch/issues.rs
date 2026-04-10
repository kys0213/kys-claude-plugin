use crate::cmd::watch::WatchEvent;
use crate::github::OpenIssue;
use std::collections::HashSet;

/// Detect new issues without autopilot labels.
///
/// Returns events for issues not in `seen_numbers` and lacking the label prefix.
pub fn detect_issues(
    issues: &[OpenIssue],
    seen_numbers: &HashSet<u64>,
    label_prefix: &str,
) -> Vec<WatchEvent> {
    issues
        .iter()
        .filter(|i| !seen_numbers.contains(&i.number))
        .filter(|i| !i.labels.iter().any(|l| l.starts_with(label_prefix)))
        .map(|i| WatchEvent::NewIssue {
            number: i.number,
            title: i.title.clone(),
        })
        .collect()
}
