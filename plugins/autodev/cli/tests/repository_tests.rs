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
    assert_eq!(db.repo_list().unwrap().len(), 0);

    let id = add_test_repo(&db);
    assert!(!id.is_empty());
    assert_eq!(db.repo_list().unwrap().len(), 1);
}

#[test]
fn repo_add_duplicate_url_fails() {
    let db = open_memory_db();
    add_test_repo(&db);
    let result = db.repo_add("https://github.com/org/test-repo", "org/test-repo");
    assert!(result.is_err());
}

#[test]
fn repo_add_different_urls() {
    let db = open_memory_db();
    add_test_repo_with_url(&db, "https://github.com/a/b", "a/b");
    add_test_repo_with_url(&db, "https://github.com/c/d", "c/d");
    assert_eq!(db.repo_list().unwrap().len(), 2);
}

#[test]
fn repo_remove() {
    let db = open_memory_db();
    add_test_repo(&db);
    assert_eq!(db.repo_list().unwrap().len(), 1);

    db.repo_remove("org/test-repo").unwrap();
    assert_eq!(db.repo_list().unwrap().len(), 0);
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
fn repo_status_summary_empty() {
    let db = open_memory_db();
    add_test_repo(&db);

    let summary = db.repo_status_summary().unwrap();
    assert_eq!(summary.len(), 1);
    assert_eq!(summary[0].name, "org/test-repo");
    assert!(summary[0].enabled);
}

#[test]
fn repo_status_summary_with_repos() {
    let db = open_memory_db();
    add_test_repo_with_url(&db, "https://github.com/a/one", "a/one");
    add_test_repo_with_url(&db, "https://github.com/b/two", "b/two");

    let summary = db.repo_status_summary().unwrap();
    assert_eq!(summary.len(), 2);

    let names: Vec<&str> = summary.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"a/one"));
    assert!(names.contains(&"b/two"));

    for row in &summary {
        assert!(row.enabled);
    }
}

// ═══════════════════════════════════════════════
// 2. 스캔 커서
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
// 3. Consumer 로그
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
