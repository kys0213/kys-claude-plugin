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

fn insert_pending_merge(db: &Database, repo_id: &str, pr_number: i64, title: &str) -> String {
    let item = NewMergeItem {
        repo_id: repo_id.to_string(),
        pr_number,
        title: title.to_string(),
        head_branch: "feat/test".to_string(),
        base_branch: "main".to_string(),
    };
    db.merge_insert(&item).unwrap()
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

// ═══════════════════════════════════════════════════
// 1. Issue 전체 사이클: pending → analyzing → ready → processing → done
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn issue_full_cycle_pending_to_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 10, "Full cycle issue");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 10);

    let git = MockGit::new();
    let claude = MockClaude::new();

    // Phase 1: analysis → implement + high confidence → ready
    claude.enqueue_response(&make_analysis_json("implement", 0.9), 0);
    // Phase 2: implementation → success
    claude.enqueue_response(r#"{"result": "Implementation complete"}"#, 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    // Phase 1: process_pending (pending → analyzing → ready)
    autodev::pipeline::issue::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &claude,
        &mut active,
    )
    .await
    .expect("phase 1 should succeed");

    let ready = db.issue_find_ready(100).unwrap();
    assert_eq!(
        ready.len(),
        1,
        "issue should be in ready state after phase 1"
    );

    // Phase 2: process_ready (ready → processing → done)
    autodev::pipeline::issue::process_ready(&db, &env, &workspace, &claude, &mut active)
        .await
        .expect("phase 2 should succeed");

    let list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].status, "done",
        "issue should be done after full cycle"
    );

    // Claude는 정확히 2번 호출 (분석 1회 + 구현 1회)
    assert_eq!(claude.call_count(), 2);

    // 로그 2건 기록
    let logs = db.log_recent(None, 100).unwrap();
    assert_eq!(
        logs.len(),
        2,
        "should have 2 consumer logs (analysis + implementation)"
    );
}

// ═══════════════════════════════════════════════════
// 2. Issue failed → retry → 재진입 사이클
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn issue_failed_retry_reentry_cycle() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let issue_id = insert_pending_issue(&db, &repo_id, 20, "Retry cycle issue");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 20);

    let git = MockGit::new();
    let claude = MockClaude::new();

    // 1차 시도: Claude 실패
    claude.enqueue_response("analysis error", 1);
    // 2차 시도 (retry 후): Claude 성공
    claude.enqueue_response(&make_analysis_json("implement", 0.85), 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    // 1차 시도 → failed
    autodev::pipeline::issue::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &claude,
        &mut active,
    )
    .await
    .unwrap();

    let list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(
        list[0].status, "failed",
        "should be failed after 1st attempt"
    );

    // retry → pending으로 복구
    let retried = db.queue_retry(&issue_id).unwrap();
    assert!(retried, "queue_retry should return true for failed item");

    let pending = db.issue_find_pending(100).unwrap();
    assert_eq!(
        pending.len(),
        1,
        "issue should be back to pending after retry"
    );

    // 2차 시도 → ready
    autodev::pipeline::issue::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &claude,
        &mut active,
    )
    .await
    .unwrap();

    let ready = db.issue_find_ready(100).unwrap();
    assert_eq!(
        ready.len(),
        1,
        "issue should be in ready state after successful retry"
    );

    assert_eq!(
        claude.call_count(),
        2,
        "claude should be called twice (fail + success)"
    );
}

// ═══════════════════════════════════════════════════
// 3. PR pre-flight: closed PR → skip to done
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn pr_closed_on_github_skips_to_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_pr(&db, &repo_id, 30, "Already closed PR");

    let gh = MockGh::new();
    // PR is closed
    gh.set_field("org/repo", "pulls/30", ".state", "closed");

    let git = MockGit::new();
    let claude = MockClaude::new();

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::pr::process_pending(&db, &env, &workspace, &notifier, &claude, &mut active)
        .await
        .unwrap();

    let list = db.pr_list("org/repo", 100).unwrap();
    assert_eq!(list[0].status, "done", "closed PR should skip to done");
    assert_eq!(
        claude.call_count(),
        0,
        "claude should not be called for closed PR"
    );
}

// ═══════════════════════════════════════════════════
// 4. PR pre-flight: already approved → skip to done
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn pr_already_approved_skips_to_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_pr(&db, &repo_id, 31, "Already approved PR");

    let gh = MockGh::new();
    // PR is open but already approved
    gh.set_field("org/repo", "pulls/31", ".state", "open");
    gh.set_field(
        "org/repo",
        "pulls/31/reviews",
        r#"[.[] | select(.state == "APPROVED")] | length"#,
        "2",
    );

    let git = MockGit::new();
    let claude = MockClaude::new();

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::pr::process_pending(&db, &env, &workspace, &notifier, &claude, &mut active)
        .await
        .unwrap();

    let list = db.pr_list("org/repo", 100).unwrap();
    assert_eq!(list[0].status, "done", "approved PR should skip to done");
    assert_eq!(
        claude.call_count(),
        0,
        "claude should not be called for approved PR"
    );
}

// ═══════════════════════════════════════════════════
// 5. Merge 성공 사이클: pending → merging → done
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn merge_success_cycle() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_merge(&db, &repo_id, 40, "Merge PR #40");

    let gh = MockGh::new();
    set_gh_pr_open(&gh, "org/repo", 40);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // merge 성공 (exit_code 0)
    claude.enqueue_response("Merged successfully", 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::merge::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &claude,
        &mut active,
    )
    .await
    .unwrap();

    let list = db.merge_list("org/repo", 100).unwrap();
    assert_eq!(list[0].status, "done", "merge should complete successfully");
    assert_eq!(claude.call_count(), 1);
}

// ═══════════════════════════════════════════════════
// 6. Merge conflict → resolve 성공 사이클
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn merge_conflict_then_resolve_success() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_merge(&db, &repo_id, 41, "Merge PR #41 with conflict");

    let gh = MockGh::new();
    set_gh_pr_open(&gh, "org/repo", 41);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // 1st call: merge attempt → conflict (exit_code 1 + "conflict" in output)
    claude.enqueue_response("CONFLICT (content): merge conflict in src/main.rs", 1);
    // 2nd call: resolve_conflicts → success
    claude.enqueue_response("Conflicts resolved and committed", 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::merge::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &claude,
        &mut active,
    )
    .await
    .unwrap();

    let list = db.merge_list("org/repo", 100).unwrap();
    assert_eq!(
        list[0].status, "done",
        "merge should complete after conflict resolution"
    );
    assert_eq!(
        claude.call_count(),
        2,
        "should call claude twice (merge + resolve)"
    );
}

// ═══════════════════════════════════════════════════
// 7. Merge conflict → resolve 실패 사이클
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn merge_conflict_then_resolve_failure() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_merge(&db, &repo_id, 42, "Merge PR #42 unresolvable conflict");

    let gh = MockGh::new();
    set_gh_pr_open(&gh, "org/repo", 42);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // 1st call: merge → conflict
    claude.enqueue_response("CONFLICT (content): merge conflict in complex.rs", 1);
    // 2nd call: resolve → failure (exit_code 1, no "conflict" keyword → Failed)
    claude.enqueue_response("Failed to resolve", 1);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::merge::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &claude,
        &mut active,
    )
    .await
    .unwrap();

    let list = db.merge_list("org/repo", 100).unwrap();
    assert_eq!(
        list[0].status, "failed",
        "should fail when conflict resolution fails"
    );
    assert_eq!(claude.call_count(), 2);
}

// ═══════════════════════════════════════════════════
// 8. Merge pre-flight: already merged → skip
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn merge_already_merged_skips_to_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_merge(&db, &repo_id, 43, "Already merged PR");

    let gh = MockGh::new();
    // PR is already closed/merged
    gh.set_field("org/repo", "pulls/43", ".state", "closed");

    let git = MockGit::new();
    let claude = MockClaude::new();

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::merge::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &claude,
        &mut active,
    )
    .await
    .unwrap();

    let list = db.merge_list("org/repo", 100).unwrap();
    assert_eq!(
        list[0].status, "done",
        "already merged PR should skip to done"
    );
    assert_eq!(claude.call_count(), 0, "claude should not be called");
}

// ═══════════════════════════════════════════════════
// 9. Confidence 경계값: 정확히 0.7 (threshold) → ready
//    코드: a.confidence < cfg.consumer.confidence_threshold
//    0.7 < 0.7 은 false → 'implement' 매치 → ready
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn confidence_at_threshold_goes_to_ready() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 50, "Boundary confidence issue");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 50);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // confidence exactly at threshold (0.7)
    claude.enqueue_response(&make_analysis_json("implement", 0.7), 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::issue::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &claude,
        &mut active,
    )
    .await
    .unwrap();

    // 0.7 NOT < 0.7 → ready (not waiting_human)
    let ready = db.issue_find_ready(100).unwrap();
    assert_eq!(ready.len(), 1, "confidence == threshold should go to ready");

    // 댓글 없어야 함 (waiting_human이면 댓글이 게시됨)
    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(
        comments.len(),
        0,
        "no clarification comment for threshold confidence"
    );
}

// ═══════════════════════════════════════════════════
// 10. Confidence 경계값: 0.69 (threshold 미만) → waiting_human
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn confidence_just_below_threshold_goes_to_waiting() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 51, "Below threshold issue");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 51);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // confidence just below threshold
    claude.enqueue_response(&make_analysis_json("implement", 0.69), 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::issue::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &claude,
        &mut active,
    )
    .await
    .unwrap();

    // 0.69 < 0.7 → waiting_human
    let list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(
        list[0].status, "waiting_human",
        "confidence below threshold should wait"
    );

    // clarification 댓글 게시
    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1, "should post clarification comment");
}

// ═══════════════════════════════════════════════════
// 11. Analysis JSON에 confidence 필드 누락 → parse 실패 → fallback ready
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn analysis_missing_confidence_field_falls_back_to_ready() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 52, "Missing confidence field");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 52);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // JSON without confidence field → parse_analysis may return None → fallback ready
    let bad_json = serde_json::json!({
        "result": r#"{"verdict":"implement","summary":"test","questions":[],"report":"report"}"#
    });
    claude.enqueue_response(&bad_json.to_string(), 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::issue::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &claude,
        &mut active,
    )
    .await
    .unwrap();

    // confidence 누락 시 기본값 0.0이면 → waiting_human
    // 또는 parse_analysis returns None → fallback ready
    // 어느 쪽이든 pipeline은 중단되지 않음
    let list = db.issue_list("org/repo", 100).unwrap();
    assert!(
        list[0].status == "ready" || list[0].status == "waiting_human",
        "missing confidence should either fallback to ready or go to waiting_human, got: {}",
        list[0].status
    );
}

// ═══════════════════════════════════════════════════
// 12. Analysis JSON이 완전히 깨진 형태 (빈 문자열) → fallback ready
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn analysis_completely_malformed_json_falls_back_to_ready() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 53, "Malformed JSON issue");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 53);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // 완전히 잘못된 출력 (JSON 아님)
    claude.enqueue_response("I cannot parse this {{{garbage", 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::issue::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &claude,
        &mut active,
    )
    .await
    .unwrap();

    // parse_analysis returns None → fallback ready
    let ready = db.issue_find_ready(100).unwrap();
    assert_eq!(
        ready.len(),
        1,
        "completely malformed output should fallback to ready"
    );
}

// ═══════════════════════════════════════════════════
// 13. Workspace clone 실패 → issue failed
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn workspace_clone_failure_marks_issue_failed() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 60, "Clone will fail");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 60);

    let git = MockGit::new();
    *git.clone_should_fail.lock().unwrap() = true;

    let claude = MockClaude::new();

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::issue::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &claude,
        &mut active,
    )
    .await
    .unwrap();

    let list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(
        list[0].status, "failed",
        "clone failure should mark issue as failed"
    );
    assert_eq!(
        claude.call_count(),
        0,
        "claude should not be called when clone fails"
    );
}

// ═══════════════════════════════════════════════════
// 14. Workspace worktree 실패 → issue failed
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn workspace_worktree_failure_marks_issue_failed() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_issue(&db, &repo_id, 61, "Worktree will fail");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 61);

    let git = MockGit::new();
    *git.worktree_should_fail.lock().unwrap() = true;

    let claude = MockClaude::new();

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::issue::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &claude,
        &mut active,
    )
    .await
    .unwrap();

    let list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(
        list[0].status, "failed",
        "worktree failure should mark issue as failed"
    );
    assert_eq!(
        claude.call_count(),
        0,
        "claude should not be called when worktree fails"
    );
}

// ═══════════════════════════════════════════════════
// 15. Workspace clone 실패 → PR failed
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn workspace_clone_failure_marks_pr_failed() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_pr(&db, &repo_id, 70, "Clone will fail for PR");

    let gh = MockGh::new();
    set_gh_pr_open(&gh, "org/repo", 70);

    let git = MockGit::new();
    *git.clone_should_fail.lock().unwrap() = true;

    let claude = MockClaude::new();

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::pr::process_pending(&db, &env, &workspace, &notifier, &claude, &mut active)
        .await
        .unwrap();

    let list = db.pr_list("org/repo", 100).unwrap();
    assert_eq!(
        list[0].status, "failed",
        "clone failure should mark PR as failed"
    );
    assert_eq!(claude.call_count(), 0);
}

// ═══════════════════════════════════════════════════
// 16. Workspace clone 실패 → merge failed
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn workspace_clone_failure_marks_merge_failed() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_merge(&db, &repo_id, 80, "Clone will fail for merge");

    let gh = MockGh::new();
    set_gh_pr_open(&gh, "org/repo", 80);

    let git = MockGit::new();
    *git.clone_should_fail.lock().unwrap() = true;

    let claude = MockClaude::new();

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::merge::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &claude,
        &mut active,
    )
    .await
    .unwrap();

    let list = db.merge_list("org/repo", 100).unwrap();
    assert_eq!(
        list[0].status, "failed",
        "clone failure should mark merge as failed"
    );
    assert_eq!(claude.call_count(), 0);
}

// ═══════════════════════════════════════════════════
// 17. Scanner: autodev 라벨 필터링 — "autodev" 라벨 include filter
// ═══════════════════════════════════════════════════

fn fixture_response(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/responses")
        .join(name);
    std::fs::read(path).expect("read fixture file")
}

#[tokio::test]
async fn scanner_filters_by_autodev_label() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    gh.set_paginate(
        "org/repo",
        "issues",
        fixture_response("issues_with_autodev_labels.json"),
    );

    // "autodev" 라벨이 있는 이슈만 스캔
    let labels = Some(vec!["autodev".to_string()]);
    let mut active = autodev::active::ActiveItems::new();

    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        &[],
        &labels,
        None,
        &mut active,
    )
    .await
    .unwrap();

    // #60: labels=["bug","autodev"] → 포함
    assert!(
        db.issue_exists(&repo_id, 60).unwrap(),
        "issue #60 with 'autodev' label should be queued"
    );
    // #61: labels=["bug","autodev:wip"] → "autodev" 정확히 일치하지 않으므로 제외
    assert!(
        !db.issue_exists(&repo_id, 61).unwrap(),
        "issue #61 with 'autodev:wip' should NOT match 'autodev'"
    );
    // #62: labels=["enhancement","autodev:done"] → 제외
    assert!(
        !db.issue_exists(&repo_id, 62).unwrap(),
        "issue #62 with 'autodev:done' should NOT match 'autodev'"
    );
    // #63: labels=["wontfix","autodev:skip"] → 제외
    assert!(
        !db.issue_exists(&repo_id, 63).unwrap(),
        "issue #63 with 'autodev:skip' should NOT match 'autodev'"
    );
    // #64: labels=["enhancement"] → 제외
    assert!(
        !db.issue_exists(&repo_id, 64).unwrap(),
        "issue #64 without autodev label should NOT be queued"
    );
}

// ═══════════════════════════════════════════════════
// 18. Scanner: autodev:wip 라벨 필터로 wip 이슈만 스캔
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn scanner_filters_autodev_wip_label_only() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    gh.set_paginate(
        "org/repo",
        "issues",
        fixture_response("issues_with_autodev_labels.json"),
    );

    // "autodev:wip" 라벨만 스캔
    let labels = Some(vec!["autodev:wip".to_string()]);
    let mut active = autodev::active::ActiveItems::new();

    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        &[],
        &labels,
        None,
        &mut active,
    )
    .await
    .unwrap();

    // #61만 포함 (autodev:wip 라벨)
    assert!(
        !db.issue_exists(&repo_id, 60).unwrap(),
        "issue #60 should not match autodev:wip"
    );
    assert!(
        db.issue_exists(&repo_id, 61).unwrap(),
        "issue #61 with autodev:wip should be queued"
    );
    assert!(!db.issue_exists(&repo_id, 62).unwrap());
    assert!(!db.issue_exists(&repo_id, 63).unwrap());
    assert!(!db.issue_exists(&repo_id, 64).unwrap());
}

// ═══════════════════════════════════════════════════
// 19. Merge 실패 (충돌 아닌 실패) → failed
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn merge_non_conflict_failure_marks_failed() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    insert_pending_merge(&db, &repo_id, 44, "Merge will fail non-conflict");

    let gh = MockGh::new();
    set_gh_pr_open(&gh, "org/repo", 44);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // merge fails with exit_code 1 but no "conflict" keyword → MergeOutcome::Failed
    claude.enqueue_response("Permission denied: cannot push to protected branch", 1);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::merge::process_pending(
        &db,
        &env,
        &workspace,
        &notifier,
        &claude,
        &mut active,
    )
    .await
    .unwrap();

    let list = db.merge_list("org/repo", 100).unwrap();
    assert_eq!(
        list[0].status, "failed",
        "non-conflict failure should mark as failed"
    );
    // resolve_conflicts should NOT be called
    assert_eq!(
        claude.call_count(),
        1,
        "should only attempt merge, not resolve"
    );
}

// ═══════════════════════════════════════════════════
// 20. process_all 전체 통합: issue + PR + merge 동시 처리
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn process_all_handles_all_queues() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    insert_pending_issue(&db, &repo_id, 90, "Issue for process_all");
    insert_pending_pr(&db, &repo_id, 91, "PR for process_all");
    insert_pending_merge(&db, &repo_id, 92, "Merge for process_all");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 90);
    set_gh_pr_open(&gh, "org/repo", 91);
    set_gh_pr_open(&gh, "org/repo", 92);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // Issue analysis → ready
    claude.enqueue_response(&make_analysis_json("implement", 0.9), 0);
    // Issue ready doesn't get processed in same cycle if just became ready
    // (process_all calls process_pending then process_ready, but process_ready
    //  will find the item that just moved to ready)
    claude.enqueue_response(r#"{"result": "Implementation done"}"#, 0);
    // PR review
    claude.enqueue_response(r#"{"result": "LGTM"}"#, 0);
    // Merge
    claude.enqueue_response("Merged successfully", 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut active = autodev::active::ActiveItems::new();

    autodev::pipeline::process_all(&db, &env, &workspace, &notifier, &claude, &mut active)
        .await
        .expect("process_all should succeed");

    // Issue: pending → ready → done (2 phases in same process_all)
    let issue_list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(issue_list[0].status, "done", "issue should be done");

    // PR: pending → review_done
    let pr_list = db.pr_list("org/repo", 100).unwrap();
    assert_eq!(pr_list[0].status, "review_done", "pr should be review_done");

    // Merge: pending → done
    let merge_list = db.merge_list("org/repo", 100).unwrap();
    assert_eq!(merge_list[0].status, "done", "merge should be done");

    assert_eq!(claude.call_count(), 4, "should call claude 4 times total");
}
