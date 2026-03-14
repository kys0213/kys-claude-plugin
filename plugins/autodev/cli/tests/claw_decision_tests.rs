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

fn make_decision(repo_id: &str, decision_type: &str, reasoning: &str) -> NewClawDecision {
    NewClawDecision {
        repo_id: repo_id.to_string(),
        spec_id: None,
        decision_type: decision_type.to_string(),
        target_work_id: None,
        reasoning: reasoning.to_string(),
        context_json: None,
    }
}

// ═══════════════════════════════════════════════
// 1. decision_add returns valid UUID
// ═══════════════════════════════════════════════

#[test]
fn decision_add_returns_valid_uuid() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    let id = db
        .decision_add(&make_decision(&repo_id, "advance", "Ready to proceed"))
        .unwrap();
    assert!(!id.is_empty());
    // UUID v4 format: 8-4-4-4-12 hex chars
    assert_eq!(id.len(), 36);
    assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);
}

// ═══════════════════════════════════════════════
// 2. decision_show returns the created decision
// ═══════════════════════════════════════════════

#[test]
fn decision_show_returns_created_decision() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    let decision = NewClawDecision {
        repo_id: repo_id.clone(),
        spec_id: Some("spec-123".to_string()),
        decision_type: "advance".to_string(),
        target_work_id: Some("issue:42".to_string()),
        reasoning: "Ready to implement".to_string(),
        context_json: Some(r#"{"confidence": 0.95}"#.to_string()),
    };
    let id = db.decision_add(&decision).unwrap();

    let loaded = db.decision_show(&id).unwrap().unwrap();
    assert_eq!(loaded.id, id);
    assert_eq!(loaded.repo_id, repo_id);
    assert_eq!(loaded.spec_id.as_deref(), Some("spec-123"));
    assert_eq!(loaded.decision_type, "advance");
    assert_eq!(loaded.target_work_id.as_deref(), Some("issue:42"));
    assert_eq!(loaded.reasoning, "Ready to implement");
    assert_eq!(
        loaded.context_json.as_deref(),
        Some(r#"{"confidence": 0.95}"#)
    );
    assert!(!loaded.created_at.is_empty());
}

// ═══════════════════════════════════════════════
// 3. decision_show returns None for non-existent id
// ═══════════════════════════════════════════════

#[test]
fn decision_show_nonexistent_returns_none() {
    let db = open_memory_db();
    let result = db.decision_show("nonexistent-id").unwrap();
    assert!(result.is_none());
}

// ═══════════════════════════════════════════════
// 4. decision_list returns all decisions ordered by created_at DESC
// ═══════════════════════════════════════════════

#[test]
fn decision_list_returns_all_ordered_desc() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    let id1 = db
        .decision_add(&make_decision(&repo_id, "advance", "First"))
        .unwrap();
    let id2 = db
        .decision_add(&make_decision(&repo_id, "skip", "Second"))
        .unwrap();
    let id3 = db
        .decision_add(&make_decision(&repo_id, "hitl", "Third"))
        .unwrap();

    let decisions = db.decision_list(None, 100).unwrap();
    assert_eq!(decisions.len(), 3);
    // DESC order: newest first
    assert_eq!(decisions[0].id, id3);
    assert_eq!(decisions[1].id, id2);
    assert_eq!(decisions[2].id, id1);
}

// ═══════════════════════════════════════════════
// 5. decision_list with repo filter
// ═══════════════════════════════════════════════

#[test]
fn decision_list_filters_by_repo() {
    let db = open_memory_db();
    let repo_id_a = add_test_repo(&db, "org/repo-a");
    let repo_id_b = add_test_repo(&db, "org/repo-b");

    db.decision_add(&make_decision(&repo_id_a, "advance", "For repo A"))
        .unwrap();
    db.decision_add(&make_decision(&repo_id_b, "skip", "For repo B"))
        .unwrap();
    db.decision_add(&make_decision(&repo_id_b, "hitl", "Also for repo B"))
        .unwrap();

    let all = db.decision_list(None, 100).unwrap();
    assert_eq!(all.len(), 3);

    let repo_a = db.decision_list(Some("org/repo-a"), 100).unwrap();
    assert_eq!(repo_a.len(), 1);
    assert_eq!(repo_a[0].reasoning, "For repo A");

    let repo_b = db.decision_list(Some("org/repo-b"), 100).unwrap();
    assert_eq!(repo_b.len(), 2);
}

// ═══════════════════════════════════════════════
// 6. decision_list with limit
// ═══════════════════════════════════════════════

#[test]
fn decision_list_respects_limit() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    for i in 0..5 {
        db.decision_add(&make_decision(
            &repo_id,
            "advance",
            &format!("Decision {i}"),
        ))
        .unwrap();
    }

    let limited = db.decision_list(None, 2).unwrap();
    assert_eq!(limited.len(), 2);
}

// ═══════════════════════════════════════════════
// 7. decision_list_by_spec returns only decisions for that spec
// ═══════════════════════════════════════════════

#[test]
fn decision_list_by_spec_filters_correctly() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    let mut d1 = make_decision(&repo_id, "advance", "For spec-1");
    d1.spec_id = Some("spec-1".to_string());
    db.decision_add(&d1).unwrap();

    let mut d2 = make_decision(&repo_id, "skip", "For spec-2");
    d2.spec_id = Some("spec-2".to_string());
    db.decision_add(&d2).unwrap();

    let mut d3 = make_decision(&repo_id, "replan", "Also for spec-1");
    d3.spec_id = Some("spec-1".to_string());
    db.decision_add(&d3).unwrap();

    // No spec
    db.decision_add(&make_decision(&repo_id, "advance", "No spec"))
        .unwrap();

    let spec1 = db.decision_list_by_spec("spec-1", 100).unwrap();
    assert_eq!(spec1.len(), 2);

    let spec2 = db.decision_list_by_spec("spec-2", 100).unwrap();
    assert_eq!(spec2.len(), 1);
}

// ═══════════════════════════════════════════════
// 8. decision_list_by_spec returns empty for non-existent spec
// ═══════════════════════════════════════════════

#[test]
fn decision_list_by_spec_nonexistent_returns_empty() {
    let db = open_memory_db();
    let results = db.decision_list_by_spec("nonexistent-spec", 100).unwrap();
    assert!(results.is_empty());
}

// ═══════════════════════════════════════════════
// 9. decision_count returns total count
// ═══════════════════════════════════════════════

#[test]
fn decision_count_returns_total() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    assert_eq!(db.decision_count(None).unwrap(), 0);

    db.decision_add(&make_decision(&repo_id, "advance", "One"))
        .unwrap();
    db.decision_add(&make_decision(&repo_id, "skip", "Two"))
        .unwrap();

    assert_eq!(db.decision_count(None).unwrap(), 2);
}

// ═══════════════════════════════════════════════
// 10. decision_count with repo filter
// ═══════════════════════════════════════════════

#[test]
fn decision_count_filters_by_repo() {
    let db = open_memory_db();
    let repo_id_a = add_test_repo(&db, "org/repo-a");
    let repo_id_b = add_test_repo(&db, "org/repo-b");

    db.decision_add(&make_decision(&repo_id_a, "advance", "A"))
        .unwrap();
    db.decision_add(&make_decision(&repo_id_b, "skip", "B1"))
        .unwrap();
    db.decision_add(&make_decision(&repo_id_b, "hitl", "B2"))
        .unwrap();

    assert_eq!(db.decision_count(None).unwrap(), 3);
    assert_eq!(db.decision_count(Some("org/repo-a")).unwrap(), 1);
    assert_eq!(db.decision_count(Some("org/repo-b")).unwrap(), 2);
}

// ═══════════════════════════════════════════════
// 11. multiple decisions with different types
// ═══════════════════════════════════════════════

#[test]
fn decision_different_types() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    for dtype in &["advance", "skip", "hitl", "replan"] {
        db.decision_add(&make_decision(&repo_id, dtype, &format!("type: {dtype}")))
            .unwrap();
    }

    let all = db.decision_list(None, 100).unwrap();
    assert_eq!(all.len(), 4);

    let types: Vec<&str> = all.iter().map(|d| d.decision_type.as_str()).collect();
    assert!(types.contains(&"advance"));
    assert!(types.contains(&"skip"));
    assert!(types.contains(&"hitl"));
    assert!(types.contains(&"replan"));
}

// ═══════════════════════════════════════════════
// 12. decision with context_json stores and retrieves correctly
// ═══════════════════════════════════════════════

#[test]
fn decision_context_json_roundtrip() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    let context = r#"{"queue_depth": 5, "retry_count": 2, "tags": ["urgent", "review"]}"#;
    let mut d = make_decision(&repo_id, "advance", "With context");
    d.context_json = Some(context.to_string());
    let id = db.decision_add(&d).unwrap();

    let loaded = db.decision_show(&id).unwrap().unwrap();
    assert_eq!(loaded.context_json.as_deref(), Some(context));

    // Verify it's valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(loaded.context_json.as_deref().unwrap()).unwrap();
    assert_eq!(parsed["queue_depth"], 5);
}
