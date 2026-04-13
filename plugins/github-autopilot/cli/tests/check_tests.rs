mod mock_fs;
mod mock_git;

use autopilot::cmd::check::spec_code::SpecCodeAnalysis;
use autopilot::cmd::check::CheckService;
use mock_fs::MockFs;
use mock_git::MockGit;

fn make_svc(git: MockGit, fs: MockFs) -> CheckService {
    CheckService::new(
        Box::new(git),
        Box::new(fs),
        vec![Box::new(SpecCodeAnalysis)],
    )
}

#[test]
fn diff_returns_3_on_first_run() {
    let git = MockGit::new().with_head("aaa1111");
    let fs = MockFs::new(); // no state file

    let code = make_svc(git, fs)
        .diff("gap-watch", &["spec/".to_string()])
        .unwrap();
    assert_eq!(code, 3);
}

#[test]
fn diff_returns_0_when_same_hash() {
    let git = MockGit::new().with_head("aaa1111").with_commit("aaa1111");
    let fs = MockFs::new().with_file(
        "/tmp/autopilot-repo/state/gap-watch.state",
        r#"{"hash":"aaa1111","timestamp":"2026-01-01T00:00:00Z"}"#,
    );

    let code = make_svc(git, fs)
        .diff("gap-watch", &["spec/".to_string()])
        .unwrap();
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

    let code = make_svc(git, fs)
        .diff("gap-watch", &["spec/".to_string()])
        .unwrap();
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

    let code = make_svc(git, fs)
        .diff("gap-watch", &["spec/".to_string()])
        .unwrap();
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

    let code = make_svc(git, fs)
        .diff("gap-watch", &["spec/".to_string()])
        .unwrap();
    assert_eq!(code, 3);
}

#[test]
fn mark_writes_state_file() {
    let git = MockGit::new().with_head("ccc3333");
    let fs = MockFs::new();

    let code = make_svc(git, fs.clone())
        .mark("build-issues", None, None)
        .unwrap();
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
fn mark_with_output_hash_appends_history() {
    let git = MockGit::new().with_head("ccc3333");
    let fs = MockFs::new();

    let code = make_svc(git, fs.clone())
        .mark("gap-watch", Some("0xA3F2B81C4D5E6F1B"), None)
        .unwrap();
    assert_eq!(code, 0);

    let written = fs.written_files();
    let state: serde_json::Value = serde_json::from_str(&written[0].1).unwrap();
    assert_eq!(state["output_history"][0]["simhash"], "0xA3F2B81C4D5E6F1B");
    assert_eq!(state["output_history"][0]["category"], "gap-analysis");
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

    let code = make_svc(git, fs).status().unwrap();
    assert_eq!(code, 0);
}

#[test]
fn mark_status_idle_increments_count() {
    let git = MockGit::new().with_head("ccc3333");
    let fs = MockFs::new();

    make_svc(git, fs.clone())
        .mark("build-issues", None, Some("idle"))
        .unwrap();
    let written = fs.written_files();
    let state: serde_json::Value = serde_json::from_str(&written[0].1).unwrap();
    assert_eq!(state["idle_count"], 1);
}

#[test]
fn mark_status_active_resets_count() {
    let git = MockGit::new().with_head("ccc3333");
    let fs = MockFs::new().with_file(
        "/tmp/autopilot-repo/state/build-issues.state",
        r#"{"hash":"bbb2222","timestamp":"2026-01-01T00:00:00Z","idle_count":4}"#,
    );

    make_svc(git, fs.clone())
        .mark("build-issues", None, Some("active"))
        .unwrap();
    let written = fs.written_files();
    let state: serde_json::Value = serde_json::from_str(&written[0].1).unwrap();
    assert_eq!(state["idle_count"], 0);
}

#[test]
fn mark_backward_compat_without_idle_count() {
    // Old state files without idle_count should default to 0
    let git = MockGit::new().with_head("ccc3333");
    let fs = MockFs::new().with_file(
        "/tmp/autopilot-repo/state/gap-watch.state",
        r#"{"hash":"aaa1111","timestamp":"2026-01-01T00:00:00Z"}"#,
    );

    make_svc(git, fs.clone())
        .mark("gap-watch", None, Some("idle"))
        .unwrap();
    let written = fs.written_files();
    let state: serde_json::Value = serde_json::from_str(&written[0].1).unwrap();
    assert_eq!(state["idle_count"], 1);
}

#[test]
fn diff_backward_compat_with_old_state_format() {
    // Old state files without output_history should still work
    let git = MockGit::new()
        .with_head("bbb2222")
        .with_commit("aaa1111")
        .with_diff("aaa1111", "bbb2222", vec!["src/lib.rs"]);
    let fs = MockFs::new().with_file(
        "/tmp/autopilot-repo/state/gap-watch.state",
        r#"{"hash":"aaa1111","timestamp":"2026-01-01T00:00:00Z"}"#,
    );

    let code = make_svc(git, fs)
        .diff("gap-watch", &["spec/".to_string()])
        .unwrap();
    assert_eq!(code, 2);
}
