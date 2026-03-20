//! E2E tests for --json output consistency:
//! Every command that supports --json should produce valid, parseable JSON
//! with expected fields.

mod e2e_helpers;

use e2e_helpers::*;
use tempfile::TempDir;

const REPO_URL: &str = "https://github.com/org/json-repo";
const REPO_NAME: &str = "org/json-repo";

// ═══════════════════════════════════════════════
// 1. spec list --json
// ═══════════════════════════════════════════════

#[test]
fn e2e_json_spec_list_empty() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    let json = run_json(&home, &["spec", "list", "--json"]);
    assert!(json.is_array(), "spec list --json should be an array");
    assert_eq!(json.as_array().unwrap().len(), 0);
}

#[test]
fn e2e_json_spec_list_has_required_fields() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    create_spec(&home, REPO_NAME, "JSON Spec", "body text");

    let json = run_json(&home, &["spec", "list", "--json"]);
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);

    let spec = &arr[0];
    assert!(spec["id"].is_string());
    assert_eq!(spec["title"].as_str().unwrap(), "JSON Spec");
    assert!(spec["status"].is_string());
    assert!(spec["repo_id"].is_string());
    assert!(spec["created_at"].is_string());
    assert!(spec["updated_at"].is_string());
}

// ═══════════════════════════════════════════════
// 2. spec show --json
// ═══════════════════════════════════════════════

#[test]
fn e2e_json_spec_show() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Show JSON", "json body");

    let json = run_json(&home, &["spec", "show", &id, "--json"]);
    assert_eq!(json["id"].as_str().unwrap(), id);
    assert_eq!(json["title"].as_str().unwrap(), "Show JSON");
    assert_eq!(json["body"].as_str().unwrap(), "json body");
    assert_eq!(json["status"].as_str().unwrap(), "active");
}

// ═══════════════════════════════════════════════
// 3. spec status --json
// ═══════════════════════════════════════════════

#[test]
fn e2e_json_spec_status() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Status JSON", "body");
    autodev(&home)
        .args(["spec", "link", &id, "--issue", "5"])
        .assert()
        .success();

    let json = run_json(&home, &["spec", "status", &id, "--json"]);
    assert_eq!(json["id"].as_str().unwrap(), id);
    assert!(json["status"].is_string());
    assert!(json["issues"]["total"].is_number());
    assert!(json["issues"]["done"].is_number());
    assert!(json["hitl"]["total"].is_number());
    assert!(json["hitl"]["pending"].is_number());
    assert!(json["decisions"].is_number());
}

// ═══════════════════════════════════════════════
// 4. spec decisions --json
// ═══════════════════════════════════════════════

#[test]
fn e2e_json_spec_decisions_empty() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Dec JSON", "body");

    let json = run_json(&home, &["spec", "decisions", &id, "--json"]);
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 0);
}

// ═══════════════════════════════════════════════
// 5. queue list --json
// ═══════════════════════════════════════════════

#[test]
fn e2e_json_queue_list_empty() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    let json = run_json(&home, &["queue", "list", "--json"]);
    assert!(json.is_array());
}

#[test]
fn e2e_json_queue_list_has_required_fields() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/json-repo:1",
        "issue",
        "pending",
        Some("JSON queue item"),
        1,
    );

    let json = run_json(&home, &["queue", "list", "--json"]);
    let arr = json.as_array().unwrap();
    assert!(!arr.is_empty());

    let item = &arr[0];
    assert!(item["work_id"].is_string());
    assert!(item["repo_id"].is_string());
    assert!(item["queue_type"].is_string());
    assert!(item["phase"].is_string());
}

// ═══════════════════════════════════════════════
// 6. queue show --json
// ═══════════════════════════════════════════════

#[test]
fn e2e_json_queue_show() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/json-repo:2",
        "issue",
        "ready",
        None,
        2,
    );

    let json = run_json(&home, &["queue", "show", "issue:org/json-repo:2", "--json"]);
    assert_eq!(json["work_id"].as_str().unwrap(), "issue:org/json-repo:2");
    assert_eq!(json["phase"].as_str().unwrap(), "ready");
}

// ═══════════════════════════════════════════════
// 7. hitl list --json
// ═══════════════════════════════════════════════

#[test]
fn e2e_json_hitl_list_empty() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    let json = run_json(&home, &["hitl", "list", "--json"]);
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 0);
}

#[test]
fn e2e_json_hitl_list_has_required_fields() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_hitl_event(
        &home,
        &repo_id,
        None,
        None,
        "medium",
        "JSON HITL event",
        &["Yes", "No"],
    );

    let json = run_json(&home, &["hitl", "list", "--json"]);
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);

    let event = &arr[0];
    assert!(event["id"].is_string());
    assert!(event["repo_id"].is_string());
    assert!(event["severity"].is_string());
    assert!(event["status"].is_string());
    assert!(event["situation"].is_string());
    assert!(event["created_at"].is_string());
}

// ═══════════════════════════════════════════════
// 8. hitl show --json
// ═══════════════════════════════════════════════

#[test]
fn e2e_json_hitl_show() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    let event_id = seed_hitl_event(
        &home,
        &repo_id,
        None,
        None,
        "high",
        "Show JSON event",
        &["Approve", "Deny"],
    );

    let json = run_json(&home, &["hitl", "show", &event_id, "--json"]);
    assert!(json["event"].is_object());
    assert!(json["responses"].is_array());
    assert_eq!(json["event"]["id"].as_str().unwrap(), event_id);
    assert_eq!(json["event"]["severity"].as_str().unwrap(), "High");
}

// ═══════════════════════════════════════════════
// 9. cron list --json
// ═══════════════════════════════════════════════

#[test]
fn e2e_json_cron_list() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    let json = run_json(&home, &["cron", "list", "--json"]);
    assert!(json.is_array());
    // Should have built-in crons from repo add
    let arr = json.as_array().unwrap();
    assert!(
        arr.iter()
            .any(|j| j["name"].as_str() == Some("claw-evaluate")),
        "should contain claw-evaluate"
    );
}

#[test]
fn e2e_json_cron_list_fields() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    let json = run_json(&home, &["cron", "list", "--json"]);
    let arr = json.as_array().unwrap();
    if let Some(job) = arr.first() {
        assert!(job["id"].is_string());
        assert!(job["name"].is_string());
        assert!(job["status"].is_string());
        assert!(job["script_path"].is_string());
    }
}

// ═══════════════════════════════════════════════
// 10. decisions list --json
// ═══════════════════════════════════════════════

#[test]
fn e2e_json_decisions_list_empty() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    let json = run_json(&home, &["decisions", "list", "--json"]);
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 0);
}

// ═══════════════════════════════════════════════
// 11. board --json
// ═══════════════════════════════════════════════

#[test]
fn e2e_json_board() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    let json = run_json(&home, &["board", "--json"]);
    // Board JSON should be an object or array
    assert!(json.is_object() || json.is_array());
}

// ═══════════════════════════════════════════════
// 12. status --json
// ═══════════════════════════════════════════════

#[test]
fn e2e_json_status() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    let json = run_json(&home, &["status", "--json"]);
    assert!(json.is_object());
    // Should have daemon and repos info
    assert!(json["daemon"].is_string() || json["daemon"].is_object());
}
