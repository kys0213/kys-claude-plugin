mod helpers;

use helpers::{cli_with_home, fixture_sql, setup_project};
use predicates::prelude::*;

/// Helper: index the project before querying.
fn index_project(tmp: &tempfile::TempDir, project: &std::path::Path) {
    cli_with_home(tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success();
}

// ============================================================
// B. query --perspective (built-in perspectives)
// ============================================================

// --- B1: tool-frequency returns valid JSON with expected keys ---
#[test]
fn b1_tool_frequency_returns_valid_json() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);
    index_project(&tmp, &project);

    let output = cli_with_home(&tmp)
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

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let arr = json.as_array().unwrap();
    assert!(!arr.is_empty());
    assert!(arr[0].get("tool").is_some());
    assert!(arr[0].get("frequency").is_some());
    assert!(arr[0].get("sessions").is_some());
}

// --- B2: tool-frequency --param top=3 limits results ---
#[test]
fn b2_tool_frequency_top_param() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);
    index_project(&tmp, &project);

    let output = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "tool-frequency",
            "--param",
            "top=3",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let arr = json.as_array().unwrap();
    assert!(arr.len() <= 3);
}

// --- B3: transitions with required param ---
#[test]
fn b3_transitions_with_param() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);
    index_project(&tmp, &project);

    let output = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "transitions",
            "--param",
            "tool=Edit",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let arr = json.as_array().unwrap();
    // May be empty if no transitions from Edit in fixture, but must be valid JSON
    for item in arr {
        assert!(item.get("to_tool").is_some());
        assert!(item.get("probability").is_some());
    }
}

// --- B4: transitions without required param fails ---
#[test]
fn b4_transitions_missing_required_param() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);
    index_project(&tmp, &project);

    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "transitions",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("missing required param"));
}

// --- B5: trends returns week_start key ---
#[test]
fn b5_trends_returns_valid_json() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);
    index_project(&tmp, &project);

    let output = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "trends",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let arr = json.as_array().unwrap();
    for item in arr {
        assert!(item.get("week_start").is_some());
        assert!(item.get("tool_name").is_some());
        assert!(item.get("count").is_some());
    }
}

// --- B6: trends --param since filters results ---
#[test]
fn b6_trends_since_param() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl", "bash_classified.jsonl"]);
    index_project(&tmp, &project);

    let output = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "trends",
            "--param",
            "since=2026-02-10",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let arr = json.as_array().unwrap();
    for item in arr {
        let week_start = item["week_start"].as_str().unwrap();
        assert!(week_start >= "2026-02-10");
    }
}

// --- B7: hotfiles returns file_path and edit_count ---
#[test]
fn b7_hotfiles_returns_valid_json() {
    let (tmp, project) = setup_project(&["file_edits.jsonl"]);
    index_project(&tmp, &project);

    let output = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "hotfiles",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let arr = json.as_array().unwrap();
    assert!(!arr.is_empty());
    assert!(arr[0].get("file_path").is_some());
    assert!(arr[0].get("edit_count").is_some());
}

// --- B8: unknown perspective fails with error message ---
#[test]
fn b8_unknown_perspective_fails() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "nonexistent",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown perspective"));
}

// --- B9: query without prior index fails ---
#[test]
fn b9_query_without_index_fails() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    // No index run — DB does not exist

    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "tool-frequency",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("index DB not found"));
}

// ============================================================
// C. query --list-perspectives
// ============================================================

// --- C1: list-perspectives shows all built-in names ---
#[test]
fn c1_list_perspectives_shows_all_builtins() {
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
    assert!(stderr.contains("tool-frequency"));
    assert!(stderr.contains("transitions"));
    assert!(stderr.contains("trends"));
    assert!(stderr.contains("hotfiles"));
    assert!(stderr.contains("repetition"));
    assert!(stderr.contains("prompts"));
    assert!(stderr.contains("session-links"));
    assert!(stderr.contains("sequences"));
    assert!(stderr.contains("sessions"));
}

// --- C2: list-perspectives shows parameter info ---
#[test]
fn c2_list_perspectives_shows_param_info() {
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
    // Check some specific param info
    assert!(stderr.contains("--param top"));
    assert!(stderr.contains("--param tool"));
    assert!(stderr.contains("(required)"));
    assert!(stderr.contains("[default:"));
}

// ============================================================
// D. query --sql-file (custom SQL)
// ============================================================

// --- D1: valid SELECT SQL file returns JSON ---
#[test]
fn d1_custom_sql_file_returns_json() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);
    index_project(&tmp, &project);

    let sql_file = fixture_sql("valid_select.sql");

    let output = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--sql-file",
            sql_file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(json.as_array().is_some());
    assert!(!json.as_array().unwrap().is_empty());
}

// --- D2: INSERT SQL file is rejected ---
#[test]
fn d2_sql_file_with_insert_rejected() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    let sql_file = fixture_sql("has_insert.sql");

    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--sql-file",
            sql_file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("only SELECT"));
}

// --- D3: nonexistent SQL file fails ---
#[test]
fn d3_nonexistent_sql_file_fails() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--sql-file",
            "/tmp/absolutely_nonexistent_file_98765.sql",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to read SQL file"));
}

// --- D4: empty SQL file fails ---
#[test]
fn d4_empty_sql_file_fails() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    // Create an empty SQL file in the temp dir
    let sql_file = tmp.path().join("empty.sql");
    std::fs::write(&sql_file, "").unwrap();

    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--sql-file",
            sql_file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("SQL file is empty"));
}

// --- D5: --sql-file with inline-written SELECT works ---
#[test]
fn d5_custom_inline_sql_returns_json() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl", "bash_classified.jsonl"]);
    index_project(&tmp, &project);

    let sql_file = tmp.path().join("custom.sql");
    std::fs::write(
        &sql_file,
        "SELECT tool_name, classified_name FROM tool_uses LIMIT 5",
    )
    .unwrap();

    let output = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--sql-file",
            sql_file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let arr = json.as_array().unwrap();
    assert!(arr.len() <= 5);
    assert!(arr[0].get("tool_name").is_some());
    assert!(arr[0].get("classified_name").is_some());
}
