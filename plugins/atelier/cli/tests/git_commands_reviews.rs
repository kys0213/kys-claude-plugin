//! Port of `git-utils/tests/commands/reviews.test.ts`.
#![allow(clippy::field_reassign_with_default)]

mod git_mocks;

use atelier::git::commands::reviews::{run, ReviewsDeps};
use atelier::git::core::github::ReviewThreadsResult;
use atelier::git::types::{CmdResult, ReviewComment, ReviewThread, ReviewsInput};
use git_mocks::MockGitHub;
use std::cell::RefCell;
use std::rc::Rc;

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

fn input(pr: Option<i64>) -> ReviewsInput {
    ReviewsInput { pr_number: pr }
}

#[test]
fn explicit_pr_number_used() {
    let received = Rc::new(RefCell::new(0i64));
    let r = received.clone();
    let mut gh = MockGitHub::default();
    gh.get_review_threads = Box::new(move |n| {
        *r.borrow_mut() = n;
        Ok(ReviewThreadsResult {
            pr_title: "test".to_string(),
            pr_url: "url".to_string(),
            threads: vec![],
        })
    });
    let deps = ReviewsDeps { github: &gh };
    run(&deps, &input(Some(42)));
    assert_eq!(*received.borrow(), 42);
}

#[test]
fn auto_detect_pr_number() {
    let received = Rc::new(RefCell::new(0i64));
    let r = received.clone();
    let mut gh = MockGitHub::default();
    gh.detect_current_pr_number = Box::new(|| Ok(Some(99)));
    gh.get_review_threads = Box::new(move |n| {
        *r.borrow_mut() = n;
        Ok(ReviewThreadsResult {
            pr_title: "test".to_string(),
            pr_url: "url".to_string(),
            threads: vec![],
        })
    });
    let deps = ReviewsDeps { github: &gh };
    run(&deps, &input(None));
    assert_eq!(*received.borrow(), 99);
}

#[test]
fn auto_detect_failure_fails() {
    let mut gh = MockGitHub::default();
    gh.detect_current_pr_number = Box::new(|| Ok(None));
    let deps = ReviewsDeps { github: &gh };
    match run(&deps, &input(None)) {
        CmdResult::Err(e) => assert!(e.contains("No PR found")),
        _ => panic!("expected err"),
    }
}

#[test]
fn threads_returned() {
    let mut gh = MockGitHub::default();
    gh.get_review_threads = Box::new(|_| {
        Ok(ReviewThreadsResult {
            pr_title: "feat: add auth".to_string(),
            pr_url: "https://github.com/org/repo/pull/1".to_string(),
            threads: vec![sample_thread()],
        })
    });
    let deps = ReviewsDeps { github: &gh };
    match run(&deps, &input(Some(1))) {
        CmdResult::Ok(d) => {
            assert_eq!(d.threads.len(), 1);
            assert_eq!(d.threads[0], sample_thread());
        }
        _ => panic!("expected ok"),
    }
}

#[test]
fn empty_threads() {
    let mut gh = MockGitHub::default();
    gh.get_review_threads = Box::new(|_| {
        Ok(ReviewThreadsResult {
            pr_title: "feat: add auth".to_string(),
            pr_url: "https://github.com/org/repo/pull/1".to_string(),
            threads: vec![],
        })
    });
    let deps = ReviewsDeps { github: &gh };
    match run(&deps, &input(Some(1))) {
        CmdResult::Ok(d) => assert!(d.threads.is_empty()),
        _ => panic!("expected ok"),
    }
}

#[test]
fn output_has_pr_title_and_url() {
    let mut gh = MockGitHub::default();
    gh.get_review_threads = Box::new(|_| {
        Ok(ReviewThreadsResult {
            pr_title: "feat: add auth".to_string(),
            pr_url: "https://github.com/org/repo/pull/1".to_string(),
            threads: vec![],
        })
    });
    let deps = ReviewsDeps { github: &gh };
    match run(&deps, &input(Some(1))) {
        CmdResult::Ok(d) => {
            assert_eq!(d.pr_title, "feat: add auth");
            assert_eq!(d.pr_url, "https://github.com/org/repo/pull/1");
        }
        _ => panic!("expected ok"),
    }
}

#[test]
fn thread_field_mapping() {
    let mut gh = MockGitHub::default();
    gh.get_review_threads = Box::new(|_| {
        Ok(ReviewThreadsResult {
            pr_title: "test".to_string(),
            pr_url: "url".to_string(),
            threads: vec![sample_thread(), resolved_thread()],
        })
    });
    let deps = ReviewsDeps { github: &gh };
    match run(&deps, &input(Some(1))) {
        CmdResult::Ok(d) => {
            assert!(!d.threads[0].is_resolved);
            assert!(!d.threads[0].is_outdated);
            assert!(d.threads[1].is_resolved);
            assert!(d.threads[1].is_outdated);
            assert_eq!(d.threads[0].path, "src/index.ts");
            assert_eq!(d.threads[0].line, 42);
            let c = &d.threads[0].comments[0];
            assert_eq!(c.author, "reviewer1");
            assert_eq!(c.body, "Please fix this.");
            assert_eq!(c.created_at, "2024-01-15T10:00:00Z");
            assert_eq!(c.url, "https://github.com/org/repo/pull/1#discussion_r1");
        }
        _ => panic!("expected ok"),
    }
}

#[test]
fn api_error_propagates() {
    let mut gh = MockGitHub::default();
    gh.get_review_threads = Box::new(|_| Err("API rate limit exceeded".to_string()));
    let deps = ReviewsDeps { github: &gh };
    match run(&deps, &input(Some(1))) {
        CmdResult::Err(e) => assert_eq!(e, "API rate limit exceeded"),
        _ => panic!("expected err"),
    }
}

#[test]
fn nonexistent_pr_propagates() {
    let mut gh = MockGitHub::default();
    gh.get_review_threads =
        Box::new(|_| Err("Could not resolve to a PullRequest with the number of 9999".to_string()));
    let deps = ReviewsDeps { github: &gh };
    match run(&deps, &input(Some(9999))) {
        CmdResult::Err(e) => assert!(e.contains("9999")),
        _ => panic!("expected err"),
    }
}
