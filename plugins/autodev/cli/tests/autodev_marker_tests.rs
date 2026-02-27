use std::collections::HashMap;
use std::path::Path;

use autodev::components::merger::Merger;
use autodev::components::notifier::Notifier;
use autodev::components::reviewer::Reviewer;
use autodev::components::workspace::Workspace;
use autodev::config::Env;
use autodev::domain::repository::*;
use autodev::infrastructure::agent::mock::MockAgent;
use autodev::infrastructure::gh::mock::MockGh;
use autodev::infrastructure::git::mock::MockGit;
use autodev::infrastructure::suggest_workflow::mock::MockSuggestWorkflow;
use autodev::queue::task_queues::{
    issue_phase, make_work_id, pr_phase, IssueItem, PrItem, TaskQueues,
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
        gh_host: None,
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
        source_issue_number: None,
        review_iteration: 0,
        gh_host: None,
    }
}

fn set_gh_issue_open(gh: &MockGh, repo_name: &str, number: i64) {
    gh.set_field(repo_name, &format!("issues/{number}"), ".state", "open");
}

fn set_gh_pr_open(gh: &MockGh, repo_name: &str, number: i64) {
    gh.set_field(repo_name, &format!("pulls/{number}"), ".state", "open");
    gh.set_field(
        repo_name,
        &format!("pulls/{number}/reviews"),
        r#"[.[] | select(.state == "APPROVED")] | length"#,
        "0",
    );
}

fn make_analysis_json(verdict: &str, confidence: f64) -> String {
    let inner = format!(
        r##"{{"verdict":"{verdict}","confidence":{confidence},"summary":"Test summary","questions":[],"reason":null,"report":"Analysis Report"}}"##,
    );
    serde_json::json!({ "result": inner }).to_string()
}

// ═══════════════════════════════════════════════
// 1. Issue analysis: [autodev] analyze 마커
// ═══════════════════════════════════════════════

#[tokio::test]
async fn issue_analysis_prompt_contains_autodev_analyze_marker() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 100);

    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(&make_analysis_json("implement", 0.9), 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();
    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 100, "Test issue"),
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
    .unwrap();

    let calls = claude.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let prompt = &calls[0].prompt;
    assert!(
        prompt.starts_with("[autodev] analyze: issue #100"),
        "analysis prompt should start with [autodev] analyze marker, got: {}",
        &prompt[..prompt.len().min(60)]
    );
}

// ═══════════════════════════════════════════════
// 2. Issue implementation: [autodev] implement 마커
// ═══════════════════════════════════════════════

#[tokio::test]
async fn issue_implement_prompt_contains_autodev_implement_marker() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockAgent::new();
    // Phase 2 implementation response
    claude.enqueue_response(r#"{"result": "Done"}"#, 0);
    // Knowledge extraction response
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    let mut item = make_issue_item(&repo_id, 101, "Implement issue");
    item.analysis_report = Some("Test analysis report".to_string());
    queues.issues.push(issue_phase::READY, item);

    let sw = MockSuggestWorkflow::new();
    autodev::pipeline::issue::process_ready(
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
    .unwrap();

    let calls = claude.calls.lock().unwrap();
    assert!(calls.len() >= 1);
    let prompt = &calls[0].prompt;
    assert!(
        prompt.starts_with("[autodev] implement: issue #101"),
        "implement prompt should start with [autodev] implement marker, got: {}",
        &prompt[..prompt.len().min(60)]
    );
}

// ═══════════════════════════════════════════════
// 3. PR review: [autodev] review 마커
// ═══════════════════════════════════════════════

#[tokio::test]
async fn pr_review_prompt_contains_autodev_review_marker() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_pr_open(&gh, "org/repo", 200);

    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(r#"{"result": "LGTM - no issues"}"#, 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();
    queues
        .prs
        .push("Pending", make_pr_item(&repo_id, 200, "Test PR"));

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
    .unwrap();

    let calls = claude.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let prompt = &calls[0].prompt;
    assert!(
        prompt.starts_with("[autodev] review: PR #200"),
        "review prompt should start with [autodev] review marker, got: {}",
        &prompt[..prompt.len().min(60)]
    );
}

// ═══════════════════════════════════════════════
// 4. PR improve: [autodev] improve 마커
// ═══════════════════════════════════════════════

#[tokio::test]
async fn pr_improve_prompt_contains_autodev_improve_marker() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(r#"{"result": "Feedback applied"}"#, 0);

    let workspace = Workspace::new(&git, &env);
    let mut queues = TaskQueues::new();

    let mut item = make_pr_item(&repo_id, 201, "PR with review feedback");
    item.review_comment = Some("Fix the null check on line 42".to_string());
    queues.prs.push(pr_phase::REVIEW_DONE, item);

    autodev::pipeline::pr::process_review_done(&db, &env, &workspace, &gh, &claude, &mut queues)
        .await
        .unwrap();

    let calls = claude.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let prompt = &calls[0].prompt;
    assert!(
        prompt.starts_with("[autodev] improve: PR #201"),
        "improve prompt should start with [autodev] improve marker, got: {}",
        &prompt[..prompt.len().min(60)]
    );
}

// ═══════════════════════════════════════════════
// 5. PR re-review: [autodev] review 마커 (재리뷰)
// ═══════════════════════════════════════════════

#[tokio::test]
async fn pr_re_review_prompt_contains_autodev_review_marker() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockAgent::new();
    // Re-review: approved
    claude.enqueue_response(r#"{"result": "Approved"}"#, 0);
    // Knowledge extraction
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues.prs.push(
        pr_phase::IMPROVED,
        make_pr_item(&repo_id, 202, "Improved PR"),
    );

    let sw = MockSuggestWorkflow::new();
    autodev::pipeline::pr::process_improved(
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
    .unwrap();

    let calls = claude.calls.lock().unwrap();
    assert!(calls.len() >= 1);
    let prompt = &calls[0].prompt;
    assert!(
        prompt.starts_with("[autodev] review: PR #202"),
        "re-review prompt should start with [autodev] review marker, got: {}",
        &prompt[..prompt.len().min(60)]
    );
}

// ═══════════════════════════════════════════════
// 6. Merger: [autodev] merge 마커
// ═══════════════════════════════════════════════

#[tokio::test]
async fn merger_prompt_contains_autodev_merge_marker() {
    let claude = MockAgent::new();
    claude.enqueue_response("Merged successfully", 0);

    let merger = Merger::new(&claude);
    let _output = merger.merge_pr(Path::new("/tmp/test"), 42).await;

    let calls = claude.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let prompt = &calls[0].prompt;
    assert!(
        prompt.starts_with("[autodev] merge: PR #42"),
        "merge prompt should start with [autodev] merge marker, got: {prompt}"
    );
    assert!(
        prompt.contains("/git-utils:merge-pr 42"),
        "merge prompt should contain merge command"
    );
}

// ═══════════════════════════════════════════════
// 7. Merger resolve: [autodev] resolve 마커
// ═══════════════════════════════════════════════

#[tokio::test]
async fn merger_resolve_prompt_contains_autodev_resolve_marker() {
    let claude = MockAgent::new();
    claude.enqueue_response("Conflicts resolved", 0);

    let merger = Merger::new(&claude);
    let _output = merger.resolve_conflicts(Path::new("/tmp/test"), 42).await;

    let calls = claude.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let prompt = &calls[0].prompt;
    assert!(
        prompt.starts_with("[autodev] resolve: PR #42"),
        "resolve prompt should start with [autodev] resolve marker, got: {prompt}"
    );
}

// ═══════════════════════════════════════════════
// 8. Knowledge per-task: [autodev] knowledge 마커
// ═══════════════════════════════════════════════

#[tokio::test]
async fn knowledge_per_task_prompt_contains_autodev_knowledge_marker() {
    let claude = MockAgent::new();
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let gh = MockGh::new();

    let git = MockGit::new();
    let sw = MockSuggestWorkflow::new();
    let tmp = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmp);
    let workspace = Workspace::new(&git, &env);
    let _ = autodev::knowledge::extractor::extract_task_knowledge(
        &claude,
        &gh,
        &workspace,
        &sw,
        "org/repo",
        42,
        "issue",
        tmp.path(),
        None,
    )
    .await;

    let calls = claude.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let prompt = &calls[0].prompt;
    assert!(
        prompt.starts_with("[autodev] knowledge: per-task issue #42"),
        "per-task knowledge prompt should start with [autodev] knowledge marker, got: {}",
        &prompt[..prompt.len().min(60)]
    );
}

// ═══════════════════════════════════════════════
// 9. Knowledge daily: [autodev] knowledge: daily 마커
// ═══════════════════════════════════════════════

#[tokio::test]
async fn knowledge_daily_prompt_contains_autodev_knowledge_daily_marker() {
    let claude = MockAgent::new();
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let report = autodev::knowledge::models::DailyReport {
        date: "2026-02-22".into(),
        summary: autodev::knowledge::models::DailySummary {
            issues_done: 1,
            prs_done: 0,
            failed: 0,
            skipped: 0,
            avg_duration_ms: 1000,
        },
        patterns: vec![],
        suggestions: vec![],
        cross_analysis: None,
    };

    let _ = autodev::knowledge::daily::generate_daily_suggestions(
        &claude,
        &report,
        Path::new("/tmp/test"),
    )
    .await;

    let calls = claude.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let prompt = &calls[0].prompt;
    assert!(
        prompt.starts_with("[autodev] knowledge: daily 2026-02-22"),
        "daily knowledge prompt should start with [autodev] knowledge: daily marker, got: {}",
        &prompt[..prompt.len().min(60)]
    );
}

// ═══════════════════════════════════════════════
// 10. Reviewer: json output_format 전달 확인
// ═══════════════════════════════════════════════

#[tokio::test]
async fn reviewer_passes_json_output_format() {
    let claude = MockAgent::new();
    claude.enqueue_response(r#"{"result": "LGTM"}"#, 0);

    let reviewer = Reviewer::new(&claude);
    let _ = reviewer
        .review_pr(
            Path::new("/tmp/test"),
            "[autodev] review: PR #99\n\nReview this PR",
            None,
        )
        .await;

    let calls = claude.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].output_format.as_deref(),
        Some("json"),
        "reviewer should pass json output_format"
    );
}
