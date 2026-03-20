//! E2E tests for error handling:
//! Nonexistent resources, invalid state transitions, duplicate registrations, bad arguments

mod e2e_helpers;

use e2e_helpers::*;
use predicates::prelude::*;
use tempfile::TempDir;

const REPO_URL: &str = "https://github.com/org/error-repo";
const REPO_NAME: &str = "org/error-repo";

// ═══════════════════════════════════════════════
// 1. Nonexistent resources
// ═══════════════════════════════════════════════

#[test]
fn e2e_error_spec_show_nonexistent() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["spec", "show", "00000000-0000-0000-0000-000000000000"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("spec not found"));
}

#[test]
fn e2e_error_spec_update_nonexistent() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["spec", "update", "nonexistent", "--body", "new body"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("spec not found"));
}

#[test]
fn e2e_error_spec_pause_nonexistent() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["spec", "pause", "nonexistent"])
        .assert()
        .failure();
}

#[test]
fn e2e_error_hitl_show_nonexistent() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["hitl", "show", "nonexistent-event-id"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn e2e_error_hitl_respond_nonexistent() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["hitl", "respond", "nonexistent", "--choice", "1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn e2e_error_queue_show_nonexistent() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["queue", "show", "issue:org/error-repo:999"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn e2e_error_queue_advance_nonexistent() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["queue", "advance", "issue:org/error-repo:999"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn e2e_error_decisions_show_nonexistent() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["decisions", "show", "nonexistent-decision-id"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ═══════════════════════════════════════════════
// 2. Invalid state transitions
// ═══════════════════════════════════════════════

#[test]
fn e2e_error_queue_advance_from_done() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/error-repo:100",
        "issue",
        "done",
        None,
        100,
    );

    autodev(&home)
        .args(["queue", "advance", "issue:org/error-repo:100"])
        .assert()
        .failure();
}

#[test]
fn e2e_error_queue_advance_from_skipped() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/error-repo:101",
        "issue",
        "skipped",
        None,
        101,
    );

    autodev(&home)
        .args(["queue", "advance", "issue:org/error-repo:101"])
        .assert()
        .failure();
}

#[test]
fn e2e_error_spec_complete_when_paused() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Paused Error", "body");

    autodev(&home)
        .args(["spec", "link", &id, "--issue", "1"])
        .assert()
        .success();
    autodev(&home)
        .args(["spec", "pause", &id])
        .assert()
        .success();

    autodev(&home)
        .args(["spec", "complete", &id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not active"));
}

#[test]
fn e2e_error_spec_complete_no_issues() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "No Issues Error", "body");

    autodev(&home)
        .args(["spec", "complete", &id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no linked issues"));
}

#[test]
fn e2e_error_hitl_double_respond() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    let event_id = seed_hitl_event(
        &home,
        &repo_id,
        None,
        None,
        "medium",
        "Double respond error",
        &["Ok"],
    );

    autodev(&home)
        .args(["hitl", "respond", &event_id, "--choice", "1"])
        .assert()
        .success();

    autodev(&home)
        .args(["hitl", "respond", &event_id, "--choice", "1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already responded"));
}

// ═══════════════════════════════════════════════
// 3. Duplicate registrations
// ═══════════════════════════════════════════════

#[test]
fn e2e_error_repo_add_duplicate() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["repo", "add", REPO_URL])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already registered"));
}

// ═══════════════════════════════════════════════
// 4. Bad arguments / missing required args
// ═══════════════════════════════════════════════

#[test]
fn e2e_error_unknown_subcommand() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["nonexistent-command"])
        .assert()
        .failure();
}

#[test]
fn e2e_error_spec_add_missing_required() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // Missing --title
    autodev(&home)
        .args(["spec", "add", "--body", "text", "--repo", REPO_NAME])
        .assert()
        .failure();
}

#[test]
fn e2e_error_spec_add_repo_not_found() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "spec",
            "add",
            "--title",
            "Bad",
            "--body",
            "text",
            "--repo",
            "org/nonexistent",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("repository not found"));
}

#[test]
fn e2e_error_cron_add_both_interval_and_schedule() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "cron",
            "add",
            "--name",
            "bad",
            "--script",
            "/bin/echo",
            "--interval",
            "60",
            "--schedule",
            "* * * * *",
        ])
        .assert()
        .failure();
}

#[test]
fn e2e_error_cron_remove_nonexistent() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["cron", "remove", "ghost-job"])
        .assert()
        .failure();
}

#[test]
fn e2e_error_spec_link_nonexistent_spec() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["spec", "link", "nonexistent-spec-id", "--issue", "1"])
        .assert()
        .failure();
}

#[test]
fn e2e_error_spec_evaluate_nonexistent() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["spec", "evaluate", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("spec not found"));
}
