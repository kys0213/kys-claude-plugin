use std::collections::HashMap;
use std::path::Path;

use autodev::config::Env;
use autodev::domain::labels;
use autodev::domain::repository::{ConsumerLogRepository, RepoRepository};
use autodev::infrastructure::agent::mock::MockAgent;
use autodev::infrastructure::gh::mock::MockGh;
use autodev::infrastructure::git::mock::MockGit;
use autodev::infrastructure::suggest_workflow::mock::MockSuggestWorkflow;
use autodev::queue::task_queues::{
    issue_phase, make_work_id, merge_phase, pr_phase, IssueItem, MergeItem, PrItem, TaskQueues,
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
        body: Some("Test body".to_string()),
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

fn make_merge_item(repo_id: &str, pr_number: i64) -> MergeItem {
    MergeItem {
        work_id: make_work_id("merge", "org/repo", pr_number),
        repo_id: repo_id.to_string(),
        repo_name: "org/repo".to_string(),
        repo_url: "https://github.com/org/repo".to_string(),
        pr_number,
        title: "Merge PR".to_string(),
        head_branch: "feat/test".to_string(),
        base_branch: "main".to_string(),
        gh_host: None,
    }
}

fn set_gh_issue_open(gh: &MockGh, number: i64) {
    gh.set_field("org/repo", &format!("issues/{number}"), ".state", "open");
}

fn set_gh_pr_open(gh: &MockGh, number: i64) {
    gh.set_field("org/repo", &format!("pulls/{number}"), ".state", "open");
    gh.set_field(
        "org/repo",
        &format!("pulls/{number}/reviews"),
        r#"[.[] | select(.state == "APPROVED")] | length"#,
        "0",
    );
}

fn set_gh_pr_mergeable(gh: &MockGh, number: i64) {
    gh.set_field("org/repo", &format!("pulls/{number}"), ".state", "open");
    gh.set_field("org/repo", &format!("pulls/{number}"), ".mergeable", "true");
    gh.set_field(
        "org/repo",
        &format!("pulls/{number}/reviews"),
        r#"[.[] | select(.state == "APPROVED")] | length"#,
        "1",
    );
}

fn make_analysis_json(verdict: &str, confidence: f64) -> String {
    let inner = format!(
        r##"{{"verdict":"{verdict}","confidence":{confidence},"summary":"Test summary","questions":[],"reason":null,"report":"Analysis Report" }}"##,
    );
    serde_json::json!({ "result": inner }).to_string()
}

fn make_analysis_json_with_questions(
    verdict: &str,
    confidence: f64,
    questions: &[&str],
    reason: Option<&str>,
) -> String {
    let questions_json: Vec<String> = questions.iter().map(|q| format!("\"{q}\"")).collect();
    let reason_json = match reason {
        Some(r) => format!("\"{r}\""),
        None => "null".to_string(),
    };
    let inner = format!(
        r#"{{"verdict":"{verdict}","confidence":{confidence},"summary":"Test summary","questions":[{questions}],"reason":{reason},"report":"Analysis Report" }}"#,
        questions = questions_json.join(","),
        reason = reason_json,
    );
    serde_json::json!({ "result": inner }).to_string()
}

fn has_worktree_remove(git: &MockGit) -> bool {
    git.calls
        .lock()
        .unwrap()
        .iter()
        .any(|(m, _)| m == "worktree_remove")
}

// ─── Pipeline helpers: simulate daemon pop → working state → _one() → handle_task_output ───

async fn run_analyze(
    queues: &mut TaskQueues,
    db: &Database,
    env: &dyn Env,
    gh: &dyn Gh,
    git: &dyn Git,
    claude: &dyn Agent,
) {
    let item = queues.issues.pop(issue_phase::PENDING).unwrap();
    queues.issues.push(issue_phase::ANALYZING, item.clone());
    let output = autodev::pipeline::issue::analyze_one(item, env, gh, git, claude).await;
    autodev::pipeline::handle_task_output(queues, db, output);
}

async fn run_implement(
    queues: &mut TaskQueues,
    db: &Database,
    env: &dyn Env,
    gh: &dyn Gh,
    git: &dyn Git,
    claude: &dyn Agent,
) {
    let item = queues.issues.pop(issue_phase::READY).unwrap();
    queues.issues.push(issue_phase::IMPLEMENTING, item.clone());
    let output = autodev::pipeline::issue::implement_one(item, env, gh, git, claude).await;
    autodev::pipeline::handle_task_output(queues, db, output);
}

async fn run_review(
    queues: &mut TaskQueues,
    db: &Database,
    env: &dyn Env,
    gh: &dyn Gh,
    git: &dyn Git,
    claude: &dyn Agent,
    sw: &dyn SuggestWorkflow,
) {
    let item = queues.prs.pop(pr_phase::PENDING).unwrap();
    queues.prs.push(pr_phase::REVIEWING, item.clone());
    let output = autodev::pipeline::pr::review_one(item, env, gh, git, claude, sw).await;
    autodev::pipeline::handle_task_output(queues, db, output);
}

async fn run_improve(
    queues: &mut TaskQueues,
    db: &Database,
    env: &dyn Env,
    gh: &dyn Gh,
    git: &dyn Git,
    claude: &dyn Agent,
) {
    let item = queues.prs.pop(pr_phase::REVIEW_DONE).unwrap();
    queues.prs.push(pr_phase::IMPROVING, item.clone());
    let output = autodev::pipeline::pr::improve_one(item, env, gh, git, claude).await;
    autodev::pipeline::handle_task_output(queues, db, output);
}

async fn run_re_review(
    queues: &mut TaskQueues,
    db: &Database,
    env: &dyn Env,
    gh: &dyn Gh,
    git: &dyn Git,
    claude: &dyn Agent,
    sw: &dyn SuggestWorkflow,
) {
    let item = queues.prs.pop(pr_phase::IMPROVED).unwrap();
    queues.prs.push(pr_phase::REVIEWING, item.clone());
    let output = autodev::pipeline::pr::re_review_one(item, env, gh, git, claude, sw).await;
    autodev::pipeline::handle_task_output(queues, db, output);
}

async fn run_merge(
    queues: &mut TaskQueues,
    db: &Database,
    env: &dyn Env,
    gh: &dyn Gh,
    git: &dyn Git,
    claude: &dyn Agent,
) {
    let item = queues.merges.pop(merge_phase::PENDING).unwrap();
    queues.merges.push(merge_phase::MERGING, item.clone());
    let output = autodev::pipeline::merge::merge_one(item, env, gh, git, claude).await;
    autodev::pipeline::handle_task_output(queues, db, output);
}

// Trait imports for helper function signatures
use autodev::infrastructure::agent::Agent;
use autodev::infrastructure::gh::Gh;
use autodev::infrastructure::git::Git;
use autodev::infrastructure::suggest_workflow::SuggestWorkflow;

// ═══════════════════════════════════════════════════════════
// Issue Pipeline: analyze_one
// ═══════════════════════════════════════════════════════════

#[tokio::test]
async fn issue_analysis_success_exits_queue_with_analyzed_label() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 1, "Bug fix"),
    );

    let gh = MockGh::new();
    set_gh_issue_open(&gh, 1);

    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(&make_analysis_json("implement", 0.95), 0);

    run_analyze(&mut queues, &db, &env, &gh, &git, &claude).await;

    // v2: exits queue entirely (HITL gate)
    assert_eq!(queues.issues.total(), 0);

    // analyzed label added, wip removed
    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 1 && label == labels::ANALYZED));

    let removed = gh.removed_labels.lock().unwrap();
    assert!(removed
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 1 && label == labels::WIP));

    // analysis comment posted
    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.contains("<!-- autodev:analysis -->"));
}

#[tokio::test]
async fn issue_closed_on_github_skips_to_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 5, "Already closed"),
    );

    let gh = MockGh::new();
    gh.set_field("org/repo", "issues/5", ".state", "closed");

    let git = MockGit::new();
    let claude = MockAgent::new();

    run_analyze(&mut queues, &db, &env, &gh, &git, &claude).await;

    assert_eq!(queues.issues.total(), 0);

    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 5 && label == labels::DONE));

    assert_eq!(claude.call_count(), 0);
}

// ═══════════════════════════════════════════════════════════
// Issue Verdict Tests
// ═══════════════════════════════════════════════════════════

#[tokio::test]
async fn issue_verdict_wontfix_posts_comment_and_marks_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 1, "Won't fix issue"),
    );

    let gh = MockGh::new();
    set_gh_issue_open(&gh, 1);

    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(
        &make_analysis_json_with_questions("wontfix", 0.95, &[], Some("Duplicate of #42")),
        0,
    );

    run_analyze(&mut queues, &db, &env, &gh, &git, &claude).await;

    assert_eq!(queues.issues.total(), 0);

    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 1 && label == labels::SKIP));

    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.contains("Won't fix"));
    assert!(comments[0].2.contains("Duplicate of #42"));
}

#[tokio::test]
async fn issue_verdict_needs_clarification_posts_questions() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 2, "Ambiguous issue"),
    );

    let gh = MockGh::new();
    set_gh_issue_open(&gh, 2);

    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(
        &make_analysis_json_with_questions(
            "needs_clarification",
            0.8,
            &["What is the expected behavior?", "Which version?"],
            None,
        ),
        0,
    );

    run_analyze(&mut queues, &db, &env, &gh, &git, &claude).await;

    assert_eq!(queues.issues.total(), 0);

    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 2 && label == labels::SKIP));

    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.contains("What is the expected behavior?"));
    assert!(comments[0].2.contains("Which version?"));
}

#[tokio::test]
async fn issue_verdict_implement_low_confidence_goes_to_waiting() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 3, "Low confidence issue"),
    );

    let gh = MockGh::new();
    set_gh_issue_open(&gh, 3);

    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(&make_analysis_json("implement", 0.3), 0);

    run_analyze(&mut queues, &db, &env, &gh, &git, &claude).await;

    assert_eq!(queues.issues.total(), 0);

    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 3 && label == labels::SKIP));

    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.contains("clarification"));
}

#[tokio::test]
async fn issue_unparseable_analysis_falls_back_to_analyzed() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 6, "Parse fail issue"),
    );

    let gh = MockGh::new();
    set_gh_issue_open(&gh, 6);

    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response("This is not valid JSON at all", 0);

    run_analyze(&mut queues, &db, &env, &gh, &git, &claude).await;

    assert_eq!(queues.issues.total(), 0);

    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 6 && label == labels::ANALYZED));

    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.contains("<!-- autodev:analysis -->"));
}

// ═══════════════════════════════════════════════════════════
// PR Pipeline: review_one
// ═══════════════════════════════════════════════════════════

#[tokio::test]
async fn pr_closed_on_github_skips_to_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues
        .prs
        .push(pr_phase::PENDING, make_pr_item(&repo_id, 10, "Closed PR"));

    let gh = MockGh::new();
    gh.set_field("org/repo", "pulls/10", ".state", "closed");

    let git = MockGit::new();
    let claude = MockAgent::new();
    let sw = MockSuggestWorkflow::new();

    run_review(&mut queues, &db, &env, &gh, &git, &claude, &sw).await;

    assert_eq!(queues.prs.total(), 0);

    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 10 && label == labels::DONE));

    assert_eq!(claude.call_count(), 0);
}

#[tokio::test]
async fn pr_review_success_posts_comment() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues
        .prs
        .push(pr_phase::PENDING, make_pr_item(&repo_id, 20, "Good PR"));

    let gh = MockGh::new();
    set_gh_pr_open(&gh, 20);

    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(r#"{"result": "LGTM - no issues found"}"#, 0);

    let sw = MockSuggestWorkflow::new();

    run_review(&mut queues, &db, &env, &gh, &git, &claude, &sw).await;

    // PR exits queue (goes to done or review_done depending on verdict)
    assert_eq!(claude.call_count(), 1);
}

#[tokio::test]
async fn pr_review_claude_failure_removes_from_queue() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues
        .prs
        .push(pr_phase::PENDING, make_pr_item(&repo_id, 21, "Failing PR"));

    let gh = MockGh::new();
    set_gh_pr_open(&gh, 21);

    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response("review error", 1);

    let sw = MockSuggestWorkflow::new();

    run_review(&mut queues, &db, &env, &gh, &git, &claude, &sw).await;

    assert_eq!(queues.prs.total(), 0);

    let removed = gh.removed_labels.lock().unwrap();
    assert!(removed
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 21 && label == labels::WIP));
}

// ═══════════════════════════════════════════════════════════
// PR Pipeline: improve_one
// ═══════════════════════════════════════════════════════════

#[tokio::test]
async fn pr_improve_success_moves_to_improved() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    let mut item = make_pr_item(&repo_id, 30, "PR with feedback");
    item.review_comment = Some("Fix the null check".to_string());
    queues.prs.push(pr_phase::REVIEW_DONE, item);

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(r#"{"result": "Feedback applied"}"#, 0);

    run_improve(&mut queues, &db, &env, &gh, &git, &claude).await;

    assert_eq!(queues.prs.len(pr_phase::IMPROVED), 1);
}

#[tokio::test]
async fn pr_improve_failure_removes_from_queue() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    let mut item = make_pr_item(&repo_id, 31, "PR with feedback");
    item.review_comment = Some("Fix this".to_string());
    queues.prs.push(pr_phase::REVIEW_DONE, item);

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response("implementation error", 1);

    run_improve(&mut queues, &db, &env, &gh, &git, &claude).await;

    assert_eq!(queues.prs.total(), 0);
}

// ═══════════════════════════════════════════════════════════
// PR Pipeline: re_review_one
// ═══════════════════════════════════════════════════════════

#[tokio::test]
async fn pr_re_review_approved_exits_queue() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues.prs.push(
        pr_phase::IMPROVED,
        make_pr_item(&repo_id, 40, "Improved PR"),
    );

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"approve\",\"summary\":\"LGTM\"}"}"#,
        0,
    );
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let sw = MockSuggestWorkflow::new();

    run_re_review(&mut queues, &db, &env, &gh, &git, &claude, &sw).await;

    assert_eq!(queues.prs.total(), 0);

    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 40 && label == labels::DONE));
}

#[tokio::test]
async fn pr_re_review_request_changes_pushes_back_to_review_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues.prs.push(
        pr_phase::IMPROVED,
        make_pr_item(&repo_id, 41, "Needs more work"),
    );

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"request_changes\",\"summary\":\"Needs more work\"}"}"#,
        0,
    );

    let sw = MockSuggestWorkflow::new();

    run_re_review(&mut queues, &db, &env, &gh, &git, &claude, &sw).await;

    assert_eq!(
        queues.prs.len(pr_phase::REVIEW_DONE),
        1,
        "request_changes should push back to ReviewDone"
    );
}

// ═══════════════════════════════════════════════════════════
// Merge Pipeline: merge_one
// ═══════════════════════════════════════════════════════════

#[tokio::test]
async fn merge_success_marks_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues
        .merges
        .push(merge_phase::PENDING, make_merge_item(&repo_id, 50));

    let gh = MockGh::new();
    set_gh_pr_mergeable(&gh, 50);

    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response("Merged successfully", 0);

    run_merge(&mut queues, &db, &env, &gh, &git, &claude).await;

    assert_eq!(queues.merges.total(), 0);

    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 50 && label == labels::DONE));
}

#[tokio::test]
async fn merge_already_merged_skips_to_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues
        .merges
        .push(merge_phase::PENDING, make_merge_item(&repo_id, 55));

    let gh = MockGh::new();
    gh.set_field("org/repo", "pulls/55", ".state", "closed");

    let git = MockGit::new();
    let claude = MockAgent::new();

    run_merge(&mut queues, &db, &env, &gh, &git, &claude).await;

    assert_eq!(queues.merges.total(), 0);

    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 55 && label == labels::DONE));

    assert_eq!(claude.call_count(), 0);
}

#[tokio::test]
async fn merge_conflict_then_resolve_success() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues
        .merges
        .push(merge_phase::PENDING, make_merge_item(&repo_id, 60));

    let gh = MockGh::new();
    set_gh_pr_mergeable(&gh, 60);

    let git = MockGit::new();
    let claude = MockAgent::new();
    // merge → conflict
    claude.enqueue_response("CONFLICT in file.rs", 1);
    // resolve → success
    claude.enqueue_response("Resolved and pushed", 0);

    run_merge(&mut queues, &db, &env, &gh, &git, &claude).await;

    assert_eq!(queues.merges.total(), 0);

    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 60 && label == labels::DONE));
}

#[tokio::test]
async fn merge_conflict_then_resolve_failure() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues
        .merges
        .push(merge_phase::PENDING, make_merge_item(&repo_id, 61));

    let gh = MockGh::new();
    set_gh_pr_mergeable(&gh, 61);

    let git = MockGit::new();
    let claude = MockAgent::new();
    // merge → conflict
    claude.enqueue_response("CONFLICT in file.rs", 1);
    // resolve → fail
    claude.enqueue_response("Cannot resolve", 1);

    run_merge(&mut queues, &db, &env, &gh, &git, &claude).await;

    assert_eq!(queues.merges.total(), 0);

    let removed = gh.removed_labels.lock().unwrap();
    assert!(removed
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 61 && label == labels::WIP));
}

// ═══════════════════════════════════════════════════════════
// Resource Cleanup Tests (worktree)
// ═══════════════════════════════════════════════════════════

#[tokio::test]
async fn merge_success_cleans_up_worktree() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_pr_mergeable(&gh, 70);

    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response("Merged successfully", 0);

    let mut queues = TaskQueues::new();
    queues
        .merges
        .push(merge_phase::PENDING, make_merge_item(&repo_id, 70));

    run_merge(&mut queues, &db, &env, &gh, &git, &claude).await;

    assert!(
        has_worktree_remove(&git),
        "merge success should clean up worktree"
    );
}

#[tokio::test]
async fn merge_conflict_cleans_up_worktree() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_pr_mergeable(&gh, 71);

    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response("CONFLICT in file.rs", 1);
    claude.enqueue_response("Cannot resolve", 1);

    let mut queues = TaskQueues::new();
    queues
        .merges
        .push(merge_phase::PENDING, make_merge_item(&repo_id, 71));

    run_merge(&mut queues, &db, &env, &gh, &git, &claude).await;

    assert!(
        has_worktree_remove(&git),
        "merge conflict path should clean up worktree"
    );
}

#[tokio::test]
async fn pr_improve_success_cleans_worktree() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(r#"{"result": "Feedback applied"}"#, 0);

    let mut queues = TaskQueues::new();
    let mut item = make_pr_item(&repo_id, 80, "PR feedback");
    item.review_comment = Some("Fix null check".to_string());
    queues.prs.push(pr_phase::REVIEW_DONE, item);

    run_improve(&mut queues, &db, &env, &gh, &git, &claude).await;

    assert!(
        has_worktree_remove(&git),
        "improve_one should clean up worktree on success"
    );
}

#[tokio::test]
async fn pr_improve_failure_cleans_worktree() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response("implementation error", 1);

    let mut queues = TaskQueues::new();
    let mut item = make_pr_item(&repo_id, 81, "PR feedback");
    item.review_comment = Some("Fix this".to_string());
    queues.prs.push(pr_phase::REVIEW_DONE, item);

    run_improve(&mut queues, &db, &env, &gh, &git, &claude).await;

    assert!(
        has_worktree_remove(&git),
        "improve_one should clean up worktree on failure"
    );
}

#[tokio::test]
async fn pr_re_review_approved_cleans_worktree() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"approve\",\"summary\":\"LGTM\"}"}"#,
        0,
    );
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let sw = MockSuggestWorkflow::new();
    let mut queues = TaskQueues::new();
    queues.prs.push(
        pr_phase::IMPROVED,
        make_pr_item(&repo_id, 82, "Improved PR"),
    );

    run_re_review(&mut queues, &db, &env, &gh, &git, &claude, &sw).await;

    assert!(
        has_worktree_remove(&git),
        "re_review_one should clean up worktree on approval"
    );
}

#[tokio::test]
async fn issue_implement_cleans_up_worktree() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(r#"{"result": "Done"}"#, 0);
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let mut queues = TaskQueues::new();
    let mut item = make_issue_item(&repo_id, 90, "Ready issue");
    item.analysis_report = Some("report".to_string());
    queues.issues.push(issue_phase::READY, item);

    run_implement(&mut queues, &db, &env, &gh, &git, &claude).await;

    assert!(
        has_worktree_remove(&git),
        "implement_one should clean up worktree"
    );
}

// ═══════════════════════════════════════════════════════════
// Review Cycle: max_iterations
// ═══════════════════════════════════════════════════════════

#[tokio::test]
async fn review_cycle_stops_at_max_iterations() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockAgent::new();

    let sw = MockSuggestWorkflow::new();

    // Round 1: improve → improved (iteration 0→1)
    claude.enqueue_response(r#"{"result": "Applied fix 1"}"#, 0);
    // Round 1: re-review → request_changes
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"request_changes\",\"summary\":\"Still needs work\"}"}"#,
        0,
    );
    // Round 2: improve → improved (iteration 1→2)
    claude.enqueue_response(r#"{"result": "Applied fix 2"}"#, 0);
    // Round 2: re-review → request_changes (iteration=2 >= max_iterations=2 → skip)
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"request_changes\",\"summary\":\"Still not right\"}"}"#,
        0,
    );

    let mut queues = TaskQueues::new();
    let mut item = make_pr_item(&repo_id, 100, "PR under review");
    item.review_comment = Some("Fix these issues".to_string());
    queues.prs.push(pr_phase::REVIEW_DONE, item);

    for _round in 1..=3 {
        if queues.prs.len(pr_phase::REVIEW_DONE) > 0 {
            run_improve(&mut queues, &db, &env, &gh, &git, &claude).await;
        }

        if queues.prs.len(pr_phase::IMPROVED) > 0 {
            run_re_review(&mut queues, &db, &env, &gh, &git, &claude, &sw).await;

            if queues.prs.len(pr_phase::REVIEW_DONE) == 0 {
                break;
            }
        }
    }

    assert_eq!(
        claude.call_count(),
        4,
        "review cycle stopped at max_iterations=2"
    );

    let added = gh.added_labels.lock().unwrap();
    assert!(
        added
            .iter()
            .any(|(r, n, l)| r == "org/repo" && *n == 100 && l == labels::SKIP),
        "autodev:skip label should be added when max_iterations reached"
    );
    drop(added);

    assert_eq!(
        queues.prs.total(),
        0,
        "all PR items should be removed from queue"
    );
}

// ═══════════════════════════════════════════════════════════
// DB Logging
// ═══════════════════════════════════════════════════════════

#[tokio::test]
async fn analyze_one_logs_to_db() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 200, "Log test"),
    );

    let gh = MockGh::new();
    set_gh_issue_open(&gh, 200);

    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(&make_analysis_json("implement", 0.9), 0);

    run_analyze(&mut queues, &db, &env, &gh, &git, &claude).await;

    let logs = db.log_recent(None, 10).expect("fetch logs");
    assert!(
        !logs.is_empty(),
        "analyze_one should produce at least one log entry"
    );
}
