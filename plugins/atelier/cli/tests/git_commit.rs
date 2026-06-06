//! Mock-based port of git-utils `tests/commands/commit.test.ts`.
//!
//! Jira detection uses the real pure `detect_ticket`, driven by the mock's
//! `get_current_branch` — the TS test's `mockJira` returns values consistent
//! with the branch name, so behaviour matches.

use std::cell::RefCell;

use anyhow::{bail, Result};

use atelier::git::commands::commit::CommitCommand;
use atelier::git::core::git::{BranchLocation, GitService, GitSpecialState};
use atelier::git::types::CommitInput;

struct MockGit {
    current_branch: String,
    add_tracked_fails: bool,
    commit_fails: bool,
    add_tracked_called: RefCell<bool>,
    committed: RefCell<Option<String>>,
}

impl MockGit {
    fn new() -> Self {
        Self {
            current_branch: "feat/something".to_string(),
            add_tracked_fails: false,
            commit_fails: false,
            add_tracked_called: RefCell::new(false),
            committed: RefCell::new(None),
        }
    }
}

impl GitService for MockGit {
    fn detect_default_branch(&self) -> Result<String> {
        Ok("main".to_string())
    }
    fn get_current_branch(&self) -> Result<String> {
        Ok(self.current_branch.clone())
    }
    fn branch_exists(&self, _name: &str, _location: BranchLocation) -> Result<bool> {
        Ok(false)
    }
    fn is_inside_work_tree(&self) -> Result<bool> {
        Ok(true)
    }
    fn has_uncommitted_changes(&self) -> Result<bool> {
        Ok(false)
    }
    fn get_special_state(&self) -> Result<GitSpecialState> {
        Ok(GitSpecialState {
            rebase: false,
            merge: false,
            detached: false,
        })
    }
    fn fetch(&self, _remote: Option<&str>) -> Result<()> {
        Ok(())
    }
    fn checkout(&self, _branch: &str, _create: bool, _track: Option<&str>) -> Result<()> {
        Ok(())
    }
    fn commit(&self, message: &str) -> Result<()> {
        *self.committed.borrow_mut() = Some(message.to_string());
        if self.commit_fails {
            bail!("commit failed");
        }
        Ok(())
    }
    fn push(&self, _branch: &str, _set_upstream: bool) -> Result<()> {
        Ok(())
    }
    fn pull(&self, _branch: &str) -> Result<()> {
        Ok(())
    }
    fn add_tracked(&self) -> Result<()> {
        *self.add_tracked_called.borrow_mut() = true;
        if self.add_tracked_fails {
            bail!("add failed");
        }
        Ok(())
    }
}

fn input(commit_type: &str, description: &str) -> CommitInput {
    CommitInput {
        commit_type: commit_type.to_string(),
        description: description.to_string(),
        scope: None,
        body: None,
        skip_add: false,
    }
}

// ---------- subject formatting: Jira branch ----------

#[test]
fn jira_ticket_subject() {
    let git = MockGit {
        current_branch: "feat/WAD-0212".to_string(),
        ..MockGit::new()
    };
    let r = CommitCommand::new(&git)
        .run(&input("feat", "add login"))
        .unwrap();
    assert_eq!(r.subject, "[WAD-0212] feat: add login");
}

#[test]
fn jira_branch_ignores_scope() {
    let git = MockGit {
        current_branch: "feat/WAD-0212".to_string(),
        ..MockGit::new()
    };
    let r = CommitCommand::new(&git)
        .run(&CommitInput {
            scope: Some("auth".to_string()),
            ..input("feat", "add login")
        })
        .unwrap();
    assert_eq!(r.subject, "[WAD-0212] feat: add login");
    assert!(!r.subject.contains("auth"));
}

#[test]
fn jira_ticket_in_output() {
    let git = MockGit {
        current_branch: "feat/WAD-0212".to_string(),
        ..MockGit::new()
    };
    let r = CommitCommand::new(&git)
        .run(&input("feat", "add login"))
        .unwrap();
    assert_eq!(r.jira_ticket.as_deref(), Some("WAD-0212"));
}

// ---------- subject formatting: plain branch ----------

#[test]
fn scope_subject() {
    let git = MockGit::new();
    let r = CommitCommand::new(&git)
        .run(&CommitInput {
            scope: Some("auth".to_string()),
            ..input("feat", "add login")
        })
        .unwrap();
    assert_eq!(r.subject, "feat(auth): add login");
}

#[test]
fn plain_subject() {
    let git = MockGit::new();
    let r = CommitCommand::new(&git)
        .run(&input("feat", "add login"))
        .unwrap();
    assert_eq!(r.subject, "feat: add login");
}

#[test]
fn no_jira_ticket_in_output() {
    let git = MockGit::new();
    let r = CommitCommand::new(&git)
        .run(&input("feat", "add login"))
        .unwrap();
    assert_eq!(r.jira_ticket, None);
}

// ---------- type validation ----------

#[test]
fn all_valid_types_succeed() {
    for t in [
        "feat", "fix", "docs", "style", "refactor", "test", "chore", "perf",
    ] {
        let git = MockGit::new();
        assert!(CommitCommand::new(&git).run(&input(t, "desc")).is_ok());
    }
}

#[test]
fn invalid_type_fails() {
    let git = MockGit::new();
    let err = CommitCommand::new(&git)
        .run(&input("invalid", "test"))
        .unwrap_err();
    assert!(err.contains("Invalid commit type"));
}

// ---------- git operations ----------

#[test]
fn add_tracked_called_when_not_skipped() {
    let git = MockGit::new();
    CommitCommand::new(&git)
        .run(&input("feat", "test"))
        .unwrap();
    assert!(*git.add_tracked_called.borrow());
}

#[test]
fn add_tracked_skipped() {
    let git = MockGit::new();
    CommitCommand::new(&git)
        .run(&CommitInput {
            skip_add: true,
            ..input("feat", "test")
        })
        .unwrap();
    assert!(!*git.add_tracked_called.borrow());
}

#[test]
fn message_has_co_authored_by() {
    let git = MockGit::new();
    CommitCommand::new(&git)
        .run(&input("feat", "test"))
        .unwrap();
    let msg = git.committed.borrow().clone().unwrap();
    assert!(msg.contains("Co-Authored-By: Claude <noreply@anthropic.com>"));
}

#[test]
fn message_has_body() {
    let git = MockGit::new();
    CommitCommand::new(&git)
        .run(&CommitInput {
            body: Some("detailed explanation".to_string()),
            ..input("feat", "test")
        })
        .unwrap();
    let msg = git.committed.borrow().clone().unwrap();
    assert!(msg.contains("feat: test"));
    assert!(msg.contains("\n\ndetailed explanation"));
}

// ---------- error handling ----------

#[test]
fn empty_description_fails() {
    let git = MockGit::new();
    let err = CommitCommand::new(&git)
        .run(&input("feat", ""))
        .unwrap_err();
    assert!(err.contains("Description is required"));
}

#[test]
fn commit_failure_propagates() {
    let git = MockGit {
        commit_fails: true,
        ..MockGit::new()
    };
    let err = CommitCommand::new(&git)
        .run(&input("feat", "test"))
        .unwrap_err();
    assert_eq!(err, "commit failed");
}

#[test]
fn add_tracked_failure_propagates() {
    let git = MockGit {
        add_tracked_fails: true,
        ..MockGit::new()
    };
    let err = CommitCommand::new(&git)
        .run(&input("feat", "test"))
        .unwrap_err();
    assert!(err.contains("add failed"));
}
