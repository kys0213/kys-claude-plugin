//! Port of `git-utils/tests/core/guard.test.ts` — mock-git unit tests plus the
//! `is_inside_project_dir` / `is_inside_any_git_repo` path helpers.
#![allow(clippy::field_reassign_with_default)]

mod git_mocks;

use atelier::git::core::guard::{
    create_guard_service, find_repo_root, is_inside_any_git_repo, is_inside_project_dir,
    GuardService,
};
use atelier::git::types::{DefaultBranchSource, ProtectionRule};
use atelier::git::types::{GuardInput, GuardOutput, GuardTarget, GuardVerdict, OpaqueExecPolicy};
use git_mocks::{MockGit, MockGitFactory};
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

fn base_input() -> GuardInput {
    GuardInput {
        target: GuardTarget::Write { file_path: None },
        project_dir: "/tmp/test".to_string(),
        create_branch_script: "git switch -c".to_string(),
        default_branch: None,
        protected_branches: None,
        opaque_exec: OpaqueExecPolicy::Ask,
        home: None,
    }
}

/// Judges with a factory that serves no other repository: reaching one panics.
fn check(git: MockGit, input: &GuardInput) -> atelier::git::types::GuardOutput {
    check_with(git, MockGitFactory::new(), input)
}

fn check_with(
    git: MockGit,
    factory: MockGitFactory,
    input: &GuardInput,
) -> atelier::git::types::GuardOutput {
    let guard = create_guard_service(&git, &factory);
    guard.check(input)
}

#[test]
fn not_a_git_repo_passes() {
    let mut git = MockGit::default();
    git.is_inside_work_tree = Box::new(|| false);
    assert_eq!(check(git, &base_input()).verdict, GuardVerdict::Allow);
}

#[test]
fn rebase_passes() {
    let mut git = MockGit::default();
    git.special_state_flags = Box::new(|| (true, false));
    assert_eq!(check(git, &base_input()).verdict, GuardVerdict::Allow);
}

#[test]
fn merge_passes() {
    let mut git = MockGit::default();
    git.special_state_flags = Box::new(|| (false, true));
    assert_eq!(check(git, &base_input()).verdict, GuardVerdict::Allow);
}

#[test]
fn detached_passes() {
    // Detached HEAD: `git branch --show-current` prints nothing.
    let mut git = MockGit::default();
    git.current_branch = Box::new(String::new);
    assert_eq!(check(git, &base_input()).verdict, GuardVerdict::Allow);
}

#[test]
fn non_default_branch_passes() {
    let mut git = MockGit::default();
    git.current_branch = Box::new(|| "feat/something".to_string());
    assert_eq!(check(git, &base_input()).verdict, GuardVerdict::Allow);
}

#[test]
fn default_branch_main_blocked() {
    assert_eq!(
        check(MockGit::default(), &base_input()).verdict,
        GuardVerdict::Block
    );
}

#[test]
fn default_branch_master_blocked() {
    let mut git = MockGit::default();
    git.current_branch = Box::new(|| "master".to_string());
    git.detect_default_branch = Box::new(|| Ok("master".to_string()));
    assert_eq!(check(git, &base_input()).verdict, GuardVerdict::Block);
}

#[test]
fn default_branch_develop_blocked() {
    let mut git = MockGit::default();
    git.current_branch = Box::new(|| "develop".to_string());
    git.detect_default_branch = Box::new(|| Ok("develop".to_string()));
    assert_eq!(check(git, &base_input()).verdict, GuardVerdict::Block);
}

#[test]
fn empty_default_branch_falls_back_to_detection() {
    // A setup that detected nothing must not bake `""` as the protected branch —
    // empty is treated as absence, so the guard falls back to readonly detection
    // and still blocks the real default (MockGit default: current + detect = main).
    let mut input = base_input();
    input.default_branch = Some(String::new());
    assert_eq!(
        check(MockGit::default(), &input).verdict,
        GuardVerdict::Block,
        "empty --default-branch must not bypass protection on the real default branch"
    );
}

#[test]
fn develop_protected_even_when_default_is_main() {
    let mut git = MockGit::default();
    git.current_branch = Box::new(|| "develop".to_string());
    git.detect_default_branch = Box::new(|| Ok("main".to_string()));
    let out = check(git, &base_input());
    assert_eq!(out.verdict, GuardVerdict::Block);
    assert_eq!(out.current_branch.as_deref(), Some("develop"));
    assert_eq!(out.default_branch.as_deref(), Some("main"));
}

#[test]
fn extra_protected_branches_blocked() {
    let mut git = MockGit::default();
    git.current_branch = Box::new(|| "staging".to_string());
    git.detect_default_branch = Box::new(|| Ok("main".to_string()));
    let mut input = base_input();
    input.protected_branches = Some(vec!["staging".to_string(), "release".to_string()]);
    let out = check(git, &input);
    assert_eq!(out.verdict, GuardVerdict::Block);
    assert_eq!(out.current_branch.as_deref(), Some("staging"));
}

#[test]
fn branch_not_in_protected_passes() {
    let mut git = MockGit::default();
    git.current_branch = Box::new(|| "feat/something".to_string());
    git.detect_default_branch = Box::new(|| Ok("main".to_string()));
    let mut input = base_input();
    input.protected_branches = Some(vec!["staging".to_string()]);
    assert_eq!(check(git, &input).verdict, GuardVerdict::Allow);
}

#[test]
fn explicit_default_branch_used() {
    let mut git = MockGit::default();
    git.current_branch = Box::new(|| "custom-default".to_string());
    let mut input = base_input();
    input.default_branch = Some("custom-default".to_string());
    let out = check(git, &input);
    assert_eq!(out.verdict, GuardVerdict::Block);
    assert_eq!(out.default_branch.as_deref(), Some("custom-default"));
}

#[test]
fn detect_failure_passes_safe_mode() {
    let mut git = MockGit::default();
    git.detect_default_branch = Box::new(|| Err("no remote".to_string()));
    assert_eq!(check(git, &base_input()).verdict, GuardVerdict::Allow);
}

#[test]
fn write_block_reason_mentions_action_and_script() {
    let out = check(MockGit::default(), &base_input());
    let reason = out.reason.unwrap();
    assert!(reason.contains("파일을 수정하려 합니다"));
    assert!(reason.contains("git switch -c"));
}

#[test]
fn block_reason_uses_default_script_when_empty() {
    // No --create-branch-script supplied (CLI forwards an empty string): the
    // guard must fall back to its own default, not render a bare `  <branch>`.
    let mut input = base_input();
    input.create_branch_script = String::new();
    let reason = check(MockGit::default(), &input).reason.unwrap();
    assert!(reason.contains(atelier::git::core::guard::DEFAULT_CREATE_BRANCH_SCRIPT));
}

#[test]
fn commit_target_not_git_commit_passes() {
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        cwd: None,
        command: Some("git push origin main".to_string()),
    };
    assert_eq!(
        check(MockGit::default(), &input).verdict,
        GuardVerdict::Allow
    );
}

#[test]
fn commit_target_git_commit_on_default_blocked() {
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        cwd: None,
        command: Some("git commit -m \"test\"".to_string()),
    };
    assert_eq!(
        check(MockGit::default(), &input).verdict,
        GuardVerdict::Block
    );
}

#[test]
fn commit_target_compound_command_blocked() {
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        cwd: None,
        command: Some("git add . && git commit -m \"test\"".to_string()),
    };
    assert_eq!(
        check(MockGit::default(), &input).verdict,
        GuardVerdict::Block
    );
}

#[test]
fn commit_target_git_log_passes() {
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        cwd: None,
        command: Some("git log --oneline".to_string()),
    };
    assert_eq!(
        check(MockGit::default(), &input).verdict,
        GuardVerdict::Allow
    );
}

#[test]
fn commit_target_empty_command_passes() {
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        cwd: None,
        command: Some(String::new()),
    };
    assert_eq!(
        check(MockGit::default(), &input).verdict,
        GuardVerdict::Allow
    );
}

#[test]
fn commit_target_no_command_passes() {
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        command: None,
        cwd: None,
    };
    assert_eq!(
        check(MockGit::default(), &input).verdict,
        GuardVerdict::Allow
    );
}

// ---- #754: "git commit" inside quoted text must not match ----

#[test]
fn commit_target_double_quoted_text_passes() {
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        cwd: None,
        command: Some(r#"gh issue create --body "remember to git commit often""#.to_string()),
    };
    let out = check(MockGit::default(), &input);
    assert_eq!(out.verdict, GuardVerdict::Allow);
    assert_eq!(
        out.reason.as_deref(),
        Some("no repository-changing command")
    );
}

#[test]
fn commit_target_single_quoted_text_passes() {
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        cwd: None,
        command: Some("gh pr comment 1 --body 'please git commit first'".to_string()),
    };
    assert_eq!(
        check(MockGit::default(), &input).verdict,
        GuardVerdict::Allow
    );
}

#[test]
fn commit_target_real_commit_with_quoted_message_blocked() {
    // The quoted message is stripped, but `git ... commit` stays outside the
    // quotes, so a real commit is still matched.
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        cwd: None,
        command: Some(r#"git commit -m "this is not a git commit hint""#.to_string()),
    };
    assert_eq!(
        check(MockGit::default(), &input).verdict,
        GuardVerdict::Block
    );
}

#[test]
fn commit_target_escaped_quotes_are_literal_text_and_pass() {
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        cwd: None,
        command: Some(r#"echo \"git commit\""#.to_string()),
    };
    assert_eq!(
        check(MockGit::default(), &input).verdict,
        GuardVerdict::Allow
    );
}

#[test]
fn block_reason_contains_branch_name() {
    let out = check(MockGit::default(), &base_input());
    let reason = out.reason.unwrap();
    assert!(reason.contains("main"));
    assert!(reason.contains("보호 브랜치"));
    assert_eq!(out.current_branch.as_deref(), Some("main"));
}

#[test]
fn outside_project_and_outside_git_repo_passes() {
    let mut input = base_input();
    input.project_dir = "/home/user/my-project".to_string();
    input.target = GuardTarget::Write {
        file_path: Some("/home/user/.claude/settings.json".to_string()),
    };
    let out = check(MockGit::default(), &input);
    assert_eq!(out.verdict, GuardVerdict::Allow);
    assert_eq!(
        out.reason.as_deref(),
        Some("file is outside any git repository")
    );
}

#[test]
fn write_in_other_repo_on_feature_branch_is_allowed() {
    let project = repo_dir();
    let other = repo_dir();
    let input = write_input(project.path(), &other.path().join("src/a.rs"));
    let factory = MockGitFactory::new().with(other.path(), || on_branch("feat/x"));
    let out = check_with(on_branch("main"), factory, &input);
    assert_eq!(out.verdict, GuardVerdict::Allow);
}

#[test]
fn write_in_other_repo_on_its_protected_branch_blocks_naming_that_root() {
    let project = repo_dir();
    let other = repo_dir();
    let input = write_input(project.path(), &other.path().join("src/a.rs"));
    let factory = MockGitFactory::new().with(other.path(), || on_branch("main"));
    let out = check_with(on_branch("feat/x"), factory, &input);
    assert_eq!(out.verdict, GuardVerdict::Block);
    let other_root = other.path().display().to_string();
    assert_eq!(out.repo_root.as_deref(), Some(other_root.as_str()));
    let reason = out.reason.unwrap();
    assert!(
        reason.contains(&format!("대상 저장소: {other_root}")),
        "{reason}"
    );
}

#[test]
fn inside_project_on_default_blocked() {
    let mut input = base_input();
    input.project_dir = "/home/user/my-project".to_string();
    input.target = GuardTarget::Write {
        file_path: Some("/home/user/my-project/src/index.ts".to_string()),
    };
    assert_eq!(
        check(MockGit::default(), &input).verdict,
        GuardVerdict::Block
    );
}

#[test]
fn inside_project_on_feature_passes() {
    let mut git = MockGit::default();
    git.current_branch = Box::new(|| "feat/something".to_string());
    let mut input = base_input();
    input.project_dir = "/home/user/my-project".to_string();
    input.target = GuardTarget::Write {
        file_path: Some("/home/user/my-project/src/index.ts".to_string()),
    };
    assert_eq!(check(git, &input).verdict, GuardVerdict::Allow);
}

#[test]
fn no_tool_file_path_runs_default_guard() {
    assert_eq!(
        check(MockGit::default(), &base_input()).verdict,
        GuardVerdict::Block
    );
}

// ---- bash classification ----

fn on_branch(branch: &str) -> MockGit {
    let branch = branch.to_string();
    let mut git = MockGit::default();
    git.current_branch = Box::new(move || branch.clone());
    git.detect_default_branch = Box::new(|| Ok("main".to_string()));
    git
}

/// Every git read panics: reaching git at all fails the test.
fn panicking_git() -> MockGit {
    let mut git = MockGit::default();
    git.is_inside_work_tree = Box::new(|| panic!("git must not be consulted"));
    git.current_branch = Box::new(|| panic!("git must not be consulted"));
    git.detect_default_branch = Box::new(|| panic!("git must not be consulted"));
    git.special_state_flags = Box::new(|| panic!("git must not be consulted"));
    git.upstream_divergence = Box::new(|| panic!("git must not be consulted"));
    git
}

/// A git on `branch` that counts every read it serves.
fn counting_git(branch: &str) -> (MockGit, Rc<Cell<usize>>) {
    let calls = Rc::new(Cell::new(0));
    let tick = |calls: &Rc<Cell<usize>>| {
        let calls = calls.clone();
        move || calls.set(calls.get() + 1)
    };
    let branch = branch.to_string();
    let mut git = MockGit::default();
    let t = tick(&calls);
    git.is_inside_work_tree = Box::new(move || {
        t();
        true
    });
    let t = tick(&calls);
    git.current_branch = Box::new(move || {
        t();
        branch.clone()
    });
    let t = tick(&calls);
    git.detect_default_branch = Box::new(move || {
        t();
        Ok("main".to_string())
    });
    let t = tick(&calls);
    git.special_state_flags = Box::new(move || {
        t();
        (false, false)
    });
    (git, calls)
}

fn repo_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    dir
}

fn bash_input(project: &Path, command: &str, cwd: Option<&Path>) -> GuardInput {
    let mut input = base_input();
    input.project_dir = project.to_string_lossy().to_string();
    input.target = GuardTarget::Commit {
        command: Some(command.to_string()),
        cwd: cwd.map(|c| c.to_string_lossy().to_string()),
    };
    input
}

fn judge(branch: &str, command: &str) -> GuardOutput {
    let repo = repo_dir();
    check(on_branch(branch), &bash_input(repo.path(), command, None))
}

fn assert_allowed_without_git(project: &Path, command: &str, cwd: Option<&Path>) {
    let (git, calls) = counting_git("main");
    let out = check(git, &bash_input(project, command, cwd));
    assert_eq!(out.verdict, GuardVerdict::Allow, "command: {command:?}");
    assert_eq!(calls.get(), 0, "git consulted for {command:?}");
}

#[test]
fn hook_bypass_blocks_without_consulting_git() {
    let repo = repo_dir();
    for cmd in [
        "git commit --no-verify -m x",
        "git push --no-verify",
        "git commit -an -m x",
        "git -c core.hooksPath=/dev/null commit -m x",
        "HUSKY=0 git commit -m x",
    ] {
        let out = check(panicking_git(), &bash_input(repo.path(), cmd, None));
        assert_eq!(out.verdict, GuardVerdict::Block, "command: {cmd:?}");
        let reason = out.reason.unwrap();
        assert!(
            reason.starts_with("[Hook Guard] git hook 우회("),
            "{reason}"
        );
        assert!(reason.contains("프로젝트 정책상 금지입니다(브랜치 무관)"));
    }
}

#[test]
fn hook_bypass_names_rule_and_token() {
    let reason = judge("feat/x", "git commit --no-verify -m x")
        .reason
        .unwrap();
    assert!(reason.contains("--no-verify"), "{reason}");
}

#[test]
fn hook_bypass_blocks_during_rebase_and_outside_any_repo() {
    let repo = repo_dir();
    let outside = tempfile::tempdir().unwrap();
    let mut rebasing = on_branch("main");
    rebasing.special_state_flags = Box::new(|| (true, false));
    let out = check(
        rebasing,
        &bash_input(repo.path(), "git commit --no-verify", None),
    );
    assert_eq!(out.verdict, GuardVerdict::Block);
    let out = check(
        panicking_git(),
        &bash_input(repo.path(), "git commit --no-verify", Some(outside.path())),
    );
    assert_eq!(out.verdict, GuardVerdict::Block);
}

#[test]
fn hook_bypass_lookalikes_are_not_blocked() {
    for cmd in [
        "git log -n 5",
        "git push -n",
        "git commit -m \"--no-verify\"",
    ] {
        assert_ne!(
            judge("feat/x", cmd).verdict,
            GuardVerdict::Block,
            "command: {cmd:?}"
        );
    }
}

#[test]
fn harmless_commands_are_allowed_with_zero_git_calls() {
    let repo = repo_dir();
    for cmd in [
        "ls",
        "cat a | grep b",
        "git status",
        "git log -n 5",
        "git push -n",
        "git push origin main",
        "cargo test",
        "npm test",
        "npx tsc",
        "python -m pytest",
        "node --test",
        "echo hi > /dev/null",
        "gh issue create --body \"remember to git commit\"",
    ] {
        assert_allowed_without_git(repo.path(), cmd, None);
    }
}

#[test]
fn harmless_command_never_touches_a_panicking_git() {
    let repo = repo_dir();
    let out = check(panicking_git(), &bash_input(repo.path(), "ls -la", None));
    assert_eq!(out.verdict, GuardVerdict::Allow);
}

#[test]
fn writes_and_commits_on_protected_branches_are_blocked() {
    for branch in ["main", "develop"] {
        for cmd in [
            "git commit -m x",
            "sed -i s/a/b/ f",
            "sed -i '' s/a/b/ f",
            "echo x > f",
            "cat <<EOF > f\nx\nEOF",
            "git stash pop",
            "git apply fix.patch",
            "rm -rf build",
            "cp a b",
            "tee out.txt",
        ] {
            let out = judge(branch, cmd);
            assert_eq!(out.verdict, GuardVerdict::Block, "{branch}: {cmd:?}");
            let reason = out.reason.unwrap();
            assert!(reason.contains(branch), "{reason}");
            assert!(reason.contains("git switch -c <branch-name>"), "{reason}");
        }
    }
}

#[test]
fn writes_and_commits_on_feature_branch_are_allowed() {
    for cmd in [
        "git commit -m x",
        "sed -i s/a/b/ f",
        "echo x > f",
        "node x.js",
    ] {
        assert_eq!(
            judge("feat/x", cmd).verdict,
            GuardVerdict::Allow,
            "command: {cmd:?}"
        );
    }
}

#[test]
fn commit_message_heredoc_on_protected_branch_is_blocked() {
    let cmd = "git commit -m \"$(cat <<'EOF'\nfix: don't break it's flow\nEOF\n)\"";
    assert_eq!(judge("main", cmd).verdict, GuardVerdict::Block);
    assert_eq!(judge("feat/x", cmd).verdict, GuardVerdict::Allow);
}

#[test]
fn commit_during_rebase_still_passes() {
    let repo = repo_dir();
    let mut git = on_branch("main");
    git.special_state_flags = Box::new(|| (true, false));
    let out = check(git, &bash_input(repo.path(), "git commit -m x", None));
    assert_eq!(out.verdict, GuardVerdict::Allow);
}

#[test]
fn script_execution_on_protected_branch_asks_by_default() {
    for cmd in ["node x.js", "python3 -c 'print(1)'", "node -e 'x'"] {
        let out = judge("main", cmd);
        assert_eq!(out.verdict, GuardVerdict::Ask, "command: {cmd:?}");
        let reason = out.reason.unwrap();
        let program = cmd.split_whitespace().next().unwrap();
        assert!(reason.contains(program), "{reason}");
        assert!(reason.contains("main"), "{reason}");
        assert!(reason.contains("확인할 수 없어"), "{reason}");
        assert!(reason.contains("바꾸지 않는다면 승인"), "{reason}");
        assert!(reason.contains("git switch -c <branch-name>"), "{reason}");
    }
}

#[test]
fn opaque_exec_policy_decides_the_verdict() {
    let repo = repo_dir();
    for (policy, expected) in [
        (OpaqueExecPolicy::Block, GuardVerdict::Block),
        (OpaqueExecPolicy::Ask, GuardVerdict::Ask),
        (OpaqueExecPolicy::Allow, GuardVerdict::Allow),
    ] {
        let mut input = bash_input(repo.path(), "node x.js", None);
        input.opaque_exec = policy;
        let out = check(on_branch("main"), &input);
        assert_eq!(out.verdict, expected, "policy: {policy:?}");
    }
}

#[test]
fn opaque_exec_policy_does_not_soften_definite_writes() {
    let repo = repo_dir();
    let mut input = bash_input(repo.path(), "node x.js && sed -i s/a/b/ f", None);
    input.opaque_exec = OpaqueExecPolicy::Allow;
    assert_eq!(
        check(on_branch("main"), &input).verdict,
        GuardVerdict::Block
    );
}

#[test]
fn unresolved_write_target_follows_the_opaque_policy() {
    let repo = repo_dir();
    let mut input = bash_input(repo.path(), "rm -rf $BUILD_DIR/out", None);
    assert_eq!(check(on_branch("main"), &input).verdict, GuardVerdict::Ask);
    input.opaque_exec = OpaqueExecPolicy::Block;
    assert_eq!(
        check(on_branch("main"), &input).verdict,
        GuardVerdict::Block
    );
}

#[test]
fn script_located_elsewhere_still_asks_from_the_project_cwd() {
    let elsewhere = tempfile::tempdir().unwrap();
    let cmd = format!("node {}/x.js", elsewhere.path().display());
    assert_eq!(judge("main", &cmd).verdict, GuardVerdict::Ask);
}

#[test]
fn targets_outside_every_repo_are_allowed_without_git() {
    let repo = repo_dir();
    let outside = tempfile::tempdir().unwrap();
    let out = outside.path().display();
    for cmd in [
        format!("rm -rf {out}/build"),
        format!("echo x > {out}/new-dir/file"),
        format!("rm -rf {out}/foo*"),
        format!("cd {out} && node x.js"),
        format!("git -C {out} commit -m x"),
    ] {
        assert_allowed_without_git(repo.path(), &cmd, None);
    }
}

#[test]
fn recursive_writes_on_a_directory_above_the_project_are_judged() {
    let repo = repo_dir();
    for cmd in ["rm -rf ..", "find .. -delete"] {
        let out = check(on_branch("main"), &bash_input(repo.path(), cmd, None));
        assert_eq!(out.verdict, GuardVerdict::Block, "command: {cmd:?}");
    }
    let out = check(
        on_branch("main"),
        &bash_input(repo.path(), "rm -rf ../*/src", None),
    );
    assert_eq!(out.verdict, GuardVerdict::Ask);
}

#[test]
fn copying_into_a_directory_above_the_project_is_allowed() {
    let repo = repo_dir();
    assert_allowed_without_git(repo.path(), "cp a ..", None);
}

#[test]
fn payload_cwd_outside_every_repo_makes_relative_effects_pass() {
    let repo = repo_dir();
    let outside = tempfile::tempdir().unwrap();
    assert_allowed_without_git(repo.path(), "rm build", Some(outside.path()));
    assert_allowed_without_git(repo.path(), "node x.js", Some(outside.path()));
    assert_eq!(
        check(
            on_branch("main"),
            &bash_input(repo.path(), "rm build", None)
        )
        .verdict,
        GuardVerdict::Block
    );
}

#[test]
fn payload_cwd_inside_project_subdir_is_judged_with_project_git() {
    let repo = repo_dir();
    let sub = repo.path().join("pkg");
    let out = check(
        on_branch("main"),
        &bash_input(repo.path(), "git commit -m x", Some(&sub)),
    );
    assert_eq!(out.verdict, GuardVerdict::Block);
}

#[test]
fn payload_cwd_in_another_repo_is_judged_with_that_repos_git() {
    let project = repo_dir();
    let other = repo_dir();
    let input = bash_input(project.path(), "git commit -m x", Some(other.path()));
    let on_feature = MockGitFactory::new().with(other.path(), || on_branch("feat/x"));
    assert_eq!(
        check_with(on_branch("main"), on_feature, &input).verdict,
        GuardVerdict::Allow
    );
    let on_main = MockGitFactory::new().with(other.path(), || on_branch("main"));
    assert_eq!(
        check_with(on_branch("feat/x"), on_main, &input).verdict,
        GuardVerdict::Block
    );
}

#[test]
fn home_is_used_to_expand_tilde_targets() {
    let repo = repo_dir();
    let home = tempfile::tempdir().unwrap();
    let (git, calls) = counting_git("main");
    let mut input = bash_input(repo.path(), "rm ~/notes.txt", None);
    input.home = Some(home.path().to_string_lossy().to_string());
    assert_eq!(check(git, &input).verdict, GuardVerdict::Allow);
    assert_eq!(calls.get(), 0);
}

#[test]
fn unreadable_command_still_blocks_a_commit_on_protected_branch() {
    let cmd = "git commit -m \"unterminated";
    assert_eq!(judge("main", cmd).verdict, GuardVerdict::Block);
    assert_eq!(judge("feat/x", cmd).verdict, GuardVerdict::Allow);
}

#[test]
fn unreadable_command_is_never_a_silent_allow_on_protected_branch() {
    for cmd in [
        "echo \"unterminated",
        "node -e \"oops",
        "cat <<'EOF' && $(rm",
    ] {
        let out = judge("main", cmd);
        assert_eq!(out.verdict, GuardVerdict::Ask, "command: {cmd:?}");
        assert!(!out.reason.unwrap().is_empty());
    }
}

#[test]
fn unreadable_command_follows_opaque_policy_and_cwd_scope() {
    let repo = repo_dir();
    let outside = tempfile::tempdir().unwrap();
    let mut input = bash_input(repo.path(), "echo \"unterminated", None);
    input.opaque_exec = OpaqueExecPolicy::Block;
    assert_eq!(
        check(on_branch("main"), &input).verdict,
        GuardVerdict::Block
    );
    assert_allowed_without_git(repo.path(), "echo \"unterminated", Some(outside.path()));
}

// ---- per-repository judgement ----

fn write_input(project: &Path, file: &Path) -> GuardInput {
    let mut input = base_input();
    input.project_dir = project.to_string_lossy().to_string();
    input.target = GuardTarget::Write {
        file_path: Some(file.to_string_lossy().to_string()),
    };
    input
}

/// A linked worktree of the checkout at `main_checkout`: its `.git` is a file
/// pointing into `<main_checkout>/.git/worktrees/<name>`.
fn linked_worktree(main_checkout: &Path, name: &str) -> tempfile::TempDir {
    let admin = main_checkout.join(".git/worktrees").join(name);
    std::fs::create_dir_all(&admin).unwrap();
    let worktree = tempfile::tempdir().unwrap();
    std::fs::write(
        worktree.path().join(".git"),
        format!("gitdir: {}\n", admin.display()),
    )
    .unwrap();
    worktree
}

fn on_branch_detecting(branch: &str, default: &str) -> MockGit {
    let default = default.to_string();
    let mut git = on_branch(branch);
    git.detect_default_branch = Box::new(move || Ok(default.clone()));
    git
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

#[test]
fn find_repo_root_returns_the_nearest_ancestor_with_a_git_entry() {
    let repo = repo_dir();
    let nested = repo.path().join("a/b");
    std::fs::create_dir_all(&nested).unwrap();
    let missing = nested.join("not/yet/there.txt");
    assert_eq!(find_repo_root(&nested), Some(repo.path().to_path_buf()));
    assert_eq!(find_repo_root(&missing), Some(repo.path().to_path_buf()));

    let inner = repo.path().join("vendor/lib");
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::write(inner.join(".git"), "gitdir: /elsewhere\n").unwrap();
    assert_eq!(find_repo_root(&inner.join("src/x.rs")), Some(inner.clone()));

    let outside = tempfile::tempdir().unwrap();
    assert_eq!(find_repo_root(&outside.path().join("x")), None);
}

#[test]
fn hits_are_judged_in_each_root_and_the_strongest_verdict_wins() {
    let project = repo_dir();
    let other = repo_dir();
    let cmd = format!("rm a && rm {}/b", other.path().display());
    let input = bash_input(project.path(), &cmd, None);

    let factory = MockGitFactory::new().with(other.path(), || on_branch("main"));
    let out = check_with(on_branch("feat/x"), factory, &input);
    assert_eq!(out.verdict, GuardVerdict::Block);
    assert_eq!(out.repo_root, Some(display(other.path())));

    let factory = MockGitFactory::new().with(other.path(), || on_branch("feat/x"));
    let out = check_with(on_branch("main"), factory, &input);
    assert_eq!(out.verdict, GuardVerdict::Block);
    assert_eq!(out.repo_root, Some(display(project.path())));

    let factory = MockGitFactory::new().with(other.path(), || on_branch("feat/x"));
    let out = check_with(on_branch("feat/x"), factory, &input);
    assert_eq!(out.verdict, GuardVerdict::Allow);
}

#[test]
fn ask_in_one_root_loses_to_block_in_another() {
    let project = repo_dir();
    let other = repo_dir();
    let cmd = format!("node x.js && rm {}/b", other.path().display());
    let input = bash_input(project.path(), &cmd, None);
    let factory = MockGitFactory::new().with(other.path(), || on_branch("main"));
    let out = check_with(on_branch("main"), factory, &input);
    assert_eq!(out.verdict, GuardVerdict::Block);
    assert_eq!(out.repo_root, Some(display(other.path())));
}

#[test]
fn ask_names_the_opaque_program_of_the_root_that_asked() {
    let project = repo_dir();
    let other = repo_dir();
    let cmd = format!("node a.js && cd {} && python3 y.py", other.path().display());
    let input = bash_input(project.path(), &cmd, None);
    let factory = MockGitFactory::new().with(other.path(), || on_branch("main"));
    let out = check_with(on_branch("feat/x"), factory, &input);
    assert_eq!(out.verdict, GuardVerdict::Ask);
    let reason = out.reason.unwrap();
    assert!(reason.contains("`python3`"), "{reason}");
    assert!(!reason.contains("`node`"), "{reason}");
    assert!(reason.contains(&format!("대상 저장소: {}", other.path().display())));
}

#[test]
fn unresolved_target_without_prefix_is_judged_in_the_project_root() {
    let project = repo_dir();
    let input = bash_input(project.path(), "rm -rf $BUILD_DIR/out", None);
    let out = check(on_branch("main"), &input);
    assert_eq!(out.verdict, GuardVerdict::Ask);
    assert_eq!(out.repo_root, Some(display(project.path())));
}

#[test]
fn project_root_hit_never_asks_the_factory() {
    let project = repo_dir();
    let factory = MockGitFactory::new();
    let requested = factory.requested();
    let input = bash_input(project.path(), "rm a", Some(&project.path().join("sub")));
    check_with(on_branch("main"), factory, &input);
    assert!(requested.borrow().is_empty());
}

#[test]
fn project_that_is_not_a_repo_sends_hits_through_the_factory_without_pin() {
    let project = tempfile::tempdir().unwrap();
    let other = repo_dir();
    let mut input = write_input(project.path(), &other.path().join("a.rs"));
    input.default_branch = Some("trunk".to_string());

    let factory = MockGitFactory::new().with(other.path(), || on_branch_detecting("trunk", "main"));
    let out = check_with(panicking_git(), factory, &input);
    assert_eq!(out.verdict, GuardVerdict::Allow, "pin must not apply");

    let factory = MockGitFactory::new().with(other.path(), || on_branch_detecting("main", "main"));
    let out = check_with(panicking_git(), factory, &input);
    assert_eq!(out.verdict, GuardVerdict::Block);
    assert_eq!(
        out.rule,
        Some(ProtectionRule::DefaultBranch {
            source: DefaultBranchSource::Detected
        })
    );
}

#[test]
fn other_repository_uses_detection_not_the_project_pin() {
    let project = repo_dir();
    let other = repo_dir();
    let mut input = write_input(project.path(), &other.path().join("a.rs"));
    input.default_branch = Some("trunk".to_string());
    let factory = MockGitFactory::new().with(other.path(), || on_branch_detecting("trunk", "main"));
    let out = check_with(on_branch("feat/x"), factory, &input);
    assert_eq!(out.verdict, GuardVerdict::Allow);
}

#[test]
fn other_repository_still_protects_develop_and_extra_branches() {
    let project = repo_dir();
    let other = repo_dir();
    let mut input = write_input(project.path(), &other.path().join("a.rs"));
    input.protected_branches = Some(vec!["staging".to_string()]);
    for (branch, rule) in [
        ("develop", ProtectionRule::Develop),
        ("staging", ProtectionRule::Extra),
    ] {
        let factory = MockGitFactory::new().with(other.path(), move || on_branch(branch));
        let out = check_with(on_branch("feat/x"), factory, &input);
        assert_eq!(out.verdict, GuardVerdict::Block, "{branch}");
        assert_eq!(out.rule, Some(rule));
    }
}

#[test]
fn worktree_of_the_project_repository_follows_the_project_pin() {
    let project = repo_dir();
    let worktree = linked_worktree(project.path(), "wt");
    let mut input = write_input(project.path(), &worktree.path().join("a.rs"));
    input.default_branch = Some("trunk".to_string());
    let factory =
        MockGitFactory::new().with(worktree.path(), || on_branch_detecting("trunk", "main"));
    let out = check_with(on_branch("feat/x"), factory, &input);
    assert_eq!(out.verdict, GuardVerdict::Block);
    assert_eq!(
        out.rule,
        Some(ProtectionRule::DefaultBranch {
            source: DefaultBranchSource::Pinned
        })
    );
    assert_eq!(out.repo_root, Some(display(worktree.path())));
}

#[test]
fn worktree_common_dir_is_resolved_through_commondir_file() {
    let project = repo_dir();
    let admin = project.path().join(".git/worktrees/wt");
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::write(admin.join("commondir"), "../..\n").unwrap();
    let worktree = tempfile::tempdir().unwrap();
    std::fs::write(
        worktree.path().join(".git"),
        format!("gitdir: {}\n", admin.display()),
    )
    .unwrap();
    let mut input = write_input(project.path(), &worktree.path().join("a.rs"));
    input.default_branch = Some("trunk".to_string());
    let factory =
        MockGitFactory::new().with(worktree.path(), || on_branch_detecting("trunk", "main"));
    assert_eq!(
        check_with(on_branch("feat/x"), factory, &input).verdict,
        GuardVerdict::Block
    );
}

#[test]
fn worktree_of_another_repository_does_not_follow_the_project_pin() {
    let project = repo_dir();
    let elsewhere = repo_dir();
    let worktree = linked_worktree(elsewhere.path(), "wt");
    let mut input = write_input(project.path(), &worktree.path().join("a.rs"));
    input.default_branch = Some("trunk".to_string());
    let factory =
        MockGitFactory::new().with(worktree.path(), || on_branch_detecting("trunk", "main"));
    assert_eq!(
        check_with(on_branch("feat/x"), factory, &input).verdict,
        GuardVerdict::Allow
    );
}

#[test]
fn write_tool_in_a_worktree_is_judged_by_the_worktree_branch() {
    let project = repo_dir();
    let worktree = linked_worktree(project.path(), "wt");
    let input = write_input(project.path(), &worktree.path().join("a.rs"));
    let factory = MockGitFactory::new().with(worktree.path(), || on_branch("feat/x"));
    let out = check_with(on_branch("main"), factory, &input);
    assert_eq!(out.verdict, GuardVerdict::Allow, "project is on main");
}

#[test]
fn protection_rule_is_reported_wherever_the_protected_gate_decides() {
    let repo = repo_dir();
    let mut input = bash_input(repo.path(), "rm a", None);
    input.protected_branches = Some(vec!["staging".to_string()]);

    let detected = check(on_branch("main"), &input);
    assert_eq!(
        detected.rule,
        Some(ProtectionRule::DefaultBranch {
            source: DefaultBranchSource::Detected
        })
    );
    assert_eq!(detected.repo_root, Some(display(repo.path())));

    input.default_branch = Some("main".to_string());
    let pinned = check(on_branch("main"), &input);
    assert_eq!(
        pinned.rule,
        Some(ProtectionRule::DefaultBranch {
            source: DefaultBranchSource::Pinned
        })
    );

    assert_eq!(
        check(on_branch("develop"), &input).rule,
        Some(ProtectionRule::Develop)
    );
    assert_eq!(
        check(on_branch("staging"), &input).rule,
        Some(ProtectionRule::Extra)
    );
    assert_eq!(check(on_branch("feat/x"), &input).rule, None);
}

#[test]
fn bash_block_message_names_branch_root_rule_and_remedy() {
    let repo = repo_dir();
    let p = display(repo.path());
    let out = check(
        on_branch("main"),
        &bash_input(repo.path(), "sed -i s/a/b/ f", None),
    );
    let expected = [
        format!("[Branch Guard] 보호 브랜치(main)에서 파일을 수정하려 합니다 — Bash `sed` → {p}/f"),
        format!("- 현재 브랜치: main / 대상 저장소: {p} (프로젝트: {p})"),
        "- 발동 규칙: 기본 브랜치 보호(자동 감지: main)".to_string(),
        format!("가드 오작동을 의심하기 전에 `git -C {p} branch --show-current` 로 브랜치를 먼저 확인하세요."),
        "해소: 새 브랜치를 만든 뒤 다시 시도하세요:".to_string(),
        "  git switch -c <branch-name>".to_string(),
    ]
    .join("\n");
    assert_eq!(out.reason.unwrap(), expected);
}

#[test]
fn write_block_message_uses_the_same_layout_with_the_path() {
    let repo = repo_dir();
    let p = display(repo.path());
    let mut input = write_input(repo.path(), &repo.path().join("src/a.rs"));
    input.default_branch = Some("main".to_string());
    let reason = check(on_branch("main"), &input).reason.unwrap();
    let lines: Vec<&str> = reason.lines().collect();
    assert_eq!(
        lines[0],
        format!(
            "[Branch Guard] 보호 브랜치(main)에서 파일을 수정하려 합니다 — Write → {p}/src/a.rs"
        )
    );
    assert_eq!(
        lines[1],
        format!("- 현재 브랜치: main / 대상 저장소: {p} (프로젝트: {p})")
    );
    assert_eq!(
        lines[2],
        "- 발동 규칙: 기본 브랜치 보호(--default-branch main)"
    );
    assert_eq!(lines.len(), 6);
}

#[test]
fn commit_block_message_says_commit_and_shows_the_command_cwd() {
    let repo = repo_dir();
    let sub = repo.path().join("pkg");
    let reason = check(
        on_branch("develop"),
        &bash_input(repo.path(), "git commit -m x", Some(&sub)),
    )
    .reason
    .unwrap();
    let p = display(repo.path());
    assert!(
        reason.starts_with(&format!(
            "[Branch Guard] 보호 브랜치(develop)에서 커밋하려 합니다 — Bash `git commit` → {p}/pkg\n"
        )),
        "{reason}"
    );
    assert!(
        reason.contains(&format!("(프로젝트: {p}, 명령 cwd: {p}/pkg)")),
        "{reason}"
    );
    assert!(
        reason.contains("- 발동 규칙: develop 고정 보호"),
        "{reason}"
    );
}

#[test]
fn ask_message_adds_the_confirmation_notice_and_block_policy_says_so() {
    let repo = repo_dir();
    let mut input = bash_input(repo.path(), "python3 y.py", None);
    let ask = check(on_branch("main"), &input).reason.unwrap();
    assert!(ask.contains(
        "스크립트/대상 내부를 확인할 수 없어 확인을 요청합니다. 저장소 파일을 바꾸지 않는다면 승인하세요."
    ));
    assert!(ask.contains("`python3`"), "{ask}");

    input.opaque_exec = OpaqueExecPolicy::Block;
    let blocked = check(on_branch("main"), &input).reason.unwrap();
    assert!(blocked.contains("확인할 수 없어 차단합니다(--opaque-exec block)"));
    assert!(!blocked.contains("승인하세요"));
}

#[test]
fn custom_branch_creation_script_is_rendered_in_the_remedy() {
    let repo = repo_dir();
    let mut input = bash_input(repo.path(), "rm a", None);
    input.create_branch_script = "gh wt new".to_string();
    let reason = check(on_branch("main"), &input).reason.unwrap();
    assert!(reason.ends_with("\n  gh wt new <branch-name>"), "{reason}");
}

#[test]
fn unreadable_command_ask_names_the_first_word_of_the_command() {
    let reason = judge("main", "echo \"unterminated").reason.unwrap();
    assert!(reason.contains("`echo`"), "{reason}");
}

#[test]
fn hook_bypass_message_shows_rule_and_token_only_when_they_differ() {
    let repo = repo_dir();
    let same = check(
        panicking_git(),
        &bash_input(repo.path(), "git commit --no-verify -m x", None),
    )
    .reason
    .unwrap();
    assert!(
        same.starts_with("[Hook Guard] git hook 우회(--no-verify)는"),
        "{same}"
    );

    let differs = check(
        panicking_git(),
        &bash_input(repo.path(), "git commit -an -m x", None),
    )
    .reason
    .unwrap();
    assert!(
        differs.starts_with("[Hook Guard] git hook 우회(commit -n: -an)는"),
        "{differs}"
    );
}

#[test]
fn bypass_blocks_before_any_repository_is_consulted() {
    let project = repo_dir();
    let other = repo_dir();
    let factory = MockGitFactory::new();
    let requested = factory.requested();
    let cmd = format!("rm {}/a && git commit --no-verify", other.path().display());
    let out = check_with(
        panicking_git(),
        factory,
        &bash_input(project.path(), &cmd, None),
    );
    assert_eq!(out.verdict, GuardVerdict::Block);
    assert!(requested.borrow().is_empty());
}

#[test]
fn rebase_in_another_repo_passes_that_root_only() {
    let project = repo_dir();
    let other = repo_dir();
    let cmd = format!("rm {}/b", other.path().display());
    let input = bash_input(project.path(), &cmd, None);
    let factory = MockGitFactory::new().with(other.path(), || {
        let mut git = on_branch("main");
        git.special_state_flags = Box::new(|| (true, false));
        git
    });
    assert_eq!(
        check_with(on_branch("main"), factory, &input).verdict,
        GuardVerdict::Allow
    );
}

// ---- path helpers ----

#[test]
fn is_inside_project_dir_cases() {
    assert!(is_inside_project_dir(
        "/home/user/project/src/file.ts",
        "/home/user/project"
    ));
    assert!(is_inside_project_dir(
        "/home/user/project",
        "/home/user/project"
    ));
    assert!(!is_inside_project_dir(
        "/home/user/.claude/settings.json",
        "/home/user/project"
    ));
    assert!(!is_inside_project_dir(
        "/home/user/project-extra/file.ts",
        "/home/user/project"
    ));
    assert!(!is_inside_project_dir(
        "/Users/user/.claude/settings.json",
        "/Users/user/Documents/my-project"
    ));
}

#[test]
fn is_inside_any_git_repo_cases() {
    let cwd = std::env::current_dir().unwrap();
    let cwd_str = cwd.to_str().unwrap();
    let file = cwd.join("Cargo.toml");
    // Absolute file_path: project_dir is irrelevant to the walk.
    assert!(is_inside_any_git_repo(
        file.to_str().unwrap(),
        "/nonexistent"
    ));

    let deep = cwd.join("some/deep/nonexistent/file.ts");
    assert!(is_inside_any_git_repo(
        deep.to_str().unwrap(),
        "/nonexistent"
    ));

    assert!(!is_inside_any_git_repo("/tmp/random-file.txt", cwd_str));
}

// ---- #780: relative file_path is anchored at project_dir, not process cwd ----

#[test]
fn relative_file_path_resolved_against_project_dir() {
    // A relative path belongs to the project regardless of the process cwd,
    // which (in this test binary) is the atelier/cli dir — not the project_dir.
    assert!(is_inside_project_dir(
        "src/index.ts",
        "/home/user/my-project"
    ));
    assert!(is_inside_project_dir(
        "./src/index.ts",
        "/home/user/my-project"
    ));
    assert!(is_inside_project_dir(".", "/home/user/my-project"));
    // `..` still escapes the project.
    assert!(!is_inside_project_dir(
        "../other/file.ts",
        "/home/user/my-project"
    ));
}

#[test]
fn relative_file_path_inside_repo_anchored_at_project_dir() {
    // project_dir is this checkout (a git repo); a relative file_path must
    // resolve under it, so the .git walk finds the repo.
    let cwd = std::env::current_dir().unwrap();
    let cwd_str = cwd.to_str().unwrap();
    assert!(is_inside_any_git_repo("src/main.rs", cwd_str));
    // Relative path under a non-repo project_dir is outside any git repo.
    assert!(!is_inside_any_git_repo("src/main.rs", "/tmp"));
}

#[test]
fn relative_file_path_outside_project_on_default_blocks() {
    // Relative file_path under project_dir, on a protected branch → blocked.
    // Proves the path was resolved into project_dir (not the process cwd):
    // is_inside_project_dir must be true so we fall through to the branch guard.
    let mut input = base_input();
    input.project_dir = "/home/user/my-project".to_string();
    input.target = GuardTarget::Write {
        file_path: Some("src/index.ts".to_string()),
    };
    assert_eq!(
        check(MockGit::default(), &input).verdict,
        GuardVerdict::Block
    );
}

// ---- branch switch and hooks directory ----

#[test]
fn writes_chained_after_a_switch_to_a_new_branch_are_allowed() {
    for cmd in [
        "git switch -c feat && echo hi > a",
        "git checkout -b feat && rm a && git commit -am x",
    ] {
        assert_eq!(judge("main", cmd).verdict, GuardVerdict::Allow, "{cmd:?}");
    }
}

#[test]
fn writes_after_a_switch_that_may_have_failed_are_judged_on_the_current_branch() {
    for cmd in [
        "git switch -c feat; echo hi > a",
        "git switch -c feat || rm a",
        "git switch -c feat && echo ok; rm a",
    ] {
        assert_eq!(judge("main", cmd).verdict, GuardVerdict::Block, "{cmd:?}");
    }
}

#[test]
fn writes_chained_after_a_switch_to_a_protected_branch_are_blocked_naming_it() {
    let out = judge("feat/x", "git switch develop && rm a");
    assert_eq!(out.verdict, GuardVerdict::Block);
    assert!(out.reason.unwrap().contains("develop"));
}

#[test]
fn a_switch_in_another_repository_does_not_move_the_project() {
    let repo = repo_dir();
    let other = repo_dir();
    let cmd = format!("git -C {} switch -c feat && rm a", other.path().display());
    let out = check(on_branch("main"), &bash_input(repo.path(), &cmd, None));
    assert_eq!(out.verdict, GuardVerdict::Block);
}

#[test]
fn hooks_directory_changes_are_blocked_on_every_branch() {
    for branch in ["main", "feat/x"] {
        let out = judge(branch, "rm .git/hooks/pre-commit");
        assert_eq!(out.verdict, GuardVerdict::Block, "{branch}");
        assert!(out.reason.unwrap().contains("[Hook Guard]"));
    }
    let repo = repo_dir();
    let hook = repo.path().join(".git/hooks/pre-commit");
    let out = check(on_branch("feat/x"), &write_input(repo.path(), &hook));
    assert_eq!(out.verdict, GuardVerdict::Block);
}

#[test]
fn hook_bypass_in_an_unterminated_command_is_blocked_on_every_branch() {
    for cmd in [
        "git commit --no-verify -m \"x",
        "git commit --no-verify -m x\necho \"oops",
    ] {
        assert_eq!(judge("feat/x", cmd).verdict, GuardVerdict::Block, "{cmd:?}");
    }
}
