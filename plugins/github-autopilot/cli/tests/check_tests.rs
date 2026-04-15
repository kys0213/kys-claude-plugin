mod mock_fs;
mod mock_git;

use std::path::{Path, PathBuf};

use autopilot::cmd::check::spec_code::SpecCodeAnalysis;
use autopilot::cmd::check::state::state_file_path;
use autopilot::cmd::check::{
    CheckService, EXIT_CODE_CHANGED, EXIT_FIRST_RUN, EXIT_NO_CHANGES, EXIT_SPEC_CHANGED,
};
use mock_fs::MockFs;
use mock_git::MockGit;

// --- Test helpers ---

const GAP_WATCH_STATE: &str = "/tmp/autopilot-repo/state/gap-watch.state";
const BUILD_ISSUES_STATE: &str = "/tmp/autopilot-repo/state/build-issues.state";

fn make_svc(git: MockGit, fs: MockFs) -> CheckService {
    CheckService::new(
        Box::new(git),
        Box::new(fs),
        vec![Box::new(SpecCodeAnalysis)],
    )
}

fn state_json(hash: &str) -> String {
    format!(r#"{{"hash":"{hash}","timestamp":"2026-01-01T00:00:00Z"}}"#)
}

fn state_json_with_idle(hash: &str, idle_count: u32) -> String {
    format!(r#"{{"hash":"{hash}","timestamp":"2026-01-01T00:00:00Z","idle_count":{idle_count}}}"#)
}

fn spec_paths() -> Vec<String> {
    vec!["spec/".to_string()]
}

// --- diff tests ---

#[test]
fn diff_returns_first_run_on_missing_state() {
    let git = MockGit::new().with_head("aaa1111");
    let fs = MockFs::new();

    let code = make_svc(git, fs).diff("gap-watch", &spec_paths()).unwrap();
    assert_eq!(code, EXIT_FIRST_RUN);
}

#[test]
fn diff_returns_no_changes_when_same_hash() {
    let git = MockGit::new().with_head("aaa1111").with_commit("aaa1111");
    let fs = MockFs::new().with_file(GAP_WATCH_STATE, &state_json("aaa1111"));

    let code = make_svc(git, fs).diff("gap-watch", &spec_paths()).unwrap();
    assert_eq!(code, EXIT_NO_CHANGES);
}

#[test]
fn diff_returns_spec_changed_when_spec_files_modified() {
    let git = MockGit::new()
        .with_head("bbb2222")
        .with_commit("aaa1111")
        .with_diff("aaa1111", "bbb2222", vec!["spec/auth.md", "src/lib.rs"]);
    let fs = MockFs::new().with_file(GAP_WATCH_STATE, &state_json("aaa1111"));

    let code = make_svc(git, fs).diff("gap-watch", &spec_paths()).unwrap();
    assert_eq!(code, EXIT_SPEC_CHANGED);
}

#[test]
fn diff_returns_code_changed_when_only_code_modified() {
    let git = MockGit::new()
        .with_head("bbb2222")
        .with_commit("aaa1111")
        .with_diff("aaa1111", "bbb2222", vec!["src/lib.rs", "src/main.rs"]);
    let fs = MockFs::new().with_file(GAP_WATCH_STATE, &state_json("aaa1111"));

    let code = make_svc(git, fs).diff("gap-watch", &spec_paths()).unwrap();
    assert_eq!(code, EXIT_CODE_CHANGED);
}

#[test]
fn diff_returns_first_run_when_stale_hash() {
    let git = MockGit::new().with_head("bbb2222");
    // aaa1111 NOT added to existing_commits
    let fs = MockFs::new().with_file(GAP_WATCH_STATE, &state_json("aaa1111"));

    let code = make_svc(git, fs).diff("gap-watch", &spec_paths()).unwrap();
    assert_eq!(code, EXIT_FIRST_RUN);
}

#[test]
fn diff_backward_compat_with_old_state_format() {
    // Old state files without output_history should still work
    let git = MockGit::new()
        .with_head("bbb2222")
        .with_commit("aaa1111")
        .with_diff("aaa1111", "bbb2222", vec!["src/lib.rs"]);
    let fs = MockFs::new().with_file(GAP_WATCH_STATE, &state_json("aaa1111"));

    let code = make_svc(git, fs).diff("gap-watch", &spec_paths()).unwrap();
    assert_eq!(code, EXIT_CODE_CHANGED);
}

// --- mark tests ---

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
fn mark_status_idle_increments_count() {
    let git = MockGit::new().with_head("ccc3333");
    let fs = MockFs::new();

    make_svc(git, fs.clone())
        .mark(
            "build-issues",
            None,
            Some(&autopilot::cmd::LoopStatus::Idle),
        )
        .unwrap();
    let written = fs.written_files();
    let state: serde_json::Value = serde_json::from_str(&written[0].1).unwrap();
    assert_eq!(state["idle_count"], 1);
}

#[test]
fn mark_status_active_resets_count() {
    let git = MockGit::new().with_head("ccc3333");
    let fs = MockFs::new().with_file(BUILD_ISSUES_STATE, &state_json_with_idle("bbb2222", 4));

    make_svc(git, fs.clone())
        .mark(
            "build-issues",
            None,
            Some(&autopilot::cmd::LoopStatus::Active),
        )
        .unwrap();
    let written = fs.written_files();
    let state: serde_json::Value = serde_json::from_str(&written[0].1).unwrap();
    assert_eq!(state["idle_count"], 0);
}

#[test]
fn mark_idle_accumulates_across_calls() {
    let git = MockGit::new().with_head("ccc3333");
    let fs = MockFs::new().with_file(BUILD_ISSUES_STATE, &state_json_with_idle("bbb2222", 3));

    make_svc(git, fs.clone())
        .mark(
            "build-issues",
            None,
            Some(&autopilot::cmd::LoopStatus::Idle),
        )
        .unwrap();
    let written = fs.written_files();
    let state: serde_json::Value = serde_json::from_str(&written[0].1).unwrap();
    assert_eq!(state["idle_count"], 4);
}

#[test]
fn mark_without_status_preserves_idle_count() {
    let git = MockGit::new().with_head("ccc3333");
    let fs = MockFs::new().with_file(BUILD_ISSUES_STATE, &state_json_with_idle("bbb2222", 5));

    make_svc(git, fs.clone())
        .mark("build-issues", None, None)
        .unwrap();
    let written = fs.written_files();
    let state: serde_json::Value = serde_json::from_str(&written[0].1).unwrap();
    assert_eq!(state["idle_count"], 5);
}

#[test]
fn mark_backward_compat_without_idle_count() {
    // Old state files without idle_count should default to 0
    let git = MockGit::new().with_head("ccc3333");
    let fs = MockFs::new().with_file(GAP_WATCH_STATE, &state_json("aaa1111"));

    make_svc(git, fs.clone())
        .mark("gap-watch", None, Some(&autopilot::cmd::LoopStatus::Idle))
        .unwrap();
    let written = fs.written_files();
    let state: serde_json::Value = serde_json::from_str(&written[0].1).unwrap();
    assert_eq!(state["idle_count"], 1);
}

// --- status tests ---

#[test]
fn status_shows_loops() {
    let git = MockGit::new();
    let fs = MockFs::new()
        .with_file(GAP_WATCH_STATE, &state_json("aaa1111"))
        .with_file(BUILD_ISSUES_STATE, &state_json("bbb2222"));

    let code = make_svc(git, fs).status().unwrap();
    assert_eq!(code, 0);
}

// --- reset tests ---

#[test]
fn reset_single_loop_removes_state_file() {
    let git = MockGit::new();
    let fs = MockFs::new().with_file(GAP_WATCH_STATE, &state_json("aaa1111"));

    let code = make_svc(git, fs.clone()).reset(Some("gap-watch")).unwrap();
    assert_eq!(code, 0);

    let removed = fs.removed_files();
    assert_eq!(removed.len(), 1);
    assert!(removed[0].to_str().unwrap().contains("gap-watch.state"));
}

#[test]
fn reset_all_loops_removes_all_state_files() {
    let git = MockGit::new();
    let fs = MockFs::new()
        .with_file(GAP_WATCH_STATE, &state_json("aaa1111"))
        .with_file(BUILD_ISSUES_STATE, &state_json("bbb2222"));

    let code = make_svc(git, fs.clone()).reset(None).unwrap();
    assert_eq!(code, 0);

    let removed = fs.removed_files();
    assert_eq!(removed.len(), 2);
}

#[test]
fn reset_nonexistent_loop_succeeds() {
    let git = MockGit::new();
    let fs = MockFs::new();

    let code = make_svc(git, fs.clone()).reset(Some("gap-watch")).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn reset_rejects_invalid_loop_name() {
    let git = MockGit::new();
    let fs = MockFs::new();

    let result = make_svc(git, fs).reset(Some("../etc/passwd"));
    assert!(result.is_err());
}

#[test]
fn reset_all_with_no_state_directory_succeeds() {
    let git = MockGit::new();
    let fs = MockFs::new();

    let code = make_svc(git, fs).reset(None).unwrap();
    assert_eq!(code, 0);
}

// --- reset → diff integration: the core scenario this feature solves ---

#[test]
fn reset_then_diff_returns_first_run() {
    let git = MockGit::new().with_head("aaa1111").with_commit("aaa1111");
    let fs = MockFs::new().with_file(GAP_WATCH_STATE, &state_json("aaa1111"));

    let svc = make_svc(git, fs.clone());

    // Before reset: diff sees existing state → no_changes
    assert_eq!(
        svc.diff("gap-watch", &spec_paths()).unwrap(),
        EXIT_NO_CHANGES
    );

    // Reset removes the state
    assert_eq!(svc.reset(Some("gap-watch")).unwrap(), 0);

    // After reset: diff sees no state → first_run
    assert_eq!(
        svc.diff("gap-watch", &spec_paths()).unwrap(),
        EXIT_FIRST_RUN
    );
}

#[test]
fn reset_then_mark_then_diff_full_cycle() {
    let git = MockGit::new().with_head("aaa1111").with_commit("aaa1111");
    let fs = MockFs::new().with_file(GAP_WATCH_STATE, &state_json("aaa1111"));

    let svc = make_svc(git, fs);

    assert_eq!(svc.reset(Some("gap-watch")).unwrap(), 0);
    assert_eq!(
        svc.diff("gap-watch", &spec_paths()).unwrap(),
        EXIT_FIRST_RUN
    );
    assert_eq!(svc.mark("gap-watch", None, None).unwrap(), 0);
    assert_eq!(
        svc.diff("gap-watch", &spec_paths()).unwrap(),
        EXIT_NO_CHANGES
    );
}

#[test]
fn reset_all_then_diff_returns_first_run_for_each() {
    let git = MockGit::new().with_head("aaa1111").with_commit("aaa1111");
    let fs = MockFs::new()
        .with_file(GAP_WATCH_STATE, &state_json("aaa1111"))
        .with_file(BUILD_ISSUES_STATE, &state_json("aaa1111"));

    let svc = make_svc(git, fs);

    // Both loops have state → no_changes
    assert_eq!(
        svc.diff("gap-watch", &spec_paths()).unwrap(),
        EXIT_NO_CHANGES
    );
    assert_eq!(
        svc.diff("build-issues", &spec_paths()).unwrap(),
        EXIT_NO_CHANGES
    );

    assert_eq!(svc.reset(None).unwrap(), 0);

    // Both loops now return first_run
    assert_eq!(
        svc.diff("gap-watch", &spec_paths()).unwrap(),
        EXIT_FIRST_RUN
    );
    assert_eq!(
        svc.diff("build-issues", &spec_paths()).unwrap(),
        EXIT_FIRST_RUN
    );
}

#[test]
fn reset_only_affects_target_loop() {
    let git = MockGit::new().with_head("aaa1111").with_commit("aaa1111");
    let fs = MockFs::new()
        .with_file(GAP_WATCH_STATE, &state_json("aaa1111"))
        .with_file(BUILD_ISSUES_STATE, &state_json("aaa1111"));

    let svc = make_svc(git, fs);

    svc.reset(Some("gap-watch")).unwrap();

    assert_eq!(
        svc.diff("gap-watch", &spec_paths()).unwrap(),
        EXIT_FIRST_RUN
    );
    assert_eq!(
        svc.diff("build-issues", &spec_paths()).unwrap(),
        EXIT_NO_CHANGES
    );
}

// --- state_file_path unit tests ---

#[test]
fn state_file_path_constructs_correct_paths() {
    let dir = Path::new("/tmp/autopilot-repo/state");
    assert_eq!(
        state_file_path(dir, "gap-watch"),
        PathBuf::from("/tmp/autopilot-repo/state/gap-watch.state")
    );
    assert_eq!(
        state_file_path(dir, "build-issues"),
        PathBuf::from("/tmp/autopilot-repo/state/build-issues.state")
    );

    let other_dir = Path::new("/tmp/autopilot-myrepo/state");
    assert_eq!(
        state_file_path(other_dir, "ci-watch"),
        PathBuf::from("/tmp/autopilot-myrepo/state/ci-watch.state")
    );
}
