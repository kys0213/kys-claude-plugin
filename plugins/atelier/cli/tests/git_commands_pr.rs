//! Port of `git-utils/tests/commands/pr.test.ts`.
#![allow(clippy::field_reassign_with_default)]

mod git_mocks;

use atelier::git::commands::pr::{run, PrDeps};
use atelier::git::types::{CmdResult, JiraTicket, PrInput};
use git_mocks::{MockGit, MockGitHub, MockJira, Recorder};
use std::cell::RefCell;
use std::rc::Rc;

fn input(title: &str) -> PrInput {
    PrInput {
        title: title.to_string(),
        description: None,
    }
}

#[test]
fn jira_ticket_title_format() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "feat/WAD-0212".to_string());
    let mut jira = MockJira::default();
    jira.detect_ticket = Box::new(|_| {
        Some(JiraTicket {
            raw: "WAD-0212".to_string(),
            normalized: "WAD-0212".to_string(),
        })
    });
    let github = MockGitHub::default();
    let deps = PrDeps {
        git: &git,
        jira: &jira,
        github: &github,
    };
    match run(&deps, &input("add login feature")) {
        CmdResult::Ok(d) => assert_eq!(d.title, "[WAD-0212] add login feature"),
        _ => panic!("expected ok"),
    }
}

#[test]
fn no_jira_ticket_uses_title_verbatim() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "feat/something".to_string());
    let jira = MockJira::default();
    let github = MockGitHub::default();
    let deps = PrDeps {
        git: &git,
        jira: &jira,
        github: &github,
    };
    match run(&deps, &input("add login feature")) {
        CmdResult::Ok(d) => assert_eq!(d.title, "add login feature"),
        _ => panic!("expected ok"),
    }
}

#[test]
fn call_order_push_then_create() {
    let rec = Rc::new(Recorder::default());
    let r1 = rec.clone();
    let r2 = rec.clone();
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "feat/something".to_string());
    git.push = Box::new(move |_, _| {
        r1.push("push");
        Ok(())
    });
    let jira = MockJira::default();
    let mut github = MockGitHub::default();
    github.create_pr = Box::new(move |_| {
        r2.push("createPr");
        Ok("https://github.com/org/repo/pull/1".to_string())
    });
    let deps = PrDeps {
        git: &git,
        jira: &jira,
        github: &github,
    };
    run(&deps, &input("test pr"));
    assert_eq!(rec.snapshot(), vec!["push", "createPr"]);
}

#[test]
fn description_passed_as_body() {
    let body = Rc::new(RefCell::new(String::new()));
    let b = body.clone();
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "feat/something".to_string());
    let jira = MockJira::default();
    let mut github = MockGitHub::default();
    github.create_pr = Box::new(move |opts| {
        *b.borrow_mut() = opts.body.to_string();
        Ok("https://github.com/org/repo/pull/1".to_string())
    });
    let deps = PrDeps {
        git: &git,
        jira: &jira,
        github: &github,
    };
    let mut i = input("test pr");
    i.description = Some("detailed description".to_string());
    run(&deps, &i);
    assert_eq!(*body.borrow(), "detailed description");
}

#[test]
fn no_description_empty_body() {
    let body = Rc::new(RefCell::new("sentinel".to_string()));
    let b = body.clone();
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "feat/something".to_string());
    let jira = MockJira::default();
    let mut github = MockGitHub::default();
    github.create_pr = Box::new(move |opts| {
        *b.borrow_mut() = opts.body.to_string();
        Ok("https://github.com/org/repo/pull/1".to_string())
    });
    let deps = PrDeps {
        git: &git,
        jira: &jira,
        github: &github,
    };
    run(&deps, &input("test pr"));
    assert_eq!(*body.borrow(), "");
}

#[test]
fn output_has_url_title_base() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "feat/something".to_string());
    let jira = MockJira::default();
    let mut github = MockGitHub::default();
    github.create_pr = Box::new(|_| Ok("https://github.com/org/repo/pull/42".to_string()));
    let deps = PrDeps {
        git: &git,
        jira: &jira,
        github: &github,
    };
    match run(&deps, &input("feat: add auth")) {
        CmdResult::Ok(d) => {
            assert_eq!(d.url, "https://github.com/org/repo/pull/42");
            assert_eq!(d.title, "feat: add auth");
            assert_eq!(d.base_branch, "main");
        }
        _ => panic!("expected ok"),
    }
}

#[test]
fn on_default_branch_fails() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "main".to_string());
    git.detect_default_branch = Box::new(|| Ok("main".to_string()));
    let jira = MockJira::default();
    let github = MockGitHub::default();
    let deps = PrDeps {
        git: &git,
        jira: &jira,
        github: &github,
    };
    match run(&deps, &input("test pr")) {
        CmdResult::Err(e) => assert!(e.contains("Cannot create PR from default branch")),
        _ => panic!("expected err"),
    }
}

#[test]
fn not_authenticated_fails() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "feat/something".to_string());
    let jira = MockJira::default();
    let mut github = MockGitHub::default();
    github.is_authenticated = Box::new(|| false);
    let deps = PrDeps {
        git: &git,
        jira: &jira,
        github: &github,
    };
    match run(&deps, &input("test pr")) {
        CmdResult::Err(e) => {
            assert!(e.contains("not authenticated"));
            assert!(e.contains("gh auth login"));
        }
        _ => panic!("expected err"),
    }
}

#[test]
fn push_failure_propagates() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "feat/something".to_string());
    git.push = Box::new(|_, _| Err("push rejected".to_string()));
    let jira = MockJira::default();
    let github = MockGitHub::default();
    let deps = PrDeps {
        git: &git,
        jira: &jira,
        github: &github,
    };
    match run(&deps, &input("test pr")) {
        CmdResult::Err(e) => assert_eq!(e, "push rejected"),
        _ => panic!("expected err"),
    }
}

#[test]
fn create_pr_failure_propagates() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "feat/something".to_string());
    let jira = MockJira::default();
    let mut github = MockGitHub::default();
    github.create_pr = Box::new(|_| Err("PR already exists".to_string()));
    let deps = PrDeps {
        git: &git,
        jira: &jira,
        github: &github,
    };
    match run(&deps, &input("test pr")) {
        CmdResult::Err(e) => assert_eq!(e, "PR already exists"),
        _ => panic!("expected err"),
    }
}

#[test]
fn empty_title_fails() {
    let git = MockGit::default();
    let jira = MockJira::default();
    let github = MockGitHub::default();
    let deps = PrDeps {
        git: &git,
        jira: &jira,
        github: &github,
    };
    match run(&deps, &input("")) {
        CmdResult::Err(e) => assert!(e.contains("Title is required")),
        _ => panic!("expected err"),
    }
}
