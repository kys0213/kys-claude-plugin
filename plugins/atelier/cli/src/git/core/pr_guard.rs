//! PR duplicate-creation guard — port of `git-utils/src/core/pr-guard.ts`.
//! Blocks `gh pr create` when the current branch already has an open PR.
//! Falls back to "allow" (safe mode) when the gh lookup errors. Takes a
//! `GitHubService` by injection for mockability.

use crate::git::core::github::GitHubService;
use crate::git::types::{PrGuardInput, PrGuardOutput};
use regex::Regex;
use std::sync::LazyLock;

static GH_PR_CREATE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bgh\s+pr\s+create\b").unwrap());

pub trait PrGuardService {
    fn check(&self, input: &PrGuardInput) -> PrGuardOutput;
}

pub struct RealPrGuardService<'a> {
    github: &'a dyn GitHubService,
}

/// Constructs a PR guard over the given GitHub service.
pub fn create_pr_guard_service(github: &dyn GitHubService) -> RealPrGuardService<'_> {
    RealPrGuardService { github }
}

impl PrGuardService for RealPrGuardService<'_> {
    fn check(&self, input: &PrGuardInput) -> PrGuardOutput {
        let pass = |reason: Option<&str>| PrGuardOutput {
            allowed: true,
            reason: reason.map(|s| s.to_string()),
            pr_number: None,
        };

        // If toolCommand is provided, only act on `gh pr create`.
        if let Some(cmd) = &input.tool_command {
            if !GH_PR_CREATE_PATTERN.is_match(cmd) {
                return pass(Some("not a gh pr create command"));
            }
        }

        let pr_number = match self.github.detect_current_pr_number() {
            Ok(n) => n,
            Err(_) => return pass(Some("could not check existing PR (safe mode)")),
        };

        let pr_number = match pr_number {
            Some(n) => n,
            None => return pass(None),
        };

        let reason = [
            "[PR Guard] 현재 브랜치에 열린 PR이 있습니다.".to_string(),
            String::new(),
            "기존 PR:".to_string(),
            format!("  번호: #{pr_number}"),
            String::new(),
            "새로운 PR을 생성하려면:".to_string(),
            "  1. 기존 PR을 머지하거나 닫기".to_string(),
            "  2. 기본 브랜치로 동기화".to_string(),
            "  3. 새 브랜치 생성 후 다시 시도".to_string(),
            String::new(),
            "기존 PR에 변경사항을 추가하려면:".to_string(),
            "  - git push만 실행하세요".to_string(),
        ]
        .join("\n");

        PrGuardOutput {
            allowed: false,
            reason: Some(reason),
            pr_number: Some(pr_number),
        }
    }
}
