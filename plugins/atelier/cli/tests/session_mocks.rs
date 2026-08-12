//! Shared test doubles and fixtures for the session subsystem tests, following
//! the same convention as `git_mocks`. The store and the repository are
//! in-memory, so the session rules are pinned without a git repo or a temp
//! directory.
//!
//! `#![allow(dead_code)]` because each test binary uses only a subset (Cargo
//! compiles this module separately into every test crate).
#![allow(dead_code)]

use atelier::session::core::baseline::{Baseline, BaselineStore};
use atelier::session::core::repo::RepoReader;
use std::cell::RefCell;
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
