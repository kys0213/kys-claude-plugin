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
    issue_phase, make_work_id, pr_phase, IssueItem, MergeItem, PrItem, TaskQueues,
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

fn open_memory_db() -> Database {
    let db = Database::open(Path::new(":memory:")).expect("open in-memory db");
    db.initialize().expect("initialize schema");
    db
}

fn add_repo(db: &Database) -> String {
    db.repo_add("https://github.com/org/repo", "org/repo")
        .expect("add repo")
}

fn make_pr_item(repo_id: &str, number: i64) -> PrItem {
    PrItem {
        work_id: make_work_id("pr", "org/repo", number),
        repo_id: repo_id.to_string(),
        repo_name: "org/repo".to_string(),
        repo_url: "https://github.com/org/repo".to_string(),
        github_number: number,
        title: "Test PR".to_string(),
        head_branch: "feat/test".to_string(),
        base_branch: "main".to_string(),
        review_comment: None,
        source_issue_number: None,
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
    }
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

fn has_worktree_remove(git: &MockGit) -> bool {
    git.calls
        .lock()
        .unwrap()
        .iter()
        .any(|(m, _)| m == "worktree_remove")
}

// ═══════════════════════════════════════════════
// C-5: Merge worktree 정리 — 성공 경로
// ═══════════════════════════════════════════════

#[tokio::test]
async fn merge_success_cleans_up_worktree() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db);

    let gh = MockGh::new();
    set_gh_pr_open(&gh, 50);

    let git = MockGit::new();
    let claude = MockClaude::new();
    claude.enqueue_response("Merged successfully", 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();
    queues.merges.push("Pending", make_merge_item(&repo_id, 50));

    autodev::pipeline::merge::process_pending(
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

    assert!(
        has_worktree_remove(&git),
        "merge success should clean up worktree"
    );
}

/// C-5: merge conflict 경로에서 worktree 정리 미누락 검증
#[tokio::test]
async fn merge_conflict_does_not_clean_up_worktree_c5() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db);

    let gh = MockGh::new();
    set_gh_pr_open(&gh, 51);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // merge → conflict
    claude.enqueue_response("CONFLICT in file.rs", 1);
    // resolve → fail
    claude.enqueue_response("Cannot resolve", 1);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();
    queues.merges.push("Pending", make_merge_item(&repo_id, 51));

    autodev::pipeline::merge::process_pending(
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

    // C-5 BUG: Conflict/Failed 경로에서 worktree_remove 미호출
    assert!(
        !has_worktree_remove(&git),
        "C-5: merge conflict path does NOT clean up worktree (resource leak)"
    );
}

/// C-5: merge non-conflict failure 경로에서 worktree 정리 누락 검증
#[tokio::test]
async fn merge_failed_does_not_clean_up_worktree_c5() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db);

    let gh = MockGh::new();
    set_gh_pr_open(&gh, 52);

    let git = MockGit::new();
    let claude = MockClaude::new();
    claude.enqueue_response("permission denied", 1);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();
    queues.merges.push("Pending", make_merge_item(&repo_id, 52));

    autodev::pipeline::merge::process_pending(
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

    // C-5 BUG: Failed 경로에서 worktree_remove 미호출
    assert!(
        !has_worktree_remove(&git),
        "C-5: merge failed path does NOT clean up worktree (resource leak)"
    );
}

// ═══════════════════════════════════════════════
// C-2: PR process_review_done worktree 정리 누락
// ═══════════════════════════════════════════════

/// C-2: PR review_done 성공 경로 — Improved로 전이 시 worktree 미정리
#[tokio::test]
async fn pr_review_done_success_does_not_clean_worktree_c2() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db);

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockClaude::new();
    claude.enqueue_response(r#"{"result": "Feedback applied"}"#, 0);

    let workspace = Workspace::new(&git, &env);
    let mut queues = TaskQueues::new();

    let mut item = make_pr_item(&repo_id, 60);
    item.review_comment = Some("Fix null check".to_string());
    queues.prs.push(pr_phase::REVIEW_DONE, item);

    autodev::pipeline::pr::process_review_done(&db, &env, &workspace, &gh, &claude, &mut queues)
        .await
        .unwrap();

    // C-2 BUG: process_review_done에서 worktree 정리 미호출
    assert!(
        !has_worktree_remove(&git),
        "C-2: process_review_done does NOT clean up worktree on success"
    );
}

/// C-2: PR review_done 실패 경로 — worktree 미정리
#[tokio::test]
async fn pr_review_done_failure_does_not_clean_worktree_c2() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db);

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockClaude::new();
    claude.enqueue_response("implementation error", 1);

    let workspace = Workspace::new(&git, &env);
    let mut queues = TaskQueues::new();

    let mut item = make_pr_item(&repo_id, 61);
    item.review_comment = Some("Fix this".to_string());
    queues.prs.push(pr_phase::REVIEW_DONE, item);

    autodev::pipeline::pr::process_review_done(&db, &env, &workspace, &gh, &claude, &mut queues)
        .await
        .unwrap();

    // C-2 BUG: 실패 시에도 worktree 미정리
    assert!(
        !has_worktree_remove(&git),
        "C-2: process_review_done does NOT clean up worktree on failure"
    );
}

// ═══════════════════════════════════════════════
// C-3: PR process_improved worktree 정리 누락
// ═══════════════════════════════════════════════

/// C-3: PR improved 승인 경로 — done 전이 시 worktree 미정리
#[tokio::test]
async fn pr_improved_approved_does_not_clean_worktree_c3() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db);

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockClaude::new();
    // Re-review: approved (exit_code 0)
    claude.enqueue_response(r#"{"result": "LGTM"}"#, 0);
    // Knowledge extraction
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let sw = MockSuggestWorkflow::new();
    let mut queues = TaskQueues::new();
    queues
        .prs
        .push(pr_phase::IMPROVED, make_pr_item(&repo_id, 70));

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

    // C-3 BUG: process_improved에서 worktree 정리 미호출
    assert!(
        !has_worktree_remove(&git),
        "C-3: process_improved does NOT clean up worktree on approval"
    );
}

/// C-3: PR improved 재리뷰 실패 → ReviewDone 재진입 시 worktree 미정리
#[tokio::test]
async fn pr_improved_request_changes_does_not_clean_worktree_c3() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db);

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockClaude::new();
    // Re-review: request_changes (verdict-based)
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"request_changes\",\"summary\":\"Needs more work\"}"}"#,
        0,
    );

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let sw = MockSuggestWorkflow::new();
    let mut queues = TaskQueues::new();
    queues
        .prs
        .push(pr_phase::IMPROVED, make_pr_item(&repo_id, 71));

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

    // C-3 BUG: 재리뷰 실패 → ReviewDone 재진입 시에도 worktree 미정리
    assert!(
        !has_worktree_remove(&git),
        "C-3: process_improved does NOT clean up worktree on request_changes"
    );

    // 대신 ReviewDone 큐에 재진입 확인
    assert_eq!(
        queues.prs.len(pr_phase::REVIEW_DONE),
        1,
        "request_changes should push back to ReviewDone"
    );
}

// ═══════════════════════════════════════════════
// C-4: max_iterations 미적용 검증
// ═══════════════════════════════════════════════

/// C-4: 리뷰 사이클이 max_iterations 없이 무한 반복 가능함을 검증
///
/// ReviewConfig::max_iterations는 2로 설정되어 있지만
/// process_improved에서 iteration count를 추적하지 않아
/// 무한 루프가 발생할 수 있다.
#[tokio::test]
async fn review_cycle_has_no_iteration_limit_c4() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db);

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockClaude::new();

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let sw = MockSuggestWorkflow::new();

    // 3번의 review → request_changes 시뮬레이션 (max_iterations=2 초과)
    // Round 1: improve → improved
    claude.enqueue_response(r#"{"result": "Applied fix 1"}"#, 0);
    // Round 1: re-review → request_changes (verdict-based)
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"request_changes\",\"summary\":\"Still needs work\"}"}"#,
        0,
    );
    // Round 2: improve → improved
    claude.enqueue_response(r#"{"result": "Applied fix 2"}"#, 0);
    // Round 2: re-review → request_changes (verdict-based)
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"request_changes\",\"summary\":\"Still not right\"}"}"#,
        0,
    );
    // Round 3: improve → improved (EXCEEDS max_iterations=2)
    claude.enqueue_response(r#"{"result": "Applied fix 3"}"#, 0);
    // Round 3: re-review → approved (verdict-based)
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"approve\",\"summary\":\"LGTM\"}"}"#,
        0,
    );
    // Knowledge extraction
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let mut queues = TaskQueues::new();

    // Start with PR in ReviewDone (first iteration)
    let mut item = make_pr_item(&repo_id, 80);
    item.review_comment = Some("Fix these issues".to_string());
    queues.prs.push(pr_phase::REVIEW_DONE, item);

    // Run 3 full cycles
    for _round in 1..=3 {
        // ReviewDone → Improved
        autodev::pipeline::pr::process_review_done(
            &db,
            &env,
            &workspace,
            &gh,
            &claude,
            &mut queues,
        )
        .await
        .unwrap();

        if queues.prs.len(pr_phase::IMPROVED) > 0 {
            // Improved → re-review
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

            // If pushed back to ReviewDone, the cycle continues
            if queues.prs.len(pr_phase::REVIEW_DONE) == 0 {
                break;
            }
        }
    }

    // C-4: 모든 7개 Claude 호출이 완료됨 = 3회 반복 가능 (max_iterations=2 초과)
    assert_eq!(
        claude.call_count(),
        7,
        "C-4: review cycle ran 3 iterations (exceeds max_iterations=2) — no limit enforced"
    );
}

// ═══════════════════════════════════════════════
// Issue pipeline: worktree 정리 정상 동작 확인 (비교 기준)
// ═══════════════════════════════════════════════

fn make_issue_item(repo_id: &str, number: i64) -> IssueItem {
    IssueItem {
        work_id: make_work_id("issue", "org/repo", number),
        repo_id: repo_id.to_string(),
        repo_name: "org/repo".to_string(),
        repo_url: "https://github.com/org/repo".to_string(),
        github_number: number,
        title: "Test issue".to_string(),
        body: Some("body".to_string()),
        labels: vec![],
        author: "alice".to_string(),
        analysis_report: Some("report".to_string()),
    }
}

#[tokio::test]
async fn issue_ready_cleans_up_worktree() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db);

    let gh = MockGh::new();
    let git = MockGit::new();
    let claude = MockClaude::new();
    claude.enqueue_response(r#"{"result": "Done"}"#, 0);
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let workspace = Workspace::new(&git, &env);
    let sw = MockSuggestWorkflow::new();
    let mut queues = TaskQueues::new();
    queues
        .issues
        .push(issue_phase::READY, make_issue_item(&repo_id, 90));

    autodev::pipeline::issue::process_ready(&db, &env, &workspace, &gh, &claude, &sw, &mut queues)
        .await
        .unwrap();

    // Issue pipeline은 항상 worktree 정리 수행 (올바른 동작)
    assert!(
        has_worktree_remove(&git),
        "issue process_ready should clean up worktree (reference for correct behavior)"
    );
}
