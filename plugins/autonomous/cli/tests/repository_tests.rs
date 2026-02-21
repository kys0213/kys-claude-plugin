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

fn add_test_repo(db: &Database) -> String {
    db.repo_add("https://github.com/org/test-repo", "org/test-repo")
        .expect("add repo")
}

fn add_test_repo_with_url(db: &Database, url: &str, name: &str) -> String {
    db.repo_add(url, name).expect("add repo")
}

// ═══════════════════════════════════════════════
// 1. 레포 CRUD
// ═══════════════════════════════════════════════

#[test]
fn repo_add_and_count() {
    let db = open_memory_db();
    assert_eq!(db.repo_count().unwrap(), 0);

    let id = add_test_repo(&db);
    assert!(!id.is_empty());
    assert_eq!(db.repo_count().unwrap(), 1);
}

#[test]
fn repo_add_duplicate_url_fails() {
    let db = open_memory_db();
    add_test_repo(&db);
    let result = db.repo_add(
        "https://github.com/org/test-repo",
        "org/test-repo",
    );
    assert!(result.is_err());
}

#[test]
fn repo_add_different_urls() {
    let db = open_memory_db();
    add_test_repo_with_url(&db, "https://github.com/a/b", "a/b");
    add_test_repo_with_url(&db, "https://github.com/c/d", "c/d");
    assert_eq!(db.repo_count().unwrap(), 2);
}

#[test]
fn repo_remove() {
    let db = open_memory_db();
    add_test_repo(&db);
    assert_eq!(db.repo_count().unwrap(), 1);

    db.repo_remove("org/test-repo").unwrap();
    assert_eq!(db.repo_count().unwrap(), 0);
}

#[test]
fn repo_remove_nonexistent_is_ok() {
    let db = open_memory_db();
    // Should not error even if repo doesn't exist
    db.repo_remove("nonexistent/repo").unwrap();
}

#[test]
fn repo_list() {
    let db = open_memory_db();
    add_test_repo(&db);

    let list = db.repo_list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "org/test-repo");
    assert_eq!(list[0].url, "https://github.com/org/test-repo");
    assert!(list[0].enabled);
}

#[test]
fn repo_list_empty() {
    let db = open_memory_db();
    let list = db.repo_list().unwrap();
    assert!(list.is_empty());
}

#[test]
fn repo_find_enabled() {
    let db = open_memory_db();
    add_test_repo(&db);

    let enabled = db.repo_find_enabled().unwrap();
    assert_eq!(enabled.len(), 1);
    assert_eq!(enabled[0].name, "org/test-repo");
}

#[test]
fn repo_status_summary_empty_queues() {
    let db = open_memory_db();
    add_test_repo(&db);

    let summary = db.repo_status_summary().unwrap();
    assert_eq!(summary.len(), 1);
    assert_eq!(summary[0].issue_pending, 0);
    assert_eq!(summary[0].pr_pending, 0);
    assert_eq!(summary[0].merge_pending, 0);
}

// ═══════════════════════════════════════════════
// 2. 이슈 큐
// ═══════════════════════════════════════════════

#[test]
fn issue_insert_and_exists() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    assert!(!db.issue_exists(&repo_id, 42).unwrap());

    let item = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 42,
        title: "Bug: something broken".into(),
        body: Some("Description here".into()),
        labels: r#"["bug"]"#.into(),
        author: "user1".into(),
    };
    let id = db.issue_insert(&item).unwrap();
    assert!(!id.is_empty());
    assert!(db.issue_exists(&repo_id, 42).unwrap());
}

#[test]
fn issue_duplicate_detection() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 1,
        title: "First".into(),
        body: None,
        labels: "[]".into(),
        author: "user1".into(),
    };
    db.issue_insert(&item).unwrap();
    assert!(db.issue_exists(&repo_id, 1).unwrap());
    assert!(!db.issue_exists(&repo_id, 2).unwrap());
}

#[test]
fn issue_find_pending_empty() {
    let db = open_memory_db();
    let _ = add_test_repo(&db);

    let pending = db.issue_find_pending(10).unwrap();
    assert!(pending.is_empty());
}

#[test]
fn issue_find_pending_respects_limit() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    for i in 1..=5 {
        let item = NewIssueItem {
            repo_id: repo_id.clone(),
            github_number: i,
            title: format!("Issue #{i}"),
            body: None,
            labels: "[]".into(),
            author: "user1".into(),
        };
        db.issue_insert(&item).unwrap();
    }

    let pending = db.issue_find_pending(3).unwrap();
    assert_eq!(pending.len(), 3);

    let all = db.issue_find_pending(100).unwrap();
    assert_eq!(all.len(), 5);
}

#[test]
fn issue_find_pending_limit_zero() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 1,
        title: "Issue".into(),
        body: None,
        labels: "[]".into(),
        author: "user1".into(),
    };
    db.issue_insert(&item).unwrap();

    let pending = db.issue_find_pending(0).unwrap();
    assert!(pending.is_empty());
}

#[test]
fn issue_status_transitions() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 10,
        title: "Status test".into(),
        body: None,
        labels: "[]".into(),
        author: "user1".into(),
    };
    let id = db.issue_insert(&item).unwrap();

    // pending → analyzing (with worker_id)
    db.issue_update_status(
        &id,
        "analyzing",
        &StatusFields {
            worker_id: Some("worker-1".into()),
            ..Default::default()
        },
    )
    .unwrap();

    // analyzing → ready (with report)
    db.issue_update_status(
        &id,
        "ready",
        &StatusFields {
            analysis_report: Some("Analysis complete".into()),
            ..Default::default()
        },
    )
    .unwrap();

    // ready → done
    db.issue_update_status(&id, "done", &StatusFields::default())
        .unwrap();

    // Should no longer be pending
    let pending = db.issue_find_pending(10).unwrap();
    assert!(pending.is_empty());
}

#[test]
fn issue_mark_failed() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 99,
        title: "Will fail".into(),
        body: None,
        labels: "[]".into(),
        author: "user1".into(),
    };
    let id = db.issue_insert(&item).unwrap();

    db.issue_mark_failed(&id, "timeout error").unwrap();

    let pending = db.issue_find_pending(10).unwrap();
    assert!(pending.is_empty()); // failed items are not pending
}

#[test]
fn issue_count_active() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    assert_eq!(db.issue_count_active().unwrap(), 0);

    let item = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 1,
        title: "Active".into(),
        body: None,
        labels: "[]".into(),
        author: "user1".into(),
    };
    let id1 = db.issue_insert(&item).unwrap();
    assert_eq!(db.issue_count_active().unwrap(), 1); // pending is active

    let item2 = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 2,
        title: "Active 2".into(),
        body: None,
        labels: "[]".into(),
        author: "user1".into(),
    };
    let id2 = db.issue_insert(&item2).unwrap();
    assert_eq!(db.issue_count_active().unwrap(), 2);

    // Mark one as done
    db.issue_update_status(&id1, "done", &StatusFields::default())
        .unwrap();
    assert_eq!(db.issue_count_active().unwrap(), 1);

    // Mark other as failed
    db.issue_mark_failed(&id2, "error").unwrap();
    assert_eq!(db.issue_count_active().unwrap(), 0);
}

#[test]
fn issue_list_by_repo() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 7,
        title: "Listed issue".into(),
        body: None,
        labels: "[]".into(),
        author: "user1".into(),
    };
    db.issue_insert(&item).unwrap();

    let list = db.issue_list("org/test-repo", 20).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].github_number, 7);
    assert_eq!(list[0].status, "pending");

    // Different repo returns empty
    let other = db.issue_list("other/repo", 20).unwrap();
    assert!(other.is_empty());
}

// ─── 경계값 테스트 ───

#[test]
fn issue_null_body() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 1,
        title: "No body".into(),
        body: None,
        labels: "[]".into(),
        author: "user1".into(),
    };
    db.issue_insert(&item).unwrap();

    let pending = db.issue_find_pending(10).unwrap();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].body.is_none());
}

#[test]
fn issue_long_title() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let long_title = "A".repeat(10000);
    let item = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 1,
        title: long_title.clone(),
        body: None,
        labels: "[]".into(),
        author: "user1".into(),
    };
    db.issue_insert(&item).unwrap();

    let pending = db.issue_find_pending(10).unwrap();
    assert_eq!(pending[0].title, long_title);
}

#[test]
fn issue_negative_github_number() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: -1,
        title: "Negative".into(),
        body: None,
        labels: "[]".into(),
        author: "user1".into(),
    };
    db.issue_insert(&item).unwrap();
    assert!(db.issue_exists(&repo_id, -1).unwrap());
    assert!(!db.issue_exists(&repo_id, 1).unwrap());
}

// ═══════════════════════════════════════════════
// 3. PR 큐
// ═══════════════════════════════════════════════

#[test]
fn pr_insert_and_exists() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    assert!(!db.pr_exists(&repo_id, 10).unwrap());

    let item = NewPrItem {
        repo_id: repo_id.clone(),
        github_number: 10,
        title: "Add feature".into(),
        body: Some("PR body".into()),
        author: "dev1".into(),
        head_branch: "feature/x".into(),
        base_branch: "main".into(),
    };
    db.pr_insert(&item).unwrap();
    assert!(db.pr_exists(&repo_id, 10).unwrap());
}

#[test]
fn pr_find_pending_and_status_transition() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewPrItem {
        repo_id: repo_id.clone(),
        github_number: 5,
        title: "PR".into(),
        body: None,
        author: "dev1".into(),
        head_branch: "feat".into(),
        base_branch: "main".into(),
    };
    db.pr_insert(&item).unwrap();

    let pending = db.pr_find_pending(10).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].github_number, 5);
    assert_eq!(pending[0].head_branch, "feat");

    // reviewing → review_done
    db.pr_update_status(
        &pending[0].id,
        "reviewing",
        &StatusFields {
            worker_id: Some("w1".into()),
            ..Default::default()
        },
    )
    .unwrap();

    db.pr_update_status(
        &pending[0].id,
        "review_done",
        &StatusFields {
            review_comment: Some("LGTM".into()),
            ..Default::default()
        },
    )
    .unwrap();

    // No longer pending
    assert!(db.pr_find_pending(10).unwrap().is_empty());
}

#[test]
fn pr_count_active_mixed() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    for i in 1..=3 {
        let item = NewPrItem {
            repo_id: repo_id.clone(),
            github_number: i,
            title: format!("PR #{i}"),
            body: None,
            author: "dev".into(),
            head_branch: format!("branch-{i}"),
            base_branch: "main".into(),
        };
        db.pr_insert(&item).unwrap();
    }

    assert_eq!(db.pr_count_active().unwrap(), 3);

    let pending = db.pr_find_pending(10).unwrap();
    db.pr_mark_failed(&pending[0].id, "error").unwrap();
    assert_eq!(db.pr_count_active().unwrap(), 2);

    db.pr_update_status(&pending[1].id, "done", &StatusFields::default())
        .unwrap();
    assert_eq!(db.pr_count_active().unwrap(), 1);
}

// ═══════════════════════════════════════════════
// 4. 머지 큐
// ═══════════════════════════════════════════════

#[test]
fn merge_insert_and_find_pending() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewMergeItem {
        repo_id: repo_id.clone(),
        pr_number: 15,
        title: "Merge PR #15".into(),
        head_branch: "feature".into(),
        base_branch: "main".into(),
    };
    let id = db.merge_insert(&item).unwrap();
    assert!(!id.is_empty());

    let pending = db.merge_find_pending(10).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].pr_number, 15);
}

#[test]
fn merge_status_conflict_then_done() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewMergeItem {
        repo_id: repo_id.clone(),
        pr_number: 20,
        title: "Merge".into(),
        head_branch: "feat".into(),
        base_branch: "main".into(),
    };
    let id = db.merge_insert(&item).unwrap();

    // pending → merging
    db.merge_update_status(
        &id,
        "merging",
        &StatusFields {
            worker_id: Some("w1".into()),
            ..Default::default()
        },
    )
    .unwrap();

    // merging → conflict
    db.merge_update_status(&id, "conflict", &StatusFields::default())
        .unwrap();

    // conflict → done (after resolution)
    db.merge_update_status(&id, "done", &StatusFields::default())
        .unwrap();

    assert!(db.merge_find_pending(10).unwrap().is_empty());
    assert_eq!(db.merge_count_active().unwrap(), 0);
}

#[test]
fn merge_mark_failed() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewMergeItem {
        repo_id: repo_id.clone(),
        pr_number: 30,
        title: "Will fail".into(),
        head_branch: "feat".into(),
        base_branch: "main".into(),
    };
    let id = db.merge_insert(&item).unwrap();
    db.merge_mark_failed(&id, "merge error").unwrap();

    assert_eq!(db.merge_count_active().unwrap(), 0);
}

// ═══════════════════════════════════════════════
// 5. 스캔 커서
// ═══════════════════════════════════════════════

#[test]
fn cursor_initial_should_scan_true() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    // No scan history → should scan
    assert!(db.cursor_should_scan(&repo_id, 300).unwrap());
}

#[test]
fn cursor_get_last_seen_empty() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let last = db.cursor_get_last_seen(&repo_id, "issues").unwrap();
    assert!(last.is_none());
}

#[test]
fn cursor_upsert_and_get() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    db.cursor_upsert(&repo_id, "issues", "2024-01-15T10:00:00Z")
        .unwrap();

    let last = db.cursor_get_last_seen(&repo_id, "issues").unwrap();
    assert_eq!(last.unwrap(), "2024-01-15T10:00:00Z");

    // Different target
    let pulls_last = db.cursor_get_last_seen(&repo_id, "pulls").unwrap();
    assert!(pulls_last.is_none());
}

#[test]
fn cursor_upsert_overwrites() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    db.cursor_upsert(&repo_id, "issues", "2024-01-01T00:00:00Z")
        .unwrap();
    db.cursor_upsert(&repo_id, "issues", "2024-06-15T12:00:00Z")
        .unwrap();

    let last = db.cursor_get_last_seen(&repo_id, "issues").unwrap();
    assert_eq!(last.unwrap(), "2024-06-15T12:00:00Z");
}

#[test]
fn cursor_should_scan_after_recent_scan() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    // Just scanned → should NOT scan with 300s interval
    db.cursor_upsert(&repo_id, "issues", "2024-01-01T00:00:00Z")
        .unwrap();

    // cursor_upsert sets last_scan to now, so should_scan with large interval returns false
    assert!(!db.cursor_should_scan(&repo_id, 9999999).unwrap());

    // With 0 interval → should always scan
    assert!(db.cursor_should_scan(&repo_id, 0).unwrap());
}

// ═══════════════════════════════════════════════
// 6. Consumer 로그
// ═══════════════════════════════════════════════

#[test]
fn log_insert_and_recent() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let log = NewConsumerLog {
        repo_id: repo_id.clone(),
        queue_type: "issue".into(),
        queue_item_id: "item-1".into(),
        worker_id: "worker-1".into(),
        command: "claude -p \"analyze\"".into(),
        stdout: "output".into(),
        stderr: "".into(),
        exit_code: 0,
        started_at: "2024-01-15T10:00:00Z".into(),
        finished_at: "2024-01-15T10:01:00Z".into(),
        duration_ms: 60000,
    };
    db.log_insert(&log).unwrap();

    let logs = db.log_recent(None, 10).unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].queue_type, "issue");
    assert_eq!(logs[0].exit_code, Some(0));
}

#[test]
fn log_recent_respects_limit() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    for i in 0..5 {
        let log = NewConsumerLog {
            repo_id: repo_id.clone(),
            queue_type: "issue".into(),
            queue_item_id: format!("item-{i}"),
            worker_id: "w1".into(),
            command: format!("cmd-{i}"),
            stdout: "".into(),
            stderr: "".into(),
            exit_code: 0,
            started_at: format!("2024-01-15T10:0{i}:00Z"),
            finished_at: format!("2024-01-15T10:0{i}:30Z"),
            duration_ms: 30000,
        };
        db.log_insert(&log).unwrap();
    }

    let logs = db.log_recent(None, 3).unwrap();
    assert_eq!(logs.len(), 3);
}

#[test]
fn log_recent_filters_by_repo() {
    let db = open_memory_db();
    let repo_id1 = add_test_repo_with_url(&db, "https://github.com/a/one", "a/one");
    let repo_id2 = add_test_repo_with_url(&db, "https://github.com/b/two", "b/two");

    for (rid, name) in [(&repo_id1, "a/one"), (&repo_id2, "b/two")] {
        let log = NewConsumerLog {
            repo_id: rid.clone(),
            queue_type: "issue".into(),
            queue_item_id: "item".into(),
            worker_id: "w1".into(),
            command: format!("cmd for {name}"),
            stdout: "".into(),
            stderr: "".into(),
            exit_code: 0,
            started_at: "2024-01-15T10:00:00Z".into(),
            finished_at: "2024-01-15T10:01:00Z".into(),
            duration_ms: 60000,
        };
        db.log_insert(&log).unwrap();
    }

    let all = db.log_recent(None, 10).unwrap();
    assert_eq!(all.len(), 2);

    let repo1_logs = db.log_recent(Some("a/one"), 10).unwrap();
    assert_eq!(repo1_logs.len(), 1);
    assert!(repo1_logs[0].command.contains("a/one"));
}

#[test]
fn log_recent_empty() {
    let db = open_memory_db();
    let logs = db.log_recent(None, 10).unwrap();
    assert!(logs.is_empty());
}

// ═══════════════════════════════════════════════
// 7. 큐 관리 (retry / clear)
// ═══════════════════════════════════════════════

#[test]
fn queue_retry_failed_item() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 1,
        title: "Retry test".into(),
        body: None,
        labels: "[]".into(),
        author: "user1".into(),
    };
    let id = db.issue_insert(&item).unwrap();
    db.issue_mark_failed(&id, "timeout").unwrap();

    // No pending items
    assert!(db.issue_find_pending(10).unwrap().is_empty());

    // Retry → back to pending
    assert!(db.queue_retry(&id).unwrap());

    let pending = db.issue_find_pending(10).unwrap();
    assert_eq!(pending.len(), 1);
}

#[test]
fn queue_retry_done_item_returns_false() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 1,
        title: "Done item".into(),
        body: None,
        labels: "[]".into(),
        author: "user1".into(),
    };
    let id = db.issue_insert(&item).unwrap();
    db.issue_update_status(&id, "done", &StatusFields::default())
        .unwrap();

    // Retry on done item returns false
    assert!(!db.queue_retry(&id).unwrap());
}

#[test]
fn queue_retry_nonexistent_id() {
    let db = open_memory_db();
    assert!(!db.queue_retry("nonexistent-id").unwrap());
}

#[test]
fn queue_retry_pr_failed() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewPrItem {
        repo_id: repo_id.clone(),
        github_number: 5,
        title: "PR retry".into(),
        body: None,
        author: "dev".into(),
        head_branch: "feat".into(),
        base_branch: "main".into(),
    };
    let id = db.pr_insert(&item).unwrap();
    db.pr_mark_failed(&id, "error").unwrap();

    assert!(db.queue_retry(&id).unwrap());
    assert_eq!(db.pr_find_pending(10).unwrap().len(), 1);
}

#[test]
fn queue_clear_removes_done_and_failed_only() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    // Insert 3 issues: pending, done, failed
    let ids: Vec<String> = (1..=3)
        .map(|i| {
            let item = NewIssueItem {
                repo_id: repo_id.clone(),
                github_number: i,
                title: format!("Issue #{i}"),
                body: None,
                labels: "[]".into(),
                author: "user1".into(),
            };
            db.issue_insert(&item).unwrap()
        })
        .collect();

    db.issue_update_status(&ids[1], "done", &StatusFields::default())
        .unwrap();
    db.issue_mark_failed(&ids[2], "error").unwrap();

    // Before clear: 1 pending, 1 done, 1 failed
    assert_eq!(db.issue_count_active().unwrap(), 1);

    db.queue_clear("org/test-repo").unwrap();

    // After clear: only pending remains
    let list = db.issue_list("org/test-repo", 20).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, "pending");
}

// ═══════════════════════════════════════════════
// 8. 상태 요약 (status_summary) 정확도
// ═══════════════════════════════════════════════

#[test]
fn repo_status_summary_with_mixed_queues() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    // 2 pending issues, 1 done
    for i in 1..=3 {
        let item = NewIssueItem {
            repo_id: repo_id.clone(),
            github_number: i,
            title: format!("Issue #{i}"),
            body: None,
            labels: "[]".into(),
            author: "user1".into(),
        };
        db.issue_insert(&item).unwrap();
    }
    let issues = db.issue_find_pending(10).unwrap();
    db.issue_update_status(&issues[2].id, "done", &StatusFields::default())
        .unwrap();

    // 1 pending PR
    let pr = NewPrItem {
        repo_id: repo_id.clone(),
        github_number: 10,
        title: "PR".into(),
        body: None,
        author: "dev".into(),
        head_branch: "feat".into(),
        base_branch: "main".into(),
    };
    db.pr_insert(&pr).unwrap();

    let summary = db.repo_status_summary().unwrap();
    assert_eq!(summary[0].issue_pending, 2); // 2 still active
    assert_eq!(summary[0].pr_pending, 1);
    assert_eq!(summary[0].merge_pending, 0);
}

// ═══════════════════════════════════════════════
// 9. 데이터 정합성: 중복 방지 (INSERT OR IGNORE + UNIQUE)
// ═══════════════════════════════════════════════

#[test]
fn issue_duplicate_insert_is_ignored() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 42,
        title: "First".into(),
        body: None,
        labels: "[]".into(),
        author: "user1".into(),
    };
    db.issue_insert(&item).unwrap();

    // 동일한 repo_id + github_number로 재삽입 → 에러 없이 무시
    let item2 = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 42,
        title: "Duplicate".into(),
        body: None,
        labels: "[]".into(),
        author: "user2".into(),
    };
    db.issue_insert(&item2).unwrap(); // should not panic

    // 원본만 남아있어야 함
    let list = db.issue_list("org/test-repo", 20).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].title, "First");
}

#[test]
fn pr_duplicate_insert_is_ignored() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewPrItem {
        repo_id: repo_id.clone(),
        github_number: 10,
        title: "First PR".into(),
        body: None,
        author: "dev1".into(),
        head_branch: "feat".into(),
        base_branch: "main".into(),
    };
    db.pr_insert(&item).unwrap();

    let item2 = NewPrItem {
        repo_id: repo_id.clone(),
        github_number: 10,
        title: "Duplicate PR".into(),
        body: None,
        author: "dev2".into(),
        head_branch: "feat2".into(),
        base_branch: "main".into(),
    };
    db.pr_insert(&item2).unwrap();

    let list = db.pr_list("org/test-repo", 20).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].title, "First PR");
}

#[test]
fn merge_duplicate_insert_is_ignored() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewMergeItem {
        repo_id: repo_id.clone(),
        pr_number: 15,
        title: "First merge".into(),
        head_branch: "feat".into(),
        base_branch: "main".into(),
    };
    db.merge_insert(&item).unwrap();

    let item2 = NewMergeItem {
        repo_id: repo_id.clone(),
        pr_number: 15,
        title: "Duplicate merge".into(),
        head_branch: "feat2".into(),
        base_branch: "main".into(),
    };
    db.merge_insert(&item2).unwrap();

    let list = db.merge_list("org/test-repo", 20).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].title, "First merge");
}

#[test]
fn merge_exists_check() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    assert!(!db.merge_exists(&repo_id, 15).unwrap());

    let item = NewMergeItem {
        repo_id: repo_id.clone(),
        pr_number: 15,
        title: "Merge".into(),
        head_branch: "feat".into(),
        base_branch: "main".into(),
    };
    db.merge_insert(&item).unwrap();

    assert!(db.merge_exists(&repo_id, 15).unwrap());
    assert!(!db.merge_exists(&repo_id, 999).unwrap());
}

// ═══════════════════════════════════════════════
// 10. retry_count 추적
// ═══════════════════════════════════════════════

#[test]
fn issue_mark_failed_increments_retry_count() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 1,
        title: "Retry count test".into(),
        body: None,
        labels: "[]".into(),
        author: "user1".into(),
    };
    let id = db.issue_insert(&item).unwrap();

    // 첫 번째 실패 → retry_count = 1
    db.issue_mark_failed(&id, "error 1").unwrap();

    let retry_count: i64 = db.conn().query_row(
        "SELECT retry_count FROM issue_queue WHERE id = ?1",
        rusqlite::params![id],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(retry_count, 1);

    // retry → pending, retry_count는 유지
    db.queue_retry(&id).unwrap();

    // 두 번째 실패 → retry_count = 2
    db.issue_mark_failed(&id, "error 2").unwrap();

    let retry_count: i64 = db.conn().query_row(
        "SELECT retry_count FROM issue_queue WHERE id = ?1",
        rusqlite::params![id],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(retry_count, 2);
}

// ═══════════════════════════════════════════════
// 11. 원자적 상태 업데이트
// ═══════════════════════════════════════════════

#[test]
fn issue_update_status_atomic_fields() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewIssueItem {
        repo_id: repo_id.clone(),
        github_number: 1,
        title: "Atomic test".into(),
        body: None,
        labels: "[]".into(),
        author: "user1".into(),
    };
    let id = db.issue_insert(&item).unwrap();

    // worker_id와 analysis_report를 동시에 설정
    db.issue_update_status(
        &id,
        "ready",
        &StatusFields {
            worker_id: Some("w1".into()),
            analysis_report: Some("report".into()),
            ..Default::default()
        },
    ).unwrap();

    let (worker_id, report): (Option<String>, Option<String>) = db.conn().query_row(
        "SELECT worker_id, analysis_report FROM issue_queue WHERE id = ?1",
        rusqlite::params![id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();

    assert_eq!(worker_id.as_deref(), Some("w1"));
    assert_eq!(report.as_deref(), Some("report"));
}

#[test]
fn pr_update_status_atomic_fields() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewPrItem {
        repo_id: repo_id.clone(),
        github_number: 1,
        title: "Atomic PR".into(),
        body: None,
        author: "dev1".into(),
        head_branch: "feat".into(),
        base_branch: "main".into(),
    };
    let id = db.pr_insert(&item).unwrap();

    // worker_id와 review_comment를 동시에 설정
    db.pr_update_status(
        &id,
        "review_done",
        &StatusFields {
            worker_id: Some("w1".into()),
            review_comment: Some("LGTM".into()),
            ..Default::default()
        },
    ).unwrap();

    let (worker_id, comment): (Option<String>, Option<String>) = db.conn().query_row(
        "SELECT worker_id, review_comment FROM pr_queue WHERE id = ?1",
        rusqlite::params![id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();

    assert_eq!(worker_id.as_deref(), Some("w1"));
    assert_eq!(comment.as_deref(), Some("LGTM"));
}

#[test]
fn merge_update_status_atomic_fields() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewMergeItem {
        repo_id: repo_id.clone(),
        pr_number: 1,
        title: "Atomic merge".into(),
        head_branch: "feat".into(),
        base_branch: "main".into(),
    };
    let id = db.merge_insert(&item).unwrap();

    // worker_id와 error_message를 동시에 설정
    db.merge_update_status(
        &id,
        "conflict",
        &StatusFields {
            worker_id: Some("w1".into()),
            error_message: Some("conflict in file.rs".into()),
            ..Default::default()
        },
    ).unwrap();

    let (worker_id, error): (Option<String>, Option<String>) = db.conn().query_row(
        "SELECT worker_id, error_message FROM merge_queue WHERE id = ?1",
        rusqlite::params![id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();

    assert_eq!(worker_id.as_deref(), Some("w1"));
    assert_eq!(error.as_deref(), Some("conflict in file.rs"));
}

#[test]
fn pr_mark_failed_increments_retry_count() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewPrItem {
        repo_id: repo_id.clone(),
        github_number: 1,
        title: "PR retry".into(),
        body: None,
        author: "dev1".into(),
        head_branch: "feat".into(),
        base_branch: "main".into(),
    };
    let id = db.pr_insert(&item).unwrap();

    db.pr_mark_failed(&id, "error 1").unwrap();

    let retry_count: i64 = db.conn().query_row(
        "SELECT retry_count FROM pr_queue WHERE id = ?1",
        rusqlite::params![id],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(retry_count, 1);

    db.queue_retry(&id).unwrap();
    db.pr_mark_failed(&id, "error 2").unwrap();

    let retry_count: i64 = db.conn().query_row(
        "SELECT retry_count FROM pr_queue WHERE id = ?1",
        rusqlite::params![id],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(retry_count, 2);
}

#[test]
fn merge_mark_failed_increments_retry_count() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let item = NewMergeItem {
        repo_id: repo_id.clone(),
        pr_number: 1,
        title: "Merge retry".into(),
        head_branch: "feat".into(),
        base_branch: "main".into(),
    };
    let id = db.merge_insert(&item).unwrap();

    db.merge_mark_failed(&id, "error 1").unwrap();

    let retry_count: i64 = db.conn().query_row(
        "SELECT retry_count FROM merge_queue WHERE id = ?1",
        rusqlite::params![id],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(retry_count, 1);

    db.queue_retry(&id).unwrap();
    db.merge_mark_failed(&id, "error 2").unwrap();

    let retry_count: i64 = db.conn().query_row(
        "SELECT retry_count FROM merge_queue WHERE id = ?1",
        rusqlite::params![id],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(retry_count, 2);
}

// ═══════════════════════════════════════════════
// 12. 마이그레이션: 기존 중복 데이터 정리
// ═══════════════════════════════════════════════

#[test]
fn migration_cleans_up_existing_duplicates() {
    // 수동으로 UNIQUE 없는 테이블 생성 후 중복 삽입 → initialize()로 마이그레이션 검증
    let db = Database::open(Path::new(":memory:")).unwrap();

    // UNIQUE 없이 테이블만 생성
    db.conn().execute_batch("
        PRAGMA foreign_keys=ON;
        CREATE TABLE IF NOT EXISTS repositories (
            id TEXT PRIMARY KEY, url TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS scan_cursors (
            repo_id TEXT NOT NULL REFERENCES repositories(id), target TEXT NOT NULL,
            last_seen TEXT NOT NULL, last_scan TEXT NOT NULL, PRIMARY KEY (repo_id, target)
        );
        CREATE TABLE IF NOT EXISTS issue_queue (
            id TEXT PRIMARY KEY, repo_id TEXT NOT NULL REFERENCES repositories(id),
            github_number INTEGER NOT NULL, title TEXT NOT NULL, body TEXT, labels TEXT,
            author TEXT NOT NULL, analysis_report TEXT, status TEXT NOT NULL DEFAULT 'pending',
            worker_id TEXT, branch_name TEXT, pr_number INTEGER, error_message TEXT,
            retry_count INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS pr_queue (
            id TEXT PRIMARY KEY, repo_id TEXT NOT NULL REFERENCES repositories(id),
            github_number INTEGER NOT NULL, title TEXT NOT NULL, body TEXT,
            author TEXT NOT NULL, head_branch TEXT NOT NULL, base_branch TEXT NOT NULL,
            review_comment TEXT, status TEXT NOT NULL DEFAULT 'pending', worker_id TEXT,
            error_message TEXT, retry_count INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS merge_queue (
            id TEXT PRIMARY KEY, repo_id TEXT NOT NULL REFERENCES repositories(id),
            pr_number INTEGER NOT NULL, title TEXT NOT NULL, head_branch TEXT NOT NULL,
            base_branch TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending',
            conflict_files TEXT, worker_id TEXT, error_message TEXT,
            retry_count INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS consumer_logs (
            id TEXT PRIMARY KEY, repo_id TEXT NOT NULL REFERENCES repositories(id),
            queue_type TEXT NOT NULL, queue_item_id TEXT NOT NULL, worker_id TEXT NOT NULL,
            command TEXT NOT NULL, stdout TEXT, stderr TEXT, exit_code INTEGER,
            started_at TEXT NOT NULL, finished_at TEXT, duration_ms INTEGER
        );
    ").unwrap();

    // 레포 추가
    db.conn().execute(
        "INSERT INTO repositories (id, url, name, enabled, created_at, updated_at) VALUES ('r1', 'https://github.com/a/b', 'a/b', 1, '2024-01-01', '2024-01-01')",
        [],
    ).unwrap();

    // 중복 이슈 삽입 (UNIQUE 없으므로 가능)
    db.conn().execute(
        "INSERT INTO issue_queue (id, repo_id, github_number, title, author, status, created_at, updated_at) VALUES ('i1', 'r1', 42, 'First', 'alice', 'pending', '2024-01-01', '2024-01-01')",
        [],
    ).unwrap();
    db.conn().execute(
        "INSERT INTO issue_queue (id, repo_id, github_number, title, author, status, created_at, updated_at) VALUES ('i2', 'r1', 42, 'Duplicate', 'bob', 'pending', '2024-01-02', '2024-01-02')",
        [],
    ).unwrap();

    // 중복 확인
    let count: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM issue_queue WHERE repo_id = 'r1' AND github_number = 42", [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(count, 2);

    // initialize() 호출 → 마이그레이션 실행
    db.initialize().unwrap();

    // 중복이 제거되고 하나만 남아야 함
    let count: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM issue_queue WHERE repo_id = 'r1' AND github_number = 42", [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(count, 1);

    // 가장 오래된 항목(First)이 유지되어야 함
    let title: String = db.conn().query_row(
        "SELECT title FROM issue_queue WHERE repo_id = 'r1' AND github_number = 42", [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(title, "First");
}
