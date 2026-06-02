//! Port of `git-utils/tests/commands/commit.test.ts`.
#![allow(clippy::field_reassign_with_default)]

mod git_mocks;

use atelier::git::commands::commit::{run, CommitDeps};
use atelier::git::types::{CmdResult, CommitInput, JiraTicket};
use git_mocks::{MockGit, MockJira};
use std::cell::RefCell;
use std::rc::Rc;

fn input(commit_type: &str, description: &str) -> CommitInput {
    CommitInput {
        commit_type: commit_type.to_string(),
        description: description.to_string(),
        scope: None,
        body: None,
        skip_add: false,
    }
}

fn wad_ticket() -> JiraTicket {
    JiraTicket {
        raw: "WAD-0212".to_string(),
        normalized: "WAD-0212".to_string(),
    }
}

#[test]
fn jira_ticket_subject_format() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "feat/WAD-0212".to_string());
    let mut jira = MockJira::default();
    jira.detect_ticket = Box::new(|_| Some(wad_ticket()));
    let deps = CommitDeps {
        git: &git,
        jira: &jira,
    };
    let out = run(&deps, &input("feat", "add login")).unwrap();
    match out {
        CmdResult::Ok(d) => assert_eq!(d.subject, "[WAD-0212] feat: add login"),
        _ => panic!("expected ok"),
    }
}

#[test]
fn jira_ticket_ignores_scope() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "feat/WAD-0212".to_string());
    let mut jira = MockJira::default();
    jira.detect_ticket = Box::new(|_| Some(wad_ticket()));
    let deps = CommitDeps {
        git: &git,
        jira: &jira,
    };
    let mut i = input("feat", "add login");
    i.scope = Some("auth".to_string());
    let out = run(&deps, &i).unwrap();
    match out {
        CmdResult::Ok(d) => {
            assert_eq!(d.subject, "[WAD-0212] feat: add login");
            assert!(!d.subject.contains("auth"));
        }
        _ => panic!("expected ok"),
    }
}

#[test]
fn jira_ticket_in_output() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "feat/WAD-0212".to_string());
    let mut jira = MockJira::default();
    jira.detect_ticket = Box::new(|_| Some(wad_ticket()));
    let deps = CommitDeps {
        git: &git,
        jira: &jira,
    };
    let out = run(&deps, &input("feat", "add login")).unwrap();
    match out {
        CmdResult::Ok(d) => assert_eq!(d.jira_ticket.as_deref(), Some("WAD-0212")),
        _ => panic!("expected ok"),
    }
}

#[test]
fn normal_branch_with_scope() {
    let git = MockGit::default();
    let jira = MockJira::default();
    let deps = CommitDeps {
        git: &git,
        jira: &jira,
    };
    let mut i = input("feat", "add login");
    i.scope = Some("auth".to_string());
    match run(&deps, &i).unwrap() {
        CmdResult::Ok(d) => assert_eq!(d.subject, "feat(auth): add login"),
        _ => panic!("expected ok"),
    }
}

#[test]
fn normal_branch_no_scope() {
    let git = MockGit::default();
    let jira = MockJira::default();
    let deps = CommitDeps {
        git: &git,
        jira: &jira,
    };
    match run(&deps, &input("feat", "add login")).unwrap() {
        CmdResult::Ok(d) => assert_eq!(d.subject, "feat: add login"),
        _ => panic!("expected ok"),
    }
}

#[test]
fn normal_branch_no_jira_ticket() {
    let git = MockGit::default();
    let jira = MockJira::default();
    let deps = CommitDeps {
        git: &git,
        jira: &jira,
    };
    match run(&deps, &input("feat", "add login")).unwrap() {
        CmdResult::Ok(d) => assert!(d.jira_ticket.is_none()),
        _ => panic!("expected ok"),
    }
}

#[test]
fn valid_types_succeed() {
    for t in [
        "feat", "fix", "docs", "style", "refactor", "test", "chore", "perf",
    ] {
        let git = MockGit::default();
        let jira = MockJira::default();
        let deps = CommitDeps {
            git: &git,
            jira: &jira,
        };
        assert!(run(&deps, &input(t, "test description")).unwrap().is_ok());
    }
}

#[test]
fn invalid_type_fails() {
    let git = MockGit::default();
    let jira = MockJira::default();
    let deps = CommitDeps {
        git: &git,
        jira: &jira,
    };
    match run(&deps, &input("invalid", "test")).unwrap() {
        CmdResult::Err(e) => assert!(e.contains("Invalid commit type")),
        _ => panic!("expected err"),
    }
}

#[test]
fn skip_add_false_calls_add_tracked() {
    let called = Rc::new(RefCell::new(false));
    let c = called.clone();
    let mut git = MockGit::default();
    git.add_tracked = Box::new(move || {
        *c.borrow_mut() = true;
        Ok(())
    });
    let jira = MockJira::default();
    let deps = CommitDeps {
        git: &git,
        jira: &jira,
    };
    run(&deps, &input("feat", "test")).unwrap();
    assert!(*called.borrow());
}

#[test]
fn skip_add_true_skips_add_tracked() {
    let called = Rc::new(RefCell::new(false));
    let c = called.clone();
    let mut git = MockGit::default();
    git.add_tracked = Box::new(move || {
        *c.borrow_mut() = true;
        Ok(())
    });
    let jira = MockJira::default();
    let deps = CommitDeps {
        git: &git,
        jira: &jira,
    };
    let mut i = input("feat", "test");
    i.skip_add = true;
    run(&deps, &i).unwrap();
    assert!(!*called.borrow());
}

#[test]
fn commit_message_has_coauthor() {
    let msg = Rc::new(RefCell::new(String::new()));
    let m = msg.clone();
    let mut git = MockGit::default();
    git.commit = Box::new(move |s| {
        *m.borrow_mut() = s.to_string();
        Ok(())
    });
    let jira = MockJira::default();
    let deps = CommitDeps {
        git: &git,
        jira: &jira,
    };
    run(&deps, &input("feat", "test")).unwrap();
    assert!(msg
        .borrow()
        .contains("Co-Authored-By: Claude <noreply@anthropic.com>"));
}

#[test]
fn commit_message_with_body() {
    let msg = Rc::new(RefCell::new(String::new()));
    let m = msg.clone();
    let mut git = MockGit::default();
    git.commit = Box::new(move |s| {
        *m.borrow_mut() = s.to_string();
        Ok(())
    });
    let jira = MockJira::default();
    let deps = CommitDeps {
        git: &git,
        jira: &jira,
    };
    let mut i = input("feat", "test");
    i.body = Some("detailed explanation".to_string());
    run(&deps, &i).unwrap();
    let m = msg.borrow();
    assert!(m.contains("feat: test"));
    assert!(m.contains("\n\ndetailed explanation"));
}

#[test]
fn empty_description_fails() {
    let git = MockGit::default();
    let jira = MockJira::default();
    let deps = CommitDeps {
        git: &git,
        jira: &jira,
    };
    match run(&deps, &input("feat", "")).unwrap() {
        CmdResult::Err(e) => assert!(e.contains("Description is required")),
        _ => panic!("expected err"),
    }
}

#[test]
fn commit_failure_propagates_as_handled_error() {
    let mut git = MockGit::default();
    git.commit = Box::new(|_| Err("commit failed".to_string()));
    let jira = MockJira::default();
    let deps = CommitDeps {
        git: &git,
        jira: &jira,
    };
    match run(&deps, &input("feat", "test")).unwrap() {
        CmdResult::Err(e) => assert_eq!(e, "commit failed"),
        _ => panic!("expected err"),
    }
}

#[test]
fn add_tracked_failure_propagates_as_outer_err() {
    let mut git = MockGit::default();
    git.add_tracked = Box::new(|| Err("add failed".to_string()));
    let jira = MockJira::default();
    let deps = CommitDeps {
        git: &git,
        jira: &jira,
    };
    // Matches the TS `rejects.toThrow('add failed')` — addTracked is not
    // wrapped in try/catch, so the error propagates (outer Err).
    let err = run(&deps, &input("feat", "test")).unwrap_err();
    assert_eq!(err, "add failed");
}
