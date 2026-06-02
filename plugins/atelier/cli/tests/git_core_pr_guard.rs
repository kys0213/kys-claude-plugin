//! Port of `git-utils/tests/core/pr-guard.test.ts` — mock-github unit tests.
#![allow(clippy::field_reassign_with_default)]

mod git_mocks;

use atelier::git::core::pr_guard::{create_pr_guard_service, PrGuardService};
use atelier::git::types::PrGuardInput;
use git_mocks::MockGitHub;

fn check(github: MockGitHub, input: PrGuardInput) -> atelier::git::types::PrGuardOutput {
    let guard = create_pr_guard_service(&github);
    guard.check(&input)
}

#[test]
fn open_pr_blocks_with_info() {
    let mut gh = MockGitHub::default();
    gh.detect_current_pr_number = Box::new(|| Ok(Some(123)));
    let out = check(gh, PrGuardInput::default());
    assert!(!out.allowed);
    assert_eq!(out.pr_number, Some(123));
    let reason = out.reason.unwrap();
    assert!(reason.contains("#123"));
    assert!(reason.contains("열린 PR이 있습니다"));
}

#[test]
fn open_pr_reason_has_guidance() {
    let mut gh = MockGitHub::default();
    gh.detect_current_pr_number = Box::new(|| Ok(Some(42)));
    let out = check(gh, PrGuardInput::default());
    let reason = out.reason.unwrap();
    assert!(reason.contains("기존 PR을 머지하거나 닫기"));
    assert!(reason.contains("git push만 실행하세요"));
}

#[test]
fn no_open_pr_allows() {
    let mut gh = MockGitHub::default();
    gh.detect_current_pr_number = Box::new(|| Ok(None));
    let out = check(gh, PrGuardInput::default());
    assert!(out.allowed);
    assert_eq!(out.pr_number, None);
}

#[test]
fn merged_pr_does_not_block() {
    let mut gh = MockGitHub::default();
    gh.detect_current_pr_number = Box::new(|| Ok(None));
    let out = check(
        gh,
        PrGuardInput {
            tool_command: Some("gh pr create --title \"new feature\"".to_string()),
        },
    );
    assert!(out.allowed);
    assert_eq!(out.pr_number, None);
}

#[test]
fn gh_failure_allows_safe_mode() {
    let mut gh = MockGitHub::default();
    gh.detect_current_pr_number = Box::new(|| Err("network error".to_string()));
    let out = check(gh, PrGuardInput::default());
    assert!(out.allowed);
    assert!(out.reason.unwrap().contains("could not check"));
}

#[test]
fn non_pr_create_command_passes() {
    let mut gh = MockGitHub::default();
    gh.detect_current_pr_number = Box::new(|| Ok(Some(123)));
    let out = check(
        gh,
        PrGuardInput {
            tool_command: Some("gh pr view".to_string()),
        },
    );
    assert!(out.allowed);
    assert_eq!(out.reason.as_deref(), Some("not a gh pr create command"));
}

#[test]
fn pr_create_command_runs_guard() {
    let mut gh = MockGitHub::default();
    gh.detect_current_pr_number = Box::new(|| Ok(Some(123)));
    let out = check(
        gh,
        PrGuardInput {
            tool_command: Some("gh pr create --title \"test\"".to_string()),
        },
    );
    assert!(!out.allowed);
    assert_eq!(out.pr_number, Some(123));
}

#[test]
fn no_command_runs_guard() {
    let mut gh = MockGitHub::default();
    gh.detect_current_pr_number = Box::new(|| Ok(Some(123)));
    let out = check(gh, PrGuardInput::default());
    assert!(!out.allowed);
}

#[test]
fn empty_command_passes() {
    let mut gh = MockGitHub::default();
    gh.detect_current_pr_number = Box::new(|| Ok(Some(123)));
    let out = check(
        gh,
        PrGuardInput {
            tool_command: Some(String::new()),
        },
    );
    assert!(out.allowed);
    assert_eq!(out.reason.as_deref(), Some("not a gh pr create command"));
}
