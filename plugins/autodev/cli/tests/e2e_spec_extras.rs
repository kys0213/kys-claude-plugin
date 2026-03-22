//! E2E tests for spec subcommands with missing coverage:
//! - spec verify (acceptance criteria verification)
//! - spec add --file (body from file)
//! - spec update --file (body from file, mutual exclusion with --body)

mod e2e_helpers;

use e2e_helpers::*;
use predicates::prelude::*;
use std::io::Write;
use tempfile::TempDir;

const REPO_URL: &str = "https://github.com/org/spec-extras";
const REPO_NAME: &str = "org/spec-extras";

// ═══════════════════════════════════════════════
// 1. spec verify
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_verify_no_acceptance_criteria() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "No AC", "body only");

    autodev(&home)
        .args(["spec", "verify", &id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no acceptance_criteria"));
}

#[test]
fn e2e_spec_verify_with_criteria_reports_unmet() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // Create spec with acceptance criteria (avoid `- [ ]` syntax which clap misparses)
    let output = autodev(&home)
        .args([
            "spec",
            "add",
            "--title",
            "Verifiable",
            "--body",
            "body",
            "--repo",
            REPO_NAME,
            "--acceptance-criteria",
            "Unit tests pass\nIntegration tests pass",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "spec add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let id = extract_uuid(&stdout).expect("spec UUID");

    // Verify without linked done issues → all criteria unmet
    autodev(&home)
        .args(["spec", "verify", &id])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Spec:")
                .and(predicate::str::contains("Criteria:"))
                .and(predicate::str::contains("UNMET")),
        );
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

// ═══════════════════════════════════════════════
// 2. spec add --file
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_add_file_reads_body_from_file() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // Write spec body to a temp file
    let file_path = home.path().join("spec_body.md");
    let mut f = std::fs::File::create(&file_path).unwrap();
    writeln!(f, "## Requirements\nFile-based spec body content").unwrap();
    drop(f);

    let output = autodev(&home)
        .args([
            "spec",
            "add",
            "--title",
            "File Spec",
            "--file",
            file_path.to_str().unwrap(),
            "--repo",
            REPO_NAME,
            "--force",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "spec add --file failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let id = extract_uuid(&stdout).expect("spec UUID");

    // Verify the body was read from the file
    autodev(&home)
        .args(["spec", "show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("File-based spec body content"));
}

#[test]
fn e2e_spec_add_file_nonexistent_errors() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "spec",
            "add",
            "--title",
            "Bad File",
            "--file",
            "/tmp/nonexistent_spec_file_xyz.md",
            "--repo",
            REPO_NAME,
            "--force",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to read file"));
}

#[test]
fn e2e_spec_add_no_body_no_file_errors() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "spec", "add", "--title", "Empty", "--repo", REPO_NAME, "--force",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--body or --file"));
}

// ═══════════════════════════════════════════════
// 3. spec update --file
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_update_file_reads_body_from_file() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Update File", "original body");

    // Write updated body to a temp file
    let file_path = home.path().join("updated_body.md");
    let mut f = std::fs::File::create(&file_path).unwrap();
    writeln!(f, "Updated body from file").unwrap();
    drop(f);

    autodev(&home)
        .args(["spec", "update", &id, "--file", file_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated"));

    // Verify the body was updated from the file
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
    let id = create_spec(&home, REPO_NAME, "Mutual Excl", "original body");

    let file_path = home.path().join("conflict.md");
    let mut f = std::fs::File::create(&file_path).unwrap();
    writeln!(f, "file content").unwrap();
    drop(f);

    autodev(&home)
        .args([
            "spec",
            "update",
            &id,
            "--body",
            "inline body",
            "--file",
            file_path.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "cannot specify both --body and --file",
        ));
}

#[test]
fn e2e_spec_update_file_nonexistent_errors() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Bad Update", "original body");

    autodev(&home)
        .args([
            "spec",
            "update",
            &id,
            "--file",
            "/tmp/nonexistent_update_file_xyz.md",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::is_empty().not());
}
