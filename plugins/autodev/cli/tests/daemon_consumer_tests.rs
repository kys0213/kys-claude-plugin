use std::collections::HashMap;
use std::path::Path;

use autodev::components::notifier::Notifier;
use autodev::components::workspace::Workspace;
use autodev::config::Env;
use autodev::infrastructure::claude::mock::MockClaude;
use autodev::infrastructure::gh::mock::MockGh;
use autodev::infrastructure::git::mock::MockGit;
use autodev::infrastructure::suggest_workflow::mock::MockSuggestWorkflow;
use autodev::queue::repository::*;
use autodev::queue::task_queues::{
    issue_phase, labels, make_work_id, pr_phase, IssueItem, MergeItem, PrItem, TaskQueues,
};
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

fn make_issue_item(repo_id: &str, number: i64, title: &str) -> IssueItem {
    IssueItem {
        work_id: make_work_id("issue", "org/repo", number),
        repo_id: repo_id.to_string(),
        repo_name: "org/repo".to_string(),
        repo_url: "https://github.com/org/repo".to_string(),
        github_number: number,
        title: title.to_string(),
        body: Some("Test issue body".to_string()),
        labels: vec!["bug".to_string()],
        author: "alice".to_string(),
        analysis_report: None,
    }
}

fn make_pr_item(repo_id: &str, number: i64, title: &str) -> PrItem {
    PrItem {
        work_id: make_work_id("pr", "org/repo", number),
        repo_id: repo_id.to_string(),
        repo_name: "org/repo".to_string(),
        repo_url: "https://github.com/org/repo".to_string(),
        github_number: number,
        title: title.to_string(),
        head_branch: "feat/test".to_string(),
        base_branch: "main".to_string(),
        review_comment: None,
    }
}

#[allow(dead_code)]
fn make_merge_item(repo_id: &str, pr_number: i64, title: &str) -> MergeItem {
    MergeItem {
        work_id: make_work_id("merge", "org/repo", pr_number),
        repo_id: repo_id.to_string(),
        repo_name: "org/repo".to_string(),
        repo_url: "https://github.com/org/repo".to_string(),
        pr_number,
        title: title.to_string(),
        head_branch: "feat/test".to_string(),
        base_branch: "main".to_string(),
    }
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
    let mut queues = TaskQueues::new();

    // Push issue to PENDING queue before processing
    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 42, "Bug: test issue"),
    );

    autodev::pipeline::issue::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &gh,
        &claude,
        &mut queues,
    )
    .await
    .expect("process_pending should succeed");

    // pending → ready (analysis 성공): item moved from PENDING to READY
    assert_eq!(
        queues.issues.len(issue_phase::PENDING),
        0,
        "no issues should remain pending"
    );
    assert_eq!(
        queues.issues.len(issue_phase::READY),
        1,
        "issue should be in ready state"
    );

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

    let gh = MockGh::new();
    set_gh_open(&gh, "org/repo", 99, "issues");

    let git = MockGit::new();
    let claude = MockClaude::new();
    claude.enqueue_response("error output", 1); // exit code 1

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    // Push issue to PENDING queue
    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 99, "Will fail"),
    );

    autodev::pipeline::issue::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &gh,
        &claude,
        &mut queues,
    )
    .await
    .expect("should handle failure gracefully");

    // Failed: removed from all queues + wip label removed
    assert_eq!(queues.issues.len(issue_phase::PENDING), 0);
    assert_eq!(queues.issues.len(issue_phase::READY), 0);
    assert_eq!(queues.total(), 0, "item should be removed from all queues");

    let removed = gh.removed_labels.lock().unwrap();
    assert!(
        removed
            .iter()
            .any(|(repo, num, label)| repo == "org/repo" && *num == 99 && label == labels::WIP),
        "wip label should be removed on failure"
    );
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

    let gh = MockGh::new();
    set_gh_open(&gh, "org/repo", 100, "pulls");
    // Set reviews endpoint to return 0 approved reviews (PR is reviewable)
    gh.set_field(
        "org/repo",
        "pulls/100/reviews",
        r#"[.[] | select(.state == "APPROVED")] | length"#,
        "0",
    );

    let git = MockGit::new();
    let claude = MockClaude::new();
    claude.enqueue_response(r#"{"result": "LGTM"}"#, 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    // Push PR to PENDING queue
    queues.prs.push(
        pr_phase::PENDING,
        make_pr_item(&repo_id, 100, "feat: add settings"),
    );

    let sw = MockSuggestWorkflow::new();
    autodev::pipeline::pr::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &gh,
        &claude,
        &sw,
        &mut queues,
    )
    .await
    .expect("process_pending should succeed");

    // Success: item moved from PENDING to REVIEW_DONE
    assert_eq!(queues.prs.len(pr_phase::PENDING), 0);
    assert_eq!(
        queues.prs.len(pr_phase::REVIEW_DONE),
        1,
        "PR should be in review_done state"
    );
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

    let gh = MockGh::new();
    set_gh_open(&gh, "org/repo", 200, "pulls");
    // Set reviews endpoint to return 0 approved reviews (PR is reviewable)
    gh.set_field(
        "org/repo",
        "pulls/200/reviews",
        r#"[.[] | select(.state == "APPROVED")] | length"#,
        "0",
    );

    let git = MockGit::new();
    let claude = MockClaude::new();
    claude.enqueue_response("review error", 1);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    // Push PR to PENDING queue
    queues
        .prs
        .push(pr_phase::PENDING, make_pr_item(&repo_id, 200, "will fail"));

    let sw = MockSuggestWorkflow::new();
    autodev::pipeline::pr::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &gh,
        &claude,
        &sw,
        &mut queues,
    )
    .await
    .expect("should handle failure");

    // Failed: removed from all queues + wip label removed
    assert_eq!(queues.prs.len(pr_phase::PENDING), 0);
    assert_eq!(queues.prs.len(pr_phase::REVIEW_DONE), 0);
    assert_eq!(queues.total(), 0, "item should be removed from all queues");

    let removed = gh.removed_labels.lock().unwrap();
    assert!(
        removed
            .iter()
            .any(|(repo, num, label)| repo == "org/repo" && *num == 200 && label == labels::WIP),
        "wip label should be removed on failure"
    );
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

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockClaude::new();

    let mut queues = TaskQueues::new();

    for i in 1..=10 {
        set_gh_open(&gh, "org/repo", i, "issues");
        queues.issues.push(
            issue_phase::PENDING,
            make_issue_item(&repo_id, i, &format!("Issue #{i}")),
        );
    }

    // only 1 response (default concurrency=1)
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"implement\",\"confidence\":0.9,\"summary\":\"ok\",\"questions\":[],\"reason\":null,\"report\":\"report\"}"}"#,
        0,
    );

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);

    autodev::pipeline::issue::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &gh,
        &claude,
        &mut queues,
    )
    .await
    .expect("should process batch");

    // default concurrency=1 → 1 processed (moved to READY), 9 remain in PENDING
    assert_eq!(queues.issues.len(issue_phase::PENDING), 9);
    assert_eq!(queues.issues.len(issue_phase::READY), 1);
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

    let gh = MockGh::new();
    set_gh_open(&gh, "org/repo", 1, "issues");
    set_gh_open(&gh, "org/repo", 10, "pulls");
    // Set reviews endpoint so PR is reviewable
    gh.set_field(
        "org/repo",
        "pulls/10/reviews",
        r#"[.[] | select(.state == "APPROVED")] | length"#,
        "0",
    );

    let git = MockGit::new();
    let claude = MockClaude::new();
    // Issue analysis response
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"implement\",\"confidence\":0.9,\"summary\":\"ok\",\"questions\":[],\"reason\":null,\"report\":\"report\"}"}"#,
        0,
    );
    // PR review response
    claude.enqueue_response(r#"{"result": "LGTM"}"#, 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    // Push issue and PR to their respective PENDING queues
    queues
        .issues
        .push(issue_phase::PENDING, make_issue_item(&repo_id, 1, "Issue"));
    queues
        .prs
        .push(pr_phase::PENDING, make_pr_item(&repo_id, 10, "PR"));

    let sw = MockSuggestWorkflow::new();
    autodev::pipeline::process_all(&db, &env, &workspace, &notifier, &gh, &claude, &sw, &mut queues)
        .await
        .expect("process_all should succeed");

    // Issue should have moved from PENDING through processing
    assert_eq!(
        queues.issues.len(issue_phase::PENDING),
        0,
        "no issues should remain pending"
    );
    // PR should have moved from PENDING through processing
    assert_eq!(
        queues.prs.len(pr_phase::PENDING),
        0,
        "no PRs should remain pending"
    );
}
