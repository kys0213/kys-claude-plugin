//! Mock-based port of git-utils `tests/commands/branch.test.ts`.

use std::cell::RefCell;

use anyhow::{bail, Result};

use atelier::git::commands::branch::BranchCommand;
use atelier::git::core::git::{BranchLocation, GitService, GitSpecialState};
use atelier::git::types::BranchInput;

/// Recorded git side effects, in call order.
#[derive(Clone, PartialEq, Debug)]
enum Event {
    Fetch,
    Pull(String),
    Checkout {
        branch: String,
        create: bool,
        track: Option<String>,
    },
}

type ExistsFn = Box<dyn Fn(&str, BranchLocation) -> bool>;

struct MockGit {
    uncommitted: bool,
    detect_default: Result<String, String>,
    branch_exists: ExistsFn,
    fetch_fails: bool,
    checkout_create_fails: bool,
    events: RefCell<Vec<Event>>,
}

impl MockGit {
    fn new() -> Self {
        Self {
            uncommitted: false,
            detect_default: Ok("main".to_string()),
            branch_exists: Box::new(|_, _| false),
            fetch_fails: false,
            checkout_create_fails: false,
            events: RefCell::new(Vec::new()),
        }
    }

    fn rendered(&self) -> Vec<String> {
        self.events
            .borrow()
            .iter()
            .map(|e| match e {
                Event::Fetch => "fetch".to_string(),
                Event::Pull(b) => format!("pull:{b}"),
                Event::Checkout { branch, create, .. } => {
                    if *create {
                        format!("checkout-create:{branch}")
                    } else {
                        format!("checkout:{branch}")
                    }
                }
            })
            .collect()
    }
}

impl GitService for MockGit {
    fn detect_default_branch(&self) -> Result<String> {
        match &self.detect_default {
            Ok(b) => Ok(b.clone()),
            Err(e) => bail!("{e}"),
        }
    }
    fn get_current_branch(&self) -> Result<String> {
        Ok("main".to_string())
    }
    fn branch_exists(&self, name: &str, location: BranchLocation) -> Result<bool> {
        Ok((self.branch_exists)(name, location))
    }
    fn is_inside_work_tree(&self) -> Result<bool> {
        Ok(true)
    }
    fn has_uncommitted_changes(&self) -> Result<bool> {
        Ok(self.uncommitted)
    }
    fn get_special_state(&self) -> Result<GitSpecialState> {
        Ok(GitSpecialState {
            rebase: false,
            merge: false,
            detached: false,
        })
    }
    fn fetch(&self, _remote: Option<&str>) -> Result<()> {
        self.events.borrow_mut().push(Event::Fetch);
        if self.fetch_fails {
            bail!("network error");
        }
        Ok(())
    }
    fn checkout(&self, branch: &str, create: bool, track: Option<&str>) -> Result<()> {
        self.events.borrow_mut().push(Event::Checkout {
            branch: branch.to_string(),
            create,
            track: track.map(str::to_string),
        });
        if create && self.checkout_create_fails {
            bail!("checkout failed");
        }
        Ok(())
    }
    fn commit(&self, _message: &str) -> Result<()> {
        Ok(())
    }
    fn push(&self, _branch: &str, _set_upstream: bool) -> Result<()> {
        Ok(())
    }
    fn pull(&self, branch: &str) -> Result<()> {
        self.events
            .borrow_mut()
            .push(Event::Pull(branch.to_string()));
        Ok(())
    }
    fn add_tracked(&self) -> Result<()> {
        Ok(())
    }
}

fn input(name: &str, base: Option<&str>) -> BranchInput {
    BranchInput {
        branch_name: name.to_string(),
        base_branch: base.map(str::to_string),
    }
}

// ---------- happy path ----------

#[test]
fn detects_default_base_when_absent() {
    let git = MockGit {
        branch_exists: Box::new(|name, loc| name == "main" && loc == BranchLocation::Any),
        ..MockGit::new()
    };
    let r = BranchCommand::new(&git)
        .run(&input("feat/new-feature", None))
        .unwrap();
    assert_eq!(r.base_branch, "main");
}

#[test]
fn uses_explicit_base() {
    let git = MockGit {
        branch_exists: Box::new(|name, loc| {
            name == "develop" && matches!(loc, BranchLocation::Any | BranchLocation::Local)
        }),
        ..MockGit::new()
    };
    let r = BranchCommand::new(&git)
        .run(&input("feat/new-feature", Some("develop")))
        .unwrap();
    assert_eq!(r.base_branch, "develop");
}

#[test]
fn returns_branch_and_base() {
    let git = MockGit {
        branch_exists: Box::new(|name, loc| {
            name == "main" && matches!(loc, BranchLocation::Any | BranchLocation::Local)
        }),
        ..MockGit::new()
    };
    let r = BranchCommand::new(&git)
        .run(&input("feat/login", None))
        .unwrap();
    assert_eq!(r.branch_name, "feat/login");
    assert_eq!(r.base_branch, "main");
}

// ---------- git operation order ----------

#[test]
fn order_fetch_checkout_pull_create() {
    let git = MockGit {
        branch_exists: Box::new(|name, loc| {
            name == "main" && matches!(loc, BranchLocation::Any | BranchLocation::Local)
        }),
        ..MockGit::new()
    };
    BranchCommand::new(&git)
        .run(&input("feat/new", None))
        .unwrap();
    assert_eq!(
        git.rendered(),
        vec![
            "fetch",
            "checkout:main",
            "pull:main",
            "checkout-create:feat/new"
        ]
    );
}

#[test]
fn remote_only_base_creates_tracking_branch() {
    let git = MockGit {
        // base exists in "any" but NOT local → create+track
        branch_exists: Box::new(|name, loc| name == "main" && loc == BranchLocation::Any),
        ..MockGit::new()
    };
    BranchCommand::new(&git)
        .run(&input("feat/new", None))
        .unwrap();
    let base_checkout = git
        .events
        .borrow()
        .iter()
        .find_map(|e| match e {
            Event::Checkout {
                branch,
                create,
                track,
            } if branch == "main" => Some((*create, track.clone())),
            _ => None,
        })
        .expect("base checkout recorded");
    assert!(base_checkout.0);
    assert_eq!(base_checkout.1.as_deref(), Some("origin/main"));
}

// ---------- precondition validation ----------

#[test]
fn uncommitted_changes_fail() {
    let git = MockGit {
        uncommitted: true,
        ..MockGit::new()
    };
    let err = BranchCommand::new(&git)
        .run(&input("feat/new", None))
        .unwrap_err();
    assert!(err.contains("Uncommitted changes"));
}

#[test]
fn missing_base_fails() {
    let git = MockGit::new(); // branch_exists always false
    let err = BranchCommand::new(&git)
        .run(&input("feat/new", None))
        .unwrap_err();
    assert!(err.contains("does not exist"));
}

#[test]
fn existing_target_fails() {
    let git = MockGit {
        branch_exists: Box::new(|name, loc| {
            (name == "main" && loc == BranchLocation::Any)
                || (name == "feat/existing" && loc == BranchLocation::Local)
        }),
        ..MockGit::new()
    };
    let err = BranchCommand::new(&git)
        .run(&input("feat/existing", None))
        .unwrap_err();
    assert!(err.contains("already exists"));
}

// ---------- error handling ----------

#[test]
fn fetch_failure_ignored() {
    let git = MockGit {
        fetch_fails: true,
        branch_exists: Box::new(|name, loc| {
            name == "main" && matches!(loc, BranchLocation::Any | BranchLocation::Local)
        }),
        ..MockGit::new()
    };
    assert!(BranchCommand::new(&git)
        .run(&input("feat/new", None))
        .is_ok());
}

#[test]
fn checkout_create_failure_propagates() {
    let git = MockGit {
        checkout_create_fails: true,
        branch_exists: Box::new(|name, loc| {
            name == "main" && matches!(loc, BranchLocation::Any | BranchLocation::Local)
        }),
        ..MockGit::new()
    };
    let err = BranchCommand::new(&git)
        .run(&input("feat/new", None))
        .unwrap_err();
    assert_eq!(err, "checkout failed");
}

#[test]
fn empty_branch_name_fails() {
    let git = MockGit::new();
    let err = BranchCommand::new(&git).run(&input("", None)).unwrap_err();
    assert!(err.contains("Branch name is required"));
}
