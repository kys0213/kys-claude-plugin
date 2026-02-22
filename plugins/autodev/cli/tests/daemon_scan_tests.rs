use std::path::{Path, PathBuf};

use autodev::infrastructure::gh::MockGh;
use autodev::queue::repository::*;
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
    let mut active = autodev::active::ActiveItems::new();
    autodev::scanner::issues::scan(&db, &gh, &repo_id, "org/repo", &ignore, &None, None, &mut active)
        .await
        .unwrap();

    assert!(db.issue_exists(&repo_id, 42).unwrap());  // alice
    assert!(db.issue_exists(&repo_id, 43).unwrap());  // bob
    assert!(!db.issue_exists(&repo_id, 44).unwrap()); // PR-linked → skipped
    assert!(!db.issue_exists(&repo_id, 45).unwrap()); // dependabot → skipped
}

#[tokio::test]
async fn scan_issues_skips_pr_linked() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "issues", "issues.json");

    let mut active = autodev::active::ActiveItems::new();
    autodev::scanner::issues::scan(&db, &gh, &repo_id, "org/repo", &[], &None, None, &mut active)
        .await
        .unwrap();

    assert!(!db.issue_exists(&repo_id, 44).unwrap());
}

#[tokio::test]
async fn scan_issues_filters_by_label() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "issues", "issues_with_labels.json");

    let labels = Some(vec!["bug".to_string()]);
    let mut active = autodev::active::ActiveItems::new();
    autodev::scanner::issues::scan(&db, &gh, &repo_id, "org/repo", &[], &labels, None, &mut active)
        .await
        .unwrap();

    assert!(db.issue_exists(&repo_id, 50).unwrap());  // has "bug"
    assert!(!db.issue_exists(&repo_id, 51).unwrap()); // only "enhancement"
}

#[tokio::test]
async fn scan_issues_no_duplicates() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "issues", "issues.json");

    let ignore = vec!["dependabot".to_string()];
    let mut active = autodev::active::ActiveItems::new();

    autodev::scanner::issues::scan(&db, &gh, &repo_id, "org/repo", &ignore, &None, None, &mut active)
        .await
        .unwrap();
    let count_first = db.issue_find_pending(100).unwrap().len();

    autodev::scanner::issues::scan(&db, &gh, &repo_id, "org/repo", &ignore, &None, None, &mut active)
        .await
        .unwrap();
    let count_second = db.issue_find_pending(100).unwrap().len();

    assert_eq!(count_first, count_second);
}

#[tokio::test]
async fn scan_issues_updates_cursor() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "issues", "issues.json");

    assert!(db.cursor_get_last_seen(&repo_id, "issues").unwrap().is_none());

    let ignore = vec!["dependabot".to_string()];
    let mut active = autodev::active::ActiveItems::new();
    autodev::scanner::issues::scan(&db, &gh, &repo_id, "org/repo", &ignore, &None, None, &mut active)
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

    let mut active = autodev::active::ActiveItems::new();
    autodev::scanner::issues::scan(&db, &gh, &repo_id, "org/repo", &[], &None, None, &mut active)
        .await
        .unwrap();

    assert!(db.issue_find_pending(100).unwrap().is_empty());
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
    let mut active = autodev::active::ActiveItems::new();
    autodev::scanner::pulls::scan(&db, &gh, &repo_id, "org/repo", &ignore, None, &mut active)
        .await
        .unwrap();

    assert!(db.pr_exists(&repo_id, 100).unwrap());  // alice
    assert!(!db.pr_exists(&repo_id, 101).unwrap()); // renovate
}

#[tokio::test]
async fn scan_prs_no_duplicates() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "pulls", "pulls.json");

    let mut active = autodev::active::ActiveItems::new();
    autodev::scanner::pulls::scan(&db, &gh, &repo_id, "org/repo", &[], None, &mut active)
        .await
        .unwrap();
    let first = db.pr_find_pending(100).unwrap().len();

    autodev::scanner::pulls::scan(&db, &gh, &repo_id, "org/repo", &[], None, &mut active)
        .await
        .unwrap();
    let second = db.pr_find_pending(100).unwrap().len();

    assert_eq!(first, second);
}

#[tokio::test]
async fn scan_prs_checks_pending_data() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let gh = mock_gh_with_fixture("org/repo", "pulls", "pulls.json");

    let mut active = autodev::active::ActiveItems::new();
    autodev::scanner::pulls::scan(&db, &gh, &repo_id, "org/repo", &[], None, &mut active)
        .await
        .unwrap();

    let pending = db.pr_find_pending(100).unwrap();
    let alice_pr = pending.iter().find(|p| p.github_number == 100).unwrap();
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
    let mut active = autodev::active::ActiveItems::new();

    let result = autodev::scanner::issues::scan(
        &db, &gh, &repo_id, "org/repo", &[], &None, None, &mut active,
    )
    .await;
    assert!(result.is_err());
}
