use std::path::Path;

use autodev::infrastructure::gh::MockGh;
use autodev::queue::repository::*;
use autodev::queue::task_queues::{issue_phase, pr_phase, TaskQueues};
use autodev::queue::Database;

// ─── Helpers ───

fn fixture_response(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/responses")
        .join(name);
    std::fs::read(path).expect("read fixture file")
}

fn open_memory_db() -> Database {
    let db = Database::open(Path::new(":memory:")).expect("open in-memory db");
    db.initialize().expect("initialize schema");
    db
}

fn add_repo(db: &Database, url: &str, name: &str) -> String {
    db.repo_add(url, name).expect("add repo")
}

fn mock_gh_with_fixture(repo_name: &str, endpoint: &str, fixture: &str) -> MockGh {
    let gh = MockGh::new();
    gh.set_paginate(repo_name, endpoint, fixture_response(fixture));
    gh
}

// ═══════════════════════════════════════════════
// 1. Issue scanner
// ═══════════════════════════════════════════════

#[tokio::test]
async fn scan_issues_queues_new_items() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "issues", "issues.json");

    let ignore = vec!["dependabot".to_string()];
    let mut queues = TaskQueues::new();
    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &ignore,
        &None,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    assert!(queues.contains("issue:org/repo:42")); // alice
    assert!(queues.contains("issue:org/repo:43")); // bob
    assert!(!queues.contains("issue:org/repo:44")); // PR-linked → skipped
    assert!(!queues.contains("issue:org/repo:45")); // dependabot → skipped

    // autodev:wip labels should have been added for queued items
    let added = gh.added_labels.lock().unwrap();
    assert!(added.iter().any(|(_, n, l)| *n == 42 && l == "autodev:wip"));
    assert!(added.iter().any(|(_, n, l)| *n == 43 && l == "autodev:wip"));
}

#[tokio::test]
async fn scan_issues_skips_pr_linked() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "issues", "issues.json");

    let mut queues = TaskQueues::new();
    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        &None,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    assert!(!queues.contains("issue:org/repo:44"));
}

#[tokio::test]
async fn scan_issues_filters_by_label() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "issues", "issues_with_labels.json");

    let labels = Some(vec!["bug".to_string()]);
    let mut queues = TaskQueues::new();
    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        &labels,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    assert!(queues.contains("issue:org/repo:50")); // has "bug"
    assert!(!queues.contains("issue:org/repo:51")); // only "enhancement"
}

#[tokio::test]
async fn scan_issues_skips_autodev_labeled() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "issues", "issues_with_autodev_labels.json");

    let mut queues = TaskQueues::new();
    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        &None,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    // #60 has "autodev" (not "autodev:*") — should be skipped because has_autodev_label
    //   checks for "autodev:" prefix; "autodev" alone does not match "autodev:"
    //   so #60 should actually be queued (no "autodev:" prefix label)
    // #61 has "autodev:wip" → skipped
    // #62 has "autodev:done" → skipped
    // #63 has "autodev:skip" → skipped
    // #64 has no autodev label → queued

    // "autodev" label on #60 does NOT start with "autodev:" so it passes the filter
    assert!(queues.contains("issue:org/repo:60"));
    assert!(!queues.contains("issue:org/repo:61")); // autodev:wip
    assert!(!queues.contains("issue:org/repo:62")); // autodev:done
    assert!(!queues.contains("issue:org/repo:63")); // autodev:skip
    assert!(queues.contains("issue:org/repo:64")); // no autodev label

    // autodev:wip labels should have been added only for queued items
    let added = gh.added_labels.lock().unwrap();
    assert!(added.iter().any(|(_, n, l)| *n == 60 && l == "autodev:wip"));
    assert!(added.iter().any(|(_, n, l)| *n == 64 && l == "autodev:wip"));
    assert!(!added.iter().any(|(_, n, _)| *n == 61));
    assert!(!added.iter().any(|(_, n, _)| *n == 62));
    assert!(!added.iter().any(|(_, n, _)| *n == 63));
}

#[tokio::test]
async fn scan_issues_no_duplicates() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "issues", "issues.json");

    let ignore = vec!["dependabot".to_string()];
    let mut queues = TaskQueues::new();

    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &ignore,
        &None,
        None,
        &mut queues,
    )
    .await
    .unwrap();
    let count_first = queues.issues.len(issue_phase::PENDING);

    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &ignore,
        &None,
        None,
        &mut queues,
    )
    .await
    .unwrap();
    let count_second = queues.issues.len(issue_phase::PENDING);

    assert_eq!(count_first, count_second);
}

#[tokio::test]
async fn scan_issues_updates_cursor() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "issues", "issues.json");

    assert!(db
        .cursor_get_last_seen(&repo_id, "issues")
        .unwrap()
        .is_none());

    let ignore = vec!["dependabot".to_string()];
    let mut queues = TaskQueues::new();
    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &ignore,
        &None,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    let cursor = db.cursor_get_last_seen(&repo_id, "issues").unwrap();
    assert!(cursor.is_some());
}

#[tokio::test]
async fn scan_issues_empty_response() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = MockGh::new();
    gh.set_paginate("org/repo", "issues", b"[]".to_vec());

    let mut queues = TaskQueues::new();
    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        &None,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    assert_eq!(queues.issues.len(issue_phase::PENDING), 0);
}

// ═══════════════════════════════════════════════
// 2. PR scanner
// ═══════════════════════════════════════════════

#[tokio::test]
async fn scan_prs_queues_new_items() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "pulls", "pulls.json");

    let ignore = vec!["renovate".to_string()];
    let mut queues = TaskQueues::new();
    autodev::scanner::pulls::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &ignore,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    assert!(queues.contains("pr:org/repo:100")); // alice
    assert!(!queues.contains("pr:org/repo:101")); // renovate

    // autodev:wip label should have been added for alice's PR
    let added = gh.added_labels.lock().unwrap();
    assert!(added.iter().any(|(_, n, l)| *n == 100 && l == "autodev:wip"));
}

#[tokio::test]
async fn scan_prs_no_duplicates() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "pulls", "pulls.json");

    let mut queues = TaskQueues::new();
    autodev::scanner::pulls::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        None,
        &mut queues,
    )
    .await
    .unwrap();
    let first = queues.prs.len(pr_phase::PENDING);

    autodev::scanner::pulls::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        None,
        &mut queues,
    )
    .await
    .unwrap();
    let second = queues.prs.len(pr_phase::PENDING);

    assert_eq!(first, second);
}

#[tokio::test]
async fn scan_prs_checks_pending_data() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "pulls", "pulls.json");

    let mut queues = TaskQueues::new();
    autodev::scanner::pulls::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        None,
        &mut queues,
    )
    .await
    .unwrap();

    // Pop from queue and check fields
    let alice_pr = queues.prs.pop(pr_phase::PENDING).unwrap();
    assert_eq!(alice_pr.github_number, 100);
    assert_eq!(alice_pr.head_branch, "feat/user-settings");
    assert_eq!(alice_pr.repo_name, "org/repo");
}

// ═══════════════════════════════════════════════
// 3. gh failure
// ═══════════════════════════════════════════════

#[tokio::test]
async fn scan_issues_gh_failure_returns_error() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    // MockGh에 응답을 설정하지 않으면 에러 반환
    let gh = MockGh::new();
    let mut queues = TaskQueues::new();

    let result = autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        &None,
        None,
        &mut queues,
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn scan_prs_gh_failure_returns_error() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    // MockGh에 응답을 설정하지 않으면 에러 반환
    let gh = MockGh::new();
    let mut queues = TaskQueues::new();

    let result = autodev::scanner::pulls::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        None,
        &mut queues,
    )
    .await;
    assert!(result.is_err());
}
