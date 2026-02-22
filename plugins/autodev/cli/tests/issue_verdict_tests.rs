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
        body: Some("Test body".to_string()),
        labels: r#"["bug"]"#.to_string(),
        author: "alice".to_string(),
    };
    db.issue_insert(&item).unwrap()
}

fn set_gh_open(gh: &MockGh, repo_name: &str, number: i64) {
    gh.set_field(repo_name, &format!("issues/{number}"), ".state", "open");
}

fn make_analysis_json(verdict: &str, confidence: f64, questions: &[&str], reason: Option<&str>) -> String {
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
// verdict: wontfix → done + 댓글 게시
// ═══════════════════════════════════════════════

#[tokio::test]
async fn issue_verdict_wontfix_posts_comment_and_marks_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 1, "Won't fix issue");

    let gh = MockGh::new();
    set_gh_open(&gh, "org/repo", 1);

    let git = MockGit::new();
    let claude = MockClaude::new();
    claude.enqueue_response(
        &make_analysis_json("wontfix", 0.95, &[], Some("Duplicate of #42")),
        0,
    );

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::issue::process_pending(
        &db, &env, &workspace, &notifier, &claude, &mut active,
    )
    .await
    .unwrap();

    // status → done
    let list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, "done");

    // 댓글이 게시되었는지 확인
    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.contains("Won't fix"));
    assert!(comments[0].2.contains("Duplicate of #42"));
}

// ═══════════════════════════════════════════════
// verdict: needs_clarification → waiting_human + 댓글 게시
// ═══════════════════════════════════════════════

#[tokio::test]
async fn issue_verdict_needs_clarification_posts_questions_and_waits() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 2, "Ambiguous issue");

    let gh = MockGh::new();
    set_gh_open(&gh, "org/repo", 2);

    let git = MockGit::new();
    let claude = MockClaude::new();
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
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::issue::process_pending(
        &db, &env, &workspace, &notifier, &claude, &mut active,
    )
    .await
    .unwrap();

    // status → waiting_human
    let list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, "waiting_human");

    // 질문이 포함된 댓글 게시 확인
    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.contains("What is the expected behavior?"));
    assert!(comments[0].2.contains("Which version?"));
}

// ═══════════════════════════════════════════════
// verdict: implement + 저신뢰도 → waiting_human (confidence_threshold 기준)
// ═══════════════════════════════════════════════

#[tokio::test]
async fn issue_verdict_implement_low_confidence_goes_to_waiting() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 3, "Low confidence issue");

    let gh = MockGh::new();
    set_gh_open(&gh, "org/repo", 3);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // implement verdict이지만 confidence가 threshold(기본 0.7) 미만
    claude.enqueue_response(&make_analysis_json("implement", 0.3, &[], None), 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::issue::process_pending(
        &db, &env, &workspace, &notifier, &claude, &mut active,
    )
    .await
    .unwrap();

    // 저신뢰도 → waiting_human (needs_clarification과 같은 분기)
    let list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, "waiting_human");

    // clarification 댓글 게시
    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.contains("clarification"));
}

// ═══════════════════════════════════════════════
// verdict: implement + 고신뢰도 → ready
// ═══════════════════════════════════════════════

#[tokio::test]
async fn issue_verdict_implement_high_confidence_goes_to_ready() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 4, "Clear bug");

    let gh = MockGh::new();
    set_gh_open(&gh, "org/repo", 4);

    let git = MockGit::new();
    let claude = MockClaude::new();
    claude.enqueue_response(&make_analysis_json("implement", 0.95, &[], None), 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::issue::process_pending(
        &db, &env, &workspace, &notifier, &claude, &mut active,
    )
    .await
    .unwrap();

    // 고신뢰도 → ready
    let ready = db.issue_find_ready(100).unwrap();
    assert_eq!(ready.len(), 1);

    // 댓글 없음 (바로 구현으로)
    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 0);
}

// ═══════════════════════════════════════════════
// GitHub에서 closed된 이슈 → skip (done)
// ═══════════════════════════════════════════════

#[tokio::test]
async fn issue_closed_on_github_skips_to_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 5, "Already closed");

    let gh = MockGh::new();
    gh.set_field("org/repo", "issues/5", ".state", "closed");

    let git = MockGit::new();
    let claude = MockClaude::new();
    // Claude should NOT be called

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::issue::process_pending(
        &db, &env, &workspace, &notifier, &claude, &mut active,
    )
    .await
    .unwrap();

    let list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(list[0].status, "done");

    // Claude 호출 0회
    assert_eq!(claude.call_count(), 0);
}

// ═══════════════════════════════════════════════
// 파싱 실패 → fallback to ready
// ═══════════════════════════════════════════════

#[tokio::test]
async fn issue_unparseable_analysis_falls_back_to_ready() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 6, "Parse fail issue");

    let gh = MockGh::new();
    set_gh_open(&gh, "org/repo", 6);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // 유효하지 않은 JSON → parse_analysis returns None
    claude.enqueue_response("This is not valid JSON at all", 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::issue::process_pending(
        &db, &env, &workspace, &notifier, &claude, &mut active,
    )
    .await
    .unwrap();

    // fallback → ready
    let ready = db.issue_find_ready(100).unwrap();
    assert_eq!(ready.len(), 1);
}
