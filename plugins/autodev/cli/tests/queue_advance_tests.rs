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

fn insert_queue_item(db: &Database, repo_id: &str, work_id: &str, phase: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    db.conn()
        .execute(
            "INSERT INTO queue_items (work_id, repo_id, queue_type, phase, title, created_at, updated_at) \
             VALUES (?1, ?2, 'issue', ?3, 'Test item', ?4, ?4)",
            rusqlite::params![work_id, repo_id, phase, now],
        )
        .expect("insert queue item");
}

// ═══════════════════════════════════════════════
// 1. queue_get_phase
// ═══════════════════════════════════════════════

#[test]
fn get_phase_returns_current_phase() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-1", "pending");

    let phase = db.queue_get_phase("work-1").unwrap();
    assert_eq!(phase, Some("pending".to_string()));
}

#[test]
fn get_phase_returns_none_for_nonexistent() {
    let db = open_memory_db();

    let phase = db.queue_get_phase("nonexistent").unwrap();
    assert_eq!(phase, None);
}

// ═══════════════════════════════════════════════
// 2. queue_advance
// ═══════════════════════════════════════════════

#[test]
fn advance_pending_to_ready() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-1", "pending");

    db.queue_advance("work-1").unwrap();

    let phase = db.queue_get_phase("work-1").unwrap();
    assert_eq!(phase, Some("ready".to_string()));
}

#[test]
fn advance_ready_to_running() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-1", "ready");

    db.queue_advance("work-1").unwrap();

    let phase = db.queue_get_phase("work-1").unwrap();
    assert_eq!(phase, Some("running".to_string()));
}

#[test]
fn advance_running_to_done() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-1", "running");

    db.queue_advance("work-1").unwrap();

    let phase = db.queue_get_phase("work-1").unwrap();
    assert_eq!(phase, Some("done".to_string()));
}

#[test]
fn advance_nonexistent_returns_error() {
    let db = open_memory_db();

    let result = db.queue_advance("nonexistent");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn advance_done_returns_error() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-1", "done");

    let result = db.queue_advance("work-1");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("terminal"));
}

#[test]
fn advance_skipped_returns_error() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-1", "skipped");

    let result = db.queue_advance("work-1");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("terminal"));
}

// ═══════════════════════════════════════════════
// 3. queue_skip
// ═══════════════════════════════════════════════

#[test]
fn skip_pending_with_reason() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-1", "pending");

    db.queue_skip("work-1", Some("not needed")).unwrap();

    let phase = db.queue_get_phase("work-1").unwrap();
    assert_eq!(phase, Some("skipped".to_string()));

    let items = db.queue_list_items(None).unwrap();
    let item = items.iter().find(|i| i.work_id == "work-1").unwrap();
    assert_eq!(item.skip_reason.as_deref(), Some("not needed"));
}

#[test]
fn skip_ready_without_reason() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-1", "ready");

    db.queue_skip("work-1", None).unwrap();

    let phase = db.queue_get_phase("work-1").unwrap();
    assert_eq!(phase, Some("skipped".to_string()));
}

#[test]
fn skip_nonexistent_returns_error() {
    let db = open_memory_db();

    let result = db.queue_skip("nonexistent", None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn skip_already_skipped_returns_error() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-1", "skipped");

    let result = db.queue_skip("work-1", None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("terminal"));
}

// ═══════════════════════════════════════════════
// 4. queue_list_items
// ═══════════════════════════════════════════════

#[test]
fn list_returns_all_items() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-1", "pending");
    insert_queue_item(&db, &repo_id, "work-2", "ready");

    let items = db.queue_list_items(None).unwrap();
    assert_eq!(items.len(), 2);
}

#[test]
fn list_filters_by_repo() {
    let db = open_memory_db();
    let repo_id1 = add_test_repo(&db);
    let repo_id2 = db
        .repo_add("https://github.com/org/other-repo", "org/other-repo")
        .unwrap();
    insert_queue_item(&db, &repo_id1, "work-1", "pending");
    insert_queue_item(&db, &repo_id2, "work-2", "pending");

    let items = db.queue_list_items(Some("org/test-repo")).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].work_id, "work-1");
}

#[test]
fn list_empty_result_when_no_items() {
    let db = open_memory_db();

    let items = db.queue_list_items(None).unwrap();
    assert!(items.is_empty());
}

// ═══════════════════════════════════════════════
// 5. CLI handler integration tests
// ═══════════════════════════════════════════════

#[test]
fn cli_queue_advance_changes_phase() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-cli-1", "pending");

    let output = autodev::cli::queue::queue_advance(&db, "work-cli-1").unwrap();
    assert!(output.contains("pending"));
    assert!(output.contains("ready"));

    let phase = db.queue_get_phase("work-cli-1").unwrap();
    assert_eq!(phase, Some("ready".to_string()));
}

#[test]
fn cli_queue_skip_stores_reason() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-cli-2", "pending");

    let output =
        autodev::cli::queue::queue_skip(&db, "work-cli-2", Some("duplicate issue")).unwrap();
    assert!(output.contains("skipped"));
    assert!(output.contains("duplicate issue"));

    let items = db.queue_list_items(None).unwrap();
    let item = items.iter().find(|i| i.work_id == "work-cli-2").unwrap();
    assert_eq!(item.phase, "skipped");
    assert_eq!(item.skip_reason.as_deref(), Some("duplicate issue"));
}

#[test]
fn cli_queue_list_db_json_output() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-cli-3", "pending");

    let output = autodev::cli::queue::queue_list_db(&db, None, true, None, false).unwrap();
    let parsed: Vec<autodev::core::models::QueueItem> =
        serde_json::from_str(&output).expect("valid JSON");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].work_id, "work-cli-3");
}

#[test]
fn cli_queue_list_db_empty() {
    let db = open_memory_db();

    let output = autodev::cli::queue::queue_list_db(&db, None, false, None, false).unwrap();
    assert!(output.contains("no queue items"));
}
