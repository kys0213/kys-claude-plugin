mod mock_git;
mod mock_github;

use autopilot::cmd::watch::ci::{detect_ci, BranchFilter};
use autopilot::cmd::watch::issues::detect_issues;
use autopilot::cmd::watch::push::detect_push;
use autopilot::cmd::watch::WatchEvent;
use autopilot::github::{CompletedRun, OpenIssue};
use mock_git::MockGit;
use std::collections::HashSet;

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
