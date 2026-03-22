//! E2E tests for queue workflow:
//! DB seeding → list/show → advance (state transitions) → skip → decisions

mod e2e_helpers;

use e2e_helpers::*;
use predicates::prelude::*;
use tempfile::TempDir;

const REPO_URL: &str = "https://github.com/org/queue-repo";
const REPO_NAME: &str = "org/queue-repo";

// ═══════════════════════════════════════════════
// 1. queue list
// ═══════════════════════════════════════════════

#[test]
fn e2e_queue_list_empty() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // Without --json or --state, it reads daemon.status.json which won't exist
    // Use --json to hit the DB path
    autodev(&home)
        .args(["queue", "list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[]"));
}

#[test]
fn e2e_queue_list_shows_seeded_items() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:1",
        "issue",
        "pending",
        Some("Test issue"),
        1,
    );

    autodev(&home)
        .args(["queue", "list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("issue:org/queue-repo:1"));
}

#[test]
fn e2e_queue_list_filter_by_state() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:10",
        "issue",
        "pending",
        None,
        10,
    );
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:11",
        "issue",
        "done",
        None,
        11,
    );

    autodev(&home)
        .args(["queue", "list", "--state", "pending"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("issue:org/queue-repo:10")
                .and(predicate::str::contains("issue:org/queue-repo:11").not()),
        );
}

#[test]
fn e2e_queue_list_filter_by_repo() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:20",
        "issue",
        "pending",
        None,
        20,
    );

    autodev(&home)
        .args(["queue", "list", "--json", "--repo", REPO_NAME])
        .assert()
        .success()
        .stdout(predicate::str::contains("issue:org/queue-repo:20"));
}

// ═══════════════════════════════════════════════
// 2. queue show
// ═══════════════════════════════════════════════

#[test]
fn e2e_queue_show_details() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:30",
        "issue",
        "pending",
        Some("Show item"),
        30,
    );

    autodev(&home)
        .args(["queue", "show", "issue:org/queue-repo:30"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("issue:org/queue-repo:30")
                .and(predicate::str::contains("pending"))
                .and(predicate::str::contains("#30")),
        );
}

#[test]
fn e2e_queue_show_not_found() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["queue", "show", "issue:org/queue-repo:999"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ═══════════════════════════════════════════════
// 3. queue advance — state transitions
// ═══════════════════════════════════════════════

#[test]
fn e2e_queue_advance_pending_to_ready() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:40",
        "issue",
        "pending",
        None,
        40,
    );

    autodev(&home)
        .args(["queue", "advance", "issue:org/queue-repo:40"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pending").and(predicate::str::contains("ready")));
}

#[test]
fn e2e_queue_advance_ready_to_running() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:41",
        "issue",
        "ready",
        None,
        41,
    );

    autodev(&home)
        .args(["queue", "advance", "issue:org/queue-repo:41"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ready").and(predicate::str::contains("running")));
}

#[test]
fn e2e_queue_advance_running_to_completed() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:42",
        "issue",
        "running",
        None,
        42,
    );

    autodev(&home)
        .args(["queue", "advance", "issue:org/queue-repo:42"])
        .assert()
        .success()
        .stdout(predicate::str::contains("running").and(predicate::str::contains("completed")));
}

#[test]
fn e2e_queue_advance_full_lifecycle() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:50",
        "issue",
        "pending",
        None,
        50,
    );

    // pending → ready
    autodev(&home)
        .args(["queue", "advance", "issue:org/queue-repo:50"])
        .assert()
        .success();
    // ready → running
    autodev(&home)
        .args(["queue", "advance", "issue:org/queue-repo:50"])
        .assert()
        .success();
    // running → completed
    autodev(&home)
        .args(["queue", "advance", "issue:org/queue-repo:50"])
        .assert()
        .success();
    // v5: advance stops at completed — queue done moves to done
    autodev(&home)
        .args(["queue", "advance", "issue:org/queue-repo:50"])
        .assert()
        .failure();

    // Verify final state is completed
    autodev(&home)
        .args(["queue", "show", "issue:org/queue-repo:50"])
        .assert()
        .success()
        .stdout(predicate::str::contains("completed"));
}

#[test]
fn e2e_queue_advance_done_fails() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:51",
        "issue",
        "done",
        None,
        51,
    );

    autodev(&home)
        .args(["queue", "advance", "issue:org/queue-repo:51"])
        .assert()
        .failure();
}

#[test]
fn e2e_queue_advance_nonexistent() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["queue", "advance", "issue:org/queue-repo:999"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn e2e_queue_advance_with_reason() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:60",
        "issue",
        "pending",
        None,
        60,
    );

    autodev(&home)
        .args([
            "queue",
            "advance",
            "issue:org/queue-repo:60",
            "--reason",
            "approved by claw",
        ])
        .assert()
        .success();

    // Decision should be recorded
    autodev(&home)
        .args(["decisions", "list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("approved by claw"));
}

// ═══════════════════════════════════════════════
// 4. queue skip
// ═══════════════════════════════════════════════

#[test]
fn e2e_queue_skip() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:70",
        "issue",
        "pending",
        None,
        70,
    );

    autodev(&home)
        .args(["queue", "skip", "issue:org/queue-repo:70"])
        .assert()
        .success()
        .stdout(predicate::str::contains("skipped"));
}

#[test]
fn e2e_queue_skip_with_reason() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:71",
        "issue",
        "pending",
        None,
        71,
    );

    autodev(&home)
        .args([
            "queue",
            "skip",
            "issue:org/queue-repo:71",
            "--reason",
            "duplicate issue",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("duplicate issue"));
}

#[test]
fn e2e_queue_skip_records_decision() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:72",
        "issue",
        "pending",
        None,
        72,
    );

    autodev(&home)
        .args([
            "queue",
            "skip",
            "issue:org/queue-repo:72",
            "--reason",
            "not relevant",
        ])
        .assert()
        .success();

    autodev(&home)
        .args(["decisions", "list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("skip").and(predicate::str::contains("not relevant")));
}

// ═══════════════════════════════════════════════
// 5. queue list --unextracted
// ═══════════════════════════════════════════════

#[test]
fn e2e_queue_list_unextracted() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);

    // Done PR without skip_reason → unextracted
    seed_pr_queue_item(&home, &repo_id, "pr:org/queue-repo:80", "done", 1);
    // Done issue → not unextracted (wrong type)
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:81",
        "issue",
        "done",
        None,
        81,
    );

    autodev(&home)
        .args(["queue", "list", "--unextracted"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("pr:org/queue-repo:80")
                .and(predicate::str::contains("issue:org/queue-repo:81").not()),
        );
}

// ═══════════════════════════════════════════════
// 5b. queue done / hitl / retry-script (V5)
// ═══════════════════════════════════════════════

#[test]
fn e2e_queue_done_transitions_completed_to_done() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:100",
        "issue",
        "completed",
        None,
        100,
    );

    autodev(&home)
        .args(["queue", "done", "issue:org/queue-repo:100"])
        .assert()
        .success()
        .stdout(predicate::str::contains("done"));

    // Verify final state
    autodev(&home)
        .args(["queue", "show", "issue:org/queue-repo:100"])
        .assert()
        .success()
        .stdout(predicate::str::contains("done"));
}

#[test]
fn e2e_queue_done_rejects_non_completed() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:101",
        "issue",
        "running",
        None,
        101,
    );

    autodev(&home)
        .args(["queue", "done", "issue:org/queue-repo:101"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Completed phase"));
}

#[test]
fn e2e_queue_done_with_reason() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:102",
        "issue",
        "completed",
        None,
        102,
    );

    autodev(&home)
        .args([
            "queue",
            "done",
            "issue:org/queue-repo:102",
            "--reason",
            "all checks passed",
        ])
        .assert()
        .success();

    // Decision should be recorded
    autodev(&home)
        .args(["decisions", "list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("all checks passed"));
}

#[test]
fn e2e_queue_hitl_transitions_completed_to_hitl() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:110",
        "issue",
        "completed",
        None,
        110,
    );

    autodev(&home)
        .args([
            "queue",
            "hitl",
            "issue:org/queue-repo:110",
            "--reason",
            "needs human review",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("hitl").and(predicate::str::contains("needs human review")),
        );

    // Verify state is hitl
    autodev(&home)
        .args(["queue", "show", "issue:org/queue-repo:110"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hitl"));

    // HITL event should be created
    autodev(&home)
        .args(["hitl", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("needs human review"));
}

#[test]
fn e2e_queue_hitl_rejects_non_completed() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:111",
        "issue",
        "pending",
        None,
        111,
    );

    autodev(&home)
        .args([
            "queue",
            "hitl",
            "issue:org/queue-repo:111",
            "--reason",
            "test",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Completed phase"));
}

#[test]
fn e2e_queue_retry_script_transitions_failed_to_done() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:120",
        "issue",
        "failed",
        None,
        120,
    );

    autodev(&home)
        .args(["queue", "retry-script", "issue:org/queue-repo:120"])
        .assert()
        .success()
        .stdout(predicate::str::contains("retry-script"));

    // Verify final state
    autodev(&home)
        .args(["queue", "show", "issue:org/queue-repo:120"])
        .assert()
        .success()
        .stdout(predicate::str::contains("done"));
}

#[test]
fn e2e_queue_retry_script_rejects_non_failed() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:121",
        "issue",
        "completed",
        None,
        121,
    );

    autodev(&home)
        .args(["queue", "retry-script", "issue:org/queue-repo:121"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed phase"));
}

#[test]
fn e2e_queue_full_v5_lifecycle() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:130",
        "issue",
        "pending",
        None,
        130,
    );

    // pending → ready → running → completed (via advance)
    autodev(&home)
        .args(["queue", "advance", "issue:org/queue-repo:130"])
        .assert()
        .success();
    autodev(&home)
        .args(["queue", "advance", "issue:org/queue-repo:130"])
        .assert()
        .success();
    autodev(&home)
        .args(["queue", "advance", "issue:org/queue-repo:130"])
        .assert()
        .success();

    // completed → done (via queue done)
    autodev(&home)
        .args(["queue", "done", "issue:org/queue-repo:130"])
        .assert()
        .success();

    // Verify final state
    autodev(&home)
        .args(["queue", "show", "issue:org/queue-repo:130"])
        .assert()
        .success()
        .stdout(predicate::str::contains("done"));
}

// ═══════════════════════════════════════════════
// 6. PR advance with review overflow → HITL
// ═══════════════════════════════════════════════

#[test]
// ═══════════════════════════════════════════════
// 7. queue done (Completed → Done)
// ═══════════════════════════════════════════════

fn e2e_queue_done_completed_to_done() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:100",
        "issue",
        "completed",
        None,
        100,
    );

    autodev(&home)
        .args(["queue", "done", "issue:org/queue-repo:100"])
        .assert()
        .success()
        .stdout(predicate::str::contains("done"));

    // Verify final state
    autodev(&home)
        .args(["queue", "show", "issue:org/queue-repo:100"])
        .assert()
        .success()
        .stdout(predicate::str::contains("done"));
}

#[test]
fn e2e_queue_done_wrong_phase_fails() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:101",
        "issue",
        "running",
        None,
        101,
    );

    autodev(&home)
        .args(["queue", "done", "issue:org/queue-repo:101"])
        .assert()
        .failure();
}

// ═══════════════════════════════════════════════
// 8. queue hitl (Completed → HITL)
// ═══════════════════════════════════════════════

#[test]
fn e2e_queue_hitl_completed_to_hitl() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:110",
        "issue",
        "completed",
        None,
        110,
    );

    autodev(&home)
        .args([
            "queue",
            "hitl",
            "issue:org/queue-repo:110",
            "--reason",
            "needs human review",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("hitl"));

    // HITL event should be created
    autodev(&home)
        .args(["hitl", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("human review"));
}

// ═══════════════════════════════════════════════
// 9. queue retry-script (Failed → Completed)
// ═══════════════════════════════════════════════

#[test]
fn e2e_queue_retry_script() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:120",
        "issue",
        "failed",
        None,
        120,
    );

    autodev(&home)
        .args(["queue", "retry-script", "issue:org/queue-repo:120"])
        .assert()
        .success()
        .stdout(predicate::str::contains("retry-script"));

    // Verify state is now done (retry-script transitions Failed → Done)
    autodev(&home)
        .args(["queue", "show", "issue:org/queue-repo:120"])
        .assert()
        .success()
        .stdout(predicate::str::contains("done"));
}

// ═══════════════════════════════════════════════
// 10. context
// ═══════════════════════════════════════════════

#[test]
fn e2e_context_json() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);
    seed_queue_item(
        &home,
        &repo_id,
        "issue:org/queue-repo:130",
        "issue",
        "running",
        Some("Test context"),
        130,
    );

    autodev(&home)
        .args(["context", "issue:org/queue-repo:130", "--json"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("issue:org/queue-repo:130")
                .and(predicate::str::contains("running"))
                .and(predicate::str::contains("130")),
        );
}

#[test]
fn e2e_context_not_found() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["context", "issue:org/queue-repo:999"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ═══════════════════════════════════════════════
// 11. PR advance with review overflow → HITL
// ═══════════════════════════════════════════════

#[test]
fn e2e_queue_advance_pr_creates_hitl_on_review_overflow() {
    let home = TempDir::new().unwrap();
    let repo_id = setup_repo(&home, REPO_URL);

    // review_iteration=3 exceeds default max_iterations=2
    seed_pr_queue_item(&home, &repo_id, "pr:org/queue-repo:90", "pending", 3);

    autodev(&home)
        .args(["queue", "advance", "pr:org/queue-repo:90"])
        .assert()
        .success();

    // HITL should have been created
    autodev(&home)
        .args(["hitl", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("review iteration"));
}
