use autodev::queue::models::*;
use autodev::queue::repository::*;
use autodev::queue::Database;
use std::path::Path;

// ─── Helpers ───

fn open_memory_db() -> Database {
    let db = Database::open(Path::new(":memory:")).expect("open in-memory db");
    db.initialize().expect("initialize schema");
    db
}

fn add_repo(db: &Database, url: &str, name: &str) -> String {
    db.repo_add(url, name).expect("add repo")
}

/// 직접 SQL로 status와 updated_at을 설정하는 헬퍼
fn force_status_and_time(db: &Database, table: &str, id: &str, status: &str, updated_at: &str) {
    db.conn()
        .execute(
            &format!("UPDATE {table} SET status = ?1, updated_at = ?2 WHERE id = ?3"),
            rusqlite::params![status, updated_at, id],
        )
        .expect("force status and time");
}

/// retry_count를 직접 설정하는 헬퍼
fn force_retry_count(db: &Database, table: &str, id: &str, count: i64) {
    db.conn()
        .execute(
            &format!("UPDATE {table} SET retry_count = ?1 WHERE id = ?2"),
            rusqlite::params![count, id],
        )
        .expect("force retry count");
}

fn old_timestamp() -> String {
    // 2시간 전 (threshold 1시간 기준으로 충분히 과거)
    (chrono::Utc::now() - chrono::Duration::hours(2)).to_rfc3339()
}

fn recent_timestamp() -> String {
    // 10초 전 (threshold 1시간 기준으로 충분히 최신)
    (chrono::Utc::now() - chrono::Duration::seconds(10)).to_rfc3339()
}

fn insert_issue(db: &Database, repo_id: &str, number: i64) -> String {
    db.issue_insert(&NewIssueItem {
        repo_id: repo_id.to_string(),
        github_number: number,
        title: format!("Issue #{number}"),
        body: None,
        labels: "[]".to_string(),
        author: "test".to_string(),
    })
    .unwrap()
}

fn insert_pr(db: &Database, repo_id: &str, number: i64) -> String {
    db.pr_insert(&NewPrItem {
        repo_id: repo_id.to_string(),
        github_number: number,
        title: format!("PR #{number}"),
        body: None,
        author: "test".to_string(),
        head_branch: "feat/x".to_string(),
        base_branch: "main".to_string(),
    })
    .unwrap()
}

fn insert_merge(db: &Database, repo_id: &str, pr_number: i64) -> String {
    db.merge_insert(&NewMergeItem {
        repo_id: repo_id.to_string(),
        pr_number,
        title: format!("Merge PR #{pr_number}"),
        head_branch: "feat/x".to_string(),
        base_branch: "main".to_string(),
    })
    .unwrap()
}

// ═══════════════════════════════════════════════
// queue_reset_stuck
// ═══════════════════════════════════════════════

#[test]
fn reset_stuck_issue_analyzing_beyond_threshold() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let id = insert_issue(&db, &repo_id, 1);

    force_status_and_time(&db, "issue_queue", &id, "analyzing", &old_timestamp());

    let reset = db.queue_reset_stuck(3600).unwrap(); // 1시간 threshold
    assert_eq!(reset, 1);

    let list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(list[0].status, "pending");
}

#[test]
fn reset_stuck_issue_processing_beyond_threshold() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let id = insert_issue(&db, &repo_id, 1);

    force_status_and_time(&db, "issue_queue", &id, "processing", &old_timestamp());

    let reset = db.queue_reset_stuck(3600).unwrap();
    assert_eq!(reset, 1);

    let list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(list[0].status, "pending");
}

#[test]
fn reset_stuck_issue_ready_beyond_threshold() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let id = insert_issue(&db, &repo_id, 1);

    force_status_and_time(&db, "issue_queue", &id, "ready", &old_timestamp());

    let reset = db.queue_reset_stuck(3600).unwrap();
    assert_eq!(reset, 1);
}

#[test]
fn reset_stuck_pr_reviewing_beyond_threshold() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let id = insert_pr(&db, &repo_id, 10);

    force_status_and_time(&db, "pr_queue", &id, "reviewing", &old_timestamp());

    let reset = db.queue_reset_stuck(3600).unwrap();
    assert_eq!(reset, 1);

    let list = db.pr_list("org/repo", 100).unwrap();
    assert_eq!(list[0].status, "pending");
}

#[test]
fn reset_stuck_merge_merging_beyond_threshold() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let id = insert_merge(&db, &repo_id, 20);

    force_status_and_time(&db, "merge_queue", &id, "merging", &old_timestamp());

    let reset = db.queue_reset_stuck(3600).unwrap();
    assert_eq!(reset, 1);

    let list = db.merge_list("org/repo", 100).unwrap();
    assert_eq!(list[0].status, "pending");
}

#[test]
fn reset_stuck_merge_conflict_beyond_threshold() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let id = insert_merge(&db, &repo_id, 20);

    force_status_and_time(&db, "merge_queue", &id, "conflict", &old_timestamp());

    let reset = db.queue_reset_stuck(3600).unwrap();
    assert_eq!(reset, 1);
}

#[test]
fn reset_stuck_skips_recent_items() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let id = insert_issue(&db, &repo_id, 1);

    // 최근 항목 → threshold 이내이므로 리셋되지 않아야 함
    force_status_and_time(&db, "issue_queue", &id, "analyzing", &recent_timestamp());

    let reset = db.queue_reset_stuck(3600).unwrap();
    assert_eq!(reset, 0);

    let list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(list[0].status, "analyzing");
}

#[test]
fn reset_stuck_skips_non_stuck_states() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    // pending, done, failed, waiting_human 상태 — 리셋 대상이 아님
    let id1 = insert_issue(&db, &repo_id, 1);
    force_status_and_time(&db, "issue_queue", &id1, "pending", &old_timestamp());

    let id2 = insert_issue(&db, &repo_id, 2);
    force_status_and_time(&db, "issue_queue", &id2, "done", &old_timestamp());

    let id3 = insert_issue(&db, &repo_id, 3);
    force_status_and_time(&db, "issue_queue", &id3, "failed", &old_timestamp());

    let id4 = insert_issue(&db, &repo_id, 4);
    force_status_and_time(&db, "issue_queue", &id4, "waiting_human", &old_timestamp());

    let reset = db.queue_reset_stuck(3600).unwrap();
    assert_eq!(reset, 0);
}

#[test]
fn reset_stuck_mixed_queues_counts_all() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let issue_id = insert_issue(&db, &repo_id, 1);
    force_status_and_time(&db, "issue_queue", &issue_id, "analyzing", &old_timestamp());

    let pr_id = insert_pr(&db, &repo_id, 10);
    force_status_and_time(&db, "pr_queue", &pr_id, "reviewing", &old_timestamp());

    let merge_id = insert_merge(&db, &repo_id, 20);
    force_status_and_time(&db, "merge_queue", &merge_id, "merging", &old_timestamp());

    let reset = db.queue_reset_stuck(3600).unwrap();
    assert_eq!(reset, 3);
}

#[test]
fn reset_stuck_returns_zero_when_empty() {
    let db = open_memory_db();
    let reset = db.queue_reset_stuck(3600).unwrap();
    assert_eq!(reset, 0);
}

// ═══════════════════════════════════════════════
// queue_auto_retry_failed
// ═══════════════════════════════════════════════

#[test]
fn auto_retry_resets_failed_issue_to_pending() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let id = insert_issue(&db, &repo_id, 1);

    db.issue_mark_failed(&id, "some error").unwrap();

    let retried = db.queue_auto_retry_failed(3).unwrap();
    assert_eq!(retried, 1);

    let list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(list[0].status, "pending");
}

#[test]
fn auto_retry_resets_failed_pr_to_pending() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let id = insert_pr(&db, &repo_id, 10);

    db.pr_mark_failed(&id, "review crash").unwrap();

    let retried = db.queue_auto_retry_failed(3).unwrap();
    assert_eq!(retried, 1);

    let list = db.pr_list("org/repo", 100).unwrap();
    assert_eq!(list[0].status, "pending");
}

#[test]
fn auto_retry_resets_failed_merge_to_pending() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let id = insert_merge(&db, &repo_id, 20);

    db.merge_mark_failed(&id, "merge crash").unwrap();

    let retried = db.queue_auto_retry_failed(3).unwrap();
    assert_eq!(retried, 1);

    let list = db.merge_list("org/repo", 100).unwrap();
    assert_eq!(list[0].status, "pending");
}

#[test]
fn auto_retry_skips_items_at_max_retries() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let id = insert_issue(&db, &repo_id, 1);

    db.issue_mark_failed(&id, "error").unwrap();
    // mark_failed은 retry_count를 1 증가시킴 → 현재 1
    // max_retries=1이면 retry_count(1) < max_retries(1)이 false이므로 리트라이 안됨
    let retried = db.queue_auto_retry_failed(1).unwrap();
    assert_eq!(retried, 0);

    let list = db.issue_list("org/repo", 100).unwrap();
    assert_eq!(list[0].status, "failed");
}

#[test]
fn auto_retry_increments_retry_count() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");
    let id = insert_issue(&db, &repo_id, 1);

    db.issue_mark_failed(&id, "error").unwrap(); // retry_count = 1

    // max_retries=3이면 retry_count(1) < 3 → 리트라이 가능
    let retried = db.queue_auto_retry_failed(3).unwrap();
    assert_eq!(retried, 1);

    // retry 후 retry_count = 2 (auto_retry도 +1)
    let count: i64 = db
        .conn()
        .query_row(
            "SELECT retry_count FROM issue_queue WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn auto_retry_mixed_queues() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    let issue_id = insert_issue(&db, &repo_id, 1);
    db.issue_mark_failed(&issue_id, "err").unwrap();

    let pr_id = insert_pr(&db, &repo_id, 10);
    db.pr_mark_failed(&pr_id, "err").unwrap();

    let merge_id = insert_merge(&db, &repo_id, 20);
    db.merge_mark_failed(&merge_id, "err").unwrap();

    let retried = db.queue_auto_retry_failed(5).unwrap();
    assert_eq!(retried, 3);
}

#[test]
fn auto_retry_skips_non_failed_items() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    // pending 상태 아이템 — failed가 아니므로 리트라이 대상이 아님
    insert_issue(&db, &repo_id, 1);

    let retried = db.queue_auto_retry_failed(3).unwrap();
    assert_eq!(retried, 0);
}

#[test]
fn auto_retry_returns_zero_when_empty() {
    let db = open_memory_db();
    let retried = db.queue_auto_retry_failed(3).unwrap();
    assert_eq!(retried, 0);
}

#[test]
fn auto_retry_respects_per_item_retry_count() {
    let db = open_memory_db();
    let repo_id = add_repo(&db, "https://github.com/org/repo", "org/repo");

    // 항목 1: retry_count=2 (max=3 미만 → 리트라이 대상)
    let id1 = insert_issue(&db, &repo_id, 1);
    force_status_and_time(&db, "issue_queue", &id1, "failed", &old_timestamp());
    force_retry_count(&db, "issue_queue", &id1, 2);

    // 항목 2: retry_count=3 (max=3 이상 → 리트라이 대상 아님)
    let id2 = insert_issue(&db, &repo_id, 2);
    force_status_and_time(&db, "issue_queue", &id2, "failed", &old_timestamp());
    force_retry_count(&db, "issue_queue", &id2, 3);

    let retried = db.queue_auto_retry_failed(3).unwrap();
    assert_eq!(retried, 1);
}
