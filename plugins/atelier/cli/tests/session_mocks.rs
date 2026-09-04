//! Shared test doubles and fixtures for the session subsystem tests, following
//! the same convention as `git_mocks`. The store and the repository are
//! in-memory, so the session rules are pinned without a git repo or a temp
//! directory.
//!
//! `#![allow(dead_code)]` because each test binary uses only a subset (Cargo
//! compiles this module separately into every test crate).
#![allow(dead_code)]

use atelier::git::types::GitSpecialState;
use atelier::session::core::baseline::{Baseline, BaselineStore};
use atelier::session::core::branch_sync::BranchSyncReader;
use atelier::session::core::open_pr::OpenPrReader;
use atelier::session::core::repo::RepoReader;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};

/// A session id that passes `is_valid_session_id`, so tests exercise the rules
/// rather than the id guard.
pub const SESSION: &str = "sess-abc12345";

pub fn paths(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

pub fn baseline(head: &str, dirty: &[&str]) -> Baseline {
    Baseline {
        head: Some(head.to_string()),
        dirty: paths(dirty),
        notified: false,
    }
}

/// In-memory `BaselineStore`.
#[derive(Default)]
pub struct MemStore {
    /// Public so tests can assert on the store as a whole (e.g. that a rejected
    /// session id wrote nothing at all).
    pub entries: RefCell<HashMap<String, Baseline>>,
}

impl MemStore {
    pub fn with(session_id: &str, baseline: Baseline) -> Self {
        let store = MemStore::default();
        store
            .entries
            .borrow_mut()
            .insert(session_id.to_string(), baseline);
        store
    }
}

impl BaselineStore for MemStore {
    fn load(&self, session_id: &str) -> Option<Baseline> {
        self.entries.borrow().get(session_id).cloned()
    }
    fn save(&self, session_id: &str, baseline: &Baseline) -> Result<(), String> {
        self.entries
            .borrow_mut()
            .insert(session_id.to_string(), baseline.clone());
        Ok(())
    }
}

/// In-memory `RepoReader` describing one repository state.
pub struct MemRepo {
    pub inside_work_tree: bool,
    pub head: Option<String>,
    pub dirty: BTreeSet<String>,
    /// Files each base commit reports as changed since, keyed by commit.
    pub committed: HashMap<String, BTreeSet<String>>,
}

impl Default for MemRepo {
    fn default() -> Self {
        MemRepo {
            inside_work_tree: true,
            head: Some("head1".to_string()),
            dirty: BTreeSet::new(),
            committed: HashMap::new(),
        }
    }
}

impl RepoReader for MemRepo {
    fn is_inside_work_tree(&self) -> bool {
        self.inside_work_tree
    }
    fn head(&self) -> Option<String> {
        self.head.clone()
    }
    fn dirty_files(&self) -> BTreeSet<String> {
        self.dirty.clone()
    }
    fn files_changed_since(&self, base_head: &str) -> BTreeSet<String> {
        self.committed.get(base_head).cloned().unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// push-check doubles
// ---------------------------------------------------------------------------

/// In-memory `BranchSyncReader` describing one branch's sync state.
pub struct MemBranchSync {
    pub inside_work_tree: bool,
    pub rebase: bool,
    pub merge: bool,
    /// Empty means detached HEAD, exactly as `branch --show-current` reports.
    pub current_branch: String,
    /// `None` models a failed default-branch detection.
    pub default_branch: Option<String>,
    /// `(behind, ahead)`; `None` models a branch with no upstream.
    pub divergence: Option<(u32, u32)>,
    /// Total reads across every method, so a test can pin that a rule
    /// answering earlier in the order left the git reads untouched.
    pub reads: Cell<usize>,
}

impl Default for MemBranchSync {
    fn default() -> Self {
        MemBranchSync {
            inside_work_tree: true,
            rebase: false,
            merge: false,
            current_branch: "feature/x".to_string(),
            default_branch: Some("main".to_string()),
            divergence: Some((0, 1)),
            reads: Cell::new(0),
        }
    }
}

impl MemBranchSync {
    fn record_read(&self) {
        self.reads.set(self.reads.get() + 1);
    }
}

impl BranchSyncReader for MemBranchSync {
    fn is_inside_work_tree(&self) -> bool {
        self.record_read();
        self.inside_work_tree
    }
    fn special_state(&self) -> GitSpecialState {
        self.record_read();
        GitSpecialState {
            rebase: self.rebase,
            merge: self.merge,
            current_branch: self.current_branch.clone(),
        }
    }
    fn default_branch(&self) -> Option<String> {
        self.record_read();
        self.default_branch.clone()
    }
    fn upstream_divergence(&self) -> Option<(u32, u32)> {
        self.record_read();
        self.divergence
    }
}

/// In-memory `OpenPrReader` that counts its calls, so tests can pin the
/// contract that silent paths never pay for a network round-trip.
pub struct MemOpenPr {
    pub answer: Result<Option<i64>, String>,
    pub calls: Cell<usize>,
}

impl MemOpenPr {
    pub fn open(pr_number: i64) -> Self {
        MemOpenPr {
            answer: Ok(Some(pr_number)),
            calls: Cell::new(0),
        }
    }
    pub fn none() -> Self {
        MemOpenPr {
            answer: Ok(None),
            calls: Cell::new(0),
        }
    }
    pub fn failing() -> Self {
        MemOpenPr {
            answer: Err("gh unavailable".to_string()),
            calls: Cell::new(0),
        }
    }
}

impl Default for MemOpenPr {
    fn default() -> Self {
        MemOpenPr::none()
    }
}

impl OpenPrReader for MemOpenPr {
    fn open_pr_number(&self) -> Result<Option<i64>, String> {
        self.calls.set(self.calls.get() + 1);
        self.answer.clone()
    }
}
