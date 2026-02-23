/// Edge cases: parameter validation, SQL security, boundary values, CLI arg errors.
mod helpers;

use helpers::{cli_with_home, fixture_sql, setup_project};
use predicates::prelude::*;

fn index_project(tmp: &tempfile::TempDir, project: &std::path::Path) {
    cli_with_home(tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success();
}

// ============================================================
// Parameter type validation
// ============================================================

/// --param top=abc → Integer parse error
#[test]
fn param_integer_type_error() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "tool-frequency",
            "--param",
            "top=abc",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("expected integer"));
}

/// --param z_threshold=not_a_number → Float parse error
#[test]
fn param_float_type_error() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "repetition",
            "--param",
            "z_threshold=not_a_number",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("expected float"));
}

/// --param since=2026/02/10 → Date format error
#[test]
fn param_date_format_error() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "trends",
            "--param",
            "since=2026/02/10",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("expected date YYYY-MM-DD"));
}

/// --param since=not-a-date → Date parse error
#[test]
fn param_date_invalid_string() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "trends",
            "--param",
            "since=yesterday",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("expected date YYYY-MM-DD"));
}

// ============================================================
// Boundary values
// ============================================================

/// --param top=0 → returns empty array
#[test]
fn param_top_zero_returns_empty() {
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
            "top=0",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let arr = json.as_array().unwrap();
    assert!(arr.is_empty(), "top=0 should return empty array");
}

/// --param top=-1 → SQLite LIMIT -1 means no limit (returns all)
#[test]
fn param_top_negative_returns_all() {
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
            "top=-1",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let arr = json.as_array().unwrap();
    // Should return all unique classified tools (no limit)
    assert!(arr.len() >= 2, "negative limit should return all rows");
}

/// Very large top value
#[test]
fn param_top_large_value() {
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
            "top=999999",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(json.as_array().is_some());
}

// ============================================================
// --param format errors
// ============================================================

/// --param "keyonly" (no = sign)
#[test]
fn param_missing_equals_sign() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "tool-frequency",
            "--param",
            "keyonly",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("expected key=value"));
}

/// --param "=value" (empty key)
#[test]
fn param_empty_key() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    // Empty key is technically parseable as key="" value="value"
    // but the perspective won't have a param named ""
    // The query will succeed since "" key is simply ignored (not a defined param)
    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "tool-frequency",
            "--param",
            "=value",
        ])
        .assert()
        .success();
}

// ============================================================
// CLI argument combinations
// ============================================================

/// query with neither --perspective nor --sql-file
#[test]
fn query_missing_perspective_and_sql_file() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    cli_with_home(&tmp)
        .args(["query", "--project", project.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--perspective or --sql-file required",
        ));
}

/// --perspective + --sql-file → sql-file takes priority
#[test]
fn sql_file_takes_priority_over_perspective() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);
    index_project(&tmp, &project);

    let sql_file = fixture_sql("valid_select.sql");

    let output = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "tool-frequency",
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
    // valid_select.sql: SELECT classified_name, COUNT(*) as cnt ...
    // Should have "classified_name" key (not "tool" which comes from perspective)
    assert!(
        arr[0].get("classified_name").is_some(),
        "sql-file should be executed, not perspective"
    );
    assert!(
        arr[0].get("tool").is_none(),
        "perspective output key 'tool' should not appear"
    );
}

// ============================================================
// SQL security edge cases
// ============================================================

/// DELETE statement blocked
#[test]
fn sql_delete_blocked() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    let sql_file = tmp.path().join("delete.sql");
    std::fs::write(&sql_file, "DELETE FROM sessions").unwrap();

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

/// UPDATE statement blocked
#[test]
fn sql_update_blocked() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    let sql_file = tmp.path().join("update.sql");
    std::fs::write(&sql_file, "UPDATE sessions SET prompt_count = 0").unwrap();

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

/// DROP TABLE blocked
#[test]
fn sql_drop_blocked() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    let sql_file = tmp.path().join("drop.sql");
    std::fs::write(&sql_file, "DROP TABLE sessions").unwrap();

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

/// CREATE TABLE blocked
#[test]
fn sql_create_blocked() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    let sql_file = tmp.path().join("create.sql");
    std::fs::write(&sql_file, "CREATE TABLE evil (id INTEGER)").unwrap();

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

/// Lowercase "select" should be accepted (case-insensitive check)
#[test]
fn sql_lowercase_select_accepted() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);
    index_project(&tmp, &project);

    let sql_file = tmp.path().join("lower.sql");
    std::fs::write(&sql_file, "select count(*) as total from sessions").unwrap();

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
}

/// SQL with leading whitespace + SELECT should work
#[test]
fn sql_leading_whitespace_accepted() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    let sql_file = tmp.path().join("whitespace.sql");
    std::fs::write(&sql_file, "  \n  SELECT count(*) as total FROM sessions").unwrap();

    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--sql-file",
            sql_file.to_str().unwrap(),
        ])
        .assert()
        .success();
}

/// SQL with CTE (WITH ... SELECT) — blocked by starts_with("SELECT") check
#[test]
fn sql_cte_with_clause_blocked() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);
    index_project(&tmp, &project);

    let sql_file = tmp.path().join("cte.sql");
    std::fs::write(
        &sql_file,
        "WITH counts AS (SELECT count(*) as c FROM sessions) SELECT c FROM counts",
    )
    .unwrap();

    // CTE starts with "WITH", not "SELECT", so it will be blocked
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

/// Verify that DB data is intact after a rejected DML attempt
#[test]
fn sql_injection_does_not_corrupt_db() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);
    index_project(&tmp, &project);

    // Attempt injection
    let sql_file = tmp.path().join("inject.sql");
    std::fs::write(&sql_file, "DELETE FROM sessions; SELECT 1").unwrap();
    let _ = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--sql-file",
            sql_file.to_str().unwrap(),
        ])
        .assert()
        .failure();

    // Verify data is still intact
    let output = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "sessions",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(
        arr.len(),
        1,
        "session data should be intact after failed injection"
    );
}

// ============================================================
// Unknown/extra params are silently ignored
// ============================================================

#[test]
fn extra_params_ignored() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);
    index_project(&tmp, &project);

    // Pass an unknown param — should be silently ignored
    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "tool-frequency",
            "--param",
            "top=5",
            "--param",
            "unknown_param=xyz",
        ])
        .assert()
        .success();
}
