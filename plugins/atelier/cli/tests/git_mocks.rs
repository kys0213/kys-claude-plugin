//! Shared test doubles for the git subsystem command tests. Each mock mirrors
//! the `mockGit`/`mockJira`/`mockGitHub` factories from the original
//! `git-utils` bun tests: every method defaults to a success/no-op and tests
//! override only the closures they care about via the builder setters.
//!
//! `#![allow(dead_code)]` because each test binary uses only a subset of the
//! mocks (Cargo compiles this module separately into every test crate).
//! `type_complexity` is allowed because the `Box<dyn Fn(...)>` override fields
//! are the whole point of the builder-style mocks.
#![allow(dead_code, clippy::type_complexity)]

use std::cell::RefCell;

use atelier::git::core::git::{BranchLocation, CheckoutOptions, GitService, PushOptions};
use atelier::git::core::github::{CreatePrOptions, GitHubService, ReviewThreadsResult};
use atelier::git::core::jira::JiraService;
use atelier::git::types::{GitSpecialState, JiraTicket};

type R<T> = Result<T, String>;

/// Mockable `GitService`. Defaults match the TS `mockGit` success state.
pub struct MockGit {
    pub is_inside_work_tree: Box<dyn Fn() -> bool>,
    pub get_current_branch: Box<dyn Fn() -> String>,
    pub detect_default_branch: Box<dyn Fn() -> R<String>>,
    pub detect_default_branch_readonly: Box<dyn Fn() -> R<String>>,
    pub get_special_state: Box<dyn Fn() -> GitSpecialState>,
    pub branch_exists: Box<dyn Fn(&str, BranchLocation) -> bool>,
    pub has_uncommitted_changes: Box<dyn Fn() -> bool>,
    pub fetch: Box<dyn Fn() -> R<()>>,
    pub checkout: Box<dyn Fn(&str, Option<&CheckoutOptions>) -> R<()>>,
    pub commit: Box<dyn Fn(&str) -> R<()>>,
    pub push: Box<dyn Fn(&str, Option<&PushOptions>) -> R<()>>,
    pub pull: Box<dyn Fn(&str) -> R<()>>,
    pub add_tracked: Box<dyn Fn() -> R<()>>,
}

impl Default for MockGit {
    fn default() -> Self {
        MockGit {
            is_inside_work_tree: Box::new(|| true),
            get_current_branch: Box::new(|| "main".to_string()),
            detect_default_branch: Box::new(|| Ok("main".to_string())),
            detect_default_branch_readonly: Box::new(|| Ok("main".to_string())),
            get_special_state: Box::new(|| GitSpecialState {
                rebase: false,
                merge: false,
                detached: false,
            }),
            branch_exists: Box::new(|_, _| false),
            has_uncommitted_changes: Box::new(|| false),
            fetch: Box::new(|| Ok(())),
            checkout: Box::new(|_, _| Ok(())),
            commit: Box::new(|_| Ok(())),
            push: Box::new(|_, _| Ok(())),
            pull: Box::new(|_| Ok(())),
            add_tracked: Box::new(|| Ok(())),
        }
    }
}

impl GitService for MockGit {
    fn detect_default_branch(&self) -> R<String> {
        (self.detect_default_branch)()
    }
    fn detect_default_branch_readonly(&self) -> R<String> {
        (self.detect_default_branch_readonly)()
    }
    fn get_current_branch(&self) -> String {
        (self.get_current_branch)()
    }
    fn branch_exists(&self, name: &str, location: BranchLocation) -> bool {
        (self.branch_exists)(name, location)
    }
    fn is_inside_work_tree(&self) -> bool {
        (self.is_inside_work_tree)()
    }
    fn has_uncommitted_changes(&self) -> bool {
        (self.has_uncommitted_changes)()
    }
    fn get_special_state(&self) -> GitSpecialState {
        (self.get_special_state)()
    }
    fn fetch(&self, _remote: Option<&str>) -> R<()> {
        (self.fetch)()
    }
    fn checkout(&self, branch: &str, options: Option<&CheckoutOptions>) -> R<()> {
        (self.checkout)(branch, options)
    }
    fn commit(&self, message: &str) -> R<()> {
        (self.commit)(message)
    }
    fn push(&self, branch: &str, options: Option<&PushOptions>) -> R<()> {
        (self.push)(branch, options)
    }
    fn pull(&self, branch: &str) -> R<()> {
        (self.pull)(branch)
    }
    fn add_tracked(&self) -> R<()> {
        (self.add_tracked)()
    }
}

/// Mockable `JiraService`; default detects nothing (matches TS `mockJira`).
pub struct MockJira {
    pub detect_ticket: Box<dyn Fn(&str) -> Option<JiraTicket>>,
}

impl Default for MockJira {
    fn default() -> Self {
        MockJira {
            detect_ticket: Box::new(|_| None),
        }
    }
}

impl JiraService for MockJira {
    fn detect_ticket(&self, branch_name: &str) -> Option<JiraTicket> {
        (self.detect_ticket)(branch_name)
    }
}

/// Mockable `GitHubService`. Defaults match the TS `mockGitHub` success state.
pub struct MockGitHub {
    pub is_authenticated: Box<dyn Fn() -> bool>,
    pub create_pr: Box<dyn Fn(&CreatePrOptions) -> R<String>>,
    pub get_review_threads: Box<dyn Fn(i64) -> R<ReviewThreadsResult>>,
    pub detect_current_pr_number: Box<dyn Fn() -> R<Option<i64>>>,
}

impl Default for MockGitHub {
    fn default() -> Self {
        MockGitHub {
            is_authenticated: Box::new(|| true),
            create_pr: Box::new(|_| Ok("https://github.com/org/repo/pull/1".to_string())),
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
    fn is_authenticated(&self) -> bool {
        (self.is_authenticated)()
    }
    fn create_pr(&self, options: &CreatePrOptions) -> R<String> {
        (self.create_pr)(options)
    }
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
