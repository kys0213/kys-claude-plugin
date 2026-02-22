use autodev::queue::Database;
use autodev::queue::repository::{
    IssueQueueRepository, MergeQueueRepository, PrQueueRepository, QueueAdmin, RepoRepository,
};
use autodev::queue::models::*;
use tempfile::TempDir;

fn setup_db() -> (TempDir, Database) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = Database::open(&db_path).unwrap();
    db.initialize().unwrap();
    (tmp, db)
}

fn seed_repo(db: &Database, name: &str) -> String {
    db.repo_add(&format!("https://github.com/{name}"), name)
        .unwrap()
}

// ─── views::query_active_items tests ───

// Note: query_active_items and query_label_counts are pub functions in tui::views
// but they depend on the tui module which requires crossterm/ratatui at compile time.
// We test the underlying data queries via SQL directly to keep tests headless.

#[test]
fn test_active_items_query_returns_non_terminal() {
    let (_tmp, db) = setup_db();
    let repo_id = seed_repo(&db, "org/repo");

    // Insert issue in pending state (active)
    db.issue_insert(&NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 1,
        title: "Bug fix".to_string(),
        body: None,
        labels: "".to_string(),
        author: "user".to_string(),
    })
    .unwrap();

    // Insert issue in done state (not active)
    let done_id = db
        .issue_insert(&NewIssueItem {
            repo_id: repo_id.clone(),
            github_number: 2,
            title: "Done issue".to_string(),
            body: None,
            labels: "".to_string(),
            author: "user".to_string(),
        })
        .unwrap();
    db.issue_update_status(&done_id, "done", &StatusFields::default())
        .unwrap();

    // Query active issues (non-terminal)
    let count: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM issue_queue WHERE status NOT IN ('done', 'failed')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "Only pending issue should be active");
}

#[test]
fn test_active_items_across_all_queues() {
    let (_tmp, db) = setup_db();
    let repo_id = seed_repo(&db, "org/repo");

    // Issue
    db.issue_insert(&NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 10,
        title: "Issue 10".to_string(),
        body: None,
        labels: "".to_string(),
        author: "alice".to_string(),
    })
    .unwrap();

    // PR
    db.pr_insert(&NewPrItem {
        repo_id: repo_id.clone(),
        github_number: 20,
        title: "PR 20".to_string(),
        body: None,
        author: "bob".to_string(),
        head_branch: "feat".to_string(),
        base_branch: "main".to_string(),
    })
    .unwrap();

    // Merge
    db.merge_insert(&NewMergeItem {
        repo_id: repo_id.clone(),
        pr_number: 30,
        title: "Merge 30".to_string(),
        head_branch: "feat".to_string(),
        base_branch: "main".to_string(),
    })
    .unwrap();

    let issue_active: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM issue_queue WHERE status NOT IN ('done', 'failed')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let pr_active: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM pr_queue WHERE status NOT IN ('done', 'failed')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let merge_active: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM merge_queue WHERE status NOT IN ('done', 'failed')",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(issue_active, 1);
    assert_eq!(pr_active, 1);
    assert_eq!(merge_active, 1);
}

// ─── Label counts tests ───

#[test]
fn test_label_counts_wip_done_failed() {
    let (_tmp, db) = setup_db();
    let repo_id = seed_repo(&db, "org/repo");

    // 2 pending issues (wip)
    for n in 1..=2 {
        db.issue_insert(&NewIssueItem {
            repo_id: repo_id.clone(),
            github_number: n,
            title: format!("Issue {n}"),
            body: None,
            labels: "".to_string(),
            author: "user".to_string(),
        })
        .unwrap();
    }

    // 1 done issue
    let done_id = db
        .issue_insert(&NewIssueItem {
            repo_id: repo_id.clone(),
            github_number: 3,
            title: "Done".to_string(),
            body: None,
            labels: "".to_string(),
            author: "user".to_string(),
        })
        .unwrap();
    db.issue_update_status(&done_id, "done", &StatusFields::default())
        .unwrap();

    // 1 failed issue
    let fail_id = db
        .issue_insert(&NewIssueItem {
            repo_id: repo_id.clone(),
            github_number: 4,
            title: "Failed".to_string(),
            body: None,
            labels: "".to_string(),
            author: "user".to_string(),
        })
        .unwrap();
    db.issue_mark_failed(&fail_id, "some error").unwrap();

    let conn = db.conn();

    let wip: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM issue_queue WHERE status NOT IN ('done', 'failed')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let done: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM issue_queue WHERE status = 'done'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let failed: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM issue_queue WHERE status = 'failed'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(wip, 2);
    assert_eq!(done, 1);
    assert_eq!(failed, 1);
}

// ─── Retry action tests ───

#[test]
fn test_retry_failed_item_from_dashboard() {
    let (_tmp, db) = setup_db();
    let repo_id = seed_repo(&db, "org/repo");

    let id = db
        .issue_insert(&NewIssueItem {
            repo_id: repo_id.clone(),
            github_number: 42,
            title: "Broken".to_string(),
            body: None,
            labels: "".to_string(),
            author: "user".to_string(),
        })
        .unwrap();

    // Fail the item
    db.issue_mark_failed(&id, "timeout").unwrap();

    // Verify it's failed
    let status: String = db
        .conn()
        .query_row(
            "SELECT status FROM issue_queue WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "failed");

    // Retry via queue_retry (same as dashboard 'r' key)
    let retried = db.queue_retry(&id).unwrap();
    assert!(retried);

    // Verify it's back to pending
    let status: String = db
        .conn()
        .query_row(
            "SELECT status FROM issue_queue WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "pending");
}

#[test]
fn test_retry_non_failed_item_returns_false() {
    let (_tmp, db) = setup_db();
    let repo_id = seed_repo(&db, "org/repo");

    let id = db
        .issue_insert(&NewIssueItem {
            repo_id: repo_id.clone(),
            github_number: 42,
            title: "In progress".to_string(),
            body: None,
            labels: "".to_string(),
            author: "user".to_string(),
        })
        .unwrap();

    // Item is pending, not failed
    let retried = db.queue_retry(&id).unwrap();
    assert!(!retried, "Should not retry a non-failed item");
}

// ─── Skip action tests ───

#[test]
fn test_skip_active_item() {
    let (_tmp, db) = setup_db();
    let repo_id = seed_repo(&db, "org/repo");

    let id = db
        .issue_insert(&NewIssueItem {
            repo_id: repo_id.clone(),
            github_number: 55,
            title: "Low priority".to_string(),
            body: None,
            labels: "".to_string(),
            author: "user".to_string(),
        })
        .unwrap();

    // Skip via direct SQL (same as dashboard 's' key)
    let now = chrono::Utc::now().to_rfc3339();
    let affected = db
        .conn()
        .execute(
            "UPDATE issue_queue SET status = 'done', error_message = 'skipped via dashboard', \
             updated_at = ?2 WHERE id = ?1 AND status NOT IN ('done')",
            rusqlite::params![id, now],
        )
        .unwrap();
    assert_eq!(affected, 1);

    // Verify it's done
    let status: String = db
        .conn()
        .query_row(
            "SELECT status FROM issue_queue WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "done");

    let msg: String = db
        .conn()
        .query_row(
            "SELECT error_message FROM issue_queue WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(msg, "skipped via dashboard");
}

#[test]
fn test_skip_already_done_item_no_op() {
    let (_tmp, db) = setup_db();
    let repo_id = seed_repo(&db, "org/repo");

    let id = db
        .issue_insert(&NewIssueItem {
            repo_id: repo_id.clone(),
            github_number: 60,
            title: "Already done".to_string(),
            body: None,
            labels: "".to_string(),
            author: "user".to_string(),
        })
        .unwrap();

    // Mark done first
    db.issue_update_status(&id, "done", &StatusFields::default())
        .unwrap();

    // Try to skip — should be no-op
    let now = chrono::Utc::now().to_rfc3339();
    let affected = db
        .conn()
        .execute(
            "UPDATE issue_queue SET status = 'done', error_message = 'skipped via dashboard', \
             updated_at = ?2 WHERE id = ?1 AND status NOT IN ('done')",
            rusqlite::params![id, now],
        )
        .unwrap();
    assert_eq!(affected, 0, "Should not affect already-done items");
}

// ─── Active items with status transitions ───

#[test]
fn test_active_items_reflect_status_changes() {
    let (_tmp, db) = setup_db();
    let repo_id = seed_repo(&db, "org/repo");

    let id = db
        .issue_insert(&NewIssueItem {
            repo_id: repo_id.clone(),
            github_number: 77,
            title: "Transitioning".to_string(),
            body: None,
            labels: "".to_string(),
            author: "user".to_string(),
        })
        .unwrap();

    // Initially pending — should appear in active
    let active: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM issue_queue WHERE status NOT IN ('done', 'failed')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(active, 1);

    // Transition to analyzing
    db.issue_update_status(&id, "analyzing", &StatusFields::default())
        .unwrap();
    let status: String = db
        .conn()
        .query_row(
            "SELECT status FROM issue_queue WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "analyzing");

    // Still active
    let active: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM issue_queue WHERE status NOT IN ('done', 'failed')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(active, 1);

    // Transition to done
    db.issue_update_status(&id, "done", &StatusFields::default())
        .unwrap();
    let active: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM issue_queue WHERE status NOT IN ('done', 'failed')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(active, 0, "Done items should not be active");
}
