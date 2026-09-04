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
use std::collections::HashMap;

use atelier::git::commands::hook::HookFs;
use atelier::git::core::git::{GitService, OriginHeadWarmer};
use atelier::git::core::github::{GitHubService, OpenPr, RepoDefaultBranch, ReviewThreadsResult};
use atelier::git::types::{DetectedBranch, Divergence, GitSpecialState};

type R<T> = Result<T, String>;

/// In-memory `HookFs` matching the TS `createMockFs`: `exists` returns true for
/// a stored key or any key under `<path>/`, so directory checks work without a
/// directory concept.
#[derive(Default)]
pub struct MockFs {
    files: RefCell<HashMap<String, String>>,
    /// Counts `write_file` calls only — seeding via `set` does not bump it, so
    /// tests can pin "this operation wrote exactly once".
    writes: RefCell<usize>,
}

impl MockFs {
    pub fn new() -> Self {
        MockFs::default()
    }
    /// Seeds pre-existing content without counting as a write.
    pub fn set(&self, path: &str, content: &str) {
        self.files
            .borrow_mut()
            .insert(path.to_string(), content.to_string());
    }
    pub fn get(&self, path: &str) -> Option<String> {
        self.files.borrow().get(path).cloned()
    }
    pub fn write_count(&self) -> usize {
        *self.writes.borrow()
    }
}

impl HookFs for MockFs {
    fn read_file(&self, path: &str) -> R<String> {
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| format!("File not found: {path}"))
    }
    fn write_file(&self, path: &str, content: &str) -> R<()> {
        *self.writes.borrow_mut() += 1;
        self.files
            .borrow_mut()
            .insert(path.to_string(), content.to_string());
        Ok(())
    }
    fn exists(&self, path: &str) -> bool {
        let files = self.files.borrow();
        if files.contains_key(path) {
            return true;
        }
        let prefix = format!("{path}/");
        files.keys().any(|k| k.starts_with(&prefix))
    }
    fn mkdir(&self, _path: &str) -> R<()> {
        Ok(())
    }
}

/// Mockable `GitService` — only the reads the branch guard consumes.
pub struct MockGit {
    pub is_inside_work_tree: Box<dyn Fn() -> bool>,
    pub current_branch: Box<dyn Fn() -> String>,
    pub detect_default_branch: Box<dyn Fn() -> R<String>>,
    /// `(rebase, merge)` flags for `get_special_state`; `current_branch` is the
    /// branch the snapshot reports, mirroring `RealGitService` (#778).
    pub special_state_flags: Box<dyn Fn() -> (bool, bool)>,
    /// Drift against `@{upstream}`. Defaults to `None` — the honest answer for
    /// a repository nobody configured an upstream on.
    pub upstream_divergence: Box<dyn Fn() -> Option<Divergence>>,
}

impl Default for MockGit {
    fn default() -> Self {
        MockGit {
            is_inside_work_tree: Box::new(|| true),
            current_branch: Box::new(|| "main".to_string()),
            detect_default_branch: Box::new(|| Ok("main".to_string())),
            special_state_flags: Box::new(|| (false, false)),
            upstream_divergence: Box::new(|| None),
        }
    }
}

impl GitService for MockGit {
    fn detect_default_branch(&self) -> R<String> {
        (self.detect_default_branch)()
    }
    fn is_inside_work_tree(&self) -> bool {
        (self.is_inside_work_tree)()
    }
    fn get_special_state(&self) -> GitSpecialState {
        let (rebase, merge) = (self.special_state_flags)();
        GitSpecialState {
            rebase,
            merge,
            current_branch: (self.current_branch)(),
        }
    }
    fn upstream_divergence(&self) -> Option<Divergence> {
        (self.upstream_divergence)()
    }
}

/// Mockable `GitHubService`. Defaults match the TS `mockGitHub` success state.
pub struct MockGitHub {
    pub get_review_threads: Box<dyn Fn(i64) -> R<ReviewThreadsResult>>,
    pub detect_current_pr_number: Box<dyn Fn() -> R<Option<i64>>>,
    /// Raw `gh` stdout (or `None` for a non-zero exit), so the mock goes
    /// through the same `DetectedBranch::new` funnel the real service does —
    /// a blank answer must collapse to absence here exactly as it would live.
    pub default_branch: Box<dyn Fn() -> Option<String>>,
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
            default_branch: Box::new(|| None),
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

impl OpenPr for MockGitHub {
    /// Derived from `detect_current_pr_number` rather than from a field of its
    /// own, exactly as `RealGitHubService` derives it — a failing lookup is
    /// indistinguishable from "nothing open" to the caller.
    fn open_pr_number(&self) -> Option<i64> {
        (self.detect_current_pr_number)().ok().flatten()
    }
}

impl RepoDefaultBranch for MockGitHub {
    fn default_branch(&self) -> Option<DetectedBranch> {
        (self.default_branch)()
            .as_deref()
            .and_then(DetectedBranch::new)
    }
}

/// Mockable `OriginHeadWarmer`. Defaults to a successful warm-up.
pub struct MockWarmer {
    pub warm_origin_head: Box<dyn Fn() -> bool>,
}

impl Default for MockWarmer {
    fn default() -> Self {
        MockWarmer {
            warm_origin_head: Box::new(|| true),
        }
    }
}

impl OriginHeadWarmer for MockWarmer {
    fn warm_origin_head(&self) -> bool {
        (self.warm_origin_head)()
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
