//! Black-box tests for the `session simplify-check` decision. The store and
//! the repository are in-memory doubles, so every rule is pinned without a git
//! repo or a temp directory.

use atelier::session::commands::simplify::{run, SilentReason, SimplifyDecision};
use atelier::session::commands::SessionDeps;
use atelier::session::core::baseline::{Baseline, BaselineStore};
use atelier::session::core::repo::RepoReader;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};

const SESSION: &str = "sess-abc12345";

/// In-memory `BaselineStore`.
#[derive(Default)]
struct MemStore {
    entries: RefCell<HashMap<String, Baseline>>,
}

impl MemStore {
    fn with(session_id: &str, baseline: Baseline) -> Self {
        let store = MemStore::default();
        store
            .entries
            .borrow_mut()
            .insert(session_id.to_string(), baseline);
        store
    }
    fn get(&self, session_id: &str) -> Option<Baseline> {
        self.entries.borrow().get(session_id).cloned()
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
struct MemRepo {
    inside_work_tree: bool,
    head: Option<String>,
    dirty: BTreeSet<String>,
    /// Files each base commit reports as changed since, keyed by commit.
    committed: HashMap<String, BTreeSet<String>>,
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

fn paths(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn baseline(head: &str, dirty: &[&str]) -> Baseline {
    Baseline {
        head: Some(head.to_string()),
        dirty: paths(dirty),
        notified: false,
    }
}

fn notified_files(decision: &SimplifyDecision) -> (Vec<String>, usize) {
    match decision {
        SimplifyDecision::Notify { files, total } => (files.clone(), *total),
        other => panic!("expected Notify, got {other:?}"),
    }
}

#[test]
fn notify_when_session_adds_code_file() {
    let store = MemStore::with(SESSION, baseline("head0", &[]));
    let repo = MemRepo {
        dirty: paths(&["src/lib.rs"]),
        ..MemRepo::default()
    };
    let deps = SessionDeps {
        store: &store,
        repo: &repo,
    };

    let (files, total) = notified_files(&run(&deps, SESSION));
    assert_eq!(files, vec!["src/lib.rs".to_string()]);
    assert_eq!(total, 1);
    // Notifying marks the session so the banner does not repeat every Stop.
    assert!(store.get(SESSION).unwrap().notified);
}

#[test]
fn silent_when_all_dirty_predates_session() {
    // The tree was already dirty at session start and the session touched
    // nothing — the old hook announced these files anyway.
    let store = MemStore::with(SESSION, baseline("head0", &["src/old.rs", "README.md"]));
    let repo = MemRepo {
        dirty: paths(&["src/old.rs", "README.md"]),
        ..MemRepo::default()
    };
    let deps = SessionDeps {
        store: &store,
        repo: &repo,
    };

    assert_eq!(
        run(&deps, SESSION),
        SimplifyDecision::Silent(SilentReason::NoSessionChanges)
    );
    assert!(!store.get(SESSION).unwrap().notified);
}

#[test]
fn silent_when_already_notified_in_session() {
    let mut already = baseline("head0", &[]);
    already.mark_notified();
    let store = MemStore::with(SESSION, already);
    let repo = MemRepo {
        dirty: paths(&["src/lib.rs"]),
        ..MemRepo::default()
    };
    let deps = SessionDeps {
        store: &store,
        repo: &repo,
    };

    assert_eq!(
        run(&deps, SESSION),
        SimplifyDecision::Silent(SilentReason::AlreadyNotified)
    );
}

#[test]
fn silent_when_only_docs_and_config_changed() {
    let store = MemStore::with(SESSION, baseline("head0", &[]));
    let repo = MemRepo {
        dirty: paths(&[
            "docs/guide.md",
            "config/app.yaml",
            "Cargo.lock",
            "LICENSE",
            ".gitignore",
            "notes.txt",
        ]),
        ..MemRepo::default()
    };
    let deps = SessionDeps {
        store: &store,
        repo: &repo,
    };

    assert_eq!(
        run(&deps, SESSION),
        SimplifyDecision::Silent(SilentReason::DocsOnly)
    );
}

#[test]
fn notify_when_docs_and_code_mixed() {
    let store = MemStore::with(SESSION, baseline("head0", &[]));
    let repo = MemRepo {
        dirty: paths(&["docs/guide.md", "src/lib.rs"]),
        ..MemRepo::default()
    };
    let deps = SessionDeps {
        store: &store,
        repo: &repo,
    };

    // Docs still count toward the total once code is in the mix — the banner
    // lists what the session touched, not only the code.
    let (files, total) = notified_files(&run(&deps, SESSION));
    assert_eq!(files, vec!["docs/guide.md", "src/lib.rs"]);
    assert_eq!(total, 2);
}

#[test]
fn counts_files_committed_since_baseline_head() {
    // Work committed during the session is gone from `git status`; without the
    // baseline HEAD diff it would be invisible.
    let store = MemStore::with(SESSION, baseline("head0", &[]));
    let repo = MemRepo {
        head: Some("head9".to_string()),
        dirty: paths(&["src/wip.rs"]),
        committed: HashMap::from([("head0".to_string(), paths(&["src/done.rs", "src/wip.rs"]))]),
        ..MemRepo::default()
    };
    let deps = SessionDeps {
        store: &store,
        repo: &repo,
    };

    // Union, not sum: a file both committed and dirty is counted once.
    let (files, total) = notified_files(&run(&deps, SESSION));
    assert_eq!(files, vec!["src/done.rs", "src/wip.rs"]);
    assert_eq!(total, 2);
}

#[test]
fn silent_when_session_id_missing() {
    // A hook payload without `session_id` cannot attribute anything.
    let store = MemStore::default();
    let repo = MemRepo {
        dirty: paths(&["src/lib.rs"]),
        ..MemRepo::default()
    };
    let deps = SessionDeps {
        store: &store,
        repo: &repo,
    };

    assert_eq!(
        run(&deps, ""),
        SimplifyDecision::Silent(SilentReason::NoSessionId)
    );
    assert!(store.entries.borrow().is_empty());
}

#[test]
fn silent_when_baseline_absent_and_records_it() {
    // Plugin installed mid-session: nothing is attributable yet, so stay quiet
    // and anchor from here so the next turn can decide.
    let store = MemStore::default();
    let repo = MemRepo {
        head: Some("head5".to_string()),
        dirty: paths(&["src/pre-existing.rs"]),
        ..MemRepo::default()
    };
    let deps = SessionDeps {
        store: &store,
        repo: &repo,
    };

    assert_eq!(
        run(&deps, SESSION),
        SimplifyDecision::Silent(SilentReason::NoBaseline)
    );
    let recorded = store.get(SESSION).expect("baseline recorded on Stop");
    assert_eq!(recorded.head.as_deref(), Some("head5"));
    assert_eq!(recorded.dirty, paths(&["src/pre-existing.rs"]));
    assert!(!recorded.notified);
}
