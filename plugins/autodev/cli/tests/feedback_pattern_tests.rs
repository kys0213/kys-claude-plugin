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

fn add_test_repo(db: &Database, name: &str) -> String {
    db.repo_add(&format!("https://github.com/{name}"), name)
        .expect("add repo")
}

fn make_pattern(repo_id: &str, pattern_type: &str, suggestion: &str) -> NewFeedbackPattern {
    NewFeedbackPattern {
        repo_id: repo_id.to_string(),
        pattern_type: pattern_type.to_string(),
        suggestion: suggestion.to_string(),
        source: "hitl".to_string(),
    }
}

// ═══════════════════════════════════════════════
// 1. feedback_upsert creates new pattern
// ═══════════════════════════════════════════════

#[test]
fn feedback_upsert_creates_new_pattern() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    let id = db
        .feedback_upsert(&make_pattern(&repo_id, "error-handling", "Use Result<T>"))
        .unwrap();
    assert!(!id.is_empty());
    assert_eq!(id.len(), 36); // UUID v4

    let patterns = db.feedback_list(&repo_id).unwrap();
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].pattern_type, "error-handling");
    assert_eq!(patterns[0].suggestion, "Use Result<T>");
    assert_eq!(patterns[0].source, "hitl");
    assert_eq!(patterns[0].occurrence_count, 1);
    assert_eq!(patterns[0].status, FeedbackPatternStatus::Active);
}

// ═══════════════════════════════════════════════
// 2. feedback_upsert increments count on duplicate
// ═══════════════════════════════════════════════

#[test]
fn feedback_upsert_increments_on_duplicate() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    let id1 = db
        .feedback_upsert(&make_pattern(&repo_id, "testing", "Add unit tests"))
        .unwrap();
    let id2 = db
        .feedback_upsert(&make_pattern(&repo_id, "testing", "Add unit tests"))
        .unwrap();

    // Same id returned
    assert_eq!(id1, id2);

    let patterns = db.feedback_list(&repo_id).unwrap();
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].occurrence_count, 2);
}

// ═══════════════════════════════════════════════
// 3. feedback_upsert updates sources_json on duplicate
// ═══════════════════════════════════════════════

#[test]
fn feedback_upsert_updates_sources_json() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    db.feedback_upsert(&make_pattern(&repo_id, "style", "Use snake_case"))
        .unwrap();

    // Second upsert with different source
    let mut pattern = make_pattern(&repo_id, "style", "Use snake_case");
    pattern.source = "pr-review".to_string();
    db.feedback_upsert(&pattern).unwrap();

    let patterns = db.feedback_list(&repo_id).unwrap();
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].occurrence_count, 2);

    // Verify sources_json contains both sources
    let sources: serde_json::Value =
        serde_json::from_str(&patterns[0].sources_json).expect("valid JSON");
    assert_eq!(sources["hitl"], 1);
    assert_eq!(sources["pr-review"], 1);
}

// ═══════════════════════════════════════════════
// 4. feedback_list returns only patterns for the specified repo_id
// ═══════════════════════════════════════════════

#[test]
fn feedback_list_scoped_by_repo() {
    let db = open_memory_db();
    let repo_a = add_test_repo(&db, "org/repo-a");
    let repo_b = add_test_repo(&db, "org/repo-b");

    db.feedback_upsert(&make_pattern(&repo_a, "testing", "Add tests"))
        .unwrap();
    db.feedback_upsert(&make_pattern(&repo_a, "style", "Use fmt"))
        .unwrap();
    db.feedback_upsert(&make_pattern(&repo_b, "testing", "Add tests"))
        .unwrap();

    let patterns_a = db.feedback_list(&repo_a).unwrap();
    assert_eq!(patterns_a.len(), 2);

    let patterns_b = db.feedback_list(&repo_b).unwrap();
    assert_eq!(patterns_b.len(), 1);
}

// ═══════════════════════════════════════════════
// 5. feedback_list_actionable filters by min_count
// ═══════════════════════════════════════════════

#[test]
fn feedback_list_actionable_filters_by_min_count() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    // Pattern with 1 occurrence
    db.feedback_upsert(&make_pattern(&repo_id, "testing", "Add tests"))
        .unwrap();

    // Pattern with 3 occurrences
    db.feedback_upsert(&make_pattern(&repo_id, "style", "Use fmt"))
        .unwrap();
    db.feedback_upsert(&make_pattern(&repo_id, "style", "Use fmt"))
        .unwrap();
    db.feedback_upsert(&make_pattern(&repo_id, "style", "Use fmt"))
        .unwrap();

    let all = db.feedback_list_actionable(&repo_id, 1).unwrap();
    assert_eq!(all.len(), 2);

    let high_count = db.feedback_list_actionable(&repo_id, 3).unwrap();
    assert_eq!(high_count.len(), 1);
    assert_eq!(high_count[0].pattern_type, "style");

    let none = db.feedback_list_actionable(&repo_id, 10).unwrap();
    assert!(none.is_empty());
}

// ═══════════════════════════════════════════════
// 6. feedback_list_actionable excludes non-active status
// ═══════════════════════════════════════════════

#[test]
fn feedback_list_actionable_excludes_non_active() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    let id = db
        .feedback_upsert(&make_pattern(&repo_id, "testing", "Add tests"))
        .unwrap();
    db.feedback_upsert(&make_pattern(&repo_id, "testing", "Add tests"))
        .unwrap();

    // Mark as applied
    db.feedback_set_status(&id, FeedbackPatternStatus::Applied)
        .unwrap();

    let actionable = db.feedback_list_actionable(&repo_id, 1).unwrap();
    assert!(actionable.is_empty());
}

// ═══════════════════════════════════════════════
// 7. feedback_set_status updates status
// ═══════════════════════════════════════════════

#[test]
fn feedback_set_status_updates_correctly() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    let id = db
        .feedback_upsert(&make_pattern(&repo_id, "testing", "Add tests"))
        .unwrap();

    let patterns = db.feedback_list(&repo_id).unwrap();
    assert_eq!(patterns[0].status, FeedbackPatternStatus::Active);

    db.feedback_set_status(&id, FeedbackPatternStatus::Rejected)
        .unwrap();

    let patterns = db.feedback_list(&repo_id).unwrap();
    assert_eq!(patterns[0].status, FeedbackPatternStatus::Rejected);
}

// ═══════════════════════════════════════════════
// 8. feedback_set_status on nonexistent id fails
// ═══════════════════════════════════════════════

#[test]
fn feedback_set_status_nonexistent_fails() {
    let db = open_memory_db();
    let result = db.feedback_set_status("nonexistent", FeedbackPatternStatus::Applied);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

// ═══════════════════════════════════════════════
// 9. patterns CLI function formats output correctly
// ═══════════════════════════════════════════════

#[test]
fn patterns_cli_formats_table() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    db.feedback_upsert(&make_pattern(&repo_id, "error-handling", "Use anyhow"))
        .unwrap();
    db.feedback_upsert(&make_pattern(&repo_id, "testing", "Add unit tests"))
        .unwrap();

    let output = autodev::cli::convention::patterns(&db, Some(&repo_id), Some(1), false).unwrap();
    assert!(output.contains("COUNT"));
    assert!(output.contains("TYPE"));
    assert!(output.contains("error-handling"));
    assert!(output.contains("testing"));
}

// ═══════════════════════════════════════════════
// 10. patterns CLI function with JSON output
// ═══════════════════════════════════════════════

#[test]
fn patterns_cli_json_output() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    db.feedback_upsert(&make_pattern(&repo_id, "style", "Use snake_case"))
        .unwrap();

    let output = autodev::cli::convention::patterns(&db, Some(&repo_id), None, true).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON output");
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 1);
    assert_eq!(parsed[0]["pattern_type"], "style");
}

// ═══════════════════════════════════════════════
// 11. patterns CLI function without repo returns message
// ═══════════════════════════════════════════════

#[test]
fn patterns_cli_no_repo_returns_message() {
    let db = open_memory_db();
    let output = autodev::cli::convention::patterns(&db, None, None, false).unwrap();
    assert!(output.contains("Specify --repo"));
}

// ═══════════════════════════════════════════════
// 12. Different (repo, pattern_type, suggestion) combos create separate entries
// ═══════════════════════════════════════════════

#[test]
fn feedback_upsert_different_combos_create_separate() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    db.feedback_upsert(&make_pattern(&repo_id, "testing", "Add unit tests"))
        .unwrap();
    db.feedback_upsert(&make_pattern(&repo_id, "testing", "Add integration tests"))
        .unwrap();
    db.feedback_upsert(&make_pattern(&repo_id, "style", "Add unit tests"))
        .unwrap();

    let patterns = db.feedback_list(&repo_id).unwrap();
    assert_eq!(patterns.len(), 3);
}
