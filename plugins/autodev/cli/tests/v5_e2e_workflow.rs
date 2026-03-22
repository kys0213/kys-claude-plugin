//! v5 E2E 블랙박스 테스트: CLI 바이너리 호출 기반.
//!
//! autodev context / queue done / queue hitl / queue retry-script
//! 명령을 실제 바이너리로 호출하여 검증한다.

mod e2e_helpers;

use e2e_helpers::*;
use predicates::prelude::*;
use tempfile::TempDir;

const REPO_URL: &str = "https://github.com/org/v5-repo";

// ═══════════════════════════════════════════════
// 1. autodev context
// ═══════════════════════════════════════════════

#[test]
fn v5_context_outputs_json() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["context", "github:org/v5-repo#42:implement", "--json"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("work_id")
                .and(predicate::str::contains("github:org/v5-repo#42:implement")),
        );
}

#[test]
fn v5_context_field_extraction() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "context",
            "github:org/v5-repo#42:implement",
            "--field",
            "queue.state",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("implement").and(predicate::str::contains("work_id").not()),
        );
}

#[test]
fn v5_context_field_source_id() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "context",
            "github:org/v5-repo#42:analyze",
            "--field",
            "queue.source_id",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("github:org/v5-repo#42"));
}

#[test]
fn v5_context_nonexistent_field_fails() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "context",
            "github:org/v5-repo#42:analyze",
            "--field",
            "nonexistent.path",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("field not found"));
}

#[test]
fn v5_context_contains_source_type() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    let output = autodev(&home)
        .args(["context", "github:org/v5-repo#1:review", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let ctx: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(ctx["source"]["type"], "mock");
    assert_eq!(ctx["queue"]["state"], "review");
}

// ═══════════════════════════════════════════════
// 2. autodev queue done
// ═══════════════════════════════════════════════

#[test]
fn v5_queue_done_prints_message() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["queue", "done", "github:org/v5-repo#42:implement"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("queue done")
                .and(predicate::str::contains("github:org/v5-repo#42:implement")),
        );
}

// ═══════════════════════════════════════════════
// 3. autodev queue hitl
// ═══════════════════════════════════════════════

#[test]
fn v5_queue_hitl_prints_reason() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "queue",
            "hitl",
            "github:org/v5-repo#42:implement",
            "--reason",
            "needs human review",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("queue hitl")
                .and(predicate::str::contains("needs human review")),
        );
}

// ═══════════════════════════════════════════════
// 4. autodev queue retry-script
// ═══════════════════════════════════════════════

#[test]
fn v5_queue_retry_script_prints_message() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["queue", "retry-script", "github:org/v5-repo#42:implement"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("retry-script")
                .and(predicate::str::contains("github:org/v5-repo#42:implement")),
        );
}
