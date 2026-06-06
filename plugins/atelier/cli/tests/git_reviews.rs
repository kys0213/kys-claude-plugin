//! Mock-based port of git-utils `tests/commands/reviews.test.ts`.

use std::cell::RefCell;

use anyhow::{bail, Result};

use atelier::git::commands::reviews::ReviewsCommand;
use atelier::git::core::github::GitHubService;
use atelier::git::types::{ReviewComment, ReviewThread, ReviewsInput, ReviewsOutput};

struct MockGitHub {
    detect: Option<i64>,
    threads: Result<ReviewsOutput, String>,
    received_pr: RefCell<Option<i64>>,
}

impl MockGitHub {
    fn new() -> Self {
        Self {
            detect: None,
            threads: Ok(ReviewsOutput {
                pr_title: "test".to_string(),
                pr_url: "https://github.com/org/repo/pull/1".to_string(),
                threads: vec![],
            }),
            received_pr: RefCell::new(None),
        }
    }
}

impl GitHubService for MockGitHub {
    fn is_authenticated(&self) -> Result<bool> {
        Ok(true)
    }
    fn create_pr(&self, _base: &str, _title: &str, _body: &str) -> Result<String> {
        unreachable!()
    }
    fn get_review_threads(&self, pr_number: i64) -> Result<ReviewsOutput> {
        *self.received_pr.borrow_mut() = Some(pr_number);
        match &self.threads {
            Ok(out) => Ok(out.clone()),
            Err(e) => bail!("{e}"),
        }
    }
    fn detect_current_pr_number(&self) -> Result<Option<i64>> {
        Ok(self.detect)
    }
}

fn sample_thread() -> ReviewThread {
    ReviewThread {
        is_resolved: false,
        is_outdated: false,
        path: "src/index.ts".to_string(),
        line: 42,
        comments: vec![ReviewComment {
            author: "reviewer1".to_string(),
            body: "Please fix this.".to_string(),
            created_at: "2024-01-15T10:00:00Z".to_string(),
            url: "https://github.com/org/repo/pull/1#discussion_r1".to_string(),
        }],
    }
}

fn resolved_thread() -> ReviewThread {
    ReviewThread {
        is_resolved: true,
        is_outdated: true,
        path: "src/utils.ts".to_string(),
        line: 10,
        comments: vec![ReviewComment {
            author: "reviewer2".to_string(),
            body: "Looks good now.".to_string(),
            created_at: "2024-01-16T12:00:00Z".to_string(),
            url: "https://github.com/org/repo/pull/1#discussion_r2".to_string(),
        }],
    }
}

fn out(threads: Vec<ReviewThread>) -> ReviewsOutput {
    ReviewsOutput {
        pr_title: "feat: add auth".to_string(),
        pr_url: "https://github.com/org/repo/pull/1".to_string(),
        threads,
    }
}

// ---------- PR number resolution ----------

#[test]
fn explicit_pr_number_used() {
    let gh = MockGitHub::new();
    ReviewsCommand::new(&gh)
        .run(&ReviewsInput {
            pr_number: Some(42),
        })
        .unwrap();
    assert_eq!(*gh.received_pr.borrow(), Some(42));
}

#[test]
fn detects_pr_number_when_absent() {
    let gh = MockGitHub {
        detect: Some(99),
        ..MockGitHub::new()
    };
    ReviewsCommand::new(&gh)
        .run(&ReviewsInput::default())
        .unwrap();
    assert_eq!(*gh.received_pr.borrow(), Some(99));
}

#[test]
fn detection_failure_errors() {
    let gh = MockGitHub::new(); // detect None
    let err = ReviewsCommand::new(&gh)
        .run(&ReviewsInput::default())
        .unwrap_err();
    assert!(err.contains("No PR found"));
}

// ---------- happy path ----------

#[test]
fn threads_present() {
    let gh = MockGitHub {
        threads: Ok(out(vec![sample_thread()])),
        ..MockGitHub::new()
    };
    let r = ReviewsCommand::new(&gh)
        .run(&ReviewsInput { pr_number: Some(1) })
        .unwrap();
    assert_eq!(r.threads.len(), 1);
    assert_eq!(r.threads[0], sample_thread());
}

#[test]
fn threads_empty() {
    let gh = MockGitHub {
        threads: Ok(out(vec![])),
        ..MockGitHub::new()
    };
    let r = ReviewsCommand::new(&gh)
        .run(&ReviewsInput { pr_number: Some(1) })
        .unwrap();
    assert!(r.threads.is_empty());
}

#[test]
fn output_has_title_and_url() {
    let gh = MockGitHub {
        threads: Ok(out(vec![])),
        ..MockGitHub::new()
    };
    let r = ReviewsCommand::new(&gh)
        .run(&ReviewsInput { pr_number: Some(1) })
        .unwrap();
    assert_eq!(r.pr_title, "feat: add auth");
    assert_eq!(r.pr_url, "https://github.com/org/repo/pull/1");
}

// ---------- thread data mapping ----------

#[test]
fn resolved_outdated_mapping() {
    let gh = MockGitHub {
        threads: Ok(out(vec![sample_thread(), resolved_thread()])),
        ..MockGitHub::new()
    };
    let r = ReviewsCommand::new(&gh)
        .run(&ReviewsInput { pr_number: Some(1) })
        .unwrap();
    assert!(!r.threads[0].is_resolved);
    assert!(!r.threads[0].is_outdated);
    assert!(r.threads[1].is_resolved);
    assert!(r.threads[1].is_outdated);
}

#[test]
fn path_line_mapping() {
    let gh = MockGitHub {
        threads: Ok(out(vec![sample_thread()])),
        ..MockGitHub::new()
    };
    let r = ReviewsCommand::new(&gh)
        .run(&ReviewsInput { pr_number: Some(1) })
        .unwrap();
    assert_eq!(r.threads[0].path, "src/index.ts");
    assert_eq!(r.threads[0].line, 42);
}

#[test]
fn comment_fields_mapping() {
    let gh = MockGitHub {
        threads: Ok(out(vec![sample_thread()])),
        ..MockGitHub::new()
    };
    let r = ReviewsCommand::new(&gh)
        .run(&ReviewsInput { pr_number: Some(1) })
        .unwrap();
    let c = &r.threads[0].comments[0];
    assert_eq!(c.author, "reviewer1");
    assert_eq!(c.body, "Please fix this.");
    assert_eq!(c.created_at, "2024-01-15T10:00:00Z");
    assert_eq!(c.url, "https://github.com/org/repo/pull/1#discussion_r1");
}

// ---------- error handling ----------

#[test]
fn api_failure_propagates() {
    let gh = MockGitHub {
        threads: Err("API rate limit exceeded".to_string()),
        ..MockGitHub::new()
    };
    let err = ReviewsCommand::new(&gh)
        .run(&ReviewsInput { pr_number: Some(1) })
        .unwrap_err();
    assert_eq!(err, "API rate limit exceeded");
}

#[test]
fn nonexistent_pr_errors() {
    let gh = MockGitHub {
        threads: Err("Could not resolve to a PullRequest with the number of 9999".to_string()),
        ..MockGitHub::new()
    };
    let err = ReviewsCommand::new(&gh)
        .run(&ReviewsInput {
            pr_number: Some(9999),
        })
        .unwrap_err();
    assert!(err.contains("9999"));
}
