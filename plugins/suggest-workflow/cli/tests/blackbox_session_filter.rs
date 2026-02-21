mod helpers;

use helpers::{cli_with_home, setup_project};
use predicates::prelude::*;

/// Helper: index the project before querying.
fn index_project(tmp: &tempfile::TempDir, project: &std::path::Path) {
    cli_with_home(tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success();
}

// ============================================================
// E. Session filter tests
// ============================================================

// --- E1: filtered-sessions perspective finds sessions by first prompt pattern ---
#[test]
fn e1_filtered_sessions_by_prompt_pattern() {
    let (tmp, project) = setup_project(&[
        "minimal.jsonl",
        "multi_tool.jsonl",
        "autodev_session.jsonl",
    ]);
    index_project(&tmp, &project);

    let output = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "filtered-sessions",
            "--param",
            "prompt_pattern=[autodev]",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "should find exactly one autodev session");
    assert!(arr[0].get("first_prompt").is_some());
    let first_prompt = arr[0]["first_prompt"].as_str().unwrap();
    assert!(first_prompt.contains("[autodev]"));
}

// --- E2: filtered-sessions returns empty for non-matching pattern ---
#[test]
fn e2_filtered_sessions_no_match() {
    let (tmp, project) = setup_project(&["minimal.jsonl", "multi_tool.jsonl"]);
    index_project(&tmp, &project);

    let output = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "filtered-sessions",
            "--param",
            "prompt_pattern=[autodev]",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let arr = json.as_array().unwrap();
    assert!(arr.is_empty(), "should find no autodev sessions");
}

// --- E3: --session-filter on tool-frequency limits to filtered sessions ---
#[test]
fn e3_session_filter_on_tool_frequency() {
    let (tmp, project) = setup_project(&[
        "minimal.jsonl",
        "multi_tool.jsonl",
        "autodev_session.jsonl",
    ]);
    index_project(&tmp, &project);

    // Query tool-frequency for all sessions
    let output_all = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "tool-frequency",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    // Query tool-frequency filtered to autodev sessions only
    let output_filtered = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "tool-frequency",
            "--session-filter",
            "first_prompt_snippet LIKE '[autodev]%'",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let all: serde_json::Value = serde_json::from_slice(&output_all).unwrap();
    let filtered: serde_json::Value = serde_json::from_slice(&output_filtered).unwrap();

    let all_arr = all.as_array().unwrap();
    let filtered_arr = filtered.as_array().unwrap();

    // Filtered results should have fewer or equal entries
    assert!(filtered_arr.len() <= all_arr.len());
    // Filtered should have at least some results (autodev session has tools)
    assert!(!filtered_arr.is_empty());
}

// --- E4: --session-filter on sessions perspective ---
#[test]
fn e4_session_filter_on_sessions() {
    let (tmp, project) = setup_project(&[
        "minimal.jsonl",
        "multi_tool.jsonl",
        "autodev_session.jsonl",
    ]);
    index_project(&tmp, &project);

    let output = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "sessions",
            "--session-filter",
            "first_prompt_snippet LIKE '[autodev]%'",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "should return only the autodev session");
}

// --- E5: --session-filter with invalid SQL is rejected ---
#[test]
fn e5_session_filter_rejects_dangerous_sql() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    // Semicolon
    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "tool-frequency",
            "--session-filter",
            "1=1; DROP TABLE sessions",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must not contain semicolons"));

    // DDL keyword
    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "tool-frequency",
            "--session-filter",
            "1=1 DROP TABLE sessions",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must not contain DDL"));

    // {SF:} placeholder injection (would cause infinite loop)
    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "tool-frequency",
            "--session-filter",
            "first_prompt_snippet LIKE '{SF:id}'",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must not contain {SF:}"));
}

// --- E6: --session-filter without {SF} markers is silently ignored ---
#[test]
fn e6_session_filter_on_derived_perspective() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl", "autodev_session.jsonl"]);
    index_project(&tmp, &project);

    // transitions is a derived-table perspective without {SF} markers
    // --session-filter should be accepted but have no effect
    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "transitions",
            "--param",
            "tool=Edit",
            "--session-filter",
            "first_prompt_snippet LIKE '[autodev]%'",
        ])
        .assert()
        .success();
}

// --- E7: first_prompt_snippet is populated after indexing ---
#[test]
fn e7_first_prompt_snippet_populated() {
    let (tmp, project) = setup_project(&["autodev_session.jsonl"]);
    index_project(&tmp, &project);

    let output = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "filtered-sessions",
            "--param",
            "prompt_pattern=[autodev]",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);

    let first_prompt = arr[0]["first_prompt"].as_str().unwrap();
    assert!(first_prompt.starts_with("[autodev] fix:"));
}

// --- E8: list-perspectives includes filtered-sessions ---
#[test]
fn e8_list_perspectives_includes_filtered_sessions() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    let output = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--list-perspectives",
        ])
        .assert()
        .success()
        .get_output()
        .stderr
        .clone();

    let stderr = String::from_utf8_lossy(&output);
    assert!(stderr.contains("filtered-sessions"));
    assert!(stderr.contains("prompt_pattern"));
}
