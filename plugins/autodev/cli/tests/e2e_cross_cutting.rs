//! E2E tests for cross-cutting concerns:
//! board, status, usage, logs, decisions, repo show

mod e2e_helpers;

use e2e_helpers::*;
use predicates::prelude::*;
use tempfile::TempDir;

const REPO_URL: &str = "https://github.com/org/cross-repo";
const REPO_NAME: &str = "org/cross-repo";

// ═══════════════════════════════════════════════
// 1. status
// ═══════════════════════════════════════════════

#[test]
fn e2e_status_no_daemon() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("stopped"));
}

#[test]
fn e2e_status_with_repo() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains(REPO_NAME));
}

// ═══════════════════════════════════════════════
// 2. board
// ═══════════════════════════════════════════════

#[test]
fn e2e_board_empty() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home).args(["board"]).assert().success();
}

#[test]
fn e2e_board_with_queue_items() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/cross-repo:1",
        "issue",
        "pending",
        Some("Board item"),
        1,
    );

    autodev(&home).args(["board"]).assert().success();
}

#[test]
fn e2e_board_filter_by_repo() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["board", "--repo", REPO_NAME])
        .assert()
        .success();
}

// ═══════════════════════════════════════════════
// 3. usage
// ═══════════════════════════════════════════════

#[test]
fn e2e_usage_empty() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home).args(["usage"]).assert().success();
}

#[test]
fn e2e_usage_with_repo_filter() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["usage", "--repo", REPO_NAME])
        .assert()
        .success();
}

#[test]
fn e2e_usage_with_since() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["usage", "--since", "2025-01-01"])
        .assert()
        .success();
}

// ═══════════════════════════════════════════════
// 4. logs
// ═══════════════════════════════════════════════

#[test]
fn e2e_logs_empty() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home).args(["logs"]).assert().success();
}

#[test]
fn e2e_logs_with_limit() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home).args(["logs", "-n", "5"]).assert().success();
}

// ═══════════════════════════════════════════════
// 5. decisions
// ═══════════════════════════════════════════════

#[test]
fn e2e_decisions_list_empty() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["decisions", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No decisions found"));
}

#[test]
fn e2e_decisions_list_after_advance() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/cross-repo:10",
        "issue",
        "pending",
        None,
        10,
    );

    autodev(&home)
        .args([
            "queue",
            "advance",
            "issue:org/cross-repo:10",
            "--reason",
            "test decision",
        ])
        .assert()
        .success();

    autodev(&home)
        .args(["decisions", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("advance"));
}

#[test]
fn e2e_decisions_list_with_limit() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["decisions", "list", "-n", "5"])
        .assert()
        .success();
}

// ═══════════════════════════════════════════════
// 6. repo management
// ═══════════════════════════════════════════════

#[test]
fn e2e_repo_list() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["repo", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(REPO_NAME));
}

#[test]
fn e2e_repo_show() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["repo", "show", REPO_NAME])
        .assert()
        .success()
        .stdout(predicate::str::contains(REPO_NAME));
}

#[test]
fn e2e_repo_remove() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["repo", "remove", REPO_NAME])
        .assert()
        .success();

    autodev(&home)
        .args(["repo", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(REPO_NAME).not());
}

// ═══════════════════════════════════════════════
// 7. config
// ═══════════════════════════════════════════════

#[test]
fn e2e_config_show() {
    let home = TempDir::new().unwrap();

    autodev(&home).args(["config", "show"]).assert().success();
}
