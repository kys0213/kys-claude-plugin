//! E2E tests for `autodev convention` subcommands:
//! detect, bootstrap, patterns, collect-feedback, propose, apply-approved

mod e2e_helpers;

use e2e_helpers::*;
use predicates::prelude::*;
use tempfile::TempDir;

const REPO_URL: &str = "https://github.com/org/convention-repo";
const REPO_NAME: &str = "org/convention-repo";

// ═══════════════════════════════════════════════
// 1. convention detect
// ═══════════════════════════════════════════════

#[test]
fn e2e_convention_detect_rust_repo() {
    let home = TempDir::new().unwrap();
    let repo_dir = TempDir::new().unwrap();

    // Create a Cargo.toml to simulate a Rust project
    std::fs::write(
        repo_dir.path().join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    autodev(&home)
        .args(["convention", "detect", &repo_dir.path().to_string_lossy()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Rust"));
}

#[test]
fn e2e_convention_detect_empty_repo() {
    let home = TempDir::new().unwrap();
    let repo_dir = TempDir::new().unwrap();

    autodev(&home)
        .args(["convention", "detect", &repo_dir.path().to_string_lossy()])
        .assert()
        .success()
        .stdout(predicate::str::contains("no technology stack detected"));
}

#[test]
fn e2e_convention_detect_nonexistent_path_fails() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["convention", "detect", "/tmp/nonexistent-path-12345"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a directory"));
}

#[test]
fn e2e_convention_detect_go_repo() {
    let home = TempDir::new().unwrap();
    let repo_dir = TempDir::new().unwrap();

    std::fs::write(
        repo_dir.path().join("go.mod"),
        "module example.com/test\n\ngo 1.21\n\nrequire github.com/gin-gonic/gin v1.9.0\n",
    )
    .unwrap();

    autodev(&home)
        .args(["convention", "detect", &repo_dir.path().to_string_lossy()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Go")
                .and(predicate::str::contains("Gin"))
                .and(predicate::str::contains("go test")),
        );
}

#[test]
fn e2e_convention_detect_python_repo() {
    let home = TempDir::new().unwrap();
    let repo_dir = TempDir::new().unwrap();

    std::fs::write(
        repo_dir.path().join("requirements.txt"),
        "fastapi==0.100.0\npytest==7.0.0\n",
    )
    .unwrap();

    autodev(&home)
        .args(["convention", "detect", &repo_dir.path().to_string_lossy()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Python")
                .and(predicate::str::contains("FastAPI"))
                .and(predicate::str::contains("pytest")),
        );
}

#[test]
fn e2e_convention_detect_build_tools() {
    let home = TempDir::new().unwrap();
    let repo_dir = TempDir::new().unwrap();

    // Create Cargo.toml so there is a language, plus Makefile and GitHub Actions
    std::fs::write(
        repo_dir.path().join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(repo_dir.path().join("Makefile"), "all:\n\techo hello\n").unwrap();
    std::fs::create_dir_all(repo_dir.path().join(".github/workflows")).unwrap();
    std::fs::write(
        repo_dir.path().join(".github/workflows/ci.yml"),
        "name: CI\n",
    )
    .unwrap();

    autodev(&home)
        .args(["convention", "detect", &repo_dir.path().to_string_lossy()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Build tools:")
                .and(predicate::str::contains("Make"))
                .and(predicate::str::contains("GitHub Actions")),
        );
}

#[test]
fn e2e_convention_detect_typescript_repo() {
    let home = TempDir::new().unwrap();
    let repo_dir = TempDir::new().unwrap();

    std::fs::write(
        repo_dir.path().join("package.json"),
        r#"{"dependencies":{"typescript":"^5.0.0","react":"^18.0.0"}}"#,
    )
    .unwrap();

    autodev(&home)
        .args(["convention", "detect", &repo_dir.path().to_string_lossy()])
        .assert()
        .success()
        .stdout(predicate::str::contains("TypeScript").and(predicate::str::contains("React")));
}

// ═══════════════════════════════════════════════
// 2. convention bootstrap
// ═══════════════════════════════════════════════

#[test]
fn e2e_convention_bootstrap_dry_run() {
    let home = TempDir::new().unwrap();
    let repo_dir = TempDir::new().unwrap();

    // Create a Rust project
    std::fs::write(
        repo_dir.path().join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    autodev(&home)
        .args([
            "convention",
            "bootstrap",
            &repo_dir.path().to_string_lossy(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("would create"));

    // Files should NOT be created in dry-run mode
    assert!(
        !repo_dir.path().join(".claude/rules").exists(),
        "dry-run should not create files"
    );
}

#[test]
fn e2e_convention_bootstrap_apply() {
    let home = TempDir::new().unwrap();
    let repo_dir = TempDir::new().unwrap();

    // Create a Rust project
    std::fs::write(
        repo_dir.path().join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    autodev(&home)
        .args([
            "convention",
            "bootstrap",
            &repo_dir.path().to_string_lossy(),
            "--apply",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("[created]"));

    // Files should be created with --apply
    assert!(
        repo_dir.path().join(".claude/rules").exists(),
        "bootstrap --apply should create .claude/rules/"
    );
}

#[test]
fn e2e_convention_bootstrap_empty_repo() {
    let home = TempDir::new().unwrap();
    let repo_dir = TempDir::new().unwrap();

    // No marker files — empty project
    autodev(&home)
        .args([
            "convention",
            "bootstrap",
            &repo_dir.path().to_string_lossy(),
            "--apply",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("git-workflow.md"));
}

#[test]
fn e2e_convention_bootstrap_skips_existing_files() {
    let home = TempDir::new().unwrap();
    let repo_dir = TempDir::new().unwrap();

    // Create a Rust project
    std::fs::write(
        repo_dir.path().join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    // First bootstrap
    autodev(&home)
        .args([
            "convention",
            "bootstrap",
            &repo_dir.path().to_string_lossy(),
            "--apply",
        ])
        .assert()
        .success();

    // Second bootstrap should skip existing files
    autodev(&home)
        .args([
            "convention",
            "bootstrap",
            &repo_dir.path().to_string_lossy(),
            "--apply",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("skipped"));
}

#[test]
fn e2e_convention_bootstrap_nonexistent_path() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args([
            "convention",
            "bootstrap",
            "/tmp/nonexistent-path-autodev-12345",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a directory"));
}

// ═══════════════════════════════════════════════
// 3. convention patterns
// ═══════════════════════════════════════════════

#[test]
fn e2e_convention_patterns_no_repo() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["convention", "patterns"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Specify --repo"));
}

#[test]
fn e2e_convention_patterns_with_repo_empty() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["convention", "patterns", "--repo", REPO_NAME])
        .assert()
        .success()
        .stdout(predicate::str::contains("No feedback patterns found"));
}

#[test]
fn e2e_convention_patterns_json_empty() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    let json = run_json(
        &home,
        &["convention", "patterns", "--repo", REPO_NAME, "--json"],
    );
    assert!(json.is_array(), "patterns --json should be an array");
    assert_eq!(json.as_array().unwrap().len(), 0);
}

#[test]
fn e2e_convention_patterns_nonexistent_repo() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["convention", "patterns", "--repo", "no/such-repo"])
        .assert()
        .failure();
}

// ═══════════════════════════════════════════════
// 4. convention collect-feedback
// ═══════════════════════════════════════════════

#[test]
fn e2e_convention_collect_feedback_nonexistent_repo() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["convention", "collect-feedback", "no/such-repo"])
        .assert()
        .failure();
}

#[test]
fn e2e_convention_collect_feedback_no_hitl_events() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // collect-feedback uses `gh` for PR reviews which will fail in test env,
    // but the HITL feedback collection part should succeed
    let output = autodev(&home)
        .args(["convention", "collect-feedback", REPO_NAME])
        .output()
        .expect("run command");

    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should report 0 patterns collected from HITL (no events exist)
    assert!(
        stdout.contains("Collected 0 feedback pattern(s)"),
        "expected 'Collected 0', got: {stdout}"
    );
}

#[test]
fn e2e_convention_collect_feedback_with_hitl_response() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);

    // Seed a HITL event and respond to it with a message
    let event_id = seed_hitl_event(
        &home,
        &repo_id,
        None,
        None,
        "medium",
        "Style issue found",
        &["Fix it", "Ignore"],
    );

    // Respond with a feedback message
    autodev(&home)
        .args([
            "hitl",
            "respond",
            &event_id,
            "--choice",
            "1",
            "--message",
            "Use consistent naming conventions",
        ])
        .assert()
        .success();

    // Collect feedback — should pick up the responded event
    let output = autodev(&home)
        .args(["convention", "collect-feedback", REPO_NAME])
        .output()
        .expect("run command");

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("Collected 1 feedback pattern(s)"),
        "expected 'Collected 1', got: {stdout}"
    );
}

// ═══════════════════════════════════════════════
// 5. convention propose
// ═══════════════════════════════════════════════

#[test]
fn e2e_convention_propose_no_patterns() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["convention", "propose", REPO_NAME])
        .assert()
        .success()
        .stdout(predicate::str::contains("No actionable patterns found"));
}

#[test]
fn e2e_convention_propose_repo_not_found() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["convention", "propose", "org/nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("repository not found"));
}

#[test]
fn e2e_convention_propose_custom_threshold() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["convention", "propose", REPO_NAME, "--threshold", "10"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No actionable patterns found"));
}

// ═══════════════════════════════════════════════
// 6. convention apply-approved
// ═══════════════════════════════════════════════

#[test]
fn e2e_convention_apply_approved_no_events() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["convention", "apply-approved", REPO_NAME])
        .assert()
        .success()
        .stdout(predicate::str::contains("0 applied"));
}

#[test]
fn e2e_convention_apply_approved_repo_not_found() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["convention", "apply-approved", "org/nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("repository not found"));
}
