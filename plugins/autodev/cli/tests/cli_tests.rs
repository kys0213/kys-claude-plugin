use assert_cmd::cargo_bin_cmd;
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// AUTODEV_HOME을 tempdir로 설정한 CLI 명령어 실행 헬퍼
fn autodev(home: &TempDir) -> Command {
    let mut cmd = cargo_bin_cmd!("autodev");
    cmd.env("AUTODEV_HOME", home.path());
    cmd
}

// ═══════════════════════════════════════════════
// 1. status
// ═══════════════════════════════════════════════

#[test]
fn status_shows_stopped_when_no_daemon() {
    let home = TempDir::new().unwrap();
    autodev(&home)
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("stopped"));
}

#[test]
fn status_shows_no_repos_initially() {
    let home = TempDir::new().unwrap();
    autodev(&home)
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("no repositories registered"));
}

// ═══════════════════════════════════════════════
// 2. repo add / list / config / remove
// ═══════════════════════════════════════════════

#[test]
fn repo_add_then_list() {
    let home = TempDir::new().unwrap();

    // add
    autodev(&home)
        .args(["repo", "add", "https://github.com/org/myrepo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("registered: org/myrepo"));

    // list
    autodev(&home)
        .args(["repo", "list"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("org/myrepo")
                .and(predicate::str::contains("https://github.com/org/myrepo")),
        );
}

#[test]
fn repo_add_with_git_suffix() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["repo", "add", "https://github.com/org/myrepo.git"])
        .assert()
        .success()
        .stdout(predicate::str::contains("registered: org/myrepo"));
}

#[test]
fn repo_add_duplicate_shows_friendly_error() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["repo", "add", "https://github.com/org/myrepo"])
        .assert()
        .success();

    autodev(&home)
        .args(["repo", "add", "https://github.com/org/myrepo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already registered: org/myrepo"));
}

#[test]
fn repo_add_with_config_writes_yaml() {
    let home = TempDir::new().unwrap();

    let config_json =
        r#"{"sources":{"github":{"gh_host":"ghe.example.com","scan_interval_secs":60}}}"#;

    autodev(&home)
        .args([
            "repo",
            "add",
            "https://github.com/org/myrepo",
            "--config",
            config_json,
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("registered: org/myrepo")
                .and(predicate::str::contains("config: written to")),
        );

    // Verify YAML file was created in workspace
    let ws_dir = home.path().join("workspaces").join("org-myrepo");
    let yaml_path = ws_dir.join(".develop-workflow.yaml");
    assert!(yaml_path.exists(), "config YAML should be created");

    let content = std::fs::read_to_string(&yaml_path).unwrap();
    assert!(content.contains("gh_host"));
    assert!(content.contains("ghe.example.com"));
}

#[test]
fn repo_add_with_invalid_config_json_fails() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args([
            "repo",
            "add",
            "https://github.com/org/myrepo",
            "--config",
            "{invalid",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid config JSON"));
}

#[test]
fn repo_list_empty() {
    let home = TempDir::new().unwrap();
    autodev(&home)
        .args(["repo", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No repositories registered"));
}

#[test]
fn repo_config_shows_defaults() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["repo", "add", "https://github.com/org/myrepo"])
        .assert()
        .success();

    autodev(&home)
        .args(["repo", "config", "org/myrepo"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Effective config for org/myrepo")
                .and(predicate::str::contains("scan_interval_secs")),
        );
}

#[test]
fn repo_remove_then_list_empty() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["repo", "add", "https://github.com/org/myrepo"])
        .assert()
        .success();

    autodev(&home)
        .args(["repo", "remove", "org/myrepo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed: org/myrepo"));

    autodev(&home)
        .args(["repo", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No repositories registered"));
}

// ═══════════════════════════════════════════════
// 3. logs
// ═══════════════════════════════════════════════

#[test]
fn logs_empty() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .arg("logs")
        .assert()
        .success()
        .stdout(predicate::str::contains("No logs found"));
}

// ═══════════════════════════════════════════════
// 4. 잘못된 명령어
// ═══════════════════════════════════════════════

#[test]
fn unknown_command_fails() {
    let home = TempDir::new().unwrap();
    autodev(&home).arg("nonexistent").assert().failure();
}

#[test]
fn no_args_shows_help() {
    let home = TempDir::new().unwrap();
    autodev(&home)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

// ═══════════════════════════════════════════════
// 5. status after repo add (integration)
// ═══════════════════════════════════════════════

#[test]
fn status_shows_repo_after_add() {
    let home = TempDir::new().unwrap();

    autodev(&home)
        .args(["repo", "add", "https://github.com/org/myrepo"])
        .assert()
        .success();

    autodev(&home)
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("org/myrepo"));
}
