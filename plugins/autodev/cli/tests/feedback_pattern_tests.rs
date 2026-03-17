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

// ═══════════════════════════════════════════════
// 13. classify_pattern_type returns correct types
// ═══════════════════════════════════════════════

#[test]
fn classify_pattern_type_error_handling() {
    use autodev::cli::convention::classify_pattern_type;
    assert_eq!(
        classify_pattern_type("Build error in CI pipeline"),
        "error-handling"
    );
    assert_eq!(
        classify_pattern_type("Compilation failure on main branch"),
        "error-handling"
    );
}

#[test]
fn classify_pattern_type_testing() {
    use autodev::cli::convention::classify_pattern_type;
    assert_eq!(classify_pattern_type("Missing test coverage"), "testing");
    assert_eq!(
        classify_pattern_type("Integration testing needed"),
        "testing"
    );
}

#[test]
fn classify_pattern_type_style() {
    use autodev::cli::convention::classify_pattern_type;
    assert_eq!(classify_pattern_type("Code style issue"), "style");
    assert_eq!(classify_pattern_type("Format check failed"), "style");
    assert_eq!(classify_pattern_type("Lint warning in module"), "style");
}

#[test]
fn classify_pattern_type_review_process() {
    use autodev::cli::convention::classify_pattern_type;
    assert_eq!(
        classify_pattern_type("Code review requested"),
        "review-process"
    );
    assert_eq!(
        classify_pattern_type("Iteration feedback"),
        "review-process"
    );
}

#[test]
fn classify_pattern_type_spec_management() {
    use autodev::cli::convention::classify_pattern_type;
    assert_eq!(
        classify_pattern_type("Spec conflict detected"),
        "spec-management"
    );
    assert_eq!(
        classify_pattern_type("Merge conflict in branch"),
        "spec-management"
    );
}

#[test]
fn classify_pattern_type_general() {
    use autodev::cli::convention::classify_pattern_type;
    assert_eq!(classify_pattern_type("Something else entirely"), "general");
    assert_eq!(classify_pattern_type(""), "general");
}

// ═══════════════════════════════════════════════
// 14. collect_feedback processes responded HITL events with messages
// ═══════════════════════════════════════════════

fn create_hitl_event(db: &Database, repo_id: &str, situation: &str) -> String {
    use autodev::core::models::{HitlSeverity, NewHitlEvent};
    db.hitl_create(&NewHitlEvent {
        repo_id: repo_id.to_string(),
        spec_id: None,
        work_id: None,
        severity: HitlSeverity::Medium,
        situation: situation.to_string(),
        context: "test context".to_string(),
        options: vec!["option1".to_string()],
    })
    .unwrap()
}

#[test]
fn collect_feedback_processes_responded_events() {
    use autodev::core::models::NewHitlResponse;

    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/collect-test");

    // Create HITL event and respond with a message
    let event_id = create_hitl_event(&db, &repo_id, "Build error in CI");
    db.hitl_respond(&NewHitlResponse {
        event_id: event_id.clone(),
        choice: Some(1),
        message: Some("Use anyhow for error handling".to_string()),
        source: "cli".to_string(),
    })
    .unwrap();

    let output =
        autodev::cli::convention::collect_feedback(&db, "org/collect-test", &repo_id).unwrap();
    assert!(output.contains("Collected 1 feedback pattern(s) from 1 HITL responses"));

    let patterns = db.feedback_list(&repo_id).unwrap();
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].pattern_type, "error-handling");
    assert_eq!(patterns[0].suggestion, "Use anyhow for error handling");
}

// ═══════════════════════════════════════════════
// 15. collect_feedback skips HITL events without messages
// ═══════════════════════════════════════════════

#[test]
fn collect_feedback_skips_events_without_messages() {
    use autodev::core::models::NewHitlResponse;

    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/no-msg-test");

    let event_id = create_hitl_event(&db, &repo_id, "Test coverage low");
    db.hitl_respond(&NewHitlResponse {
        event_id: event_id.clone(),
        choice: Some(1),
        message: None,
        source: "cli".to_string(),
    })
    .unwrap();

    let output =
        autodev::cli::convention::collect_feedback(&db, "org/no-msg-test", &repo_id).unwrap();
    assert!(output.contains("Collected 0 feedback pattern(s) from 1 HITL responses"));

    let patterns = db.feedback_list(&repo_id).unwrap();
    assert!(patterns.is_empty());
}

// ═══════════════════════════════════════════════
// 16. collect_feedback skips pending (non-responded) events
// ═══════════════════════════════════════════════

#[test]
fn collect_feedback_skips_pending_events() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/pending-test");

    // Create HITL event but do NOT respond
    create_hitl_event(&db, &repo_id, "Style issue found");

    let output =
        autodev::cli::convention::collect_feedback(&db, "org/pending-test", &repo_id).unwrap();
    assert!(output.contains("Collected 0 feedback pattern(s) from 0 HITL responses"));

    let patterns = db.feedback_list(&repo_id).unwrap();
    assert!(patterns.is_empty());
}

// ═══════════════════════════════════════════════
// 17. collect_feedback_from_hitl auto-collects for single event
// ═══════════════════════════════════════════════

#[test]
fn collect_feedback_from_hitl_creates_pattern() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/auto-collect-test");

    autodev::cli::convention::collect_feedback_from_hitl(
        &db,
        &repo_id,
        "Code review needed",
        "Add more assertions",
    )
    .unwrap();

    let patterns = db.feedback_list(&repo_id).unwrap();
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].pattern_type, "review-process");
    assert_eq!(patterns[0].suggestion, "Add more assertions");
}

// ═══════════════════════════════════════════════
// 18. collect_feedback_from_hitl skips empty messages
// ═══════════════════════════════════════════════

#[test]
fn collect_feedback_from_hitl_skips_empty_message() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/empty-msg-test");

    autodev::cli::convention::collect_feedback_from_hitl(&db, &repo_id, "Some situation", "  ")
        .unwrap();

    let patterns = db.feedback_list(&repo_id).unwrap();
    assert!(patterns.is_empty());
}

// ═══════════════════════════════════════════════
// 19. pattern_type_to_rule_file maps correctly
// ═══════════════════════════════════════════════

#[test]
fn pattern_type_to_rule_file_maps_correctly() {
    use autodev::cli::convention::pattern_type_to_rule_file;
    assert_eq!(
        pattern_type_to_rule_file("error-handling"),
        ".claude/rules/error-handling.md"
    );
    assert_eq!(
        pattern_type_to_rule_file("testing"),
        ".claude/rules/testing.md"
    );
    assert_eq!(pattern_type_to_rule_file("style"), ".claude/rules/style.md");
}

// ═══════════════════════════════════════════════
// 20. propose_updates creates HITL events for patterns above threshold
// ═══════════════════════════════════════════════

#[test]
fn propose_updates_creates_hitl_events() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/propose-test");

    // Create a pattern with 3 occurrences
    for _ in 0..3 {
        db.feedback_upsert(&make_pattern(&repo_id, "error-handling", "Use anyhow"))
            .unwrap();
    }

    let output = autodev::cli::convention::propose_updates(&db, &repo_id, 3).unwrap();
    assert!(output.contains("Proposed 1 convention update(s)"));

    // Verify HITL event was created
    let events = db.hitl_list(Some("org/propose-test")).unwrap();
    assert_eq!(events.len(), 1);
    assert!(events[0]
        .situation
        .contains("Convention update suggested: error-handling"));
}

// ═══════════════════════════════════════════════
// 21. propose_updates skips patterns below threshold
// ═══════════════════════════════════════════════

#[test]
fn propose_updates_skips_below_threshold() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/propose-skip-test");

    // Create a pattern with only 2 occurrences (below threshold of 3)
    db.feedback_upsert(&make_pattern(&repo_id, "testing", "Add tests"))
        .unwrap();
    db.feedback_upsert(&make_pattern(&repo_id, "testing", "Add tests"))
        .unwrap();

    let output = autodev::cli::convention::propose_updates(&db, &repo_id, 3).unwrap();
    assert!(output.contains("No actionable patterns found."));

    // No HITL events should be created
    let events = db.hitl_list(Some("org/propose-skip-test")).unwrap();
    assert!(events.is_empty());
}

// ═══════════════════════════════════════════════
// 22. propose_updates marks patterns as Proposed
// ═══════════════════════════════════════════════

#[test]
fn propose_updates_marks_patterns_as_proposed() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/propose-status-test");

    for _ in 0..3 {
        db.feedback_upsert(&make_pattern(&repo_id, "style", "Use snake_case"))
            .unwrap();
    }

    autodev::cli::convention::propose_updates(&db, &repo_id, 3).unwrap();

    // Pattern should now be Proposed
    let patterns = db.feedback_list(&repo_id).unwrap();
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].status, FeedbackPatternStatus::Proposed);
}

// ═══════════════════════════════════════════════
// 23. propose_updates returns no actionable when none qualify
// ═══════════════════════════════════════════════

#[test]
fn propose_updates_no_actionable_patterns() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/propose-empty-test");

    let output = autodev::cli::convention::propose_updates(&db, &repo_id, 3).unwrap();
    assert!(output.contains("No actionable patterns found."));
}

// ═══════════════════════════════════════════════
// 24. propose_updates is idempotent (already-proposed patterns are skipped)
// ═══════════════════════════════════════════════

#[test]
fn propose_updates_idempotent() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/propose-idempotent-test");

    for _ in 0..3 {
        db.feedback_upsert(&make_pattern(&repo_id, "error-handling", "Use Result<T>"))
            .unwrap();
    }

    // First call should propose
    let output1 = autodev::cli::convention::propose_updates(&db, &repo_id, 3).unwrap();
    assert!(output1.contains("Proposed 1 convention update(s)"));

    // Second call should find no actionable patterns (status is now Proposed)
    let output2 = autodev::cli::convention::propose_updates(&db, &repo_id, 3).unwrap();
    assert!(output2.contains("No actionable patterns found."));
}
