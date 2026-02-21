use std::collections::HashMap;
use std::path::{Path, PathBuf};

use autodev::config::Env;
use autodev::queue::models::*;
use autodev::queue::repository::*;
use autodev::queue::Database;
use serial_test::serial;

// ─── TestEnv ───

struct TestEnv {
    vars: HashMap<String, String>,
}

impl TestEnv {
    fn new(tmpdir: &tempfile::TempDir) -> Self {
        let mut vars = HashMap::new();
        vars.insert(
            "AUTODEV_HOME".to_string(),
            tmpdir.path().to_str().unwrap().to_string(),
        );
        Self { vars }
    }
}

impl Env for TestEnv {
    fn var(&self, key: &str) -> Result<String, std::env::VarError> {
        self.vars
            .get(key)
            .cloned()
            .ok_or(std::env::VarError::NotPresent)
    }
}

// ─── Helpers ───

fn fixtures_bin() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bin")
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

fn setup_subprocess_env() {
    std::env::set_var("PATH", path_with_fake_bin());
    std::env::remove_var("CLAUDE_MOCK_EXIT_CODE");
    std::env::remove_var("CLAUDE_MOCK_STDERR");
    std::env::remove_var("GIT_MOCK_EXIT_CODE");
}

fn cleanup_env() {
    std::env::remove_var("CLAUDE_MOCK_EXIT_CODE");
    std::env::remove_var("CLAUDE_MOCK_STDERR");
    std::env::remove_var("GIT_MOCK_EXIT_CODE");
}

fn insert_pending_issue(db: &Database, repo_id: &str, number: i64, title: &str) -> String {
    let item = NewIssueItem {
        repo_id: repo_id.to_string(),
        github_number: number,
        title: title.to_string(),
        body: Some("Test issue body".to_string()),
        labels: r#"["bug"]"#.to_string(),
        author: "alice".to_string(),
    };
    db.issue_insert(&item).unwrap()
}

fn insert_pending_pr(db: &Database, repo_id: &str, number: i64, title: &str) -> String {
    let item = NewPrItem {
        repo_id: repo_id.to_string(),
        github_number: number,
        title: title.to_string(),
        body: Some("Test PR body".to_string()),
        author: "alice".to_string(),
        head_branch: "feat/test".to_string(),
        base_branch: "main".to_string(),
    };
    db.pr_insert(&item).unwrap()
}

// ═══════════════════════════════════════════════
// 1. Issue consumer — success
// ═══════════════════════════════════════════════

#[tokio::test]
#[serial]
async fn issue_consumer_success_transitions_to_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 42, "Bug: test issue");

    setup_subprocess_env();

    autodev::consumer::issue::process_pending(&db, &env)
        .await
        .expect("process_pending should succeed");

    let pending = db.issue_find_pending(100).unwrap();
    assert!(pending.is_empty(), "no issues should remain pending");

    let logs = db.log_recent(None, 100).unwrap();
    assert!(!logs.is_empty(), "consumer should have written logs");
    cleanup_env();
}

// ═══════════════════════════════════════════════
// 2. Issue consumer — claude failure
// ═══════════════════════════════════════════════

#[tokio::test]
#[serial]
async fn issue_consumer_claude_failure_marks_failed() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 99, "Will fail");

    setup_subprocess_env();
    std::env::set_var("CLAUDE_MOCK_EXIT_CODE", "1");

    autodev::consumer::issue::process_pending(&db, &env)
        .await
        .expect("should handle failure gracefully");

    let pending = db.issue_find_pending(100).unwrap();
    assert!(pending.is_empty());

    let list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, "failed");
    cleanup_env();
}

// ═══════════════════════════════════════════════
// 3. PR consumer — success
// ═══════════════════════════════════════════════

#[tokio::test]
#[serial]
async fn pr_consumer_success_transitions_to_review_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_pr(&db, &repo_id, 100, "feat: add settings");

    setup_subprocess_env();

    autodev::consumer::pr::process_pending(&db, &env)
        .await
        .expect("process_pending should succeed");

    let pending = db.pr_find_pending(100).unwrap();
    assert!(pending.is_empty());

    let list = db.pr_list("org/repo", 100).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, "review_done");
    cleanup_env();
}

// ═══════════════════════════════════════════════
// 4. PR consumer — failure
// ═══════════════════════════════════════════════

#[tokio::test]
#[serial]
async fn pr_consumer_claude_failure_marks_failed() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_pr(&db, &repo_id, 200, "will fail");

    setup_subprocess_env();
    std::env::set_var("CLAUDE_MOCK_EXIT_CODE", "1");

    autodev::consumer::pr::process_pending(&db, &env)
        .await
        .expect("should handle failure");

    let list = db.pr_list("org/repo", 100).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, "failed");
    cleanup_env();
}

// ═══════════════════════════════════════════════
// 5. Consumer batch limit
// ═══════════════════════════════════════════════

#[tokio::test]
#[serial]
async fn issue_consumer_processes_up_to_limit() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    for i in 1..=10 {
        insert_pending_issue(&db, &repo_id, i, &format!("Issue #{i}"));
    }

    setup_subprocess_env();

    // process_pending uses config issue_concurrency (default: 1)
    autodev::consumer::issue::process_pending(&db, &env)
        .await
        .expect("should process batch");

    let remaining = db.issue_find_pending(100).unwrap();
    assert_eq!(remaining.len(), 9); // default concurrency=1 → 1 processed, 9 remain
    cleanup_env();
}

// ═══════════════════════════════════════════════
// 6. process_all
// ═══════════════════════════════════════════════

#[tokio::test]
#[serial]
async fn process_all_handles_issues_and_prs() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    insert_pending_issue(&db, &repo_id, 1, "Issue");
    insert_pending_pr(&db, &repo_id, 10, "PR");

    setup_subprocess_env();

    autodev::consumer::process_all(&db, &env)
        .await
        .expect("process_all should succeed");

    assert!(db.issue_find_pending(100).unwrap().is_empty());
    assert!(db.pr_find_pending(100).unwrap().is_empty());
    cleanup_env();
}
