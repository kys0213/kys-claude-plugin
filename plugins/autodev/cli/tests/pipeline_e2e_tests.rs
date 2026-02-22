use std::collections::HashMap;
use std::path::Path;

use autodev::components::notifier::Notifier;
use autodev::components::workspace::Workspace;
use autodev::config::Env;
use autodev::infrastructure::claude::mock::MockClaude;
use autodev::infrastructure::gh::mock::MockGh;
use autodev::infrastructure::git::mock::MockGit;
use autodev::queue::repository::*;
use autodev::queue::task_queues::{
    issue_phase, labels, make_work_id, IssueItem, MergeItem, PrItem, TaskQueues,
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

fn fixture_response(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/responses")
        .join(name);
    std::fs::read(path).expect("read fixture file")
}

/// Assert that the added_labels on MockGh contain the given (repo_name, number, label) tuple.
fn assert_label_added(gh: &MockGh, repo_name: &str, number: i64, label: &str) {
    let added = gh.added_labels.lock().unwrap();
    assert!(
        added
            .iter()
            .any(|(r, n, l)| r == repo_name && *n == number && l == label),
        "expected added label ({repo_name}, {number}, {label}) but found: {added:?}"
    );
}

/// Assert that the removed_labels on MockGh contain the given (repo_name, number, label) tuple.
fn assert_label_removed(gh: &MockGh, repo_name: &str, number: i64, label: &str) {
    let removed = gh.removed_labels.lock().unwrap();
    assert!(
        removed
            .iter()
            .any(|(r, n, l)| r == repo_name && *n == number && l == label),
        "expected removed label ({repo_name}, {number}, {label}) but found: {removed:?}"
    );
}

/// Assert that the added_labels on MockGh do NOT contain the given (repo_name, number, label).
fn assert_label_not_added(gh: &MockGh, repo_name: &str, number: i64, label: &str) {
    let added = gh.added_labels.lock().unwrap();
    assert!(
        !added
            .iter()
            .any(|(r, n, l)| r == repo_name && *n == number && l == label),
        "expected label ({repo_name}, {number}, {label}) NOT to be added but it was"
    );
}

// ═══════════════════════════════════════════════════
// 1. Issue full cycle: pending → analyzing → ready → implementing → done
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn issue_full_cycle_pending_to_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 10);

    let git = MockGit::new();
    let claude = MockClaude::new();

    // Phase 1: analysis with high confidence -> ready
    claude.enqueue_response(&make_analysis_json("implement", 0.9), 0);
    // Phase 2: implementation -> success
    claude.enqueue_response(r#"{"result": "Implementation complete"}"#, 0);
    // Phase 2.5: knowledge extraction (best effort, after implementation done)
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    // Push issue to Pending queue
    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 10, "Full cycle issue"),
    );

    // Phase 1: process_pending (Pending -> Ready)
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
    .expect("phase 1 should succeed");

    assert_eq!(
        queues.issues.len(issue_phase::READY),
        1,
        "issue should be in Ready queue after phase 1"
    );

    // Phase 2: process_ready (Ready -> done, removed from queue)
    autodev::pipeline::issue::process_ready(&db, &env, &workspace, &gh, &claude, &mut queues)
        .await
        .expect("phase 2 should succeed");

    // Item removed from queue (done)
    assert_eq!(
        queues.issues.total(),
        0,
        "issue should be removed from queue after done"
    );

    // Labels: done added, wip removed
    assert_label_added(&gh, "org/repo", 10, labels::DONE);
    assert_label_removed(&gh, "org/repo", 10, labels::WIP);

    // Claude called 3 times (analysis + implementation + knowledge extraction)
    assert_eq!(claude.call_count(), 3);

    // 2 consumer logs recorded (knowledge extraction doesn't create a consumer log)
    let logs = db.log_recent(None, 100).unwrap();
    assert_eq!(
        logs.len(),
        2,
        "should have 2 consumer logs (analysis + implementation)"
    );
}

// ═══════════════════════════════════════════════════
// 2. Issue failed -> re-discovery -> success cycle
//    In the new architecture, there is no DB retry.
//    When an issue fails, wip is removed. On next scan,
//    it would be re-discovered and re-queued.
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn issue_failed_retry_reentry_cycle() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 20);

    let git = MockGit::new();
    let claude = MockClaude::new();

    // 1st attempt: Claude fails (exit_code 1)
    claude.enqueue_response("analysis error", 1);
    // 2nd attempt (after re-queue): Claude succeeds
    claude.enqueue_response(&make_analysis_json("implement", 0.85), 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    // Push issue to Pending
    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 20, "Retry cycle issue"),
    );

    // 1st attempt -> fails, item removed from queue, wip removed
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

    assert_eq!(
        queues.issues.total(),
        0,
        "issue should be gone from queue after failure"
    );
    assert_label_removed(&gh, "org/repo", 20, labels::WIP);

    // Simulate re-discovery: push same issue again (as scanner would)
    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 20, "Retry cycle issue"),
    );

    // 2nd attempt -> succeeds -> Ready
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

    assert_eq!(
        queues.issues.len(issue_phase::READY),
        1,
        "issue should be in Ready queue after successful retry"
    );

    assert_eq!(
        claude.call_count(),
        2,
        "claude should be called twice (fail + success)"
    );
}

// ═══════════════════════════════════════════════════
// 3. PR pre-flight: closed PR -> skip to done
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn pr_closed_on_github_skips_to_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    // PR is closed
    gh.set_field("org/repo", "pulls/30", ".state", "closed");

    let git = MockGit::new();
    let claude = MockClaude::new();

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues
        .prs
        .push("Pending", make_pr_item(&repo_id, 30, "Already closed PR"));

    autodev::pipeline::pr::process_pending(
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

    // Item removed from queue
    assert_eq!(
        queues.prs.total(),
        0,
        "closed PR should be removed from queue"
    );

    // Labels: done added, wip removed
    assert_label_added(&gh, "org/repo", 30, labels::DONE);
    assert_label_removed(&gh, "org/repo", 30, labels::WIP);

    assert_eq!(
        claude.call_count(),
        0,
        "claude should not be called for closed PR"
    );
}

// ═══════════════════════════════════════════════════
// 4. PR pre-flight: already approved -> skip to done
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn pr_already_approved_skips_to_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

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
    let mut queues = TaskQueues::new();

    queues
        .prs
        .push("Pending", make_pr_item(&repo_id, 31, "Already approved PR"));

    autodev::pipeline::pr::process_pending(
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

    // Item removed from queue
    assert_eq!(
        queues.prs.total(),
        0,
        "approved PR should be removed from queue"
    );

    // Labels: done added, wip removed
    assert_label_added(&gh, "org/repo", 31, labels::DONE);
    assert_label_removed(&gh, "org/repo", 31, labels::WIP);

    assert_eq!(
        claude.call_count(),
        0,
        "claude should not be called for approved PR"
    );
}

// ═══════════════════════════════════════════════════
// 5. Merge success cycle: pending -> merging -> done
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn merge_success_cycle() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_pr_open(&gh, "org/repo", 40);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // merge success (exit_code 0)
    claude.enqueue_response("Merged successfully", 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues
        .merges
        .push("Pending", make_merge_item(&repo_id, 40, "Merge PR #40"));

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

    // Item removed from queue
    assert_eq!(
        queues.merges.total(),
        0,
        "merge should be removed from queue after done"
    );

    // Labels: done added, wip removed
    assert_label_added(&gh, "org/repo", 40, labels::DONE);
    assert_label_removed(&gh, "org/repo", 40, labels::WIP);

    assert_eq!(claude.call_count(), 1);
}

// ═══════════════════════════════════════════════════
// 6. Merge conflict -> resolve success
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn merge_conflict_then_resolve_success() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_pr_open(&gh, "org/repo", 41);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // 1st call: merge attempt -> conflict (exit_code 1 + "conflict" in output)
    claude.enqueue_response("CONFLICT (content): merge conflict in src/main.rs", 1);
    // 2nd call: resolve_conflicts -> success
    claude.enqueue_response("Conflicts resolved and committed", 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues.merges.push(
        "Pending",
        make_merge_item(&repo_id, 41, "Merge PR #41 with conflict"),
    );

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

    // Item removed from queue (done after conflict resolution)
    assert_eq!(
        queues.merges.total(),
        0,
        "merge should be removed from queue after conflict resolution"
    );

    // Labels: done added, wip removed
    assert_label_added(&gh, "org/repo", 41, labels::DONE);
    assert_label_removed(&gh, "org/repo", 41, labels::WIP);

    assert_eq!(
        claude.call_count(),
        2,
        "should call claude twice (merge + resolve)"
    );
}

// ═══════════════════════════════════════════════════
// 7. Merge conflict -> resolve failure
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn merge_conflict_then_resolve_failure() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_pr_open(&gh, "org/repo", 42);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // 1st call: merge -> conflict
    claude.enqueue_response("CONFLICT (content): merge conflict in complex.rs", 1);
    // 2nd call: resolve -> failure (exit_code 1, no "conflict" keyword -> Failed)
    claude.enqueue_response("Failed to resolve", 1);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues.merges.push(
        "Pending",
        make_merge_item(&repo_id, 42, "Merge PR #42 unresolvable conflict"),
    );

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

    // Item removed from queue (failed)
    assert_eq!(
        queues.merges.total(),
        0,
        "merge should be removed from queue after resolve failure"
    );

    // wip removed but NO done label (it failed)
    assert_label_removed(&gh, "org/repo", 42, labels::WIP);
    assert_label_not_added(&gh, "org/repo", 42, labels::DONE);

    assert_eq!(claude.call_count(), 2);
}

// ═══════════════════════════════════════════════════
// 8. Merge pre-flight: already merged -> skip to done
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn merge_already_merged_skips_to_done() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    // PR is already closed/merged
    gh.set_field("org/repo", "pulls/43", ".state", "closed");

    let git = MockGit::new();
    let claude = MockClaude::new();

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues.merges.push(
        "Pending",
        make_merge_item(&repo_id, 43, "Already merged PR"),
    );

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

    // Item removed from queue
    assert_eq!(
        queues.merges.total(),
        0,
        "already merged PR should be removed from queue"
    );

    // Labels: done added, wip removed
    assert_label_added(&gh, "org/repo", 43, labels::DONE);
    assert_label_removed(&gh, "org/repo", 43, labels::WIP);

    assert_eq!(claude.call_count(), 0, "claude should not be called");
}

// ═══════════════════════════════════════════════════
// 9. Confidence at threshold (0.7) -> ready
//    Code: a.confidence < cfg.consumer.confidence_threshold
//    0.7 < 0.7 is false -> 'implement' match -> ready
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn confidence_at_threshold_goes_to_ready() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 50);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // confidence exactly at threshold (0.7)
    claude.enqueue_response(&make_analysis_json("implement", 0.7), 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 50, "Boundary confidence issue"),
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

    // 0.7 NOT < 0.7 -> ready (not waiting_human)
    assert_eq!(
        queues.issues.len(issue_phase::READY),
        1,
        "confidence == threshold should go to ready"
    );

    // No clarification comment should be posted
    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(
        comments.len(),
        0,
        "no clarification comment for threshold confidence"
    );
}

// ═══════════════════════════════════════════════════
// 10. Confidence just below threshold (0.69) -> waiting (skip label + comment)
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn confidence_just_below_threshold_goes_to_waiting() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 51);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // confidence just below threshold
    claude.enqueue_response(&make_analysis_json("implement", 0.69), 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 51, "Below threshold issue"),
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

    // 0.69 < 0.7 -> waiting_human: item removed from queue, skip label added, comment posted
    assert_eq!(
        queues.issues.total(),
        0,
        "issue should be removed from queue when confidence below threshold"
    );

    // skip label added, wip removed
    assert_label_added(&gh, "org/repo", 51, labels::SKIP);
    assert_label_removed(&gh, "org/repo", 51, labels::WIP);

    // clarification comment posted
    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1, "should post clarification comment");
}

// ═══════════════════════════════════════════════════
// 11. Analysis JSON missing confidence field -> fallback
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn analysis_missing_confidence_field_falls_back_to_ready() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 52);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // JSON without confidence field -> parse_analysis may return None -> fallback ready
    let bad_json = serde_json::json!({
        "result": r#"{"verdict":"implement","summary":"test","questions":[],"report":"report"}"#
    });
    claude.enqueue_response(&bad_json.to_string(), 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 52, "Missing confidence field"),
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

    // When confidence is missing, parse_analysis may either:
    //   - return Some with confidence=0.0 -> skip label (waiting_human)
    //   - return None -> fallback ready
    // Either way, pipeline should not crash
    let in_ready = queues.issues.len(issue_phase::READY);
    let total = queues.issues.total();

    assert!(
        in_ready == 1 || total == 0,
        "missing confidence should either fallback to ready (ready=1) or go to skip (total=0), got ready={in_ready}, total={total}"
    );
}

// ═══════════════════════════════════════════════════
// 12. Completely malformed JSON -> fallback ready
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn analysis_completely_malformed_json_falls_back_to_ready() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 53);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // Completely malformed output (not JSON)
    claude.enqueue_response("I cannot parse this {{{garbage", 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 53, "Malformed JSON issue"),
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

    // parse_analysis returns None -> fallback ready
    assert_eq!(
        queues.issues.len(issue_phase::READY),
        1,
        "completely malformed output should fallback to ready"
    );
}

// ═══════════════════════════════════════════════════
// 13. Workspace clone failure -> issue failed
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn workspace_clone_failure_marks_issue_failed() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 60);

    let git = MockGit::new();
    *git.clone_should_fail.lock().unwrap() = true;

    let claude = MockClaude::new();

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 60, "Clone will fail"),
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

    // Item removed from queue (failed)
    assert_eq!(
        queues.issues.total(),
        0,
        "clone failure should remove issue from queue"
    );

    // wip removed
    assert_label_removed(&gh, "org/repo", 60, labels::WIP);

    assert_eq!(
        claude.call_count(),
        0,
        "claude should not be called when clone fails"
    );
}

// ═══════════════════════════════════════════════════
// 14. Workspace worktree failure -> issue failed
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn workspace_worktree_failure_marks_issue_failed() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 61);

    let git = MockGit::new();
    *git.worktree_should_fail.lock().unwrap() = true;

    let claude = MockClaude::new();

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 61, "Worktree will fail"),
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

    // Item removed from queue (failed)
    assert_eq!(
        queues.issues.total(),
        0,
        "worktree failure should remove issue from queue"
    );

    // wip removed
    assert_label_removed(&gh, "org/repo", 61, labels::WIP);

    assert_eq!(
        claude.call_count(),
        0,
        "claude should not be called when worktree fails"
    );
}

// ═══════════════════════════════════════════════════
// 15. Workspace clone failure -> PR failed
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn workspace_clone_failure_marks_pr_failed() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_pr_open(&gh, "org/repo", 70);

    let git = MockGit::new();
    *git.clone_should_fail.lock().unwrap() = true;

    let claude = MockClaude::new();

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues.prs.push(
        "Pending",
        make_pr_item(&repo_id, 70, "Clone will fail for PR"),
    );

    autodev::pipeline::pr::process_pending(
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

    // Item removed from queue (failed)
    assert_eq!(
        queues.prs.total(),
        0,
        "clone failure should remove PR from queue"
    );

    // wip removed
    assert_label_removed(&gh, "org/repo", 70, labels::WIP);

    assert_eq!(claude.call_count(), 0);
}

// ═══════════════════════════════════════════════════
// 16. Workspace clone failure -> merge failed
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn workspace_clone_failure_marks_merge_failed() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_pr_open(&gh, "org/repo", 80);

    let git = MockGit::new();
    *git.clone_should_fail.lock().unwrap() = true;

    let claude = MockClaude::new();

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues.merges.push(
        "Pending",
        make_merge_item(&repo_id, 80, "Clone will fail for merge"),
    );

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

    // Item removed from queue (failed)
    assert_eq!(
        queues.merges.total(),
        0,
        "clone failure should remove merge from queue"
    );

    // wip removed
    assert_label_removed(&gh, "org/repo", 80, labels::WIP);

    assert_eq!(claude.call_count(), 0);
}

// ═══════════════════════════════════════════════════
// 17. Scanner: autodev label filtering - "autodev" label include filter
// ═══════════════════════════════════════════════════

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

    // Scan with "autodev" label filter
    let filter_labels = Some(vec!["autodev".to_string()]);
    let mut queues = TaskQueues::new();

    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        &filter_labels,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    // #60: labels=["bug","autodev"] -> "autodev" matches filter, no "autodev:" prefix -> queued
    assert!(
        queues.contains("issue:org/repo:60"),
        "issue #60 with 'autodev' label should be queued"
    );
    // #61: labels=["bug","autodev:wip"] -> has "autodev:" prefix -> skipped by has_autodev_label
    assert!(
        !queues.contains("issue:org/repo:61"),
        "issue #61 with 'autodev:wip' should NOT match 'autodev'"
    );
    // #62: labels=["enhancement","autodev:done"] -> skipped
    assert!(
        !queues.contains("issue:org/repo:62"),
        "issue #62 with 'autodev:done' should NOT match 'autodev'"
    );
    // #63: labels=["wontfix","autodev:skip"] -> skipped
    assert!(
        !queues.contains("issue:org/repo:63"),
        "issue #63 with 'autodev:skip' should NOT match 'autodev'"
    );
    // #64: labels=["enhancement"] -> no autodev label -> filter doesn't match
    assert!(
        !queues.contains("issue:org/repo:64"),
        "issue #64 without autodev label should NOT be queued"
    );
}

// ═══════════════════════════════════════════════════
// 18. Scanner: autodev:wip label filter - only wip issues
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

    // Scan with "autodev:wip" label filter
    let filter_labels = Some(vec!["autodev:wip".to_string()]);
    let mut queues = TaskQueues::new();

    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        &filter_labels,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    // Only #61 should match (autodev:wip label)
    // But #61 has "autodev:wip" which starts with "autodev:" -> has_autodev_label returns true -> skipped
    assert!(
        !queues.contains("issue:org/repo:60"),
        "issue #60 should not match autodev:wip"
    );
    assert!(
        !queues.contains("issue:org/repo:61"),
        "issue #61 with autodev:wip should be skipped by has_autodev_label check"
    );
    assert!(!queues.contains("issue:org/repo:62"));
    assert!(!queues.contains("issue:org/repo:63"));
    assert!(!queues.contains("issue:org/repo:64"));
}

// ═══════════════════════════════════════════════════
// 19. Merge non-conflict failure -> failed
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn merge_non_conflict_failure_marks_failed() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_pr_open(&gh, "org/repo", 44);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // merge fails with exit_code 1 but no "conflict" keyword -> MergeOutcome::Failed
    claude.enqueue_response("Permission denied: cannot push to protected branch", 1);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues.merges.push(
        "Pending",
        make_merge_item(&repo_id, 44, "Merge will fail non-conflict"),
    );

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

    // Item removed from queue (failed)
    assert_eq!(
        queues.merges.total(),
        0,
        "non-conflict failure should remove merge from queue"
    );

    // wip removed, no done label
    assert_label_removed(&gh, "org/repo", 44, labels::WIP);
    assert_label_not_added(&gh, "org/repo", 44, labels::DONE);

    // resolve_conflicts should NOT be called
    assert_eq!(
        claude.call_count(),
        1,
        "should only attempt merge, not resolve"
    );
}

// ═══════════════════════════════════════════════════
// 20. process_all: issue + PR + merge all processed
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn process_all_handles_all_queues() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_issue_open(&gh, "org/repo", 90);
    set_gh_pr_open(&gh, "org/repo", 91);
    set_gh_pr_open(&gh, "org/repo", 92);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // 1. Issue analysis -> ready (issue::process_pending)
    claude.enqueue_response(&make_analysis_json("implement", 0.9), 0);
    // 2. Issue implementation -> done (issue::process_ready)
    claude.enqueue_response(r#"{"result": "Implementation done"}"#, 0);
    // 3. Issue knowledge extraction (best effort, after issue done)
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);
    // 4. PR review -> ReviewDone (pr::process_pending, verdict=request_changes)
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"request_changes\",\"summary\":\"Needs changes\"}"}"#,
        0,
    );
    // 5. PR feedback implementation -> Improved (pr::process_review_done)
    claude.enqueue_response(r#"{"result": "Feedback applied"}"#, 0);
    // 6. PR re-review -> done/approved (pr::process_improved, verdict=approve)
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"approve\",\"summary\":\"Approved\"}"}"#,
        0,
    );
    // 7. PR knowledge extraction (best effort, after PR done)
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);
    // 8. Merge -> success (merge::process_pending)
    claude.enqueue_response("Merged successfully", 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues.issues.push(
        issue_phase::PENDING,
        make_issue_item(&repo_id, 90, "Issue for process_all"),
    );
    queues
        .prs
        .push("Pending", make_pr_item(&repo_id, 91, "PR for process_all"));
    queues.merges.push(
        "Pending",
        make_merge_item(&repo_id, 92, "Merge for process_all"),
    );

    autodev::pipeline::process_all(&db, &env, &workspace, &notifier, &gh, &claude, &mut queues)
        .await
        .expect("process_all should succeed");

    // Issue: pending -> ready -> done (both phases in same process_all)
    assert_eq!(
        queues.issues.total(),
        0,
        "issue should be done and removed from queue"
    );
    assert_label_added(&gh, "org/repo", 90, labels::DONE);

    // PR: pending -> ReviewDone -> Improved -> done (full cycle in same process_all)
    assert_eq!(
        queues.prs.total(),
        0,
        "pr should be done and removed from queue"
    );
    assert_label_added(&gh, "org/repo", 91, labels::DONE);

    // Merge: pending -> done (removed from queue)
    assert_eq!(
        queues.merges.total(),
        0,
        "merge should be done and removed from queue"
    );
    assert_label_added(&gh, "org/repo", 92, labels::DONE);

    assert_eq!(
        claude.call_count(),
        8,
        "should call claude 8 times total (6 pipeline + 2 knowledge extraction)"
    );
}
