//! E2E tests for coverage gaps identified in issue #471:
//!
//! 1. `repo update --config` (deep merge, invalid JSON, nonexistent repo)
//! 2. `spec verify` (no acceptance criteria, nonexistent spec ID)
//! 3. `repo show --json`, `repo list --json` (JSON output fields)
//! 4. `cron trigger` (nonexistent job error)
//! 5. `spec add --file`, `spec update --file` (file-based input, mutual exclusion)
//! 6. `hitl scan-replies` (empty result path)

mod e2e_helpers;

use e2e_helpers::*;
use predicates::prelude::*;
use tempfile::TempDir;

const REPO_URL: &str = "https://github.com/org/gap-repo";
const REPO_NAME: &str = "org/gap-repo";

// ═══════════════════════════════════════════════
// 1. repo update --config
// ═══════════════════════════════════════════════

#[test]
fn e2e_repo_update_deep_merge() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // First update: set initial config
    autodev(&home)
        .args([
            "repo",
            "update",
            REPO_NAME,
            "--config",
            r#"{"daemon":{"poll_interval":30}}"#,
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("updated: org/gap-repo")
                .and(predicate::str::contains("config: written to")),
        );

    // Second update: deep merge should preserve poll_interval and add log_level
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
        .stdout(predicate::str::contains("updated: org/gap-repo"));
}

#[test]
fn e2e_repo_update_invalid_json() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["repo", "update", REPO_NAME, "--config", "not-valid-json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid config JSON"));
}

#[test]
fn e2e_repo_update_nonexistent_repo() {
    let home = TempDir::new().unwrap();
    // Don't register any repo — update should fail
    // We need the DB to exist, so register a different repo first
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "repo",
            "update",
            "org/nonexistent",
            "--config",
            r#"{"key":"value"}"#,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("repository not found"));
}

#[test]
fn e2e_repo_update_empty_json_noop() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["repo", "update", REPO_NAME, "--config", "{}"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no changes applied"));
}

// ═══════════════════════════════════════════════
// 2. spec verify
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_verify_no_acceptance_criteria() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "No AC Spec", "body text");

    autodev(&home)
        .args(["spec", "verify", &id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no acceptance_criteria"));
}

#[test]
fn e2e_spec_verify_nonexistent_id() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["spec", "verify", "00000000-0000-0000-0000-000000000000"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("spec not found"));
}

#[test]
fn e2e_spec_verify_with_criteria() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // Create a spec, then update it with acceptance criteria
    let id = create_spec(&home, REPO_NAME, "AC Spec", "body");

    autodev(&home)
        .args([
            "spec",
            "update",
            &id,
            "--acceptance-criteria",
            "Criterion A\nCriterion B",
        ])
        .assert()
        .success();

    autodev(&home)
        .args(["spec", "verify", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Criteria:").and(predicate::str::contains("unmet")));
}

// ═══════════════════════════════════════════════
// 3. repo show --json, repo list --json
// ═══════════════════════════════════════════════

#[test]
fn e2e_json_repo_list() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    let json = run_json(&home, &["repo", "list", "--json"]);
    let arr = json.as_array().expect("repo list --json should be array");
    assert!(!arr.is_empty(), "should contain at least one repo");

    let repo = &arr[0];
    assert!(repo["name"].is_string());
    assert!(repo["url"].is_string());
    assert!(repo["enabled"].is_boolean());
    assert_eq!(repo["name"].as_str().unwrap(), REPO_NAME);
}

#[test]
fn e2e_json_repo_list_empty() {
    let home = TempDir::new().unwrap();
    // Initialize DB by running any command that triggers DB creation
    autodev(&home).args(["status"]).assert().success();

    let json = run_json(&home, &["repo", "list", "--json"]);
    let arr = json.as_array().expect("repo list --json should be array");
    assert_eq!(arr.len(), 0);
}

#[test]
fn e2e_json_repo_show_has_required_fields() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    let json = run_json(&home, &["repo", "show", REPO_NAME, "--json"]);
    assert!(json["name"].is_string());
    assert!(json["url"].is_string());
    assert!(json["enabled"].is_boolean());
    assert!(json["config"].is_object(), "should include config object");
    assert_eq!(json["name"].as_str().unwrap(), REPO_NAME);
}

#[test]
fn e2e_json_repo_show_nonexistent() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["repo", "show", "org/nonexistent", "--json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("repository not found"));
}

// ═══════════════════════════════════════════════
// 4. cron trigger
// ═══════════════════════════════════════════════

#[test]
fn e2e_cron_trigger_nonexistent_job() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["cron", "trigger", "nonexistent-job"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cron job not found"));
}

// ═══════════════════════════════════════════════
// 5. spec add --file, spec update --file
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_add_from_file() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // Create a temp file with spec body content
    let spec_file = home.path().join("spec_body.md");
    std::fs::write(&spec_file, "Spec body from file content").unwrap();

    let output = autodev(&home)
        .args([
            "spec",
            "add",
            "--title",
            "File Spec",
            "--file",
            spec_file.to_str().unwrap(),
            "--repo",
            REPO_NAME,
            "--force",
        ])
        .output()
        .expect("spec add --file");
    assert!(
        output.status.success(),
        "spec add --file failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let id = extract_uuid(&stdout).expect("spec UUID");

    // Verify the file content was used as body
    autodev(&home)
        .args(["spec", "show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Spec body from file content"));
}

#[test]
fn e2e_spec_add_no_body_no_file_fails() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // Neither --body nor --file should fail
    autodev(&home)
        .args([
            "spec", "add", "--title", "No Body", "--repo", REPO_NAME, "--force",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--body or --file"));
}

#[test]
fn e2e_spec_update_from_file() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Update File Spec", "original body");

    let spec_file = home.path().join("updated_body.md");
    std::fs::write(&spec_file, "Updated body from file").unwrap();

    autodev(&home)
        .args(["spec", "update", &id, "--file", spec_file.to_str().unwrap()])
        .assert()
        .success();

    autodev(&home)
        .args(["spec", "show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated body from file"));
}

#[test]
fn e2e_spec_update_body_and_file_mutual_exclusion() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Mutual Excl", "body");

    let spec_file = home.path().join("body.md");
    std::fs::write(&spec_file, "file body").unwrap();

    autodev(&home)
        .args([
            "spec",
            "update",
            &id,
            "--body",
            "inline body",
            "--file",
            spec_file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "cannot specify both --body and --file",
        ));
}

// ═══════════════════════════════════════════════
// 6. hitl scan-replies (empty result)
// ═══════════════════════════════════════════════

#[test]
fn e2e_hitl_scan_replies_empty() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // scan-replies with no pending HITL events should report no replies
    autodev(&home)
        .args(["hitl", "scan-replies"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No new replies found"));
}
