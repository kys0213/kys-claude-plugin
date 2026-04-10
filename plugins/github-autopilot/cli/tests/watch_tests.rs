mod mock_github;

use autopilot::cmd::watch::events::{detect_events, BranchMode, EventFilter, WatchEvent};
use autopilot::github::GitHub;
use mock_github::{issues_event, push_event, workflow_run_event, MockGitHub};

fn autopilot_filter() -> EventFilter {
    EventFilter {
        default_branch: "main".to_string(),
        branch_mode: BranchMode::Autopilot,
    }
}

fn all_filter() -> EventFilter {
    EventFilter {
        default_branch: "main".to_string(),
        branch_mode: BranchMode::All,
    }
}

// ── PushEvent tests ──

#[test]
fn push_on_default_branch_emits_main_updated() {
    let gh = MockGitHub::new().with_events(vec![push_event("1", "main", "aaa", "bbb", 3)], "etag1");
    let resp = gh.fetch_events(None).unwrap().unwrap();
    let events = detect_events(&resp, &autopilot_filter(), "0");
    assert_eq!(events.len(), 1);
    match &events[0] {
        WatchEvent::MainUpdated {
            before,
            after,
            count,
        } => {
            assert_eq!(before, "aaa");
            assert_eq!(after, "bbb");
            assert_eq!(*count, 3);
        }
        other => panic!("expected MainUpdated, got {other}"),
    }
}

#[test]
fn push_on_feature_branch_ignored() {
    let gh = MockGitHub::new().with_events(
        vec![push_event("1", "feat/something", "aaa", "bbb", 1)],
        "etag1",
    );
    let resp = gh.fetch_events(None).unwrap().unwrap();
    let events = detect_events(&resp, &autopilot_filter(), "0");
    assert!(events.is_empty());
}

// ── WorkflowRunEvent tests ──

#[test]
fn workflow_failure_on_autopilot_branch() {
    let gh = MockGitHub::new().with_events(
        vec![workflow_run_event(
            "2",
            100,
            "CI",
            "feature/issue-42",
            "failure",
        )],
        "etag2",
    );
    let resp = gh.fetch_events(None).unwrap().unwrap();
    let events = detect_events(&resp, &autopilot_filter(), "0");
    assert_eq!(events.len(), 1);
    match &events[0] {
        WatchEvent::CiFailure {
            run_id,
            workflow,
            branch,
        } => {
            assert_eq!(*run_id, 100);
            assert_eq!(workflow, "CI");
            assert_eq!(branch, "feature/issue-42");
        }
        other => panic!("expected CiFailure, got {other}"),
    }
}

#[test]
fn workflow_success_on_autopilot_branch() {
    let gh = MockGitHub::new().with_events(
        vec![workflow_run_event(
            "3",
            200,
            "Build",
            "draft/issue-10",
            "success",
        )],
        "etag3",
    );
    let resp = gh.fetch_events(None).unwrap().unwrap();
    let events = detect_events(&resp, &autopilot_filter(), "0");
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], WatchEvent::CiSuccess { .. }));
}

#[test]
fn workflow_on_default_branch_detected() {
    let gh = MockGitHub::new().with_events(
        vec![workflow_run_event("4", 300, "CI", "main", "failure")],
        "etag4",
    );
    let resp = gh.fetch_events(None).unwrap().unwrap();
    let events = detect_events(&resp, &autopilot_filter(), "0");
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], WatchEvent::CiFailure { .. }));
}

#[test]
fn workflow_on_user_branch_filtered_in_autopilot_mode() {
    let gh = MockGitHub::new().with_events(
        vec![workflow_run_event(
            "5",
            400,
            "CI",
            "user/my-feature",
            "failure",
        )],
        "etag5",
    );
    let resp = gh.fetch_events(None).unwrap().unwrap();
    let events = detect_events(&resp, &autopilot_filter(), "0");
    assert!(events.is_empty());
}

#[test]
fn workflow_on_user_branch_allowed_in_all_mode() {
    let gh = MockGitHub::new().with_events(
        vec![workflow_run_event(
            "5",
            400,
            "CI",
            "user/my-feature",
            "failure",
        )],
        "etag5",
    );
    let resp = gh.fetch_events(None).unwrap().unwrap();
    let events = detect_events(&resp, &all_filter(), "0");
    assert_eq!(events.len(), 1);
}

// ── IssuesEvent tests ──

#[test]
fn new_issue_opened() {
    let gh = MockGitHub::new().with_events(
        vec![issues_event("6", "opened", 55, "Add OAuth support")],
        "etag6",
    );
    let resp = gh.fetch_events(None).unwrap().unwrap();
    let events = detect_events(&resp, &autopilot_filter(), "0");
    assert_eq!(events.len(), 1);
    match &events[0] {
        WatchEvent::NewIssue { number, title } => {
            assert_eq!(*number, 55);
            assert_eq!(title, "Add OAuth support");
        }
        other => panic!("expected NewIssue, got {other}"),
    }
}

#[test]
fn issue_closed_ignored() {
    let gh = MockGitHub::new().with_events(
        vec![issues_event("7", "closed", 55, "Add OAuth support")],
        "etag7",
    );
    let resp = gh.fetch_events(None).unwrap().unwrap();
    let events = detect_events(&resp, &autopilot_filter(), "0");
    assert!(events.is_empty());
}

// ── Filtering tests ──

#[test]
fn old_events_skipped() {
    let gh = MockGitHub::new().with_events(
        vec![
            push_event("10", "main", "a", "b", 1),
            push_event("20", "main", "b", "c", 1),
        ],
        "etag8",
    );
    let resp = gh.fetch_events(None).unwrap().unwrap();
    // last_seen_id = "10" → only event "20" should be returned
    let events = detect_events(&resp, &autopilot_filter(), "10");
    assert_eq!(events.len(), 1);
    match &events[0] {
        WatchEvent::MainUpdated { after, .. } => assert_eq!(after, "c"),
        other => panic!("expected MainUpdated, got {other}"),
    }
}

#[test]
fn empty_events_returns_empty() {
    let gh = MockGitHub::new().with_events(vec![], "etag9");
    let resp = gh.fetch_events(None).unwrap().unwrap();
    let events = detect_events(&resp, &autopilot_filter(), "0");
    assert!(events.is_empty());
}

// ── 304 Not Modified test ──

#[test]
fn fetch_events_304_returns_none() {
    let gh = MockGitHub::new().with_no_changes();
    let result = gh.fetch_events(Some("old-etag")).unwrap();
    assert!(result.is_none());
}

// ── Mixed events test ──

#[test]
fn mixed_events_all_detected() {
    let gh = MockGitHub::new().with_events(
        vec![
            push_event("1", "main", "a", "b", 2),
            workflow_run_event("2", 100, "CI", "feature/issue-1", "failure"),
            issues_event("3", "opened", 10, "New feature"),
            workflow_run_event("4", 200, "Build", "main", "success"),
        ],
        "etag-mix",
    );
    let resp = gh.fetch_events(None).unwrap().unwrap();
    let events = detect_events(&resp, &autopilot_filter(), "0");
    assert_eq!(events.len(), 4);
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
