//! PR-creation duplicate guard, ported from git-utils `core/pr-guard.ts`.
//!
//! Before a `gh pr create`, checks whether the current branch already has an
//! open PR and blocks if so. On a `gh` failure it fails open (safe mode).

use regex::Regex;
use std::sync::LazyLock;

use crate::git::types::{PrGuardInput, PrGuardOutput};

use super::github::GitHubService;

// Matches the standard `gh pr create` form Claude's Bash tool emits; variants
// like `gh --repo owner/repo pr create` are intentionally not matched.
static GH_PR_CREATE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bgh\s+pr\s+create\b").unwrap());

/// The PR-creation guard contract.
pub trait PrGuardService {
    fn check(&self, input: &PrGuardInput) -> PrGuardOutput;
}

/// Real guard backed by an injected [`GitHubService`].
pub struct RealPrGuardService<'a> {
    github: &'a dyn GitHubService,
}

impl<'a> RealPrGuardService<'a> {
    pub fn new(github: &'a dyn GitHubService) -> Self {
        Self { github }
    }
}

fn allow(reason: Option<&str>) -> PrGuardOutput {
    PrGuardOutput {
        allowed: true,
        reason: reason.map(str::to_string),
        pr_number: None,
    }
}

impl PrGuardService for RealPrGuardService<'_> {
    fn check(&self, input: &PrGuardInput) -> PrGuardOutput {
        // When a tool command is provided, only guard `gh pr create`.
        if let Some(cmd) = &input.tool_command {
            if !GH_PR_CREATE_PATTERN.is_match(cmd) {
                return allow(Some("not a gh pr create command"));
            }
        }

        // A failure to query is treated as "allow" (safe mode).
        let pr_number = match self.github.detect_current_pr_number() {
            Ok(n) => n,
            Err(_) => return allow(Some("could not check existing PR (safe mode)")),
        };

        match pr_number {
            None => allow(None),
            Some(n) => PrGuardOutput {
                allowed: false,
                pr_number: Some(n),
                reason: Some(format!(
                    "[PR Guard] 현재 브랜치에 열린 PR이 있습니다.\n\
                     \n\
                     기존 PR:\n  \
                     번호: #{n}\n\
                     \n\
                     새로운 PR을 생성하려면:\n  \
                     1. 기존 PR을 머지하거나 닫기\n  \
                     2. 기본 브랜치로 동기화\n  \
                     3. 새 브랜치 생성 후 다시 시도\n\
                     \n\
                     기존 PR에 변경사항을 추가하려면:\n  \
                     - git push만 실행하세요"
                )),
            },
        }
    }
}
