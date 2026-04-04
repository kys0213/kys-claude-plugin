mod mock_fs;
mod mock_git;

use mock_fs::MockFs;
use mock_git::MockGit;

#[test]
fn diff_returns_3_on_first_run() {
    let git = MockGit::new().with_head("aaa1111");
    let fs = MockFs::new(); // no state file

    let code = autopilot::cmd::check::diff(&git, &fs, "gap-watch", &["spec/".to_string()]).unwrap();
    assert_eq!(code, 3);
}

#[test]
fn diff_returns_0_when_same_hash() {
    let git = MockGit::new().with_head("aaa1111").with_commit("aaa1111");
    let fs = MockFs::new().with_file(
        "/tmp/autopilot-repo/state/gap-watch.state",
        r#"{"hash":"aaa1111","timestamp":"2026-01-01T00:00:00Z"}"#,
    );

    let code = autopilot::cmd::check::diff(&git, &fs, "gap-watch", &["spec/".to_string()]).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn diff_returns_1_when_spec_changed() {
    let git = MockGit::new()
        .with_head("bbb2222")
        .with_commit("aaa1111")
        .with_diff("aaa1111", "bbb2222", vec!["spec/auth.md", "src/lib.rs"]);
    let fs = MockFs::new().with_file(
        "/tmp/autopilot-repo/state/gap-watch.state",
        r#"{"hash":"aaa1111","timestamp":"2026-01-01T00:00:00Z"}"#,
    );

    let code = autopilot::cmd::check::diff(&git, &fs, "gap-watch", &["spec/".to_string()]).unwrap();
    assert_eq!(code, 1);
}

#[test]
fn diff_returns_2_when_only_code_changed() {
    let git = MockGit::new()
        .with_head("bbb2222")
        .with_commit("aaa1111")
        .with_diff("aaa1111", "bbb2222", vec!["src/lib.rs", "src/main.rs"]);
    let fs = MockFs::new().with_file(
        "/tmp/autopilot-repo/state/gap-watch.state",
        r#"{"hash":"aaa1111","timestamp":"2026-01-01T00:00:00Z"}"#,
    );

    let code = autopilot::cmd::check::diff(&git, &fs, "gap-watch", &["spec/".to_string()]).unwrap();
    assert_eq!(code, 2);
}

#[test]
fn diff_returns_3_when_stale_hash() {
    let git = MockGit::new().with_head("bbb2222");
    // aaa1111 NOT added to existing_commits
    let fs = MockFs::new().with_file(
        "/tmp/autopilot-repo/state/gap-watch.state",
        r#"{"hash":"aaa1111","timestamp":"2026-01-01T00:00:00Z"}"#,
    );

    let code = autopilot::cmd::check::diff(&git, &fs, "gap-watch", &["spec/".to_string()]).unwrap();
    assert_eq!(code, 3);
}

#[test]
fn mark_writes_state_file() {
    let git = MockGit::new().with_head("ccc3333");
    let fs = MockFs::new();

    let code = autopilot::cmd::check::mark(&git, &fs, "build-issues").unwrap();
    assert_eq!(code, 0);

    let written = fs.written_files();
    assert_eq!(written.len(), 1);
    assert!(written[0]
        .0
        .to_str()
        .unwrap()
        .contains("build-issues.state"));

    let state: serde_json::Value = serde_json::from_str(&written[0].1).unwrap();
    assert_eq!(state["hash"], "ccc3333");
    assert!(state["timestamp"].is_string());
}

#[test]
fn status_shows_loops() {
    let git = MockGit::new();
    let fs = MockFs::new()
        .with_file(
            "/tmp/autopilot-repo/state/gap-watch.state",
            r#"{"hash":"aaa1111","timestamp":"2026-01-01T00:00:00Z"}"#,
        )
        .with_file(
            "/tmp/autopilot-repo/state/build-issues.state",
            r#"{"hash":"bbb2222","timestamp":"2026-01-02T00:00:00Z"}"#,
        );

    let code = autopilot::cmd::check::status(&git, &fs).unwrap();
    assert_eq!(code, 0);
}
