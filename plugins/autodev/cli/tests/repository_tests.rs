use autodev::core::models::*;
use autodev::core::repository::*;
use autodev::infra::db::Database;
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
fn repo_remove_nonexistent_returns_error() {
    let db = open_memory_db();
    // Should error when repo doesn't exist
    let result = db.repo_remove("nonexistent/repo");
    assert!(result.is_err());
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

// ═══════════════════════════════════════════════
// 4. HITL (Human-in-the-Loop)
// ═══════════════════════════════════════════════

fn create_test_hitl_event(db: &Database, repo_id: &str) -> String {
    use autodev::core::models::{HitlSeverity, NewHitlEvent};
    use autodev::core::repository::HitlRepository;

    let event = NewHitlEvent {
        repo_id: repo_id.to_string(),
        spec_id: Some("spec-1".to_string()),
        work_id: Some("pr:org/repo:42".to_string()),
        severity: HitlSeverity::High,
        situation: "Test conflict detected".to_string(),
        context: "File A conflicts with File B".to_string(),
        options: vec![
            "Keep A".to_string(),
            "Keep B".to_string(),
            "Merge both".to_string(),
        ],
    };
    db.hitl_create(&event).unwrap()
}

#[test]
fn hitl_create_and_show() {
    use autodev::core::repository::HitlRepository;

    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    let event_id = create_test_hitl_event(&db, &repo_id);

    assert!(!event_id.is_empty());

    let event = db.hitl_show(&event_id).unwrap().unwrap();
    assert_eq!(event.id, event_id);
    assert_eq!(event.repo_id, repo_id);
    assert_eq!(event.spec_id, Some("spec-1".to_string()));
    assert_eq!(event.work_id, Some("pr:org/repo:42".to_string()));
    assert_eq!(event.severity.to_string(), "high");
    assert_eq!(event.status.to_string(), "pending");
    assert_eq!(event.situation, "Test conflict detected");
    assert_eq!(event.context, "File A conflicts with File B");

    // Verify options are stored as JSON
    let options: Vec<String> = serde_json::from_str(&event.options).unwrap();
    assert_eq!(options.len(), 3);
    assert_eq!(options[0], "Keep A");
}

#[test]
fn hitl_show_nonexistent_returns_none() {
    use autodev::core::repository::HitlRepository;

    let db = open_memory_db();
    let result = db.hitl_show("nonexistent-id").unwrap();
    assert!(result.is_none());
}

#[test]
fn hitl_list_all() {
    use autodev::core::repository::HitlRepository;

    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    create_test_hitl_event(&db, &repo_id);
    create_test_hitl_event(&db, &repo_id);

    let events = db.hitl_list(None).unwrap();
    assert_eq!(events.len(), 2);
}

#[test]
fn hitl_list_by_repo() {
    use autodev::core::repository::HitlRepository;

    let db = open_memory_db();
    let repo_id1 = add_test_repo_with_url(&db, "https://github.com/a/one", "a/one");
    let repo_id2 = add_test_repo_with_url(&db, "https://github.com/b/two", "b/two");

    create_test_hitl_event(&db, &repo_id1);
    create_test_hitl_event(&db, &repo_id2);

    let all = db.hitl_list(None).unwrap();
    assert_eq!(all.len(), 2);

    let repo1_events = db.hitl_list(Some("a/one")).unwrap();
    assert_eq!(repo1_events.len(), 1);

    let repo2_events = db.hitl_list(Some("b/two")).unwrap();
    assert_eq!(repo2_events.len(), 1);
}

#[test]
fn hitl_list_empty() {
    use autodev::core::repository::HitlRepository;

    let db = open_memory_db();
    let events = db.hitl_list(None).unwrap();
    assert!(events.is_empty());
}

#[test]
fn hitl_respond_updates_status() {
    use autodev::core::models::NewHitlResponse;
    use autodev::core::repository::HitlRepository;

    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    let event_id = create_test_hitl_event(&db, &repo_id);

    let response = NewHitlResponse {
        event_id: event_id.clone(),
        choice: Some(1),
        message: Some("Going with option A".to_string()),
        source: "cli".to_string(),
    };
    db.hitl_respond(&response).unwrap();

    // Event status should be updated to responded
    let event = db.hitl_show(&event_id).unwrap().unwrap();
    assert_eq!(event.status.to_string(), "responded");

    // Response should be retrievable
    let responses = db.hitl_responses(&event_id).unwrap();
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].choice, Some(1));
    assert_eq!(
        responses[0].message,
        Some("Going with option A".to_string())
    );
    assert_eq!(responses[0].source, "cli");
}

#[test]
fn hitl_set_status() {
    use autodev::core::models::HitlStatus;
    use autodev::core::repository::HitlRepository;

    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    let event_id = create_test_hitl_event(&db, &repo_id);

    db.hitl_set_status(&event_id, HitlStatus::Expired).unwrap();

    let event = db.hitl_show(&event_id).unwrap().unwrap();
    assert_eq!(event.status.to_string(), "expired");
}

#[test]
fn hitl_pending_count() {
    use autodev::core::models::{HitlStatus, NewHitlResponse};
    use autodev::core::repository::HitlRepository;

    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    assert_eq!(db.hitl_pending_count(None).unwrap(), 0);

    let id1 = create_test_hitl_event(&db, &repo_id);
    create_test_hitl_event(&db, &repo_id);

    assert_eq!(db.hitl_pending_count(None).unwrap(), 2);
    assert_eq!(db.hitl_pending_count(Some("org/test-repo")).unwrap(), 2);

    // Respond to one
    db.hitl_respond(&NewHitlResponse {
        event_id: id1.clone(),
        choice: Some(1),
        message: None,
        source: "cli".to_string(),
    })
    .unwrap();

    assert_eq!(db.hitl_pending_count(None).unwrap(), 1);

    // Expire the other
    let events = db.hitl_list(None).unwrap();
    let pending_event = events.iter().find(|e| e.id != id1).unwrap();
    db.hitl_set_status(&pending_event.id, HitlStatus::Expired)
        .unwrap();

    assert_eq!(db.hitl_pending_count(None).unwrap(), 0);
}

#[test]
fn hitl_responses_empty() {
    use autodev::core::repository::HitlRepository;

    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    let event_id = create_test_hitl_event(&db, &repo_id);

    let responses = db.hitl_responses(&event_id).unwrap();
    assert!(responses.is_empty());
}

#[test]
fn hitl_pending_count_filters_by_repo() {
    use autodev::core::repository::HitlRepository;

    let db = open_memory_db();
    let repo_id1 = add_test_repo_with_url(&db, "https://github.com/a/one", "a/one");
    let repo_id2 = add_test_repo_with_url(&db, "https://github.com/b/two", "b/two");

    create_test_hitl_event(&db, &repo_id1);
    create_test_hitl_event(&db, &repo_id2);
    create_test_hitl_event(&db, &repo_id2);

    assert_eq!(db.hitl_pending_count(None).unwrap(), 3);
    assert_eq!(db.hitl_pending_count(Some("a/one")).unwrap(), 1);
    assert_eq!(db.hitl_pending_count(Some("b/two")).unwrap(), 2);
}
