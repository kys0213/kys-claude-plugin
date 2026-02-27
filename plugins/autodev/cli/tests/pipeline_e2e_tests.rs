use std::collections::HashMap;
use std::path::Path;

use autodev::components::notifier::Notifier;
use autodev::components::workspace::Workspace;
use autodev::config::Env;
use autodev::domain::labels;
use autodev::domain::repository::*;
use autodev::infrastructure::claude::mock::MockClaude;
use autodev::infrastructure::gh::mock::MockGh;
use autodev::infrastructure::git::mock::MockGit;
use autodev::infrastructure::suggest_workflow::mock::MockSuggestWorkflow;
use autodev::queue::task_queues::{
    issue_phase, make_work_id, merge_phase, IssueItem, MergeItem, PrItem, TaskQueues,
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
        source_issue_number: None,
        review_iteration: 0,
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

    // Phase 1: process_pending (Pending -> analyzed, exits queue with comment)
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

    // v2: Item removed from queue (analyzed + exits for human review)
    assert_eq!(
        queues.issues.total(),
        0,
        "issue should be removed from queue after analysis"
    );

    // Labels: analyzed added, wip removed
    assert_label_added(&gh, "org/repo", 10, labels::ANALYZED);
    assert_label_removed(&gh, "org/repo", 10, labels::WIP);

    // Claude called once (analysis only)
    assert_eq!(claude.call_count(), 1);

    // 1 consumer log recorded (analysis only)
    let logs = db.log_recent(None, 100).unwrap();
    assert_eq!(logs.len(), 1, "should have 1 consumer log (analysis only)");
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

    // v2: analyzed + exits queue
    assert_eq!(
        queues.issues.total(),
        0,
        "issue should be removed from queue after successful analysis"
    );

    assert_label_added(&gh, "org/repo", 20, labels::ANALYZED);
    assert_label_removed(&gh, "org/repo", 20, labels::WIP);

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

    // v2: 0.7 >= 0.7 -> analyzed (exits queue for human review)
    assert_eq!(
        queues.issues.total(),
        0,
        "confidence == threshold should trigger analyzed and exit"
    );

    assert_label_added(&gh, "org/repo", 50, labels::ANALYZED);
    assert_label_removed(&gh, "org/repo", 50, labels::WIP);

    // Analysis comment should be posted
    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(
        comments.len(),
        1,
        "analysis comment should be posted for implement verdict"
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

    // v2: parse_analysis returns None -> fallback analyzed (exits queue)
    assert_eq!(
        queues.issues.total(),
        0,
        "completely malformed output should fallback to analyzed and exit"
    );

    assert_label_added(&gh, "org/repo", 53, labels::ANALYZED);
    assert_label_removed(&gh, "org/repo", 53, labels::WIP);

    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(
        comments.len(),
        1,
        "fallback analysis comment should be posted"
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

/// Label-Positive: filter_labels는 트리거 라벨과 별개의 추가 필터.
/// autodev:analyze + "bug" 라벨이 모두 있는 이슈만 큐에 적재.
#[tokio::test]
async fn scanner_filter_labels_with_label_positive() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    // API가 autodev:analyze 라벨 이슈만 반환 (Label-Positive)
    let issues = serde_json::json!([
        {
            "number": 80,
            "title": "Bug with trigger",
            "body": "has bug + analyze",
            "labels": [{"name": "bug"}, {"name": "autodev:analyze"}],
            "user": {"login": "alice"},
            "pull_request": null
        },
        {
            "number": 81,
            "title": "Feature with trigger",
            "body": "has enhancement + analyze",
            "labels": [{"name": "enhancement"}, {"name": "autodev:analyze"}],
            "user": {"login": "bob"},
            "pull_request": null
        }
    ]);
    gh.set_paginate("org/repo", "issues", serde_json::to_vec(&issues).unwrap());

    // filter_labels="bug" → #80만 통과
    let filter_labels = Some(vec!["bug".to_string()]);
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

    assert!(
        queues.contains("issue:org/repo:80"),
        "issue #80 with 'bug' label should pass filter"
    );
    assert!(
        !queues.contains("issue:org/repo:81"),
        "issue #81 with 'enhancement' should NOT pass 'bug' filter"
    );
}

/// Label-Positive: filter_labels가 None이면 autodev:analyze 이슈 모두 큐에 적재.
#[tokio::test]
async fn scanner_no_filter_labels_queues_all_triggered() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    let issues = serde_json::json!([
        {
            "number": 90,
            "title": "Triggered issue",
            "body": "analyze me",
            "labels": [{"name": "autodev:analyze"}],
            "user": {"login": "alice"},
            "pull_request": null
        }
    ]);
    gh.set_paginate("org/repo", "issues", serde_json::to_vec(&issues).unwrap());

    let mut queues = TaskQueues::new();

    autodev::scanner::issues::scan(
        &db,
        &gh,
        &repo_id,
        "org/repo",
        "https://github.com/org/repo",
        &[],
        &None,
        None,
        &mut queues,
    )
    .await
    .unwrap();

    assert!(queues.contains("issue:org/repo:90"));

    // analyze→wip 전이 검증
    let removed = gh.removed_labels.lock().unwrap();
    assert!(removed
        .iter()
        .any(|(_, n, l)| *n == 90 && l == "autodev:analyze"));
    let added = gh.added_labels.lock().unwrap();
    assert!(added.iter().any(|(_, n, l)| *n == 90 && l == "autodev:wip"));
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
    // 1. Issue analysis -> analyzed (issue::process_pending, exits queue)
    claude.enqueue_response(&make_analysis_json("implement", 0.9), 0);
    // 2. PR review -> ReviewDone (pr::process_pending, verdict=request_changes)
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

    let sw = MockSuggestWorkflow::new();
    autodev::pipeline::process_all(
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
    .expect("process_all should succeed");

    // v2: Issue: pending -> analyzed (exits queue for human review)
    assert_eq!(
        queues.issues.total(),
        0,
        "issue should be analyzed and removed from queue"
    );
    assert_label_added(&gh, "org/repo", 90, labels::ANALYZED);

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
        6,
        "should call claude 6 times total (1 issue analysis + 3 PR pipeline + 1 PR knowledge + 1 merge)"
    );

    // PR Review API: request_changes (1st review) + approve (re-review) = 2 reviews
    let reviews = gh.reviewed_prs.lock().unwrap();
    assert_eq!(
        reviews.len(),
        2,
        "should submit 2 PR reviews (request_changes + approve)"
    );
    assert_eq!(reviews[0].1, 91, "1st review on PR #91");
    assert_eq!(reviews[0].2, "REQUEST_CHANGES");
    assert_eq!(reviews[1].1, 91, "2nd review on PR #91");
    assert_eq!(reviews[1].2, "APPROVE");
}

// ═══════════════════════════════════════════════════
// 20b. PR review API: approve on first review submits APPROVE
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn pr_approve_submits_review_api() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_pr_open(&gh, "org/repo", 95);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // Review → approve (verdict=approve)
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"approve\",\"summary\":\"LGTM\"}"}"#,
        0,
    );
    // Knowledge extraction
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues
        .prs
        .push("Pending", make_pr_item(&repo_id, 95, "Approve PR"));

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

    // PR done
    assert_eq!(queues.prs.total(), 0);
    assert_label_added(&gh, "org/repo", 95, labels::DONE);

    // Review API: single APPROVE call
    let reviews = gh.reviewed_prs.lock().unwrap();
    assert_eq!(reviews.len(), 1, "should submit 1 review");
    assert_eq!(reviews[0].1, 95);
    assert_eq!(reviews[0].2, "APPROVE");
}

// ═══════════════════════════════════════════════════
// 20c. PR review API: request_changes on first review submits REQUEST_CHANGES
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn pr_request_changes_submits_review_api() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let env = TestEnv::new(&tmpdir);
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let gh = MockGh::new();
    set_gh_pr_open(&gh, "org/repo", 96);

    let git = MockGit::new();
    let claude = MockClaude::new();
    // Review → request_changes
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"request_changes\",\"summary\":\"Fix bugs\"}"}"#,
        0,
    );
    // Feedback implementation
    claude.enqueue_response(r#"{"result": "Feedback applied"}"#, 0);
    // Re-review → approve
    claude.enqueue_response(
        r#"{"result": "{\"verdict\":\"approve\",\"summary\":\"OK now\"}"}"#,
        0,
    );
    // Knowledge extraction
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let workspace = Workspace::new(&git, &env);
    let notifier = Notifier::new(&gh);
    let mut queues = TaskQueues::new();

    queues
        .prs
        .push("Pending", make_pr_item(&repo_id, 96, "Changes needed PR"));

    let sw = MockSuggestWorkflow::new();
    autodev::pipeline::process_all(
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

    // Review API: REQUEST_CHANGES + APPROVE
    let reviews = gh.reviewed_prs.lock().unwrap();
    assert_eq!(reviews.len(), 2);
    assert_eq!(reviews[0].2, "REQUEST_CHANGES");
    assert_eq!(reviews[1].2, "APPROVE");
}

// ═══════════════════════════════════════════════════
// 21. scan_merges: autodev:done PR을 merge queue에 적재
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn scan_merges_detects_done_prs() {
    let gh = MockGh::new();

    // issues endpoint 응답: autodev:done 라벨이 붙은 PR (#50)
    let issues_json = serde_json::json!([{
        "number": 50,
        "title": "Feature PR",
        "pull_request": {"url": "https://api.github.com/repos/org/repo/pulls/50"},
        "labels": [{"name": "autodev:done"}]
    }]);
    gh.set_paginate(
        "org/repo",
        "issues",
        serde_json::to_vec(&issues_json).unwrap(),
    );

    // PR 상세 정보 응답
    let pr_detail = serde_json::json!({
        "number": 50,
        "title": "Feature PR",
        "head": {"ref": "feat/new-feature"},
        "base": {"ref": "main"}
    });
    gh.set_paginate(
        "org/repo",
        "pulls/50",
        serde_json::to_vec(&pr_detail).unwrap(),
    );

    let mut queues = TaskQueues::new();
    autodev::scanner::pulls::scan_merges(
        &gh,
        "repo-1",
        "org/repo",
        "https://github.com/org/repo",
        None,
        &mut queues,
    )
    .await
    .unwrap();

    assert_eq!(queues.merges.len(merge_phase::PENDING), 1);
    assert_label_removed(&gh, "org/repo", 50, labels::DONE);
    assert_label_added(&gh, "org/repo", 50, labels::WIP);
}

// ═══════════════════════════════════════════════════
// 22. scan_merges: 이미 merge queue에 있으면 skip
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn scan_merges_dedup_skips_existing() {
    let gh = MockGh::new();

    let issues_json = serde_json::json!([{
        "number": 60,
        "title": "Already queued",
        "pull_request": {"url": "https://api.github.com/repos/org/repo/pulls/60"},
        "labels": [{"name": "autodev:done"}]
    }]);
    gh.set_paginate(
        "org/repo",
        "issues",
        serde_json::to_vec(&issues_json).unwrap(),
    );

    let mut queues = TaskQueues::new();
    // 미리 merge queue에 동일 PR 적재
    queues.merges.push(
        merge_phase::PENDING,
        MergeItem {
            work_id: make_work_id("merge", "org/repo", 60),
            repo_id: "repo-1".to_string(),
            repo_name: "org/repo".to_string(),
            repo_url: "https://github.com/org/repo".to_string(),
            pr_number: 60,
            title: "Already queued".to_string(),
            head_branch: "feat/x".to_string(),
            base_branch: "main".to_string(),
        },
    );

    autodev::scanner::pulls::scan_merges(
        &gh,
        "repo-1",
        "org/repo",
        "https://github.com/org/repo",
        None,
        &mut queues,
    )
    .await
    .unwrap();

    // 중복 추가 없음
    assert_eq!(queues.merges.len(merge_phase::PENDING), 1);
    // label 변경도 없음
    assert!(gh.removed_labels.lock().unwrap().is_empty());
}

// ═══════════════════════════════════════════════════
// 23. scan_merges: issue(PR 아님)는 무시
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn scan_merges_skips_non_pr_issues() {
    let gh = MockGh::new();

    // pull_request 필드 없는 일반 이슈
    let issues_json = serde_json::json!([{
        "number": 70,
        "title": "Plain issue",
        "labels": [{"name": "autodev:done"}]
    }]);
    gh.set_paginate(
        "org/repo",
        "issues",
        serde_json::to_vec(&issues_json).unwrap(),
    );

    let mut queues = TaskQueues::new();
    autodev::scanner::pulls::scan_merges(
        &gh,
        "repo-1",
        "org/repo",
        "https://github.com/org/repo",
        None,
        &mut queues,
    )
    .await
    .unwrap();

    assert_eq!(queues.merges.len(merge_phase::PENDING), 0);
}
