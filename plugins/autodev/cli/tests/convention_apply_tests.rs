use autodev::cli::convention::{apply_approved, parse_convention_context};
use autodev::core::models::*;
use autodev::core::repository::*;
use autodev::infra::db::Database;
use std::path::Path;
use tempfile::TempDir;

// ─── Helpers ───

fn open_memory_db() -> Database {
    let db = Database::open(Path::new(":memory:")).expect("open in-memory db");
    db.initialize().expect("initialize schema");
    db
}

fn add_test_repo(db: &Database, name: &str) -> String {
    db.workspace_add(&format!("https://github.com/{name}"), name)
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

fn create_convention_hitl(db: &Database, repo_id: &str, pattern_type: &str) -> String {
    let rule_file = format!(".claude/rules/{pattern_type}.md");
    db.hitl_create(&NewHitlEvent {
        repo_id: repo_id.to_string(),
        spec_id: None,
        work_id: None,
        severity: HitlSeverity::Medium,
        situation: format!("Convention update suggested: {pattern_type}"),
        context: format!(
            "Rule file: {rule_file}\nOccurrences: 3\nSuggestion: Use best practices for {pattern_type}\nSources: {{\"hitl\": 3}}"
        ),
        options: vec![
            "Apply this convention rule".to_string(),
            "Edit and apply".to_string(),
            "Reject".to_string(),
        ],
    })
    .unwrap()
}

fn respond_hitl(db: &Database, event_id: &str, choice: i32, message: Option<&str>) {
    db.hitl_respond(&NewHitlResponse {
        event_id: event_id.to_string(),
        choice: Some(choice),
        message: message.map(|s| s.to_string()),
        source: "cli".to_string(),
    })
    .unwrap();
}

// ═══════════════════════════════════════════════
// 1. parse_convention_context extracts rule file and suggestion
// ═══════════════════════════════════════════════

#[test]
fn parse_convention_context_extracts_correctly() {
    let context = "Rule file: .claude/rules/error-handling.md\nOccurrences: 5\nSuggestion: Use thiserror for all error types\nSources: {\"hitl\": 3, \"pr-review\": 2}";
    let result = parse_convention_context(context);
    assert!(result.is_some());
    let (rule_file, suggestion) = result.unwrap();
    assert_eq!(rule_file, ".claude/rules/error-handling.md");
    assert_eq!(suggestion, "Use thiserror for all error types");
}

#[test]
fn parse_convention_context_returns_none_for_missing_rule_file() {
    let context = "Occurrences: 5\nSuggestion: Use thiserror";
    assert!(parse_convention_context(context).is_none());
}

#[test]
fn parse_convention_context_returns_none_for_missing_suggestion() {
    let context = "Rule file: .claude/rules/error-handling.md\nOccurrences: 5";
    assert!(parse_convention_context(context).is_none());
}

#[test]
fn parse_convention_context_returns_none_for_empty_string() {
    assert!(parse_convention_context("").is_none());
}

// ═══════════════════════════════════════════════
// 2. apply_approved writes rule file when choice=1
// ═══════════════════════════════════════════════

#[test]
fn apply_approved_writes_rule_file_on_choice_1() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/apply-test");
    let tmp = TempDir::new().unwrap();

    let event_id = create_convention_hitl(&db, &repo_id, "error-handling");
    respond_hitl(&db, &event_id, 1, None);

    let output = apply_approved(&db, "org/apply-test", &repo_id, tmp.path()).unwrap();
    assert!(output.contains("1 applied"));

    let rule_path = tmp.path().join(".claude/rules/error-handling.md");
    assert!(rule_path.exists());
    let content = std::fs::read_to_string(&rule_path).unwrap();
    assert!(content.contains("Use best practices for error-handling"));
    assert!(content.contains("# Error Handling"));
}

// ═══════════════════════════════════════════════
// 3. apply_approved uses response message when choice=2
// ═══════════════════════════════════════════════

#[test]
fn apply_approved_uses_response_message_on_choice_2() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/edit-apply-test");
    let tmp = TempDir::new().unwrap();

    let event_id = create_convention_hitl(&db, &repo_id, "testing");
    respond_hitl(
        &db,
        &event_id,
        2,
        Some("Always write integration tests before merging"),
    );

    let output = apply_approved(&db, "org/edit-apply-test", &repo_id, tmp.path()).unwrap();
    assert!(output.contains("1 applied"));

    let rule_path = tmp.path().join(".claude/rules/testing.md");
    assert!(rule_path.exists());
    let content = std::fs::read_to_string(&rule_path).unwrap();
    assert!(content.contains("Always write integration tests before merging"));
    // Should NOT contain the original suggestion
    assert!(!content.contains("Use best practices for testing"));
}

// ═══════════════════════════════════════════════
// 4. apply_approved skips rejected (choice=3)
// ═══════════════════════════════════════════════

#[test]
fn apply_approved_skips_rejected_choice_3() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/reject-test");
    let tmp = TempDir::new().unwrap();

    let event_id = create_convention_hitl(&db, &repo_id, "style");
    respond_hitl(&db, &event_id, 3, None);

    let output = apply_approved(&db, "org/reject-test", &repo_id, tmp.path()).unwrap();
    assert!(output.contains("1 rejected"));
    assert!(output.contains("0 applied"));

    let rule_path = tmp.path().join(".claude/rules/style.md");
    assert!(!rule_path.exists());
}

// ═══════════════════════════════════════════════
// 5. apply_approved updates pattern status to Applied
// ═══════════════════════════════════════════════

#[test]
fn apply_approved_updates_pattern_status_to_applied() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/status-applied-test");
    let tmp = TempDir::new().unwrap();

    // Create and propose pattern
    for _ in 0..3 {
        db.feedback_upsert(&make_pattern(&repo_id, "error-handling", "Use anyhow"))
            .unwrap();
    }
    autodev::cli::convention::propose_updates(&db, &repo_id, 3).unwrap();

    // Find the HITL event created by propose_updates
    let events = db.hitl_list(Some("org/status-applied-test")).unwrap();
    assert_eq!(events.len(), 1);
    respond_hitl(&db, &events[0].id, 1, None);

    apply_approved(&db, "org/status-applied-test", &repo_id, tmp.path()).unwrap();

    // Pattern status should be Applied
    let patterns = db.feedback_list(&repo_id).unwrap();
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].status, FeedbackPatternStatus::Applied);
}

// ═══════════════════════════════════════════════
// 6. apply_approved updates pattern status to Rejected
// ═══════════════════════════════════════════════

#[test]
fn apply_approved_updates_pattern_status_to_rejected() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/status-rejected-test");
    let tmp = TempDir::new().unwrap();

    for _ in 0..3 {
        db.feedback_upsert(&make_pattern(&repo_id, "style", "Use snake_case"))
            .unwrap();
    }
    autodev::cli::convention::propose_updates(&db, &repo_id, 3).unwrap();

    let events = db.hitl_list(Some("org/status-rejected-test")).unwrap();
    respond_hitl(&db, &events[0].id, 3, None);

    apply_approved(&db, "org/status-rejected-test", &repo_id, tmp.path()).unwrap();

    let patterns = db.feedback_list(&repo_id).unwrap();
    assert_eq!(patterns[0].status, FeedbackPatternStatus::Rejected);
}

// ═══════════════════════════════════════════════
// 7. Idempotency: already-applied events are skipped on second run
// ═══════════════════════════════════════════════

#[test]
fn apply_approved_idempotent_already_applied() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/idempotent-test");
    let tmp = TempDir::new().unwrap();

    let event_id = create_convention_hitl(&db, &repo_id, "error-handling");
    respond_hitl(&db, &event_id, 1, None);

    // First apply
    let output1 = apply_approved(&db, "org/idempotent-test", &repo_id, tmp.path()).unwrap();
    assert!(output1.contains("1 applied"));

    // Second apply - HITL event is now marked as Applied, so it is skipped
    let output2 = apply_approved(&db, "org/idempotent-test", &repo_id, tmp.path()).unwrap();
    assert!(output2.contains("0 applied"));
    assert!(output2.contains("0 rejected"));
    assert!(output2.contains("0 skipped"));
}

// ═══════════════════════════════════════════════
// 8. apply_approved appends to existing rule file
// ═══════════════════════════════════════════════

#[test]
fn apply_approved_appends_to_existing_rule_file() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/append-test");
    let tmp = TempDir::new().unwrap();

    // Create existing rule file
    let rules_dir = tmp.path().join(".claude/rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(
        rules_dir.join("error-handling.md"),
        "# Error Handling\n\nExisting content\n",
    )
    .unwrap();

    let event_id = create_convention_hitl(&db, &repo_id, "error-handling");
    respond_hitl(&db, &event_id, 1, None);

    apply_approved(&db, "org/append-test", &repo_id, tmp.path()).unwrap();

    let content = std::fs::read_to_string(rules_dir.join("error-handling.md")).unwrap();
    assert!(content.contains("Existing content"));
    assert!(content.contains("Use best practices for error-handling"));
}

// ═══════════════════════════════════════════════
// 9. apply_approved with no convention events returns zeros
// ═══════════════════════════════════════════════

#[test]
fn apply_approved_no_events_returns_zeros() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/empty-apply-test");
    let tmp = TempDir::new().unwrap();

    let output = apply_approved(&db, "org/empty-apply-test", &repo_id, tmp.path()).unwrap();
    assert!(output.contains("0 applied"));
    assert!(output.contains("0 rejected"));
    assert!(output.contains("0 skipped"));
}

// ═══════════════════════════════════════════════
// 10. apply_approved choice=2 with empty message skips
// ═══════════════════════════════════════════════

#[test]
fn apply_approved_choice_2_empty_message_skips() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/empty-edit-test");
    let tmp = TempDir::new().unwrap();

    let event_id = create_convention_hitl(&db, &repo_id, "testing");
    respond_hitl(&db, &event_id, 2, Some("   "));

    let output = apply_approved(&db, "org/empty-edit-test", &repo_id, tmp.path()).unwrap();
    assert!(output.contains("1 skipped"));

    let rule_path = tmp.path().join(".claude/rules/testing.md");
    assert!(!rule_path.exists());
}

// ═══════════════════════════════════════════════
// 11. apply_approved marks HITL event status to Applied
// ═══════════════════════════════════════════════

#[test]
fn apply_approved_marks_hitl_event_as_applied() {
    use autodev::core::models::HitlStatus;
    use autodev::core::repository::HitlRepository;

    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/hitl-status-test");
    let tmp = TempDir::new().unwrap();

    let event_id = create_convention_hitl(&db, &repo_id, "error-handling");
    respond_hitl(&db, &event_id, 1, None);

    apply_approved(&db, "org/hitl-status-test", &repo_id, tmp.path()).unwrap();

    // HITL event should now have Applied status
    let event = db.hitl_show(&event_id).unwrap().unwrap();
    assert_eq!(event.status, HitlStatus::Applied);
}

// ═══════════════════════════════════════════════
// 12. apply_approved marks rejected HITL event as Applied too
// ═══════════════════════════════════════════════

#[test]
fn apply_approved_marks_rejected_hitl_event_as_applied() {
    use autodev::core::models::HitlStatus;
    use autodev::core::repository::HitlRepository;

    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/hitl-reject-status-test");
    let tmp = TempDir::new().unwrap();

    let event_id = create_convention_hitl(&db, &repo_id, "style");
    respond_hitl(&db, &event_id, 3, None);

    apply_approved(&db, "org/hitl-reject-status-test", &repo_id, tmp.path()).unwrap();

    // HITL event should be Applied (consumed), regardless of rejection
    let event = db.hitl_show(&event_id).unwrap().unwrap();
    assert_eq!(event.status, HitlStatus::Applied);
}
