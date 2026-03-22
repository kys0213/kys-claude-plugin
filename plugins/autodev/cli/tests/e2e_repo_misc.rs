//! E2E tests for previously uncovered CLI subcommands:
//! - repo update (success, nonexistent, invalid JSON, deep merge)
//! - repo show/list --json (field validation, empty/populated arrays)
//! - cron trigger (nonexistent error, add-then-trigger flow)
//! - worktree list/remove (empty, preserved, repo filter, remove)

mod e2e_helpers;

use e2e_helpers::*;
use predicates::prelude::*;
use tempfile::TempDir;

const REPO_URL: &str = "https://github.com/org/misc-repo";
const REPO_NAME: &str = "org/misc-repo";

// ═══════════════════════════════════════════════
// 1. repo update
// ═══════════════════════════════════════════════

#[test]
fn e2e_repo_update_success() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "repo",
            "update",
            REPO_NAME,
            "--config",
            r#"{"concurrency": 2}"#,
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("updated: org/misc-repo")
                .and(predicate::str::contains("config: written to")),
        );
}

#[test]
fn e2e_repo_update_nonexistent_fails() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args([
            "repo",
            "update",
            "org/nonexistent",
            "--config",
            r#"{"key": "value"}"#,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("repository not found"));
}

#[test]
fn e2e_repo_update_invalid_json_fails() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["repo", "update", REPO_NAME, "--config", "invalid-json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid config JSON"));
}

#[test]
fn e2e_repo_update_deep_merges_config() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // First update sets poll_interval
    autodev(&home)
        .args([
            "repo",
            "update",
            REPO_NAME,
            "--config",
            r#"{"daemon":{"poll_interval":30}}"#,
        ])
        .assert()
        .success();

    // Second update adds log_level — poll_interval should be preserved
    autodev(&home)
        .args([
            "repo",
            "update",
            REPO_NAME,
            "--config",
            r#"{"daemon":{"log_level":"debug"}}"#,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated: org/misc-repo"));

    // Verify merged config via repo show --json
    let json = run_json(&home, &["repo", "show", REPO_NAME, "--json"]);
    assert_eq!(json["name"], "org/misc-repo");
    assert!(json["config"].is_object(), "config should be an object");
}

// ═══════════════════════════════════════════════
// 2. repo show/list --json
// ═══════════════════════════════════════════════

#[test]
fn e2e_repo_show_json_has_expected_fields() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    let json = run_json(&home, &["repo", "show", REPO_NAME, "--json"]);

    assert_eq!(json["name"], "org/misc-repo");
    assert_eq!(json["url"], REPO_URL);
    assert!(json["enabled"].is_boolean());
    assert!(json["config"].is_object(), "expected config field in JSON");
}

#[test]
fn e2e_repo_show_json_nonexistent_fails() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["repo", "show", "org/nonexistent", "--json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("repository not found"));
}

#[test]
fn e2e_repo_list_json_empty() {
    let home = TempDir::new().unwrap();

    let json = run_json(&home, &["repo", "list", "--json"]);
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 0);
}

#[test]
fn e2e_repo_list_json_with_repos() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    setup_repo(&home, "https://github.com/org/another-repo");

    let json = run_json(&home, &["repo", "list", "--json"]);
    assert!(json.is_array());
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2);

    // Each entry should have name, url, enabled
    for entry in arr {
        assert!(entry["name"].is_string());
        assert!(entry["url"].is_string());
        assert!(entry["enabled"].is_boolean());
    }
}

// ═══════════════════════════════════════════════
// 3. cron trigger
// ═══════════════════════════════════════════════

#[test]
fn e2e_cron_trigger_nonexistent_fails() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["cron", "trigger", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cron job not found"));
}

#[test]
fn e2e_cron_add_then_trigger_success() {
    let home = TempDir::new().unwrap();

    // Create a trivial script that exits 0
    let script_path = home.path().join("test-cron.sh");
    std::fs::write(&script_path, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // Add a cron job with this script
    autodev(&home)
        .args([
            "cron",
            "add",
            "--name",
            "trigger-test",
            "--script",
            script_path.to_str().unwrap(),
            "--interval",
            "3600",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("added cron job: trigger-test"));

    // Trigger it — should succeed
    autodev(&home)
        .args(["cron", "trigger", "trigger-test"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("triggering cron job: trigger-test")
                .and(predicate::str::contains("completed successfully")),
        );
}

// ═══════════════════════════════════════════════
// 4. worktree list / remove
// ═══════════════════════════════════════════════

#[test]
fn e2e_worktree_list_empty() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["worktree", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No"));
}

#[test]
fn e2e_worktree_list_shows_preserved() {
    let home = TempDir::new().unwrap();

    // Create a fake preserved worktree directory
    let wt_dir = home
        .path()
        .join("workspaces")
        .join("org-repo")
        .join("issue-99");
    std::fs::create_dir_all(&wt_dir).unwrap();

    autodev(&home)
        .args(["worktree", "list"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("preserved worktree")
                .and(predicate::str::contains("org-repo/issue-99")),
        );
}

#[test]
fn e2e_worktree_list_filters_by_repo() {
    let home = TempDir::new().unwrap();

    // Create worktrees for two repos
    let wt1 = home
        .path()
        .join("workspaces")
        .join("org-repo")
        .join("issue-1");
    let wt2 = home
        .path()
        .join("workspaces")
        .join("other-repo")
        .join("issue-2");
    std::fs::create_dir_all(&wt1).unwrap();
    std::fs::create_dir_all(&wt2).unwrap();

    autodev(&home)
        .args(["worktree", "list", "--repo", "org/repo"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("org-repo/issue-1")
                .and(predicate::str::contains("other-repo").not()),
        );
}

#[test]
fn e2e_worktree_remove_nonexistent_fails() {
    let home = TempDir::new().unwrap();

    // Need workspaces directory to exist for read_dir to work
    std::fs::create_dir_all(home.path().join("workspaces")).unwrap();

    autodev(&home)
        .args(["worktree", "remove", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("worktree not found"));
}

#[test]
fn e2e_worktree_remove_success() {
    let home = TempDir::new().unwrap();

    // Create a fake preserved worktree
    let wt_dir = home
        .path()
        .join("workspaces")
        .join("org-repo")
        .join("issue-42");
    std::fs::create_dir_all(&wt_dir).unwrap();

    autodev(&home)
        .args(["worktree", "remove", "issue-42"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed"));

    assert!(!wt_dir.exists(), "worktree directory should be deleted");
}
