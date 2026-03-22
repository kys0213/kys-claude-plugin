//! E2E tests for `autodev claw` subcommands:
//! init, rules, edit

mod e2e_helpers;

use e2e_helpers::*;
use predicates::prelude::*;
use tempfile::TempDir;

const REPO_URL: &str = "https://github.com/org/claw-repo";
const REPO_NAME: &str = "org/claw-repo";

// ═══════════════════════════════════════════════
// 1. claw init
// ═══════════════════════════════════════════════

#[test]
fn e2e_claw_init_creates_workspace() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["claw", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Claw workspace initialized"));
}

#[test]
fn e2e_claw_init_idempotent() {
    let home = TempDir::new().unwrap();

    // First init
    autodev(&home).args(["claw", "init"]).assert().success();

    // Second init should also succeed (idempotent)
    autodev(&home)
        .args(["claw", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Claw workspace initialized"));
}

#[test]
fn e2e_claw_init_with_repo() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["claw", "init", "--repo", REPO_NAME])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Per-repo claw override initialized",
        ));
}

// ═══════════════════════════════════════════════
// 2. claw rules
// ═══════════════════════════════════════════════

#[test]
fn e2e_claw_rules_without_init_fails() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["claw", "rules"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not initialized"));
}

#[test]
fn e2e_claw_rules_after_init_lists_rules() {
    let home = TempDir::new().unwrap();

    // Initialize workspace first
    autodev(&home).args(["claw", "init"]).assert().success();

    // List rules should show global rules
    autodev(&home)
        .args(["claw", "rules"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[global]"));
}

#[test]
fn e2e_claw_rules_json_output() {
    let home = TempDir::new().unwrap();

    // Initialize workspace first
    autodev(&home).args(["claw", "init"]).assert().success();

    // JSON output should be a valid JSON array
    let json = run_json(&home, &["claw", "rules", "--json"]);
    assert!(json.is_array(), "claw rules --json should be an array");
    let arr = json.as_array().unwrap();
    assert!(
        !arr.is_empty(),
        "claw rules should return at least one rule after init"
    );
}

#[test]
fn e2e_claw_rules_with_repo_without_repo_init_fails() {
    let home = TempDir::new().unwrap();

    // Initialize global workspace
    autodev(&home).args(["claw", "init"]).assert().success();

    // Listing rules with --repo without initializing per-repo should fail
    autodev(&home)
        .args(["claw", "rules", "--repo", REPO_NAME])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not initialized"));
}

#[test]
fn e2e_claw_rules_with_repo_after_repo_init() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // Initialize global + per-repo
    autodev(&home)
        .args(["claw", "init", "--repo", REPO_NAME])
        .assert()
        .success();

    // Listing rules with --repo should succeed
    autodev(&home)
        .args(["claw", "rules", "--repo", REPO_NAME])
        .assert()
        .success()
        .stdout(predicate::str::contains("[global]"));
}

// ═══════════════════════════════════════════════
// 3. claw edit
// ═══════════════════════════════════════════════

#[test]
fn e2e_claw_edit_without_init_fails() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["claw", "edit", "scheduling"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not initialized"));
}

#[test]
fn e2e_claw_edit_nonexistent_rule_fails() {
    let home = TempDir::new().unwrap();

    autodev(&home).args(["claw", "init"]).assert().success();

    // Use EDITOR=true to avoid blocking on interactive editor
    autodev(&home)
        .env("EDITOR", "true")
        .args(["claw", "edit", "nonexistent-rule-name"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn e2e_claw_edit_existing_rule_succeeds() {
    let home = TempDir::new().unwrap();

    autodev(&home).args(["claw", "init"]).assert().success();

    // Use EDITOR=true (exits 0 without modifying) to simulate editor
    autodev(&home)
        .env("EDITOR", "true")
        .args(["claw", "edit", "scheduling"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated"));
}
