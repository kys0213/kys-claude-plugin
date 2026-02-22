use std::collections::HashMap;
use std::path::Path;

use autodev::components::notifier::Notifier;
use autodev::components::workspace::Workspace;
use autodev::config::Env;
use autodev::infrastructure::claude::MockClaude;
use autodev::infrastructure::gh::MockGh;
use autodev::infrastructure::git::MockGit;
use autodev::queue::models::*;
use autodev::queue::repository::*;
use autodev::queue::Database;

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

fn open_memory_db() -> Database {
    let db = Database::open(Path::new(":memory:")).expect("open in-memory db");
    db.initialize().expect("initialize schema");
    db
}

fn add_repo(db: &Database, url: &str, name: &str) -> String {
    db.repo_add(url, name).expect("add repo")
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

/// MockGh에 "이슈/PR이 open" 응답 설정
fn set_gh_open(gh: &MockGh, repo_name: &str, number: i64, kind: &str) {
    let path = format!("{kind}/{number}");
    gh.set_field(repo_name, &path, ".state", "open");
}

// ═══════════════════════════════════════════════
// 1. Issue pipeline — success (analysis → ready)
// ═══════════════════════════════════════════════

#[tokio::test]
async fn issue_pipeline_success_transitions_to_ready() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 42, "Bug: test issue");

    let gh = MockGh::new();
    set_gh_open(&gh, "org/repo", 42, "issues");

    let git = MockGit::new();
    let claude = MockClaude::new();

    // analysis 결과 JSON
    let analysis_json = serde_json::json!({
        "result": serde_json::json!({
            "verdict": "implement",
            "confidence": 0.9,
            "summary": "Clear bug to fix",
            "questions": [],
            "reason": null,
            "report": "## Analysis\nBug in login page."
        }).to_string()
    });
    claude.enqueue_response(&analysis_json.to_string(), 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::issue::process_pending(
        &db, &env, &workspace, &notifier, &claude, &mut active,
    )
    .await
    .expect("process_pending should succeed");

    // pending → ready (analysis 성공)
    let pending = db.issue_find_pending(100).unwrap();
    assert!(pending.is_empty(), "no issues should remain pending");

    let ready = db.issue_find_ready(100).unwrap();
    assert_eq!(ready.len(), 1, "issue should be in ready state");

    let logs = db.log_recent(None, 100).unwrap();
    assert!(!logs.is_empty(), "consumer should have written logs");
}

// ═══════════════════════════════════════════════
// 2. Issue pipeline — claude failure
// ═══════════════════════════════════════════════

#[tokio::test]
async fn issue_pipeline_claude_failure_marks_failed() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 99, "Will fail");

    let gh = MockGh::new();
    set_gh_open(&gh, "org/repo", 99, "issues");

    let git = MockGit::new();
    let claude = MockClaude::new();
    claude.enqueue_response("error output", 1); // exit code 1

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::issue::process_pending(
        &db, &env, &workspace, &notifier, &claude, &mut active,
    )
    .await
    .expect("should handle failure gracefully");

    let pending = db.issue_find_pending(100).unwrap();
    assert!(pending.is_empty());

    let list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, "failed");
}

// ═══════════════════════════════════════════════
// 3. PR pipeline — success
// ═══════════════════════════════════════════════

#[tokio::test]
async fn pr_pipeline_success_transitions_to_review_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_pr(&db, &repo_id, 100, "feat: add settings");

    let gh = MockGh::new();
    set_gh_open(&gh, "org/repo", 100, "pulls");

    let git = MockGit::new();
    let claude = MockClaude::new();
    claude.enqueue_response(r#"{"result": "LGTM"}"#, 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::pr::process_pending(
        &db, &env, &workspace, &notifier, &claude, &mut active,
    )
    .await
    .expect("process_pending should succeed");

    let pending = db.pr_find_pending(100).unwrap();
    assert!(pending.is_empty());

    let list = db.pr_list("org/repo", 100).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, "review_done");
}

// ═══════════════════════════════════════════════
// 4. PR pipeline — failure
// ═══════════════════════════════════════════════

#[tokio::test]
async fn pr_pipeline_claude_failure_marks_failed() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_pr(&db, &repo_id, 200, "will fail");

    let gh = MockGh::new();
    set_gh_open(&gh, "org/repo", 200, "pulls");

    let git = MockGit::new();
    let claude = MockClaude::new();
    claude.enqueue_response("review error", 1);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::pr::process_pending(
        &db, &env, &workspace, &notifier, &claude, &mut active,
    )
    .await
    .expect("should handle failure");

    let list = db.pr_list("org/repo", 100).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, "failed");
}

// ═══════════════════════════════════════════════
// 5. Pipeline batch limit
// ═══════════════════════════════════════════════

#[tokio::test]
async fn issue_pipeline_processes_up_to_limit() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    for i in 1..=10 {
        insert_pending_issue(&db, &repo_id, i, &format!("Issue #{i}"));
    }

    let gh = MockGh::new();
    for i in 1..=10 {
        set_gh_open(&gh, "org/repo", i, "issues");
    }

    let git = MockGit::new();
    let claude = MockClaude::new();
    // only 1 response (default concurrency=1)
    claude.enqueue_response(r#"{"result": "{\"verdict\":\"implement\",\"confidence\":0.9,\"summary\":\"ok\",\"questions\":[],\"reason\":null,\"report\":\"report\"}"}"#, 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::issue::process_pending(
        &db, &env, &workspace, &notifier, &claude, &mut active,
    )
    .await
    .expect("should process batch");

    let remaining = db.issue_find_pending(100).unwrap();
    assert_eq!(remaining.len(), 9); // default concurrency=1 → 1 processed, 9 remain
}

// ═══════════════════════════════════════════════
// 6. process_all
// ═══════════════════════════════════════════════

#[tokio::test]
async fn process_all_handles_issues_and_prs() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    insert_pending_issue(&db, &repo_id, 1, "Issue");
    insert_pending_pr(&db, &repo_id, 10, "PR");

    let gh = MockGh::new();
    set_gh_open(&gh, "org/repo", 1, "issues");
    set_gh_open(&gh, "org/repo", 10, "pulls");

    let git = MockGit::new();
    let claude = MockClaude::new();
    // Issue analysis response
    claude.enqueue_response(r#"{"result": "{\"verdict\":\"implement\",\"confidence\":0.9,\"summary\":\"ok\",\"questions\":[],\"reason\":null,\"report\":\"report\"}"}"#, 0);
    // PR review response
    claude.enqueue_response(r#"{"result": "LGTM"}"#, 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::process_all(
        &db, &env, &workspace, &notifier, &claude, &mut active,
    )
    .await
    .expect("process_all should succeed");

    assert!(db.issue_find_pending(100).unwrap().is_empty());
    assert!(db.pr_find_pending(100).unwrap().is_empty());
}
