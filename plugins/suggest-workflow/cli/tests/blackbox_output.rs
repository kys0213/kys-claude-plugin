/// F1~F3: Output format and exit code verification.
/// F1: query stdout is valid JSON array.
/// F2: stderr contains human-readable messages, stdout is clean JSON.
/// F3: success exit code = 0, user errors = non-zero.
mod helpers;

use helpers::{cli_with_home, setup_project};

// ============================================================
// F1: stdout is valid JSON
// ============================================================

#[test]
fn f1_query_stdout_is_valid_json_array() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);

    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success();

    let output = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "tool-frequency",
            "--param",
            "top=5",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value =
        serde_json::from_slice(&output).expect("stdout should be valid JSON");
    assert!(json.is_array(), "query output should be a JSON array");
}

#[test]
fn f1_all_perspectives_return_valid_json() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl", "bash_classified.jsonl"]);

    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success();

    let perspectives_with_params = [
        ("tool-frequency", vec!["top=5"]),
        ("transitions", vec!["tool=Read"]),
        ("trends", vec!["since=2020-01-01"]),
        ("hotfiles", vec!["top=5"]),
        ("repetition", vec!["z_threshold=0.5"]),
        ("prompts", vec!["search=refactor", "top=5"]),
        ("session-links", vec!["min_overlap=0.0"]),
        ("sequences", vec!["min_count=1"]),
        ("sessions", vec!["top=10"]),
    ];

    for (perspective, params) in &perspectives_with_params {
        let mut args = vec![
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            perspective,
        ];
        for p in params {
            args.push("--param");
            args.push(p);
        }

        let output = cli_with_home(&tmp)
            .args(&args)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let json: serde_json::Value = serde_json::from_slice(&output).unwrap_or_else(|e| {
            panic!("perspective '{}' stdout not valid JSON: {}", perspective, e)
        });
        assert!(
            json.is_array(),
            "perspective '{}' should return JSON array, got: {:?}",
            perspective,
            json
        );
    }
}

#[test]
fn f1_sql_file_returns_valid_json() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);

    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success();

    let sql_path = helpers::fixture_sql("valid_select.sql");
    let output = cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--sql-file",
            sql_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value =
        serde_json::from_slice(&output).expect("--sql-file stdout should be valid JSON");
    assert!(json.is_array(), "--sql-file output should be a JSON array");
}

// ============================================================
// F2: stderr is human-readable, stdout is clean
// ============================================================

#[test]
fn f2_index_writes_summary_to_stderr() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);

    let output = cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .clone();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("new") || stderr.contains("Indexed"),
        "index stderr should contain human-readable summary, got: {}",
        stderr
    );

    // stdout should be empty (index doesn't output data)
    assert!(output.stdout.is_empty(), "index stdout should be empty");
}

#[test]
fn f2_query_stdout_has_no_log_messages() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);

    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success();

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
        .clone();

    let stdout = String::from_utf8_lossy(&output.stdout);

    // stdout should only contain JSON (no log messages like "DB:", "Indexed:", etc.)
    let trimmed = stdout.trim();
    assert!(
        trimmed.starts_with('['),
        "query stdout should start with '[' (JSON array), got: {}",
        &trimmed[..trimmed.len().min(100)]
    );
}

// ============================================================
// F3: Exit codes
// ============================================================

#[test]
fn f3_success_returns_zero() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);

    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .code(0);

    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "tool-frequency",
        ])
        .assert()
        .code(0);
}

#[test]
fn f3_missing_required_param_returns_nonzero() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);

    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success();

    // transitions requires --param tool=<value>
    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "transitions",
        ])
        .assert()
        .failure();
}

#[test]
fn f3_unknown_perspective_returns_nonzero() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);

    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success();

    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "nonexistent",
        ])
        .assert()
        .failure();
}

#[test]
fn f3_no_index_db_returns_nonzero() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);

    // Query without indexing first
    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "tool-frequency",
        ])
        .assert()
        .failure();
}

#[test]
fn f3_invalid_sql_file_returns_nonzero() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);

    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success();

    let sql_path = helpers::fixture_sql("has_insert.sql");
    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--sql-file",
            sql_path.to_str().unwrap(),
        ])
        .assert()
        .failure();
}
