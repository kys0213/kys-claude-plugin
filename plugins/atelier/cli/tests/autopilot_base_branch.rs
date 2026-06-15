//! Black-box tests for `atelier autopilot base-branch` — the deterministic PR
//! base-branch resolver (`work_branch` > `branch_strategy`) that replaced the
//! duplicated/buggy bash computation in branch-promoter and guard-pr-base (#776).

use assert_cmd::Command;
use std::fs;

fn atelier() -> Command {
    Command::cargo_bin("atelier").expect("locate `atelier` cargo binary")
}

fn project_with_config(body: &str) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    fs::write(dir.path().join("github-autopilot.local.md"), body).unwrap();
    dir
}

fn base_branch_in(dir: &tempfile::TempDir) -> assert_cmd::assert::Assert {
    atelier()
        .args([
            "autopilot",
            "base-branch",
            "--project-dir",
            dir.path().to_str().unwrap(),
        ])
        .assert()
}

#[test]
fn work_branch_wins() {
    let dir = project_with_config(
        "---\nbranch_strategy: \"draft-develop-main\"\nwork_branch: \"alpha\"\n---\n",
    );
    base_branch_in(&dir).success().stdout("alpha\n");
}

#[test]
fn draft_develop_main_resolves_develop() {
    let dir = project_with_config(
        "---\nwork_branch: \"\"\nbranch_strategy: \"draft-develop-main\"\n---\n",
    );
    base_branch_in(&dir).success().stdout("develop\n");
}

#[test]
fn draft_main_resolves_main() {
    let dir = project_with_config("---\nwork_branch: \"\"\nbranch_strategy: \"draft-main\"\n---\n");
    base_branch_in(&dir).success().stdout("main\n");
}

#[test]
fn missing_config_defaults_to_main() {
    // No github-autopilot.local.md → never errors, resolves to main so the
    // branch-promoter `$(...)` consumer is always safe.
    let dir = tempfile::TempDir::new().unwrap();
    base_branch_in(&dir).success().stdout("main\n");
}
