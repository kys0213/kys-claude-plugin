//! E2E tests for cron CRUD operations:
//! add (interval/schedule) → list → update → pause/resume → remove

mod e2e_helpers;

use e2e_helpers::*;
use predicates::prelude::*;
use tempfile::TempDir;

const REPO_URL: &str = "https://github.com/org/cron-repo";
const REPO_NAME: &str = "org/cron-repo";

// ═══════════════════════════════════════════════
// 1. cron add — interval
// ═══════════════════════════════════════════════

#[test]
fn e2e_cron_add_interval() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "cron",
            "add",
            "--name",
            "my-job",
            "--script",
            "/bin/echo",
            "--interval",
            "300",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("added cron job: my-job"));
}

#[test]
fn e2e_cron_add_schedule() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // Use 7-field cron expression compatible with cron crate
    // Format: sec min hour day_of_month month day_of_week year
    autodev(&home)
        .args([
            "cron",
            "add",
            "--name",
            "sched-job",
            "--script",
            "/bin/echo",
            "--schedule",
            "0 0 * * * * *",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("added cron job: sched-job"));
}

#[test]
fn e2e_cron_add_with_repo() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "cron",
            "add",
            "--name",
            "repo-job",
            "--script",
            "/bin/echo",
            "--repo",
            REPO_NAME,
            "--interval",
            "600",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("added cron job: repo-job"));
}

#[test]
fn e2e_cron_add_requires_schedule_or_interval() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["cron", "add", "--name", "bad-job", "--script", "/bin/echo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--interval").or(predicate::str::contains("--schedule")));
}

#[test]
fn e2e_cron_add_invalid_schedule_fails() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // An invalid cron expression should fail validation
    autodev(&home)
        .args([
            "cron",
            "add",
            "--name",
            "bad-schedule",
            "--script",
            "/bin/echo",
            "--schedule",
            "invalid-cron",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid cron expression"));
}

// ═══════════════════════════════════════════════
// 2. cron list
// ═══════════════════════════════════════════════

#[test]
fn e2e_cron_list_shows_added_job() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "cron",
            "add",
            "--name",
            "list-job",
            "--script",
            "/bin/echo",
            "--interval",
            "60",
        ])
        .assert()
        .success();

    autodev(&home)
        .args(["cron", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list-job"));
}

#[test]
fn e2e_cron_list_json() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "cron",
            "add",
            "--name",
            "json-job",
            "--script",
            "/bin/echo",
            "--interval",
            "60",
        ])
        .assert()
        .success();

    let json = run_json(&home, &["cron", "list", "--json"]);
    let arr = json.as_array().expect("should be array");
    assert!(
        arr.iter().any(|j| j["name"] == "json-job"),
        "should contain json-job"
    );
}

#[test]
fn e2e_cron_list_includes_builtin_jobs() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    // repo add seeds built-in per-repo crons
    autodev(&home)
        .args(["cron", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("claw-evaluate"));
}

// ═══════════════════════════════════════════════
// 3. cron update
// ═══════════════════════════════════════════════

#[test]
fn e2e_cron_update_interval() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "cron",
            "add",
            "--name",
            "upd-job",
            "--script",
            "/bin/echo",
            "--interval",
            "60",
        ])
        .assert()
        .success();

    autodev(&home)
        .args(["cron", "update", "upd-job", "--interval", "120"])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated cron job: upd-job"));
}

#[test]
fn e2e_cron_update_to_schedule() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "cron",
            "add",
            "--name",
            "sched-upd",
            "--script",
            "/bin/echo",
            "--interval",
            "60",
        ])
        .assert()
        .success();

    // Use 7-field cron expression
    autodev(&home)
        .args([
            "cron",
            "update",
            "sched-upd",
            "--schedule",
            "0 */5 * * * * *",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated cron job: sched-upd"));
}

#[test]
fn e2e_cron_update_requires_interval_or_schedule() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "cron",
            "add",
            "--name",
            "no-upd",
            "--script",
            "/bin/echo",
            "--interval",
            "60",
        ])
        .assert()
        .success();

    autodev(&home)
        .args(["cron", "update", "no-upd"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--interval").or(predicate::str::contains("--schedule")));
}

// ═══════════════════════════════════════════════
// 4. cron pause / resume
// ═══════════════════════════════════════════════

#[test]
fn e2e_cron_pause_and_resume() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "cron",
            "add",
            "--name",
            "pausable",
            "--script",
            "/bin/echo",
            "--interval",
            "60",
        ])
        .assert()
        .success();

    autodev(&home)
        .args(["cron", "pause", "pausable"])
        .assert()
        .success()
        .stdout(predicate::str::contains("paused cron job: pausable"));

    // List should show paused status
    autodev(&home)
        .args(["cron", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("○").and(predicate::str::contains("pausable")));

    autodev(&home)
        .args(["cron", "resume", "pausable"])
        .assert()
        .success()
        .stdout(predicate::str::contains("resumed cron job: pausable"));

    autodev(&home)
        .args(["cron", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("●").and(predicate::str::contains("pausable")));
}

// ═══════════════════════════════════════════════
// 5. cron remove
// ═══════════════════════════════════════════════

#[test]
fn e2e_cron_remove() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "cron",
            "add",
            "--name",
            "removable",
            "--script",
            "/bin/echo",
            "--interval",
            "60",
        ])
        .assert()
        .success();

    autodev(&home)
        .args(["cron", "remove", "removable"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed cron job: removable"));

    // Should no longer appear in list
    let json = run_json(&home, &["cron", "list", "--json"]);
    let arr = json.as_array().unwrap();
    assert!(
        !arr.iter().any(|j| j["name"] == "removable"),
        "removed job should not appear"
    );
}

#[test]
fn e2e_cron_remove_nonexistent() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args(["cron", "remove", "ghost-job"])
        .assert()
        .failure();
}

// ═══════════════════════════════════════════════
// 6. cron per-repo scoping
// ═══════════════════════════════════════════════

#[test]
fn e2e_cron_per_repo_pause_resume() {
    let home = TempDir::new().unwrap();
    setup_repo(&home, REPO_URL);

    autodev(&home)
        .args([
            "cron",
            "add",
            "--name",
            "repo-scoped",
            "--script",
            "/bin/echo",
            "--repo",
            REPO_NAME,
            "--interval",
            "300",
        ])
        .assert()
        .success();

    autodev(&home)
        .args(["cron", "pause", "repo-scoped", "--repo", REPO_NAME])
        .assert()
        .success()
        .stdout(predicate::str::contains("paused"));

    autodev(&home)
        .args(["cron", "resume", "repo-scoped", "--repo", REPO_NAME])
        .assert()
        .success()
        .stdout(predicate::str::contains("resumed"));
}
