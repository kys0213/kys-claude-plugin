use std::path::{Path, PathBuf};

use autodev::queue::repository::*;
use autodev::queue::Database;
use serial_test::serial;

// ─── Helpers ───

fn fixtures_bin() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bin")
}

fn fixture_response(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/responses")
        .join(name)
}

fn path_with_fake_bin() -> String {
    let fake = fixtures_bin();
    let original = std::env::var("PATH").unwrap_or_default();
    format!("{}:{original}", fake.display())
}

fn open_memory_db() -> Database {
    let db = Database::open(Path::new(":memory:")).expect("open in-memory db");
    db.initialize().expect("initialize schema");
    db
}

fn add_repo(db: &Database, url: &str, name: &str) -> String {
    db.repo_add(url, name).expect("add repo")
}

fn setup_env(response_file: &str) {
    std::env::set_var("PATH", path_with_fake_bin());
    std::env::set_var("GH_MOCK_RESPONSE_FILE", fixture_response(response_file));
    std::env::remove_var("GH_MOCK_EXIT_CODE");
    std::env::remove_var("GH_MOCK_STDERR");
}

fn cleanup_env() {
    std::env::remove_var("GH_MOCK_RESPONSE_FILE");
    std::env::remove_var("GH_MOCK_EXIT_CODE");
    std::env::remove_var("GH_MOCK_STDERR");
}

// ═══════════════════════════════════════════════
// 1. Issue scanner
// ═══════════════════════════════════════════════

#[tokio::test]
#[serial]
async fn scan_issues_queues_new_items() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    setup_env("issues.json");

    let ignore = vec!["dependabot".to_string()];
    autodev::scanner::issues::scan(&db, &repo_id, "org/repo", &ignore, &None, None)
        .await
        .unwrap();

    assert!(db.issue_exists(&repo_id, 42).unwrap());  // alice
    assert!(db.issue_exists(&repo_id, 43).unwrap());  // bob
    assert!(!db.issue_exists(&repo_id, 44).unwrap()); // PR-linked → skipped
    assert!(!db.issue_exists(&repo_id, 45).unwrap()); // dependabot → skipped
    cleanup_env();
}

#[tokio::test]
#[serial]
async fn scan_issues_skips_pr_linked() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    setup_env("issues.json");

    autodev::scanner::issues::scan(&db, &repo_id, "org/repo", &[], &None, None)
        .await
        .unwrap();

    assert!(!db.issue_exists(&repo_id, 44).unwrap());
    cleanup_env();
}

#[tokio::test]
#[serial]
async fn scan_issues_filters_by_label() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    setup_env("issues_with_labels.json");

    let labels = Some(vec!["bug".to_string()]);
    autodev::scanner::issues::scan(&db, &repo_id, "org/repo", &[], &labels, None)
        .await
        .unwrap();

    assert!(db.issue_exists(&repo_id, 50).unwrap());  // has "bug"
    assert!(!db.issue_exists(&repo_id, 51).unwrap()); // only "enhancement"
    cleanup_env();
}

#[tokio::test]
#[serial]
async fn scan_issues_no_duplicates() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    setup_env("issues.json");

    let ignore = vec!["dependabot".to_string()];

    autodev::scanner::issues::scan(&db, &repo_id, "org/repo", &ignore, &None, None)
        .await
        .unwrap();
    let count_first = db.issue_find_pending(100).unwrap().len();

    autodev::scanner::issues::scan(&db, &repo_id, "org/repo", &ignore, &None, None)
        .await
        .unwrap();
    let count_second = db.issue_find_pending(100).unwrap().len();

    assert_eq!(count_first, count_second);
    cleanup_env();
}

#[tokio::test]
#[serial]
async fn scan_issues_updates_cursor() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    setup_env("issues.json");

    assert!(db.cursor_get_last_seen(&repo_id, "issues").unwrap().is_none());

    let ignore = vec!["dependabot".to_string()];
    autodev::scanner::issues::scan(&db, &repo_id, "org/repo", &ignore, &None, None)
        .await
        .unwrap();

    let cursor = db.cursor_get_last_seen(&repo_id, "issues").unwrap();
    assert!(cursor.is_some());
    cleanup_env();
}

#[tokio::test]
#[serial]
async fn scan_issues_empty_response() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    std::env::set_var("PATH", path_with_fake_bin());
    std::env::remove_var("GH_MOCK_RESPONSE_FILE");
    std::env::remove_var("GH_MOCK_EXIT_CODE");

    autodev::scanner::issues::scan(&db, &repo_id, "org/repo", &[], &None, None)
        .await
        .unwrap();

    assert!(db.issue_find_pending(100).unwrap().is_empty());
    cleanup_env();
}

// ═══════════════════════════════════════════════
// 2. PR scanner
// ═══════════════════════════════════════════════

#[tokio::test]
#[serial]
async fn scan_prs_queues_new_items() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    setup_env("pulls.json");

    let ignore = vec!["renovate".to_string()];
    autodev::scanner::pulls::scan(&db, &repo_id, "org/repo", &ignore, None)
        .await
        .unwrap();

    assert!(db.pr_exists(&repo_id, 100).unwrap());  // alice
    assert!(!db.pr_exists(&repo_id, 101).unwrap()); // renovate
    cleanup_env();
}

#[tokio::test]
#[serial]
async fn scan_prs_no_duplicates() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    setup_env("pulls.json");

    autodev::scanner::pulls::scan(&db, &repo_id, "org/repo", &[], None)
        .await
        .unwrap();
    let first = db.pr_find_pending(100).unwrap().len();

    autodev::scanner::pulls::scan(&db, &repo_id, "org/repo", &[], None)
        .await
        .unwrap();
    let second = db.pr_find_pending(100).unwrap().len();

    assert_eq!(first, second);
    cleanup_env();
}

#[tokio::test]
#[serial]
async fn scan_prs_checks_pending_data() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    setup_env("pulls.json");

    autodev::scanner::pulls::scan(&db, &repo_id, "org/repo", &[], None)
        .await
        .unwrap();

    let pending = db.pr_find_pending(100).unwrap();
    let alice_pr = pending.iter().find(|p| p.github_number == 100).unwrap();
    assert_eq!(alice_pr.head_branch, "feat/user-settings");
    assert_eq!(alice_pr.repo_name, "org/repo");
    cleanup_env();
}

// ═══════════════════════════════════════════════
// 3. gh failure
// ═══════════════════════════════════════════════

#[tokio::test]
#[serial]
async fn scan_issues_gh_failure_returns_error() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    std::env::set_var("PATH", path_with_fake_bin());
    std::env::set_var("GH_MOCK_EXIT_CODE", "1");

    let result = autodev::scanner::issues::scan(&db, &repo_id, "org/repo", &[], &None, None).await;
    assert!(result.is_err());
    cleanup_env();
}
