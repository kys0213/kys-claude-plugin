use std::collections::HashMap;
use std::path::Path;

use autodev::components::notifier::Notifier;
use autodev::components::workspace::Workspace;
use autodev::config::Env;
use autodev::domain::labels;
use autodev::domain::repository::RepoRepository;
use autodev::infrastructure::agent::mock::MockAgent;
use autodev::infrastructure::gh::mock::MockGh;
use autodev::infrastructure::git::mock::MockGit;
use autodev::queue::task_queues::{issue_phase, make_work_id, IssueItem, TaskQueues};
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

fn set_gh_open(gh: &MockGh, repo_name: &str, number: i64) {
    gh.set_field(repo_name, &format!("issues/{number}"), ".state", "open");
}

fn make_analysis_json(
    verdict: &str,
    confidence: f64,
    questions: &[&str],
    reason: Option<&str>,
) -> String {
    let questions_json: Vec<String> = questions.iter().map(|q| format!("\"{q}\"")).collect();
    let reason_json = match reason {
        Some(r) => format!("\"{}\"", r),
        None => "null".to_string(),
    };
    let report_text = "## Analysis Report";
    let inner = format!(
        r#"{{"verdict":"{verdict}","confidence":{confidence},"summary":"Test summary","questions":[{questions}],"reason":{reason},"report":"{report_text}"}}"#,
        questions = questions_json.join(","),
        reason = reason_json,
    );
    // Wrap in Claude JSON envelope
    serde_json::json!({ "result": inner }).to_string()
}

// ═══════════════════════════════════════════════
// verdict: wontfix → queue empty + skip label + comment posted
// ═══════════════════════════════════════════════

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
    set_gh_open(&gh, "org/repo", 1);

    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(
        &make_analysis_json("wontfix", 0.95, &[], Some("Duplicate of #42")),
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
    .unwrap();

    // queue should be empty (item removed)
    assert_eq!(queues.issues.total(), 0);

    // skip label added, wip removed
    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 1 && label == labels::SKIP));

    let removed = gh.removed_labels.lock().unwrap();
    assert!(removed
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 1 && label == labels::WIP));

    // comment posted with "Won't fix" and "Duplicate of #42"
    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.contains("Won't fix"));
    assert!(comments[0].2.contains("Duplicate of #42"));
}

// ═══════════════════════════════════════════════
// verdict: needs_clarification → queue empty + skip label + comment with questions
// ═══════════════════════════════════════════════

#[tokio::test]
async fn issue_verdict_needs_clarification_posts_questions_and_waits() {
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
    set_gh_open(&gh, "org/repo", 2);

    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(
        &make_analysis_json(
            "needs_clarification",
            0.8,
            &["What is the expected behavior?", "Which version?"],
            None,
        ),
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
    .unwrap();

    // queue should be empty (item removed)
    assert_eq!(queues.issues.total(), 0);

    // skip label added, wip removed
    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 2 && label == labels::SKIP));

    let removed = gh.removed_labels.lock().unwrap();
    assert!(removed
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 2 && label == labels::WIP));

    // comment posted with questions
    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.contains("What is the expected behavior?"));
    assert!(comments[0].2.contains("Which version?"));
}

// ═══════════════════════════════════════════════
// verdict: implement + low confidence → queue empty + skip label + comment
// ═══════════════════════════════════════════════

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
    set_gh_open(&gh, "org/repo", 3);

    let git = MockGit::new();
    let claude = MockAgent::new();
    // implement verdict but confidence below threshold (default 0.7)
    claude.enqueue_response(&make_analysis_json("implement", 0.3, &[], None), 0);

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
    .unwrap();

    // queue should be empty (item removed, not moved to Ready)
    assert_eq!(queues.issues.total(), 0);

    // skip label added, wip removed
    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 3 && label == labels::SKIP));

    let removed = gh.removed_labels.lock().unwrap();
    assert!(removed
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 3 && label == labels::WIP));

    // clarification comment posted
    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.contains("clarification"));
}

// ═══════════════════════════════════════════════
// verdict: implement + high confidence → Ready queue has 1 item
// ═══════════════════════════════════════════════

#[tokio::test]
async fn issue_verdict_implement_high_confidence_goes_to_ready() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let mut queues = TaskQueues::new();
    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 4, "Clear bug"),
    );

    let gh = MockGh::new();
    set_gh_open(&gh, "org/repo", 4);

    let git = MockGit::new();
    let claude = MockAgent::new();
    claude.enqueue_response(&make_analysis_json("implement", 0.95, &[], None), 0);

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
    .unwrap();

    // v2: high confidence → analyzed 라벨 + queue 이탈 (HITL 게이트)
    assert_eq!(
        queues.issues.len(issue_phase::READY),
        0,
        "v2: exits queue, not moved to Ready"
    );
    assert_eq!(
        queues.issues.total(),
        0,
        "v2: issue should exit queue entirely"
    );

    // analyzed 라벨 추가, wip 라벨 제거
    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 4 && label == labels::ANALYZED));
    assert!(!added.iter().any(|(_, _, label)| label == labels::SKIP));

    let removed = gh.removed_labels.lock().unwrap();
    assert!(removed
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 4 && label == labels::WIP));

    // 분석 코멘트가 게시됨
    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.contains("<!-- autodev:analysis -->"));
}

// ═══════════════════════════════════════════════
// GitHub closed issue → queue empty + done label + wip removed, Claude not called
// ═══════════════════════════════════════════════

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
    // Claude should NOT be called

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
    .unwrap();

    // queue should be empty (item removed)
    assert_eq!(queues.issues.total(), 0);

    // done label added, wip removed
    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 5 && label == labels::DONE));

    let removed = gh.removed_labels.lock().unwrap();
    assert!(removed
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 5 && label == labels::WIP));

    // Claude not called
    assert_eq!(claude.call_count(), 0);
}

// ═══════════════════════════════════════════════
// Unparseable analysis → fallback to Ready
// ═══════════════════════════════════════════════

#[tokio::test]
async fn issue_unparseable_analysis_falls_back_to_ready() {
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
    set_gh_open(&gh, "org/repo", 6);

    let git = MockGit::new();
    let claude = MockAgent::new();
    // Invalid JSON → parse_analysis returns None
    claude.enqueue_response("This is not valid JSON at all", 0);

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
    .unwrap();

    // v2: fallback → analyzed 라벨 + queue 이탈
    assert_eq!(
        queues.issues.len(issue_phase::READY),
        0,
        "v2: exits queue, not moved to Ready"
    );
    assert_eq!(
        queues.issues.total(),
        0,
        "v2: issue should exit queue entirely"
    );

    // analyzed 라벨 추가
    let added = gh.added_labels.lock().unwrap();
    assert!(added
        .iter()
        .any(|(repo, n, label)| repo == "org/repo" && *n == 6 && label == labels::ANALYZED));

    // 분석 코멘트가 게시됨 (fallback)
    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.contains("<!-- autodev:analysis -->"));
}
