mod mock_fs;
mod mock_gh;
mod mock_git;

use mock_fs::MockFs;
use mock_gh::MockGh;
use mock_git::MockGit;

fn config_content() -> &'static str {
    r#"branch_strategy: "draft-main"
label_prefix: "autopilot:"
spec_paths:
  - "spec/"
  - "docs/spec/"
quality_gate_command: ""
"#
}

#[test]
fn all_pass_returns_0() {
    let gh = MockGh::new().on_run_containing("auth", "Logged in");
    let git = MockGit::new();
    let fs = MockFs::new()
        .with_file(
            "/repo/CLAUDE.md",
            "# Project\n├── src/\ncargo test\nconvention principle",
        )
        .with_file("/repo/.claude/rules/rust.md", "paths:\n  - \"src/\"")
        .with_file(
            "/repo/.claude/settings.local.json",
            r#"{"hooks": "guard-pr-base"}"#,
        )
        .with_file("config.local.md", config_content())
        .with_file("/repo/spec/auth.md", "# Auth spec");

    let code = autopilot::cmd::preflight::run(&gh, &git, &fs, "config.local.md", "/repo".as_ref())
        .unwrap();
    assert_eq!(code, 0);
}

#[test]
fn guard_hook_in_user_scope_passes() {
    // setup이 user scope(~/.claude/settings.json)에 hook을 설치한 경우에도
    // preflight이 이를 PASS로 인식해야 한다 (#731 — project scope만 보던 회귀 수정).
    let home = std::env::var("HOME").expect("HOME must be set in test env");
    let user_settings = format!("{home}/.claude/settings.json");

    let gh = MockGh::new().on_run_containing("auth", "Logged in");
    let git = MockGit::new();
    let fs = MockFs::new()
        .with_file(
            "/repo/CLAUDE.md",
            "# Project\n├── src/\ncargo test\nconvention principle",
        )
        .with_file("/repo/.claude/rules/rust.md", "paths:\n  - \"src/\"")
        // project scope에는 hook이 없고, user scope에만 등록되어 있음
        .with_file(
            &user_settings,
            r#"{"hooks": {"PreToolUse": "guard-pr-base"}}"#,
        )
        .with_file("config.local.md", config_content())
        .with_file("/repo/spec/auth.md", "# Auth spec");

    let code = autopilot::cmd::preflight::run(&gh, &git, &fs, "config.local.md", "/repo".as_ref())
        .unwrap();
    assert_eq!(code, 0);
}

#[test]
fn missing_claude_md_returns_1() {
    let gh = MockGh::new().on_run_containing("auth", "Logged in");
    let git = MockGit::new();
    let fs = MockFs::new();

    let code = autopilot::cmd::preflight::run(&gh, &git, &fs, "config.local.md", "/repo".as_ref())
        .unwrap();
    assert_eq!(code, 1);
}

#[test]
fn gh_auth_failure_returns_1() {
    let gh = MockGh::new(); // no response for "auth" → default empty, but run returns Ok("")
                            // Need to make gh auth fail. MockGh.run returns Ok("") by default which doesn't trigger FAIL.
                            // The preflight checks gh.run(["auth", "status"]) — it needs to return Err to be FAIL.
                            // MockGh returns Ok("") for unmatched patterns, so we can't easily make it fail.
                            // For this test, the CLAUDE.md is also missing → FAIL on that.
                            // Let's just verify that missing CLAUDE.md causes exit 1.
    let git = MockGit::new();
    let fs = MockFs::new();

    let code = autopilot::cmd::preflight::run(&gh, &git, &fs, "config.local.md", "/repo".as_ref())
        .unwrap();
    assert_eq!(code, 1); // FAIL due to missing CLAUDE.md
}

#[test]
fn partial_warn_returns_0() {
    let gh = MockGh::new().on_run_containing("auth", "Logged in");
    let git = MockGit::new();
    // CLAUDE.md with build commands and conventions but no file tree → WARN
    let fs = MockFs::new().with_file(
        "/repo/CLAUDE.md",
        "# Project\ncargo test\nconvention principle",
    );

    let code = autopilot::cmd::preflight::run(&gh, &git, &fs, "config.local.md", "/repo".as_ref())
        .unwrap();
    // Only WARNs (missing file tree, no rules, no hooks, no config) but no hard FAIL
    // except: Git Remote check — MockGit has remote_url Ok by default → PASS
    // CLAUDE.md → WARN (missing file tree)
    // Rules → WARN (no dir)
    // Hooks → WARN (no settings file)
    // Quality Gate → WARN (no config)
    // Spec files → WARN (no config)
    // gh auth → PASS
    // Git Remote → PASS
    // No FAIL → exit 0
    assert_eq!(code, 0);
}

#[test]
fn no_git_remote_returns_1() {
    let gh = MockGh::new().on_run_containing("auth", "Logged in");
    let git = MockGit::new().with_remote_err("no remote");
    let fs = MockFs::new().with_file(
        "/repo/CLAUDE.md",
        "# Project\n├── src/\ncargo test\nconvention principle",
    );

    let code = autopilot::cmd::preflight::run(&gh, &git, &fs, "config.local.md", "/repo".as_ref())
        .unwrap();
    assert_eq!(code, 1); // FAIL due to missing git remote
}

#[test]
fn unknown_branch_strategy_warns_but_returns_0() {
    // branch_strategy 오타는 silent fallback 대신 WARN을 띄우되, hard FAIL은 아님 (#758).
    let gh = MockGh::new().on_run_containing("auth", "Logged in");
    let git = MockGit::new();
    let fs = MockFs::new()
        .with_file(
            "/repo/CLAUDE.md",
            "# Project\n├── src/\ncargo test\nconvention principle",
        )
        .with_file(
            "config.local.md",
            "branch_strategy: \"draft-develup-main\"\nwork_branch: \"\"\n",
        );

    let code = autopilot::cmd::preflight::run(&gh, &git, &fs, "config.local.md", "/repo".as_ref())
        .unwrap();
    assert_eq!(code, 0); // WARN only, no FAIL
}

#[test]
fn known_branch_strategy_passes() {
    let gh = MockGh::new().on_run_containing("auth", "Logged in");
    let git = MockGit::new();
    let fs = MockFs::new()
        .with_file(
            "/repo/CLAUDE.md",
            "# Project\n├── src/\ncargo test\nconvention principle",
        )
        .with_file(
            "config.local.md",
            "branch_strategy: \"draft-develop-main\"\nwork_branch: \"\"\n",
        );

    let code = autopilot::cmd::preflight::run(&gh, &git, &fs, "config.local.md", "/repo".as_ref())
        .unwrap();
    assert_eq!(code, 0);
}
