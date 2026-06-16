//! Shared test doubles for the git subsystem command tests. After the git CLI
//! was narrowed to its mechanical surface (guard/hook/reviews), the mocks only
//! need to cover the `GitService` reads the guard consumes and the
//! `GitHubService` calls reviews/pr-guard make.
//!
//! `#![allow(dead_code)]` because each test binary uses only a subset of the
//! mocks (Cargo compiles this module separately into every test crate).
//! `type_complexity` is allowed because the `Box<dyn Fn(...)>` override fields
//! are the whole point of the builder-style mocks.
#![allow(dead_code, clippy::type_complexity)]

use std::cell::RefCell;

use atelier::git::core::git::GitService;
use atelier::git::core::github::{GitHubService, ReviewThreadsResult};
use atelier::git::types::GitSpecialState;

type R<T> = Result<T, String>;

/// Mockable `GitService` — only the reads the branch guard consumes.
pub struct MockGit {
    pub is_inside_work_tree: Box<dyn Fn() -> bool>,
    pub get_current_branch: Box<dyn Fn() -> String>,
    pub detect_default_branch_readonly: Box<dyn Fn() -> R<String>>,
    /// `(rebase, merge)` flags for `get_special_state`; `current_branch` is
    /// derived from `get_current_branch`, mirroring `RealGitService` (#778).
    pub special_state_flags: Box<dyn Fn() -> (bool, bool)>,
}

impl Default for MockGit {
    fn default() -> Self {
        MockGit {
            is_inside_work_tree: Box::new(|| true),
            get_current_branch: Box::new(|| "main".to_string()),
            detect_default_branch_readonly: Box::new(|| Ok("main".to_string())),
            special_state_flags: Box::new(|| (false, false)),
        }
    }
}

impl GitService for MockGit {
    fn detect_default_branch_readonly(&self) -> R<String> {
        (self.detect_default_branch_readonly)()
    }
    fn is_inside_work_tree(&self) -> bool {
        (self.is_inside_work_tree)()
    }
    fn get_special_state(&self) -> GitSpecialState {
        let (rebase, merge) = (self.special_state_flags)();
        GitSpecialState {
            rebase,
            merge,
            current_branch: (self.get_current_branch)(),
        }
    }
}

/// Mockable `GitHubService`. Defaults match the TS `mockGitHub` success state.
pub struct MockGitHub {
    pub get_review_threads: Box<dyn Fn(i64) -> R<ReviewThreadsResult>>,
    pub detect_current_pr_number: Box<dyn Fn() -> R<Option<i64>>>,
}

impl Default for MockGitHub {
    fn default() -> Self {
        MockGitHub {
            get_review_threads: Box::new(|_| {
                Ok(ReviewThreadsResult {
                    pr_title: String::new(),
                    pr_url: String::new(),
                    threads: vec![],
                })
            }),
            detect_current_pr_number: Box::new(|| Ok(None)),
        }
    }
}

impl GitHubService for MockGitHub {
    fn get_review_threads(&self, pr_number: i64) -> R<ReviewThreadsResult> {
        (self.get_review_threads)(pr_number)
    }
    fn detect_current_pr_number(&self) -> R<Option<i64>> {
        (self.detect_current_pr_number)()
    }
}

/// Records the arguments a mock receives, for tests that pin call order or the
/// exact value passed (matching the TS `calls` arrays / captured params).
#[derive(Default)]
pub struct Recorder {
    pub calls: RefCell<Vec<String>>,
}

impl Recorder {
    pub fn push(&self, s: impl Into<String>) {
        self.calls.borrow_mut().push(s.into());
    }
    pub fn snapshot(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}
