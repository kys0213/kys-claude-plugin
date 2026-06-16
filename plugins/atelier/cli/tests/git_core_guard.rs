//! Port of `git-utils/tests/core/guard.test.ts` — mock-git unit tests plus the
//! `is_inside_project_dir` / `is_inside_any_git_repo` path helpers.
#![allow(clippy::field_reassign_with_default)]

mod git_mocks;

use atelier::git::core::guard::{
    create_guard_service, is_inside_any_git_repo, is_inside_project_dir, GuardService,
};
use atelier::git::types::{GuardInput, GuardTarget};
use git_mocks::MockGit;

fn base_input() -> GuardInput {
    GuardInput {
        target: GuardTarget::Write { file_path: None },
        project_dir: "/tmp/test".to_string(),
        create_branch_script: "git switch -c".to_string(),
        default_branch: None,
        protected_branches: None,
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
    git.special_state_flags = Box::new(|| (true, false));
    assert!(check(git, &base_input()).allowed);
}

#[test]
fn merge_passes() {
    let mut git = MockGit::default();
    git.special_state_flags = Box::new(|| (false, true));
    assert!(check(git, &base_input()).allowed);
}

#[test]
fn detached_passes() {
    // Detached HEAD: `git branch --show-current` prints nothing.
    let mut git = MockGit::default();
    git.current_branch = Box::new(String::new);
    assert!(check(git, &base_input()).allowed);
}

#[test]
fn non_default_branch_passes() {
    let mut git = MockGit::default();
    git.current_branch = Box::new(|| "feat/something".to_string());
    assert!(check(git, &base_input()).allowed);
}

#[test]
fn default_branch_main_blocked() {
    assert!(!check(MockGit::default(), &base_input()).allowed);
}

#[test]
fn default_branch_master_blocked() {
    let mut git = MockGit::default();
    git.current_branch = Box::new(|| "master".to_string());
    git.detect_default_branch_readonly = Box::new(|| Ok("master".to_string()));
    assert!(!check(git, &base_input()).allowed);
}

#[test]
fn default_branch_develop_blocked() {
    let mut git = MockGit::default();
    git.current_branch = Box::new(|| "develop".to_string());
    git.detect_default_branch_readonly = Box::new(|| Ok("develop".to_string()));
    assert!(!check(git, &base_input()).allowed);
}

#[test]
fn empty_default_branch_falls_back_to_detection() {
    // A setup that detected nothing must not bake `""` as the protected branch —
    // empty is treated as absence, so the guard falls back to readonly detection
    // and still blocks the real default (MockGit default: current + detect = main).
    let mut input = base_input();
    input.default_branch = Some(String::new());
    assert!(
        !check(MockGit::default(), &input).allowed,
        "empty --default-branch must not bypass protection on the real default branch"
    );
}

#[test]
fn develop_protected_even_when_default_is_main() {
    let mut git = MockGit::default();
    git.current_branch = Box::new(|| "develop".to_string());
    git.detect_default_branch_readonly = Box::new(|| Ok("main".to_string()));
    let out = check(git, &base_input());
    assert!(!out.allowed);
    assert_eq!(out.current_branch.as_deref(), Some("develop"));
    assert_eq!(out.default_branch.as_deref(), Some("main"));
}

#[test]
fn extra_protected_branches_blocked() {
    let mut git = MockGit::default();
    git.current_branch = Box::new(|| "staging".to_string());
    git.detect_default_branch_readonly = Box::new(|| Ok("main".to_string()));
    let mut input = base_input();
    input.protected_branches = Some(vec!["staging".to_string(), "release".to_string()]);
    let out = check(git, &input);
    assert!(!out.allowed);
    assert_eq!(out.current_branch.as_deref(), Some("staging"));
}

#[test]
fn branch_not_in_protected_passes() {
    let mut git = MockGit::default();
    git.current_branch = Box::new(|| "feat/something".to_string());
    git.detect_default_branch_readonly = Box::new(|| Ok("main".to_string()));
    let mut input = base_input();
    input.protected_branches = Some(vec!["staging".to_string()]);
    assert!(check(git, &input).allowed);
}

#[test]
fn explicit_default_branch_used() {
    let mut git = MockGit::default();
    git.current_branch = Box::new(|| "custom-default".to_string());
    let mut input = base_input();
    input.default_branch = Some("custom-default".to_string());
    let out = check(git, &input);
    assert!(!out.allowed);
    assert_eq!(out.default_branch.as_deref(), Some("custom-default"));
}

#[test]
fn detect_failure_passes_safe_mode() {
    let mut git = MockGit::default();
    git.detect_default_branch_readonly = Box::new(|| Err("no remote".to_string()));
    assert!(check(git, &base_input()).allowed);
}

#[test]
fn write_block_reason_mentions_action_and_script() {
    let out = check(MockGit::default(), &base_input());
    let reason = out.reason.unwrap();
    assert!(reason.contains("파일을 수정하려 합니다"));
    assert!(reason.contains("git switch -c"));
}

#[test]
fn commit_target_not_git_commit_passes() {
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        command: Some("git push origin main".to_string()),
    };
    assert!(check(MockGit::default(), &input).allowed);
}

#[test]
fn commit_target_git_commit_on_default_blocked() {
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        command: Some("git commit -m \"test\"".to_string()),
    };
    assert!(!check(MockGit::default(), &input).allowed);
}

#[test]
fn commit_target_compound_command_blocked() {
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        command: Some("git add . && git commit -m \"test\"".to_string()),
    };
    assert!(!check(MockGit::default(), &input).allowed);
}

#[test]
fn commit_target_git_log_passes() {
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        command: Some("git log --oneline".to_string()),
    };
    assert!(check(MockGit::default(), &input).allowed);
}

#[test]
fn commit_target_empty_command_passes() {
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        command: Some(String::new()),
    };
    assert!(check(MockGit::default(), &input).allowed);
}

#[test]
fn commit_target_no_command_passes() {
    let mut input = base_input();
    input.target = GuardTarget::Commit { command: None };
    assert!(check(MockGit::default(), &input).allowed);
}

// ---- #754: "git commit" inside quoted text must not match ----

#[test]
fn commit_target_double_quoted_text_passes() {
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        command: Some(r#"gh issue create --body "remember to git commit often""#.to_string()),
    };
    let out = check(MockGit::default(), &input);
    assert!(out.allowed);
    assert_eq!(out.reason.as_deref(), Some("not a git commit command"));
}

#[test]
fn commit_target_single_quoted_text_passes() {
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        command: Some("gh pr comment 1 --body 'please git commit first'".to_string()),
    };
    assert!(check(MockGit::default(), &input).allowed);
}

#[test]
fn commit_target_real_commit_with_quoted_message_blocked() {
    // The quoted message is stripped, but `git ... commit` stays outside the
    // quotes, so a real commit is still matched.
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        command: Some(r#"git commit -m "this is not a git commit hint""#.to_string()),
    };
    assert!(!check(MockGit::default(), &input).allowed);
}

#[test]
fn commit_target_escaped_quotes_stay_conservative() {
    // `\"` is a literal quote char, not a quote opener — the text still
    // matches and blocks (conservative toward the old behavior).
    let mut input = base_input();
    input.target = GuardTarget::Commit {
        command: Some(r#"echo \"git commit\""#.to_string()),
    };
    assert!(!check(MockGit::default(), &input).allowed);
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
    input.target = GuardTarget::Write {
        file_path: Some(
            this_project
                .join("Cargo.toml")
                .to_string_lossy()
                .to_string(),
        ),
    };
    assert!(!check(MockGit::default(), &input).allowed);
}

#[test]
fn inside_project_on_default_blocked() {
    let mut input = base_input();
    input.project_dir = "/home/user/my-project".to_string();
    input.target = GuardTarget::Write {
        file_path: Some("/home/user/my-project/src/index.ts".to_string()),
    };
    assert!(!check(MockGit::default(), &input).allowed);
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
    assert!(check(git, &input).allowed);
}

#[test]
fn no_tool_file_path_runs_default_guard() {
    assert!(!check(MockGit::default(), &base_input()).allowed);
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
    assert!(!check(MockGit::default(), &input).allowed);
}
