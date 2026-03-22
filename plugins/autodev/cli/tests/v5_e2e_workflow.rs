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
// 1. autodev context (--field uses v5 MockDataSource)
// ═══════════════════════════════════════════════

#[test]
fn v5_context_outputs_json() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/v5-repo:42",
        "issue",
        "running",
        Some("Implement feature"),
        42,
    );

    autodev(&home)
        .args(["context", "issue:org/v5-repo:42", "--json"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("work_id")
                .and(predicate::str::contains("issue:org/v5-repo:42")),
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
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/v5-repo:1",
        "issue",
        "running",
        Some("Review task"),
        1,
    );

    let output = autodev(&home)
        .args(["context", "issue:org/v5-repo:1", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let ctx: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(ctx["queue"]["phase"], "running");
}

// ═══════════════════════════════════════════════
// 2. autodev queue done
// ═══════════════════════════════════════════════

#[test]
fn v5_queue_done_transitions_completed_to_done() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/v5-repo:42",
        "issue",
        "completed",
        Some("Implement feature"),
        42,
    );

    autodev(&home)
        .args(["queue", "done", "issue:org/v5-repo:42"])
        .assert()
        .success()
        .stdout(predicate::str::contains("done"));
}

// ═══════════════════════════════════════════════
// 3. autodev queue hitl
// ═══════════════════════════════════════════════

#[test]
fn v5_queue_hitl_transitions_completed_to_hitl() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/v5-repo:43",
        "issue",
        "completed",
        Some("Needs review"),
        43,
    );

    autodev(&home)
        .args([
            "queue",
            "hitl",
            "issue:org/v5-repo:43",
            "--reason",
            "needs human review",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("hitl"));
}

// ═══════════════════════════════════════════════
// 4. autodev queue retry-script
// ═══════════════════════════════════════════════

#[test]
fn v5_queue_retry_script_transitions_failed_to_completed() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/v5-repo:44",
        "issue",
        "failed",
        Some("Script failed"),
        44,
    );

    autodev(&home)
        .args(["queue", "retry-script", "issue:org/v5-repo:44"])
        .assert()
        .success()
        .stdout(predicate::str::contains("retry"));
}
