//! Mock-based port of git-utils `tests/commands/pr.test.ts`.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::{bail, Result};

use atelier::git::commands::pr::PrCommand;
use atelier::git::core::git::{BranchLocation, GitService, GitSpecialState};
use atelier::git::core::github::GitHubService;
use atelier::git::types::{PrInput, ReviewsOutput};

type Log = Rc<RefCell<Vec<String>>>;

struct MockGit {
    current_branch: String,
    default_branch: String,
    push_fails: bool,
    log: Log,
}

impl GitService for MockGit {
    fn detect_default_branch(&self) -> Result<String> {
        Ok(self.default_branch.clone())
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
    fn commit(&self, _message: &str) -> Result<()> {
        Ok(())
    }
    fn push(&self, _branch: &str, _set_upstream: bool) -> Result<()> {
        self.log.borrow_mut().push("push".to_string());
        if self.push_fails {
            bail!("push rejected");
        }
        Ok(())
    }
    fn pull(&self, _branch: &str) -> Result<()> {
        Ok(())
    }
    fn add_tracked(&self) -> Result<()> {
        Ok(())
    }
}

struct MockGitHub {
    authenticated: bool,
    create_fails: bool,
    url: String,
    last_body: RefCell<Option<String>>,
    log: Log,
}

impl GitHubService for MockGitHub {
    fn is_authenticated(&self) -> Result<bool> {
        Ok(self.authenticated)
    }
    fn create_pr(&self, _base: &str, _title: &str, body: &str) -> Result<String> {
        self.log.borrow_mut().push("createPr".to_string());
        *self.last_body.borrow_mut() = Some(body.to_string());
        if self.create_fails {
            bail!("PR already exists");
        }
        Ok(self.url.clone())
    }
    fn get_review_threads(&self, _pr_number: i64) -> Result<ReviewsOutput> {
        unreachable!()
    }
    fn detect_current_pr_number(&self) -> Result<Option<i64>> {
        Ok(None)
    }
}

struct Mocks {
    git: MockGit,
    github: MockGitHub,
    log: Log,
}

fn mocks() -> Mocks {
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    Mocks {
        git: MockGit {
            current_branch: "feat/something".to_string(),
            default_branch: "main".to_string(),
            push_fails: false,
            log: log.clone(),
        },
        github: MockGitHub {
            authenticated: true,
            create_fails: false,
            url: "https://github.com/org/repo/pull/1".to_string(),
            last_body: RefCell::new(None),
            log: log.clone(),
        },
        log,
    }
}

fn pr(title: &str, description: Option<&str>) -> PrInput {
    PrInput {
        title: title.to_string(),
        description: description.map(str::to_string),
    }
}

// ---------- title formatting ----------

#[test]
fn jira_ticket_title() {
    let mut m = mocks();
    m.git.current_branch = "feat/WAD-0212".to_string();
    let r = PrCommand::new(&m.git, &m.github)
        .run(&pr("add login feature", None))
        .unwrap();
    assert_eq!(r.title, "[WAD-0212] add login feature");
}

#[test]
fn no_ticket_title_unchanged() {
    let m = mocks();
    let r = PrCommand::new(&m.git, &m.github)
        .run(&pr("add login feature", None))
        .unwrap();
    assert_eq!(r.title, "add login feature");
}

// ---------- happy path ----------

#[test]
fn push_then_create_order() {
    let m = mocks();
    PrCommand::new(&m.git, &m.github)
        .run(&pr("test pr", None))
        .unwrap();
    assert_eq!(*m.log.borrow(), vec!["push", "createPr"]);
}

#[test]
fn description_passed_as_body() {
    let m = mocks();
    PrCommand::new(&m.git, &m.github)
        .run(&pr("test pr", Some("detailed description")))
        .unwrap();
    assert_eq!(
        m.github.last_body.borrow().as_deref(),
        Some("detailed description")
    );
}

#[test]
fn empty_body_when_no_description() {
    let m = mocks();
    PrCommand::new(&m.git, &m.github)
        .run(&pr("test pr", None))
        .unwrap();
    assert_eq!(m.github.last_body.borrow().as_deref(), Some(""));
}

#[test]
fn output_has_url_title_base() {
    let mut m = mocks();
    m.github.url = "https://github.com/org/repo/pull/42".to_string();
    let r = PrCommand::new(&m.git, &m.github)
        .run(&pr("feat: add auth", None))
        .unwrap();
    assert_eq!(r.url, "https://github.com/org/repo/pull/42");
    assert_eq!(r.title, "feat: add auth");
    assert_eq!(r.base_branch, "main");
}

// ---------- precondition validation ----------

#[test]
fn on_default_branch_fails() {
    let mut m = mocks();
    m.git.current_branch = "main".to_string();
    let err = PrCommand::new(&m.git, &m.github)
        .run(&pr("test pr", None))
        .unwrap_err();
    assert!(err.contains("Cannot create PR from default branch"));
}

#[test]
fn unauthenticated_fails() {
    let mut m = mocks();
    m.github.authenticated = false;
    let err = PrCommand::new(&m.git, &m.github)
        .run(&pr("test pr", None))
        .unwrap_err();
    assert!(err.contains("not authenticated"));
    assert!(err.contains("gh auth login"));
}

// ---------- error handling ----------

#[test]
fn push_failure_propagates() {
    let mut m = mocks();
    m.git.push_fails = true;
    let err = PrCommand::new(&m.git, &m.github)
        .run(&pr("test pr", None))
        .unwrap_err();
    assert_eq!(err, "push rejected");
}

#[test]
fn create_failure_propagates() {
    let mut m = mocks();
    m.github.create_fails = true;
    let err = PrCommand::new(&m.git, &m.github)
        .run(&pr("test pr", None))
        .unwrap_err();
    assert_eq!(err, "PR already exists");
}

#[test]
fn empty_title_fails() {
    let m = mocks();
    let err = PrCommand::new(&m.git, &m.github)
        .run(&pr("", None))
        .unwrap_err();
    assert!(err.contains("Title is required"));
}
