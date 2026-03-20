//! E2E tests for HITL (Human-in-the-Loop) workflow:
//! creation via spec complete, DB seeding → list/show → respond → timeout

mod e2e_helpers;

use e2e_helpers::*;
use predicates::prelude::*;
use tempfile::TempDir;

const REPO_URL: &str = "https://github.com/org/hitl-repo";
const REPO_NAME: &str = "org/hitl-repo";

// ═══════════════════════════════════════════════
// 1. hitl list
// ═══════════════════════════════════════════════

#[test]
fn e2e_hitl_list_empty() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["hitl", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No HITL events found"));
}

#[test]
fn e2e_hitl_list_shows_seeded_event() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    let event_id = seed_hitl_event(
        &home,
        &repo_id,
        None,
        None,
        "medium",
        "Need human decision",
        &["Option A", "Option B"],
    );

    autodev(&home)
        .args(["hitl", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&event_id[..8]));
}

#[test]
fn e2e_hitl_list_from_spec_complete() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);
    let spec_id = create_spec(&home, REPO_NAME, "HITL Spec", "body");

    autodev(&home)
        .args(["spec", "link", &spec_id, "--issue", "1"])
        .assert()
        .success();
    autodev(&home)
        .args(["spec", "complete", &spec_id])
        .assert()
        .success();

    autodev(&home)
        .args(["hitl", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pending"));
}

// ═══════════════════════════════════════════════
// 2. hitl show
// ═══════════════════════════════════════════════

#[test]
fn e2e_hitl_show_details() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    let event_id = seed_hitl_event(
        &home,
        &repo_id,
        None,
        None,
        "high",
        "Critical decision needed",
        &["Approve", "Reject"],
    );

    autodev(&home)
        .args(["hitl", "show", &event_id])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Critical decision needed")
                .and(predicate::str::contains("high"))
                .and(predicate::str::contains("pending"))
                .and(predicate::str::contains("Approve"))
                .and(predicate::str::contains("Reject")),
        );
}

#[test]
fn e2e_hitl_show_not_found() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["hitl", "show", "nonexistent-id"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ═══════════════════════════════════════════════
// 3. hitl respond
// ═══════════════════════════════════════════════

#[test]
fn e2e_hitl_respond_with_choice() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    let event_id = seed_hitl_event(
        &home,
        &repo_id,
        None,
        None,
        "medium",
        "Choose wisely",
        &["Option 1", "Option 2"],
    );

    autodev(&home)
        .args(["hitl", "respond", &event_id, "--choice", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Responded"));

    // Verify status changed to responded
    autodev(&home)
        .args(["hitl", "show", &event_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("responded"));
}

#[test]
fn e2e_hitl_respond_with_message() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    let event_id = seed_hitl_event(
        &home,
        &repo_id,
        None,
        None,
        "low",
        "Info needed",
        &["Continue"],
    );

    autodev(&home)
        .args([
            "hitl",
            "respond",
            &event_id,
            "--message",
            "Please proceed with option A",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Responded"));
}

#[test]
fn e2e_hitl_respond_with_choice_and_message() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    let event_id = seed_hitl_event(
        &home,
        &repo_id,
        None,
        None,
        "medium",
        "Decision point",
        &["Yes", "No"],
    );

    autodev(&home)
        .args([
            "hitl",
            "respond",
            &event_id,
            "--choice",
            "1",
            "--message",
            "Go ahead",
        ])
        .assert()
        .success();
}

#[test]
fn e2e_hitl_respond_requires_choice_or_message() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    let event_id = seed_hitl_event(
        &home,
        &repo_id,
        None,
        None,
        "medium",
        "Empty respond",
        &["Ok"],
    );

    autodev(&home)
        .args(["hitl", "respond", &event_id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--choice").or(predicate::str::contains("--message")));
}

#[test]
fn e2e_hitl_respond_already_responded() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    let event_id = seed_hitl_event(
        &home,
        &repo_id,
        None,
        None,
        "medium",
        "Double respond",
        &["Ok"],
    );

    autodev(&home)
        .args(["hitl", "respond", &event_id, "--choice", "1"])
        .assert()
        .success();

    // Second respond should fail
    autodev(&home)
        .args(["hitl", "respond", &event_id, "--choice", "1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already responded"));
}

// ═══════════════════════════════════════════════
// 4. hitl timeout
// ═══════════════════════════════════════════════

#[test]
fn e2e_hitl_timeout_no_expired() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["hitl", "timeout", "--hours", "24"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No expired"));
}

#[test]
fn e2e_hitl_timeout_expires_old_events() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);

    // Create an event 48 hours ago
    let event_id = seed_old_hitl_event(&home, &repo_id, 48);

    autodev(&home)
        .args(["hitl", "timeout", "--hours", "24", "--action", "expire"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Processed 1 events")
                .and(predicate::str::contains(&event_id[..8])),
        );

    // Event should now be expired
    autodev(&home)
        .args(["hitl", "show", &event_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("expired"));
}

#[test]
fn e2e_hitl_timeout_remind_doesnt_change_status() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    let event_id = seed_old_hitl_event(&home, &repo_id, 48);

    autodev(&home)
        .args(["hitl", "timeout", "--hours", "24", "--action", "remind"])
        .assert()
        .success()
        .stdout(predicate::str::contains("remind"));

    // Status should still be pending (remind doesn't change it)
    autodev(&home)
        .args(["hitl", "show", &event_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("pending"));
}

// ═══════════════════════════════════════════════
// 5. hitl with spec_id
// ═══════════════════════════════════════════════

#[test]
fn e2e_hitl_show_includes_spec_id() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    let spec_id = create_spec(&home, REPO_NAME, "Linked HITL Spec", "body");
    let event_id = seed_hitl_event(
        &home,
        &repo_id,
        Some(&spec_id),
        None,
        "medium",
        "Spec-linked event",
        &["Ok"],
    );

    autodev(&home)
        .args(["hitl", "show", &event_id])
        .assert()
        .success()
        .stdout(predicate::str::contains(&spec_id));
}

#[test]
fn e2e_hitl_show_includes_work_id() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    let event_id = seed_hitl_event(
        &home,
        &repo_id,
        None,
        Some("issue:org/hitl-repo:100"),
        "high",
        "Work-linked event",
        &["Continue", "Stop"],
    );

    autodev(&home)
        .args(["hitl", "show", &event_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("issue:org/hitl-repo:100"));
}
