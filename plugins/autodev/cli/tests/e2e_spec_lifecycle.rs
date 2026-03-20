//! E2E tests for the spec lifecycle:
//! add → list → show → update → pause/resume → link/unlink → complete → HITL → respond

mod e2e_helpers;

use e2e_helpers::*;
use predicates::prelude::*;
use tempfile::TempDir;

const REPO_URL: &str = "https://github.com/org/spec-repo";
const REPO_NAME: &str = "org/spec-repo";

// ═══════════════════════════════════════════════
// 1. spec add
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_add_returns_uuid() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "My Spec", "Spec body text");
    assert_eq!(id.len(), 36, "should be a UUID");
}

#[test]
fn e2e_spec_add_warns_missing_sections() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    // Body with no required sections → blocked unless --force
    autodev(&home)
        .args([
            "spec",
            "add",
            "--title",
            "Incomplete",
            "--body",
            "just text",
            "--repo",
            REPO_NAME,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Missing required sections"));
}

#[test]
fn e2e_spec_add_no_warning_when_sections_present() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let body =
        "## 요구사항\nfoo\n## 아키텍처\nbar\n## 기술 스택\nbaz\n## 테스트\nqux\n## 수용 기준\nend";
    autodev(&home)
        .args([
            "spec", "add", "--title", "Complete", "--body", body, "--repo", REPO_NAME,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Missing sections").not());
}

#[test]
fn e2e_spec_add_with_test_commands() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec_with_tests(&home, REPO_NAME, "TC Spec", "body", r#"["echo ok"]"#);
    assert_eq!(id.len(), 36);
}

// ═══════════════════════════════════════════════
// 2. spec list
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_list_empty() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    autodev(&home)
        .args(["spec", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No specs found"));
}

#[test]
fn e2e_spec_list_shows_added_spec() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    create_spec(&home, REPO_NAME, "Listed Spec", "body");

    autodev(&home)
        .args(["spec", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Listed Spec"));
}

#[test]
fn e2e_spec_list_filter_by_repo() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    create_spec(&home, REPO_NAME, "Filtered Spec", "body");

    autodev(&home)
        .args(["spec", "list", "--repo", REPO_NAME])
        .assert()
        .success()
        .stdout(predicate::str::contains("Filtered Spec"));

    // Unknown repo returns empty (filter applied client-side)
    autodev(&home)
        .args(["spec", "list", "--repo", "org/nonexistent"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Filtered Spec").not());
}

// ═══════════════════════════════════════════════
// 3. spec show
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_show_displays_details() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Show Spec", "Detailed body");

    autodev(&home)
        .args(["spec", "show", &id])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Show Spec")
                .and(predicate::str::contains("Detailed body"))
                .and(predicate::str::contains(&id)),
        );
}

#[test]
fn e2e_spec_show_not_found() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    autodev(&home)
        .args(["spec", "show", "nonexistent-id"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("spec not found"));
}

// ═══════════════════════════════════════════════
// 4. spec update
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_update_body() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Update Spec", "original body");

    autodev(&home)
        .args(["spec", "update", &id, "--body", "updated body"])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated"));

    autodev(&home)
        .args(["spec", "show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated body"));
}

#[test]
fn e2e_spec_update_test_commands() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "TC Update", "body");

    autodev(&home)
        .args([
            "spec",
            "update",
            &id,
            "--test-commands",
            r#"["cargo test"]"#,
        ])
        .assert()
        .success();

    autodev(&home)
        .args(["spec", "show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("cargo test"));
}

// ═══════════════════════════════════════════════
// 5. spec pause / resume
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_pause_and_resume() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Pausable", "body");

    autodev(&home)
        .args(["spec", "pause", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("paused"));

    // Verify status is paused
    autodev(&home)
        .args(["spec", "show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("paused"));

    autodev(&home)
        .args(["spec", "resume", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("resumed"));

    autodev(&home)
        .args(["spec", "show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("active"));
}

// ═══════════════════════════════════════════════
// 6. spec link / unlink
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_link_and_unlink_issue() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Linkable", "body");

    autodev(&home)
        .args(["spec", "link", &id, "--issue", "42"])
        .assert()
        .success()
        .stdout(predicate::str::contains("linked"));

    autodev(&home)
        .args(["spec", "show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("#42"));

    autodev(&home)
        .args(["spec", "unlink", &id, "--issue", "42"])
        .assert()
        .success()
        .stdout(predicate::str::contains("unlinked"));
}

#[test]
fn e2e_spec_link_multiple_issues() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Multi Link", "body");

    autodev(&home)
        .args(["spec", "link", &id, "--issue", "10"])
        .assert()
        .success();
    autodev(&home)
        .args(["spec", "link", &id, "--issue", "20"])
        .assert()
        .success();

    autodev(&home)
        .args(["spec", "show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("#10").and(predicate::str::contains("#20")));
}

// ═══════════════════════════════════════════════
// 7. spec complete → HITL creation
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_complete_requires_linked_issues() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "No Issues", "body");

    autodev(&home)
        .args(["spec", "complete", &id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no linked issues"));
}

#[test]
fn e2e_spec_complete_creates_hitl() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Completable", "body");

    autodev(&home)
        .args(["spec", "link", &id, "--issue", "1"])
        .assert()
        .success();

    autodev(&home)
        .args(["spec", "complete", &id])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("completing")
                .and(predicate::str::contains("HITL event created")),
        );

    // Verify spec status is now completing
    autodev(&home)
        .args(["spec", "show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("completing"));

    // Verify HITL event was created
    autodev(&home)
        .args(["hitl", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ready for compl"));
}

#[test]
fn e2e_spec_complete_only_active_specs() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Paused Spec", "body");

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

// ═══════════════════════════════════════════════
// 8. spec complete → hitl respond → completion
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_complete_confirm_via_hitl() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "To Complete", "body");

    autodev(&home)
        .args(["spec", "link", &id, "--issue", "1"])
        .assert()
        .success();

    // Complete → creates HITL
    let output = autodev(&home)
        .args(["spec", "complete", &id])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let uuids = extract_all_uuids(&stdout);
    let hitl_id = uuids.last().expect("HITL event ID in output").clone();

    // Confirm completion via HITL respond
    autodev(&home)
        .args(["hitl", "respond", &hitl_id, "--choice", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Completed"));

    // Verify spec is now completed
    autodev(&home)
        .args(["spec", "show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("completed"));
}

#[test]
fn e2e_spec_complete_reject_via_hitl() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "To Reject", "body");

    autodev(&home)
        .args(["spec", "link", &id, "--issue", "1"])
        .assert()
        .success();

    let output = autodev(&home)
        .args(["spec", "complete", &id])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let uuids = extract_all_uuids(&stdout);
    let hitl_id = uuids.last().expect("HITL event ID in output").clone();

    // Reject → back to Active
    autodev(&home)
        .args(["hitl", "respond", &hitl_id, "--choice", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Active"));

    autodev(&home)
        .args(["spec", "show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("active"));
}

// ═══════════════════════════════════════════════
// 9. spec prioritize
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_prioritize() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id1 = create_spec(&home, REPO_NAME, "Spec A", "body");
    let id2 = create_spec(&home, REPO_NAME, "Spec B", "body");

    autodev(&home)
        .args(["spec", "prioritize", &id1, &id2])
        .assert()
        .success()
        .stdout(predicate::str::contains("Prioritized 2 specs"));
}

// ═══════════════════════════════════════════════
// 10. spec status
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_status_shows_summary() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Status Spec", "body");

    autodev(&home)
        .args(["spec", "link", &id, "--issue", "5"])
        .assert()
        .success();

    autodev(&home)
        .args(["spec", "status", &id])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Status Spec")
                .and(predicate::str::contains("Issues:"))
                .and(predicate::str::contains("HITL:")),
        );
}

// ═══════════════════════════════════════════════
// 11. spec decisions
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_decisions_empty() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Decisions Spec", "body");

    autodev(&home)
        .args(["spec", "decisions", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("No decisions found"));
}

// ═══════════════════════════════════════════════
// 12. spec evaluate
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_evaluate() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "Eval Spec", "body");

    autodev(&home)
        .args(["spec", "evaluate", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Triggered claw-evaluate"));
}

// ═══════════════════════════════════════════════
// 13. spec conflicts (no conflicts case)
// ═══════════════════════════════════════════════

#[test]
fn e2e_spec_conflicts_none() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let id = create_spec(&home, REPO_NAME, "No Conflict", "body");

    autodev(&home)
        .args(["spec", "conflicts", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("No conflicts detected"));
}
