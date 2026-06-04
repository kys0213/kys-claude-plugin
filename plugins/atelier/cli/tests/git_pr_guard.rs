//! Mock-based port of git-utils `tests/core/pr-guard.test.ts`.

use anyhow::{bail, Result};

use atelier::git::core::github::GitHubService;
use atelier::git::core::pr_guard::{PrGuardService, RealPrGuardService};
use atelier::git::types::{PrGuardInput, ReviewsOutput};

/// What the mock's `detect_current_pr_number` should return.
enum Detect {
    Open(i64),
    NoPr,
    Err,
}

struct MockGitHub {
    detect: Detect,
}

impl GitHubService for MockGitHub {
    fn is_authenticated(&self) -> Result<bool> {
        Ok(true)
    }
    fn create_pr(&self, _base: &str, _title: &str, _body: &str) -> Result<String> {
        Ok("https://github.com/org/repo/pull/1".to_string())
    }
    fn get_review_threads(&self, _pr_number: i64) -> Result<ReviewsOutput> {
        unreachable!("pr-guard never fetches review threads")
    }
    fn detect_current_pr_number(&self) -> Result<Option<i64>> {
        match self.detect {
            Detect::Open(n) => Ok(Some(n)),
            Detect::NoPr => Ok(None),
            Detect::Err => bail!("network error"),
        }
    }
}

fn check(detect: Detect, tool_command: Option<&str>) -> atelier::git::types::PrGuardOutput {
    let gh = MockGitHub { detect };
    let guard = RealPrGuardService::new(&gh);
    guard.check(&PrGuardInput {
        tool_command: tool_command.map(str::to_string),
    })
}

// ---------- open PR present ----------

#[test]
fn blocks_with_pr_info() {
    let r = check(Detect::Open(123), None);
    assert!(!r.allowed);
    assert_eq!(r.pr_number, Some(123));
    let reason = r.reason.unwrap();
    assert!(reason.contains("#123"));
    assert!(reason.contains("열린 PR이 있습니다"));
}

#[test]
fn reason_includes_guidance() {
    let r = check(Detect::Open(42), None);
    let reason = r.reason.unwrap();
    assert!(reason.contains("기존 PR을 머지하거나 닫기"));
    assert!(reason.contains("git push만 실행하세요"));
}

// ---------- no open PR ----------

#[test]
fn allows_when_no_pr() {
    let r = check(Detect::NoPr, None);
    assert!(r.allowed);
    assert_eq!(r.pr_number, None);
}

#[test]
fn merged_pr_branch_not_blocked() {
    let r = check(Detect::NoPr, Some("gh pr create --title \"new feature\""));
    assert!(r.allowed);
    assert_eq!(r.pr_number, None);
}

// ---------- safe mode on gh failure ----------

#[test]
fn fails_open_on_detect_error() {
    let r = check(Detect::Err, None);
    assert!(r.allowed);
    assert!(r.reason.unwrap().contains("could not check"));
}

// ---------- toolCommand filtering ----------

#[test]
fn non_create_command_passes() {
    let r = check(Detect::Open(123), Some("gh pr view"));
    assert!(r.allowed);
    assert_eq!(r.reason.as_deref(), Some("not a gh pr create command"));
}

#[test]
fn create_command_runs_guard() {
    let r = check(Detect::Open(123), Some("gh pr create --title \"test\""));
    assert!(!r.allowed);
    assert_eq!(r.pr_number, Some(123));
}

#[test]
fn undefined_command_runs_guard() {
    let r = check(Detect::Open(123), None);
    assert!(!r.allowed);
}

#[test]
fn empty_command_passes() {
    let r = check(Detect::Open(123), Some(""));
    assert!(r.allowed);
    assert_eq!(r.reason.as_deref(), Some("not a gh pr create command"));
}
