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
    db.workspace_add("https://github.com/org/test-repo", "org/test-repo")
        .expect("add repo")
}

fn add_test_spec(db: &Database, repo_id: &str) -> String {
    let spec = NewSpec {
        repo_id: repo_id.to_string(),
        title: "Test Spec".to_string(),
        body: "Test body content".to_string(),
        source_path: None,
        test_commands: None,
        acceptance_criteria: None,
    };
    db.spec_add(&spec).expect("add spec")
}

// ═══════════════════════════════════════════════
// 1. Spec CRUD
// ═══════════════════════════════════════════════

#[test]
fn spec_add_returns_id() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    let spec_id = add_test_spec(&db, &repo_id);
    assert!(!spec_id.is_empty());
}

#[test]
fn spec_show_returns_created_spec() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    let spec_id = add_test_spec(&db, &repo_id);

    let spec = db.spec_show(&spec_id).unwrap().unwrap();
    assert_eq!(spec.id, spec_id);
    assert_eq!(spec.title, "Test Spec");
    assert_eq!(spec.body, "Test body content");
    assert_eq!(spec.status, SpecStatus::Active);
    assert!(spec.source_path.is_none());
    assert!(spec.test_commands.is_none());
    assert!(spec.acceptance_criteria.is_none());
}

#[test]
fn spec_show_nonexistent_returns_none() {
    let db = open_memory_db();
    let result = db.spec_show("nonexistent-id").unwrap();
    assert!(result.is_none());
}

#[test]
fn spec_add_with_all_fields() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);

    let spec = NewSpec {
        repo_id: repo_id.clone(),
        title: "Full Spec".to_string(),
        body: "Full body".to_string(),
        source_path: Some("docs/spec.md".to_string()),
        test_commands: Some(r#"["cargo test", "cargo clippy"]"#.to_string()),
        acceptance_criteria: Some("- [ ] All tests pass\n- [ ] No warnings".to_string()),
    };
    let id = db.spec_add(&spec).unwrap();

    let loaded = db.spec_show(&id).unwrap().unwrap();
    assert_eq!(loaded.source_path.as_deref(), Some("docs/spec.md"));
    assert_eq!(
        loaded.test_commands.as_deref(),
        Some(r#"["cargo test", "cargo clippy"]"#)
    );
    assert!(loaded
        .acceptance_criteria
        .unwrap()
        .contains("All tests pass"));
}

#[test]
fn spec_list_empty() {
    let db = open_memory_db();
    let specs = db.spec_list(None).unwrap();
    assert!(specs.is_empty());
}

#[test]
fn spec_list_returns_all() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    add_test_spec(&db, &repo_id);
    add_test_spec(&db, &repo_id);

    let specs = db.spec_list(None).unwrap();
    assert_eq!(specs.len(), 2);
}

#[test]
fn spec_list_filters_by_repo() {
    let db = open_memory_db();
    let repo_id1 = db
        .workspace_add("https://github.com/a/one", "a/one")
        .unwrap();
    let repo_id2 = db
        .workspace_add("https://github.com/b/two", "b/two")
        .unwrap();

    add_test_spec(&db, &repo_id1);
    add_test_spec(&db, &repo_id2);
    add_test_spec(&db, &repo_id2);

    let all = db.spec_list(None).unwrap();
    assert_eq!(all.len(), 3);

    let repo1_specs = db.spec_list(Some("a/one")).unwrap();
    assert_eq!(repo1_specs.len(), 1);

    let repo2_specs = db.spec_list(Some("b/two")).unwrap();
    assert_eq!(repo2_specs.len(), 2);
}

// ═══════════════════════════════════════════════
// 2. Spec Update
// ═══════════════════════════════════════════════

#[test]
fn spec_update_changes_fields() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    let spec_id = add_test_spec(&db, &repo_id);

    db.spec_update(
        &spec_id,
        "Updated body",
        Some(r#"["npm test"]"#),
        Some("- [ ] Done"),
    )
    .unwrap();

    let spec = db.spec_show(&spec_id).unwrap().unwrap();
    assert_eq!(spec.body, "Updated body");
    assert_eq!(spec.test_commands.as_deref(), Some(r#"["npm test"]"#));
    assert_eq!(spec.acceptance_criteria.as_deref(), Some("- [ ] Done"));
}

#[test]
fn spec_update_nonexistent_returns_error() {
    let db = open_memory_db();
    let result = db.spec_update("nonexistent", "body", None, None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("spec not found"));
}

// ═══════════════════════════════════════════════
// 3. Spec Status
// ═══════════════════════════════════════════════

#[test]
fn spec_set_status_paused() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    let spec_id = add_test_spec(&db, &repo_id);

    db.spec_set_status(&spec_id, SpecStatus::Paused).unwrap();

    let spec = db.spec_show(&spec_id).unwrap().unwrap();
    assert_eq!(spec.status, SpecStatus::Paused);
}

#[test]
fn spec_set_status_completed() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    let spec_id = add_test_spec(&db, &repo_id);

    db.spec_set_status(&spec_id, SpecStatus::Completed).unwrap();

    let spec = db.spec_show(&spec_id).unwrap().unwrap();
    assert_eq!(spec.status, SpecStatus::Completed);
}

#[test]
fn spec_set_status_archived() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    let spec_id = add_test_spec(&db, &repo_id);

    db.spec_set_status(&spec_id, SpecStatus::Archived).unwrap();

    let spec = db.spec_show(&spec_id).unwrap().unwrap();
    assert_eq!(spec.status, SpecStatus::Archived);
}

#[test]
fn spec_set_status_nonexistent_returns_error() {
    let db = open_memory_db();
    let result = db.spec_set_status("nonexistent", SpecStatus::Paused);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("spec not found"));
}

// ═══════════════════════════════════════════════
// 4. Spec Issues (link/unlink)
// ═══════════════════════════════════════════════

#[test]
fn spec_link_issue_and_list() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    let spec_id = add_test_spec(&db, &repo_id);

    db.spec_link_issue(&spec_id, 42).unwrap();
    db.spec_link_issue(&spec_id, 99).unwrap();

    let issues = db.spec_issues(&spec_id).unwrap();
    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].issue_number, 42);
    assert_eq!(issues[1].issue_number, 99);
}

#[test]
fn spec_issues_empty() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    let spec_id = add_test_spec(&db, &repo_id);

    let issues = db.spec_issues(&spec_id).unwrap();
    assert!(issues.is_empty());
}

#[test]
fn spec_unlink_issue() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    let spec_id = add_test_spec(&db, &repo_id);

    db.spec_link_issue(&spec_id, 42).unwrap();
    db.spec_link_issue(&spec_id, 99).unwrap();

    db.spec_unlink_issue(&spec_id, 42).unwrap();

    let issues = db.spec_issues(&spec_id).unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].issue_number, 99);
}

#[test]
fn spec_unlink_nonexistent_returns_error() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    let spec_id = add_test_spec(&db, &repo_id);

    let result = db.spec_unlink_issue(&spec_id, 999);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("issue link not found"));
}

#[test]
fn spec_link_duplicate_issue_fails() {
    let db = open_memory_db();
    let repo_id = add_test_repo(&db);
    let spec_id = add_test_spec(&db, &repo_id);

    db.spec_link_issue(&spec_id, 42).unwrap();
    let result = db.spec_link_issue(&spec_id, 42);
    assert!(result.is_err());
}

// ═══════════════════════════════════════════════
// 5. SpecStatus model
// ═══════════════════════════════════════════════

#[test]
fn spec_status_roundtrip() {
    for status in &[
        SpecStatus::Active,
        SpecStatus::Paused,
        SpecStatus::Completed,
        SpecStatus::Archived,
    ] {
        let s = status.as_str();
        let parsed: SpecStatus = s.parse().unwrap();
        assert_eq!(&parsed, status);
    }
}

#[test]
fn spec_status_from_str_invalid() {
    assert!("invalid".parse::<SpecStatus>().is_err());
}

#[test]
fn spec_status_display() {
    assert_eq!(SpecStatus::Active.to_string(), "active");
    assert_eq!(SpecStatus::Paused.to_string(), "paused");
    assert_eq!(SpecStatus::Completed.to_string(), "completed");
    assert_eq!(SpecStatus::Archived.to_string(), "archived");
}
