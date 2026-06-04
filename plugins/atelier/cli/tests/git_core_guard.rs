//! Port of `git-utils/tests/core/guard.test.ts` — mock-git unit tests plus the
//! `is_inside_project_dir` / `is_inside_any_git_repo` path helpers.
#![allow(clippy::field_reassign_with_default)]

mod git_mocks;

use atelier::git::core::guard::{
    create_guard_service, is_git_commit_command, is_inside_any_git_repo, is_inside_project_dir,
    GuardService,
};
use atelier::git::types::{GitSpecialState, GuardInput, GuardTarget};
use git_mocks::MockGit;

fn base_input() -> GuardInput {
    GuardInput {
        target: GuardTarget::Write,
        project_dir: "/tmp/test".to_string(),
        create_branch_script: "./create-branch.sh".to_string(),
        default_branch: None,
        protected_branches: None,
        tool_command: None,
        tool_file_path: None,
    }
}

fn check(git: MockGit, input: &GuardInput) -> atelier::git::types::GuardOutput {
    let guard = create_guard_service(&git);
    guard.check(input)
}

#[test]
fn not_a_git_repo_passes() {
    let mut git = MockGit::default();
    git.is_inside_work_tree = Box::new(|| false);
    assert!(check(git, &base_input()).allowed);
}

#[test]
fn rebase_passes() {
    let mut git = MockGit::default();
    git.get_special_state = Box::new(|| GitSpecialState {
        rebase: true,
        merge: false,
        detached: false,
    });
    assert!(check(git, &base_input()).allowed);
}

#[test]
fn merge_passes() {
    let mut git = MockGit::default();
    git.get_special_state = Box::new(|| GitSpecialState {
        rebase: false,
        merge: true,
        detached: false,
    });
    assert!(check(git, &base_input()).allowed);
}

#[test]
fn detached_passes() {
    let mut git = MockGit::default();
    git.get_special_state = Box::new(|| GitSpecialState {
        rebase: false,
        merge: false,
        detached: true,
    });
    assert!(check(git, &base_input()).allowed);
}

#[test]
fn non_default_branch_passes() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "feat/something".to_string());
    assert!(check(git, &base_input()).allowed);
}

#[test]
fn default_branch_main_blocked() {
    assert!(!check(MockGit::default(), &base_input()).allowed);
}

#[test]
fn default_branch_master_blocked() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "master".to_string());
    git.detect_default_branch = Box::new(|| Ok("master".to_string()));
    assert!(!check(git, &base_input()).allowed);
}

#[test]
fn default_branch_develop_blocked() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "develop".to_string());
    git.detect_default_branch = Box::new(|| Ok("develop".to_string()));
    assert!(!check(git, &base_input()).allowed);
}

#[test]
fn develop_protected_even_when_default_is_main() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "develop".to_string());
    git.detect_default_branch = Box::new(|| Ok("main".to_string()));
    let out = check(git, &base_input());
    assert!(!out.allowed);
    assert_eq!(out.current_branch.as_deref(), Some("develop"));
    assert_eq!(out.default_branch.as_deref(), Some("main"));
}

#[test]
fn extra_protected_branches_blocked() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "staging".to_string());
    git.detect_default_branch = Box::new(|| Ok("main".to_string()));
    let mut input = base_input();
    input.protected_branches = Some(vec!["staging".to_string(), "release".to_string()]);
    let out = check(git, &input);
    assert!(!out.allowed);
    assert_eq!(out.current_branch.as_deref(), Some("staging"));
}

#[test]
fn branch_not_in_protected_passes() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "feat/something".to_string());
    git.detect_default_branch = Box::new(|| Ok("main".to_string()));
    let mut input = base_input();
    input.protected_branches = Some(vec!["staging".to_string()]);
    assert!(check(git, &input).allowed);
}

#[test]
fn explicit_default_branch_used() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "custom-default".to_string());
    let mut input = base_input();
    input.default_branch = Some("custom-default".to_string());
    let out = check(git, &input);
    assert!(!out.allowed);
    assert_eq!(out.default_branch.as_deref(), Some("custom-default"));
}

#[test]
fn detect_failure_passes_safe_mode() {
    let mut git = MockGit::default();
    git.detect_default_branch = Box::new(|| Err("no remote".to_string()));
    assert!(check(git, &base_input()).allowed);
}

#[test]
fn write_block_reason_mentions_action_and_script() {
    let out = check(MockGit::default(), &base_input());
    let reason = out.reason.unwrap();
    assert!(reason.contains("파일을 수정하려 합니다"));
    assert!(reason.contains("./create-branch.sh"));
}

#[test]
fn commit_target_not_git_commit_passes() {
    let mut input = base_input();
    input.target = GuardTarget::Commit;
    input.tool_command = Some("git push origin main".to_string());
    assert!(check(MockGit::default(), &input).allowed);
}

#[test]
fn commit_target_git_commit_on_default_blocked() {
    let mut input = base_input();
    input.target = GuardTarget::Commit;
    input.tool_command = Some("git commit -m \"test\"".to_string());
    assert!(!check(MockGit::default(), &input).allowed);
}

#[test]
fn commit_target_compound_command_blocked() {
    let mut input = base_input();
    input.target = GuardTarget::Commit;
    input.tool_command = Some("git add . && git commit -m \"test\"".to_string());
    assert!(!check(MockGit::default(), &input).allowed);
}

#[test]
fn commit_target_git_log_passes() {
    let mut input = base_input();
    input.target = GuardTarget::Commit;
    input.tool_command = Some("git log --oneline".to_string());
    assert!(check(MockGit::default(), &input).allowed);
}

#[test]
fn commit_target_empty_command_passes() {
    let mut input = base_input();
    input.target = GuardTarget::Commit;
    input.tool_command = Some(String::new());
    assert!(check(MockGit::default(), &input).allowed);
}

#[test]
fn commit_target_no_command_passes() {
    let mut input = base_input();
    input.target = GuardTarget::Commit;
    assert!(check(MockGit::default(), &input).allowed);
}

#[test]
fn commit_target_gh_issue_body_substring_passes() {
    // 본문에 "git commit" 텍스트가 있어도 실제 git 명령이 아니므로 통과 (#754).
    let mut input = base_input();
    input.target = GuardTarget::Commit;
    input.tool_command =
        Some(r#"gh issue create --title "x" --body "먼저 git commit 후 push 하세요""#.to_string());
    assert!(check(MockGit::default(), &input).allowed);
}

#[test]
fn commit_target_env_prefix_blocked() {
    let mut input = base_input();
    input.target = GuardTarget::Commit;
    input.tool_command = Some("GIT_AUTHOR_NAME=bot git commit -m \"x\"".to_string());
    assert!(!check(MockGit::default(), &input).allowed);
}

#[test]
fn commit_target_global_opts_blocked() {
    let mut input = base_input();
    input.target = GuardTarget::Commit;
    input.tool_command = Some("git -C /repo -c user.name=x commit -m \"x\"".to_string());
    assert!(!check(MockGit::default(), &input).allowed);
}

#[test]
fn is_git_commit_command_cases() {
    // 실제 git commit
    assert!(is_git_commit_command("git commit -m \"x\""));
    assert!(is_git_commit_command("git add . && git commit -m \"x\""));
    assert!(is_git_commit_command("GIT_AUTHOR_NAME=bot git commit"));
    assert!(is_git_commit_command("git -C /repo commit -m \"x\""));
    assert!(is_git_commit_command("/usr/bin/git commit"));
    // git 의 비-commit 서브커맨드
    assert!(!is_git_commit_command("git push origin main"));
    assert!(!is_git_commit_command("git log --oneline"));
    assert!(!is_git_commit_command("git commit-graph write")); // commit-graph는 commit 아님
                                                               // git 명령이 아닌데 본문에 substring
    assert!(!is_git_commit_command(
        "gh issue create --body \"git commit 하세요\""
    ));
    assert!(!is_git_commit_command("echo \"git commit\""));
    assert!(!is_git_commit_command("curl -d \"git commit\" https://x"));
    assert!(!is_git_commit_command(""));
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
    input.tool_file_path = Some("/home/user/.claude/settings.json".to_string());
    let out = check(MockGit::default(), &input);
    assert!(out.allowed);
    assert_eq!(
        out.reason.as_deref(),
        Some("file is outside any git repository")
    );
}

#[test]
fn outside_project_but_inside_other_git_repo_blocked() {
    // The current checkout IS a git repo; point at a file inside it from a
    // bogus project dir to simulate "another repo".
    let this_project = std::env::current_dir().unwrap();
    let mut input = base_input();
    input.project_dir = "/some/other/project".to_string();
    input.tool_file_path = Some(
        this_project
            .join("Cargo.toml")
            .to_string_lossy()
            .to_string(),
    );
    assert!(!check(MockGit::default(), &input).allowed);
}

#[test]
fn inside_project_on_default_blocked() {
    let mut input = base_input();
    input.project_dir = "/home/user/my-project".to_string();
    input.tool_file_path = Some("/home/user/my-project/src/index.ts".to_string());
    assert!(!check(MockGit::default(), &input).allowed);
}

#[test]
fn inside_project_on_feature_passes() {
    let mut git = MockGit::default();
    git.get_current_branch = Box::new(|| "feat/something".to_string());
    let mut input = base_input();
    input.project_dir = "/home/user/my-project".to_string();
    input.tool_file_path = Some("/home/user/my-project/src/index.ts".to_string());
    assert!(check(git, &input).allowed);
}

#[test]
fn no_tool_file_path_runs_default_guard() {
    assert!(!check(MockGit::default(), &base_input()).allowed);
}

#[test]
fn commit_target_ignores_tool_file_path() {
    let mut input = base_input();
    input.target = GuardTarget::Commit;
    input.tool_file_path = Some("/external/path/file.txt".to_string());
    input.tool_command = Some("git commit -m \"test\"".to_string());
    assert!(!check(MockGit::default(), &input).allowed);
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
    let file = cwd.join("Cargo.toml");
    assert!(is_inside_any_git_repo(file.to_str().unwrap()));

    let deep = cwd.join("some/deep/nonexistent/file.ts");
    assert!(is_inside_any_git_repo(deep.to_str().unwrap()));

    assert!(!is_inside_any_git_repo("/tmp/random-file.txt"));
}
