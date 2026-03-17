use autodev::core::models::{QueueItemRow, QueuePhase, QueueType};
use autodev::core::phase::TaskKind;
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
    assert_eq!(phase, Some(QueuePhase::Pending));
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
    assert_eq!(phase, Some(QueuePhase::Ready));
}

#[test]
fn advance_ready_to_running() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-1", "ready");

    db.queue_advance("work-1").unwrap();

    let phase = db.queue_get_phase("work-1").unwrap();
    assert_eq!(phase, Some(QueuePhase::Running));
}

#[test]
fn advance_running_to_done() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-1", "running");

    db.queue_advance("work-1").unwrap();

    let phase = db.queue_get_phase("work-1").unwrap();
    assert_eq!(phase, Some(QueuePhase::Done));
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
    assert_eq!(phase, Some(QueuePhase::Skipped));

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
    assert_eq!(phase, Some(QueuePhase::Skipped));
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

    let result = autodev::cli::queue::queue_advance(&db, "work-cli-1", None).unwrap();
    assert!(result.output.contains("pending"));
    assert!(result.output.contains("ready"));

    let phase = db.queue_get_phase("work-cli-1").unwrap();
    assert_eq!(phase, Some(QueuePhase::Ready));
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
    assert_eq!(item.phase, QueuePhase::Skipped);
    assert_eq!(item.skip_reason.as_deref(), Some("duplicate issue"));
}

#[test]
fn cli_queue_list_db_json_output() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-cli-3", "pending");

    let output = autodev::cli::queue::queue_list_db(&db, None, true, None, false).unwrap();
    let parsed: Vec<autodev::core::models::QueueItemRow> =
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

// ═══════════════════════════════════════════════
// 6. migrate_v2 idempotent
// ═══════════════════════════════════════════════

#[test]
fn migrate_v2_idempotent() {
    let db = open_memory_db();
    // initialize already calls migrate_v2, calling it again should not error
    db.initialize().unwrap();
}

// ═══════════════════════════════════════════════
// 7. queue_upsert / queue_remove / queue_load_active / queue_transit
// ═══════════════════════════════════════════════

fn make_row(repo_id: &str, work_id: &str, phase: QueuePhase) -> QueueItemRow {
    let now = chrono::Utc::now().to_rfc3339();
    QueueItemRow {
        work_id: work_id.to_string(),
        repo_id: repo_id.to_string(),
        queue_type: QueueType::Issue,
        phase,
        title: Some("Test".to_string()),
        skip_reason: None,
        created_at: now.clone(),
        updated_at: now,
        task_kind: TaskKind::Analyze,
        github_number: 42,
        metadata_json: None,
        failure_count: 0,
        escalation_level: 0,
    }
}

#[test]
fn queue_upsert_insert() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    let row = make_row(&repo_id, "issue:org/test-repo:1", QueuePhase::Pending);

    db.queue_upsert(&row).unwrap();

    let items = db.queue_list_items(None).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].work_id, "issue:org/test-repo:1");
    assert_eq!(items[0].task_kind, TaskKind::Analyze);
    assert_eq!(items[0].github_number, 42);
}

#[test]
fn queue_upsert_update() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let row = make_row(&repo_id, "issue:org/test-repo:1", QueuePhase::Pending);
    db.queue_upsert(&row).unwrap();

    // Update phase
    let mut row2 = row;
    row2.phase = QueuePhase::Running;
    db.queue_upsert(&row2).unwrap();

    let phase = db.queue_get_phase("issue:org/test-repo:1").unwrap();
    assert_eq!(phase, Some(QueuePhase::Running));

    // Should still be 1 item
    let items = db.queue_list_items(None).unwrap();
    assert_eq!(items.len(), 1);
}

#[test]
fn queue_upsert_preserves_created_at() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let row = make_row(&repo_id, "issue:org/test-repo:1", QueuePhase::Pending);
    let original_created = row.created_at.clone();
    db.queue_upsert(&row).unwrap();

    // Upsert with different created_at
    let mut row2 = row;
    row2.phase = QueuePhase::Running;
    row2.created_at = "2099-01-01T00:00:00Z".to_string();
    db.queue_upsert(&row2).unwrap();

    let items = db.queue_list_items(None).unwrap();
    // ON CONFLICT DO UPDATE does not touch created_at
    assert_eq!(items[0].created_at, original_created);
}

#[test]
fn queue_load_active_excludes_terminal() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    db.queue_upsert(&make_row(&repo_id, "w-1", QueuePhase::Pending))
        .unwrap();
    db.queue_upsert(&make_row(&repo_id, "w-2", QueuePhase::Running))
        .unwrap();
    db.queue_upsert(&make_row(&repo_id, "w-3", QueuePhase::Done))
        .unwrap();
    db.queue_upsert(&make_row(&repo_id, "w-4", QueuePhase::Skipped))
        .unwrap();

    let active = db.queue_load_active(&repo_id).unwrap();
    assert_eq!(active.len(), 2);
    let ids: Vec<&str> = active.iter().map(|r| r.work_id.as_str()).collect();
    assert!(ids.contains(&"w-1"));
    assert!(ids.contains(&"w-2"));
}

#[test]
fn queue_transit_cas_success() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    db.queue_upsert(&make_row(&repo_id, "w-1", QueuePhase::Pending))
        .unwrap();

    let ok = db
        .queue_transit("w-1", QueuePhase::Pending, QueuePhase::Running)
        .unwrap();
    assert!(ok);

    let phase = db.queue_get_phase("w-1").unwrap();
    assert_eq!(phase, Some(QueuePhase::Running));
}

#[test]
fn queue_transit_cas_wrong_phase() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    db.queue_upsert(&make_row(&repo_id, "w-1", QueuePhase::Running))
        .unwrap();

    let ok = db
        .queue_transit("w-1", QueuePhase::Pending, QueuePhase::Running)
        .unwrap();
    assert!(!ok);

    // Phase unchanged
    let phase = db.queue_get_phase("w-1").unwrap();
    assert_eq!(phase, Some(QueuePhase::Running));
}

#[test]
fn queue_remove_marks_done() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    db.queue_upsert(&make_row(&repo_id, "w-1", QueuePhase::Running))
        .unwrap();

    db.queue_remove("w-1").unwrap();

    let phase = db.queue_get_phase("w-1").unwrap();
    assert_eq!(phase, Some(QueuePhase::Done));
}

// ═══════════════════════════════════════════════
// 8. CLI output includes task_kind
// ═══════════════════════════════════════════════

#[test]
fn cli_queue_list_shows_task_kind() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    db.queue_upsert(&make_row(
        &repo_id,
        "issue:org/test-repo:42",
        QueuePhase::Pending,
    ))
    .unwrap();

    let output = autodev::cli::queue::queue_list_db(&db, None, false, None, false).unwrap();
    assert!(output.contains("analyze"));
    assert!(output.contains("#42"));
}

// ═══════════════════════════════════════════════
// 9. Decision recording (H2)
// ═══════════════════════════════════════════════

#[test]
fn advance_records_decision() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-dec-1", "pending");

    autodev::cli::queue::queue_advance(&db, "work-dec-1", Some("auto-approved")).unwrap();

    let decisions = db.decision_list(None, 10).unwrap();
    assert!(!decisions.is_empty());
    let d = &decisions[0];
    assert_eq!(
        d.decision_type,
        autodev::core::models::DecisionType::Advance
    );
    assert_eq!(d.target_work_id.as_deref(), Some("work-dec-1"));
    assert_eq!(d.reasoning, "auto-approved");
}

#[test]
fn skip_records_decision() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    insert_queue_item(&db, &repo_id, "work-dec-2", "pending");

    autodev::cli::queue::queue_skip(&db, "work-dec-2", Some("out of scope")).unwrap();

    let decisions = db.decision_list(None, 10).unwrap();
    assert!(!decisions.is_empty());
    let d = &decisions[0];
    assert_eq!(d.decision_type, autodev::core::models::DecisionType::Skip);
    assert_eq!(d.target_work_id.as_deref(), Some("work-dec-2"));
    assert_eq!(d.reasoning, "out of scope");
}

// ═══════════════════════════════════════════════
// 10. queue_get_item
// ═══════════════════════════════════════════════

#[test]
fn queue_get_item_returns_existing() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    db.queue_upsert(&make_row(
        &repo_id,
        "issue:org/test-repo:99",
        QueuePhase::Pending,
    ))
    .unwrap();

    let item = db.queue_get_item("issue:org/test-repo:99").unwrap();
    assert!(item.is_some());
    let item = item.unwrap();
    assert_eq!(item.work_id, "issue:org/test-repo:99");
    assert_eq!(item.phase, QueuePhase::Pending);
}

#[test]
fn queue_get_item_returns_none_for_nonexistent() {
    let db = open_memory_db();

    let item = db.queue_get_item("nonexistent").unwrap();
    assert!(item.is_none());
}

// ═══════════════════════════════════════════════
// 11. HITL auto-trigger on review overflow (H3)
// ═══════════════════════════════════════════════

#[test]
fn advance_creates_hitl_on_review_overflow() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    // Insert a PR item with review_iteration >= max (2)
    let pr_metadata = serde_json::json!({
        "Pr": {
            "head_branch": "feat/test",
            "base_branch": "main",
            "review_comment": null,
            "source_issue_number": null,
            "review_iteration": 3
        }
    });
    let now = chrono::Utc::now().to_rfc3339();
    db.conn()
        .execute(
            "INSERT INTO queue_items (work_id, repo_id, queue_type, phase, title, created_at, updated_at, metadata_json) \
             VALUES (?1, ?2, 'pr', 'pending', 'PR with high iterations', ?3, ?3, ?4)",
            rusqlite::params!["pr:org/test-repo:50", repo_id, now, pr_metadata.to_string()],
        )
        .unwrap();

    autodev::cli::queue::queue_advance(&db, "pr:org/test-repo:50", None).unwrap();

    // Should have created a HITL event
    let hitl_events = db.hitl_list(None).unwrap();
    assert!(
        !hitl_events.is_empty(),
        "HITL event should be created for review overflow"
    );
    let event = &hitl_events[0];
    assert!(event.situation.contains("review iteration"));
    assert_eq!(event.work_id.as_deref(), Some("pr:org/test-repo:50"));
}

#[test]
fn advance_no_hitl_when_below_threshold() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    // Insert a PR item with review_iteration below max
    let pr_metadata = serde_json::json!({
        "Pr": {
            "head_branch": "feat/test",
            "base_branch": "main",
            "review_comment": null,
            "source_issue_number": null,
            "review_iteration": 1
        }
    });
    let now = chrono::Utc::now().to_rfc3339();
    db.conn()
        .execute(
            "INSERT INTO queue_items (work_id, repo_id, queue_type, phase, title, created_at, updated_at, metadata_json) \
             VALUES (?1, ?2, 'pr', 'pending', 'Normal PR', ?3, ?3, ?4)",
            rusqlite::params!["pr:org/test-repo:51", repo_id, now, pr_metadata.to_string()],
        )
        .unwrap();

    autodev::cli::queue::queue_advance(&db, "pr:org/test-repo:51", None).unwrap();

    // Should NOT have created a HITL event
    let hitl_events = db.hitl_list(None).unwrap();
    assert!(
        hitl_events.is_empty(),
        "No HITL event for low iteration count"
    );
}
