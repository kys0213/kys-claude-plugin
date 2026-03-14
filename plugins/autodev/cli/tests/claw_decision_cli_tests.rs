use autodev::cli::decisions;
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

fn add_decision(db: &Database, repo_id: &str, dtype: &str, reasoning: &str) -> String {
    db.decision_add(&NewClawDecision {
        repo_id: repo_id.to_string(),
        spec_id: None,
        decision_type: dtype.to_string(),
        target_work_id: None,
        reasoning: reasoning.to_string(),
        context_json: None,
    })
    .unwrap()
}

// ═══════════════════════════════════════════════
// CLI list tests
// ═══════════════════════════════════════════════

#[test]
fn cli_list_empty_shows_message() {
    let db = open_memory_db();
    let output = decisions::list(&db, None, 20, false).unwrap();
    assert!(output.contains("No decisions found"));
}

#[test]
fn cli_list_shows_header_and_rows() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");
    add_decision(&db, &repo_id, "advance", "Ready to proceed");

    let output = decisions::list(&db, None, 20, false).unwrap();
    assert!(output.contains("ID"));
    assert!(output.contains("TYPE"));
    assert!(output.contains("REASONING"));
    assert!(output.contains("advance"));
}

#[test]
fn cli_list_json_returns_valid_json() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");
    add_decision(&db, &repo_id, "skip", "Not ready yet");

    let output = decisions::list(&db, None, 20, true).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 1);
    assert_eq!(parsed[0]["decision_type"], "skip");
}

#[test]
fn cli_list_respects_limit() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");
    for i in 0..5 {
        add_decision(&db, &repo_id, "advance", &format!("Decision {i}"));
    }

    let output = decisions::list(&db, None, 2, true).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), 2);
}

#[test]
fn cli_list_filters_by_repo() {
    let db = open_memory_db();
    let repo_a = add_test_repo(&db, "org/repo-a");
    let repo_b = add_test_repo(&db, "org/repo-b");
    add_decision(&db, &repo_a, "advance", "For A");
    add_decision(&db, &repo_b, "skip", "For B");

    let output = decisions::list(&db, Some("org/repo-a"), 20, true).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), 1);
    assert_eq!(parsed[0]["reasoning"], "For A");
}

// ═══════════════════════════════════════════════
// CLI show tests
// ═══════════════════════════════════════════════

#[test]
fn cli_show_displays_decision_details() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");

    let id = db
        .decision_add(&NewClawDecision {
            repo_id: repo_id.clone(),
            spec_id: Some("spec-42".to_string()),
            decision_type: "hitl".to_string(),
            target_work_id: Some("issue:7".to_string()),
            reasoning: "Need human review".to_string(),
            context_json: Some(r#"{"key": "val"}"#.to_string()),
        })
        .unwrap();

    let output = decisions::show(&db, &id, false).unwrap();
    assert!(output.contains(&id));
    assert!(output.contains("hitl"));
    assert!(output.contains("spec-42"));
    assert!(output.contains("issue:7"));
    assert!(output.contains("Need human review"));
    assert!(output.contains(r#"{"key": "val"}"#));
}

#[test]
fn cli_show_json_returns_valid_json() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db, "org/repo-a");
    let id = add_decision(&db, &repo_id, "replan", "Replanning needed");

    let output = decisions::show(&db, &id, true).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["id"], id);
    assert_eq!(parsed["decision_type"], "replan");
}

#[test]
fn cli_show_nonexistent_returns_error() {
    let db = open_memory_db();
    let result = decisions::show(&db, "nonexistent-id", false);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("decision not found"));
}
