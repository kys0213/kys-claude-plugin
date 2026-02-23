mod helpers;

use helpers::{cli_with_home, setup_project};
use predicates::prelude::*;

// --- A1: First indexing creates DB and reports new sessions ---
#[test]
fn a1_first_index_creates_db() {
    let (tmp, project) =
        setup_project(&["minimal.jsonl", "multi_tool.jsonl", "bash_classified.jsonl"]);

    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("3 new"));
}

// --- A2: Incremental indexing skips unchanged sessions ---
#[test]
fn a2_incremental_skips_unchanged() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);

    // First indexing
    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success();

    // Second run — no changes
    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("0 new"))
        .stderr(predicate::str::contains("1 unchanged"));
}

// --- A3: Incremental detects modified session file ---
#[test]
fn a3_incremental_detects_update() {
    let (tmp, project) = setup_project(&["minimal.jsonl", "multi_tool.jsonl"]);

    // First indexing
    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success();

    // Modify one fixture (append a line to change size/mtime)
    let canonical = project.canonicalize().unwrap();
    let normalized = canonical
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string();
    let encoded = format!("-{}", &normalized[1..].replace('/', "-"));
    let sessions_dir = tmp.path().join(".claude").join("projects").join(&encoded);

    let target = sessions_dir.join("minimal.jsonl");
    let mut content = std::fs::read_to_string(&target).unwrap();
    content.push_str("{\"type\":\"user\",\"message\":{\"content\":\"extra prompt\"},\"timestamp\":\"2026-02-14T12:00:00+00:00\"}\n");
    // Sleep briefly to ensure mtime changes
    std::thread::sleep(std::time::Duration::from_millis(50));
    std::fs::write(&target, content).unwrap();

    // Re-index
    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("1 updated"))
        .stderr(predicate::str::contains("1 unchanged"));
}

// --- A4: Incremental detects new session file ---
#[test]
fn a4_incremental_detects_addition() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);

    // First indexing
    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success();

    // Add a new fixture
    let canonical = project.canonicalize().unwrap();
    let normalized = canonical
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string();
    let encoded = format!("-{}", &normalized[1..].replace('/', "-"));
    let sessions_dir = tmp.path().join(".claude").join("projects").join(&encoded);

    let src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sessions/multi_tool.jsonl");
    std::fs::copy(&src, sessions_dir.join("multi_tool.jsonl")).unwrap();

    // Re-index
    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("1 new"))
        .stderr(predicate::str::contains("1 unchanged"));
}

// --- A5: Incremental detects deleted session file ---
#[test]
fn a5_incremental_detects_deletion() {
    let (tmp, project) = setup_project(&["minimal.jsonl", "multi_tool.jsonl"]);

    // First indexing
    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success();

    // Delete one fixture
    let canonical = project.canonicalize().unwrap();
    let normalized = canonical
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string();
    let encoded = format!("-{}", &normalized[1..].replace('/', "-"));
    let sessions_dir = tmp.path().join(".claude").join("projects").join(&encoded);

    std::fs::remove_file(sessions_dir.join("minimal.jsonl")).unwrap();

    // Re-index
    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("1 deleted"))
        .stderr(predicate::str::contains("1 unchanged"));
}

// --- A6: --full forces complete rebuild ---
#[test]
fn a6_full_rebuild() {
    let (tmp, project) =
        setup_project(&["minimal.jsonl", "multi_tool.jsonl", "bash_classified.jsonl"]);

    // First indexing
    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("3 new"));

    // Full rebuild — all should be new again
    cli_with_home(&tmp)
        .args(["index", "--full", "--project", project.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("3 new"));
}

// --- A7: Empty project (no session files) ---
#[test]
fn a7_empty_project_no_sessions() {
    let (tmp, project) = setup_project(&[]);

    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("0 new"));
}

// --- A8: Malformed JSONL is handled gracefully ---
#[test]
fn a8_malformed_jsonl_handled() {
    let (tmp, project) = setup_project(&["malformed.jsonl"]);

    // Should succeed (skip bad lines, index what's parseable)
    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success();
}
