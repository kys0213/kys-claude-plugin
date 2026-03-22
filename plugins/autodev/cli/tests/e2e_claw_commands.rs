//! E2E tests for claw subcommands: init, rules, edit

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
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["claw", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Claw workspace initialized"));

    // Verify workspace directory was created
    assert!(home.path().join("claw-workspace").exists());
    assert!(home.path().join("claw-workspace/CLAUDE.md").exists());
    assert!(home.path().join("claw-workspace/.claude/rules").is_dir());
    assert!(home.path().join("claw-workspace/commands").is_dir());
    assert!(home.path().join("claw-workspace/skills/decompose").is_dir());
}

#[test]
fn e2e_claw_init_with_repo_creates_per_repo_override() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["claw", "init", "--repo", REPO_NAME])
        .assert()
        .success()
        .stdout(predicate::str::contains("Claw workspace initialized").and(
            predicate::str::contains("Per-repo claw override initialized"),
        ));

    // Verify per-repo override structure was created
    let repo_claw = home
        .path()
        .join("workspaces")
        .join("org-claw-repo")
        .join("claw");
    assert!(repo_claw.join(".claude/rules").is_dir());
    assert!(repo_claw.join("commands").is_dir());
    assert!(repo_claw.join("skills").is_dir());
}

#[test]
fn e2e_claw_init_is_idempotent() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // Init twice should succeed without errors
    autodev(&home).args(["claw", "init"]).assert().success();

    autodev(&home)
        .args(["claw", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Claw workspace initialized"));
}

#[test]
fn e2e_claw_init_without_repo_succeeds() {
    // claw init does not require a registered repo
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["claw", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Claw workspace initialized"));
}

// ═══════════════════════════════════════════════
// 2. claw rules
// ═══════════════════════════════════════════════

#[test]
fn e2e_claw_rules_lists_global_rules() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // Init workspace first
    autodev(&home).args(["claw", "init"]).assert().success();

    autodev(&home)
        .args(["claw", "rules"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("[global]")
                .and(predicate::str::contains("scheduling.md"))
                .and(predicate::str::contains("branch-naming.md")),
        );
}

#[test]
fn e2e_claw_rules_json_output() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home).args(["claw", "init"]).assert().success();

    let json = run_json(&home, &["claw", "rules", "--json"]);
    let arr = json.as_array().expect("should be array");
    assert!(!arr.is_empty(), "rules list should not be empty after init");
    // Each entry should be a string containing [global]
    assert!(
        arr.iter()
            .any(|v| v.as_str().unwrap_or("").contains("[global]")),
        "should contain global rules"
    );
}

#[test]
fn e2e_claw_rules_with_repo_includes_overrides() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // Init global + per-repo
    autodev(&home)
        .args(["claw", "init", "--repo", REPO_NAME])
        .assert()
        .success();

    // Add a rule file to per-repo overrides
    let repo_rules_dir = home
        .path()
        .join("workspaces")
        .join("org-claw-repo")
        .join("claw/.claude/rules");
    std::fs::write(
        repo_rules_dir.join("custom-rule.md"),
        "# Custom Rule\n\nA per-repo rule.",
    )
    .unwrap();

    autodev(&home)
        .args(["claw", "rules", "--repo", REPO_NAME])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("[global]")
                .and(predicate::str::contains(REPO_NAME))
                .and(predicate::str::contains("custom-rule.md")),
        );
}

#[test]
fn e2e_claw_rules_fails_without_init() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["claw", "rules"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not initialized"));
}

#[test]
fn e2e_claw_rules_repo_fails_without_repo_init() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // Init global workspace only
    autodev(&home).args(["claw", "init"]).assert().success();

    autodev(&home)
        .args(["claw", "rules", "--repo", REPO_NAME])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not initialized"));
}

// ═══════════════════════════════════════════════
// 3. claw edit
// ═══════════════════════════════════════════════

#[test]
fn e2e_claw_edit_fails_without_init() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["claw", "edit", "scheduling"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not initialized"));
}

#[test]
fn e2e_claw_edit_nonexistent_rule_fails() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home).args(["claw", "init"]).assert().success();

    // Set EDITOR to a no-op command so we don't block
    autodev(&home)
        .env("EDITOR", "true")
        .args(["claw", "edit", "nonexistent-rule"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn e2e_claw_edit_existing_rule_succeeds() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home).args(["claw", "init"]).assert().success();

    // Use "true" as EDITOR (no-op, exits 0 without modifying)
    autodev(&home)
        .env("EDITOR", "true")
        .args(["claw", "edit", "scheduling"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated"));
}
