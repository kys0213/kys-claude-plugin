//! Mock-based port of git-utils `tests/core/guard.test.ts`.

use std::cell::Cell;

use anyhow::{bail, Result};

use atelier::git::core::git::{BranchLocation, GitService, GitSpecialState};
use atelier::git::core::guard::{
    is_inside_any_git_repo, is_inside_project_dir, GuardService, RealGuardService,
};
use atelier::git::types::{GuardInput, GuardTarget};

struct MockGit {
    inside_work_tree: bool,
    current_branch: String,
    default_branch: Result<String, ()>,
    special: GitSpecialState,
    detect_called: Cell<bool>,
}

impl Default for MockGit {
    fn default() -> Self {
        Self {
            inside_work_tree: true,
            current_branch: "main".to_string(),
            default_branch: Ok("main".to_string()),
            special: GitSpecialState {
                rebase: false,
                merge: false,
                detached: false,
            },
            detect_called: Cell::new(false),
        }
    }
}

impl GitService for MockGit {
    fn detect_default_branch(&self) -> Result<String> {
        self.detect_called.set(true);
        match &self.default_branch {
            Ok(b) => Ok(b.clone()),
            Err(()) => bail!("no remote"),
        }
    }
    fn get_current_branch(&self) -> Result<String> {
        Ok(self.current_branch.clone())
    }
    fn branch_exists(&self, _name: &str, _location: BranchLocation) -> Result<bool> {
        Ok(false)
    }
    fn is_inside_work_tree(&self) -> Result<bool> {
        Ok(self.inside_work_tree)
    }
    fn has_uncommitted_changes(&self) -> Result<bool> {
        Ok(false)
    }
    fn get_special_state(&self) -> Result<GitSpecialState> {
        Ok(self.special.clone())
    }
    fn fetch(&self, _remote: Option<&str>) -> Result<()> {
        Ok(())
    }
    fn checkout(&self, _branch: &str, _create: bool, _track: Option<&str>) -> Result<()> {
        Ok(())
    }
    fn commit(&self, _message: &str) -> Result<()> {
        Ok(())
    }
    fn push(&self, _branch: &str, _set_upstream: bool) -> Result<()> {
        Ok(())
    }
    fn pull(&self, _branch: &str) -> Result<()> {
        Ok(())
    }
    fn add_tracked(&self) -> Result<()> {
        Ok(())
    }
}

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

fn cwd_file() -> String {
    format!(
        "{}/package.json",
        std::env::current_dir().unwrap().display()
    )
}

// ---------- common guard logic ----------

#[test]
fn not_a_repo_passes() {
    let git = MockGit {
        inside_work_tree: false,
        ..Default::default()
    };
    assert!(RealGuardService::new(&git).check(&base_input()).allowed);
}

#[test]
fn rebase_passes() {
    let git = MockGit {
        special: GitSpecialState {
            rebase: true,
            merge: false,
            detached: false,
        },
        ..Default::default()
    };
    assert!(RealGuardService::new(&git).check(&base_input()).allowed);
}

#[test]
fn merge_passes() {
    let git = MockGit {
        special: GitSpecialState {
            rebase: false,
            merge: true,
            detached: false,
        },
        ..Default::default()
    };
    assert!(RealGuardService::new(&git).check(&base_input()).allowed);
}

#[test]
fn detached_passes() {
    let git = MockGit {
        special: GitSpecialState {
            rebase: false,
            merge: false,
            detached: true,
        },
        ..Default::default()
    };
    assert!(RealGuardService::new(&git).check(&base_input()).allowed);
}

#[test]
fn feature_branch_passes() {
    let git = MockGit {
        current_branch: "feat/something".to_string(),
        ..Default::default()
    };
    assert!(RealGuardService::new(&git).check(&base_input()).allowed);
}

#[test]
fn main_branch_blocked() {
    let git = MockGit::default();
    assert!(!RealGuardService::new(&git).check(&base_input()).allowed);
}

#[test]
fn master_branch_blocked() {
    let git = MockGit {
        current_branch: "master".to_string(),
        default_branch: Ok("master".to_string()),
        ..Default::default()
    };
    assert!(!RealGuardService::new(&git).check(&base_input()).allowed);
}

#[test]
fn develop_branch_blocked() {
    let git = MockGit {
        current_branch: "develop".to_string(),
        default_branch: Ok("develop".to_string()),
        ..Default::default()
    };
    assert!(!RealGuardService::new(&git).check(&base_input()).allowed);
}

#[test]
fn develop_protected_even_when_default_is_main() {
    let git = MockGit {
        current_branch: "develop".to_string(),
        default_branch: Ok("main".to_string()),
        ..Default::default()
    };
    let r = RealGuardService::new(&git).check(&base_input());
    assert!(!r.allowed);
    assert_eq!(r.current_branch.as_deref(), Some("develop"));
    assert_eq!(r.default_branch.as_deref(), Some("main"));
}

#[test]
fn extra_protected_branch_blocked() {
    let git = MockGit {
        current_branch: "staging".to_string(),
        default_branch: Ok("main".to_string()),
        ..Default::default()
    };
    let input = GuardInput {
        protected_branches: Some(vec!["staging".to_string(), "release".to_string()]),
        ..base_input()
    };
    let r = RealGuardService::new(&git).check(&input);
    assert!(!r.allowed);
    assert_eq!(r.current_branch.as_deref(), Some("staging"));
}

#[test]
fn non_protected_branch_allowed() {
    let git = MockGit {
        current_branch: "feat/something".to_string(),
        default_branch: Ok("main".to_string()),
        ..Default::default()
    };
    let input = GuardInput {
        protected_branches: Some(vec!["staging".to_string()]),
        ..base_input()
    };
    assert!(RealGuardService::new(&git).check(&input).allowed);
}

// ---------- default branch detection fallback ----------

#[test]
fn explicit_default_branch_used() {
    let git = MockGit {
        current_branch: "custom-default".to_string(),
        ..Default::default()
    };
    let input = GuardInput {
        default_branch: Some("custom-default".to_string()),
        ..base_input()
    };
    let r = RealGuardService::new(&git).check(&input);
    assert!(!r.allowed);
    assert_eq!(r.default_branch.as_deref(), Some("custom-default"));
}

#[test]
fn detect_called_when_default_absent() {
    let git = MockGit::default();
    let _ = RealGuardService::new(&git).check(&base_input());
    assert!(git.detect_called.get());
}

#[test]
fn detect_failure_passes() {
    let git = MockGit {
        default_branch: Err(()),
        ..Default::default()
    };
    assert!(RealGuardService::new(&git).check(&base_input()).allowed);
}

// ---------- target: write ----------

#[test]
fn write_guard_runs_without_tool_command() {
    let git = MockGit::default();
    assert!(!RealGuardService::new(&git).check(&base_input()).allowed);
}

#[test]
fn write_block_reason_has_guidance() {
    let git = MockGit::default();
    let r = RealGuardService::new(&git).check(&base_input());
    let reason = r.reason.unwrap();
    assert!(reason.contains("파일을 수정하려 합니다"));
    assert!(reason.contains("./create-branch.sh"));
}

// ---------- target: commit ----------

fn commit_input(cmd: Option<&str>) -> GuardInput {
    GuardInput {
        target: GuardTarget::Commit,
        tool_command: cmd.map(str::to_string),
        ..base_input()
    }
}

#[test]
fn commit_non_commit_command_passes() {
    let git = MockGit::default();
    let r = RealGuardService::new(&git).check(&commit_input(Some("git push origin main")));
    assert!(r.allowed);
}

#[test]
fn commit_command_on_default_blocked() {
    let git = MockGit::default();
    let r = RealGuardService::new(&git).check(&commit_input(Some("git commit -m \"test\"")));
    assert!(!r.allowed);
}

#[test]
fn commit_chained_command_blocked() {
    let git = MockGit::default();
    let r = RealGuardService::new(&git)
        .check(&commit_input(Some("git add . && git commit -m \"test\"")));
    assert!(!r.allowed);
}

#[test]
fn commit_git_log_passes() {
    let git = MockGit::default();
    let r = RealGuardService::new(&git).check(&commit_input(Some("git log --oneline")));
    assert!(r.allowed);
}

#[test]
fn commit_empty_command_passes() {
    let git = MockGit::default();
    assert!(
        RealGuardService::new(&git)
            .check(&commit_input(Some("")))
            .allowed
    );
}

#[test]
fn commit_undefined_command_passes() {
    let git = MockGit::default();
    assert!(
        RealGuardService::new(&git)
            .check(&commit_input(None))
            .allowed
    );
}

// ---------- block message format ----------

#[test]
fn block_message_has_branch_name() {
    let git = MockGit::default();
    let r = RealGuardService::new(&git).check(&base_input());
    let reason = r.reason.unwrap();
    assert!(reason.contains("main"));
    assert!(reason.contains("보호 브랜치"));
    assert_eq!(r.current_branch.as_deref(), Some("main"));
}

// ---------- external file path (write guard) ----------

#[test]
fn external_file_outside_git_repo_passes() {
    let git = MockGit::default();
    let input = GuardInput {
        project_dir: "/home/user/my-project".to_string(),
        tool_file_path: Some("/home/user/.claude/settings.json".to_string()),
        ..base_input()
    };
    let r = RealGuardService::new(&git).check(&input);
    assert!(r.allowed);
    assert_eq!(
        r.reason.as_deref(),
        Some("file is outside any git repository")
    );
}

#[test]
fn external_file_inside_other_git_repo_blocked() {
    let git = MockGit::default();
    let input = GuardInput {
        project_dir: "/some/other/project".to_string(),
        tool_file_path: Some(cwd_file()),
        ..base_input()
    };
    assert!(!RealGuardService::new(&git).check(&input).allowed);
}

#[test]
fn internal_file_on_default_blocked() {
    let git = MockGit::default();
    let input = GuardInput {
        project_dir: "/home/user/my-project".to_string(),
        tool_file_path: Some("/home/user/my-project/src/index.ts".to_string()),
        ..base_input()
    };
    assert!(!RealGuardService::new(&git).check(&input).allowed);
}

#[test]
fn internal_file_on_feature_passes() {
    let git = MockGit {
        current_branch: "feat/something".to_string(),
        ..Default::default()
    };
    let input = GuardInput {
        project_dir: "/home/user/my-project".to_string(),
        tool_file_path: Some("/home/user/my-project/src/index.ts".to_string()),
        ..base_input()
    };
    assert!(RealGuardService::new(&git).check(&input).allowed);
}

#[test]
fn no_tool_file_path_runs_guard() {
    let git = MockGit::default();
    assert!(!RealGuardService::new(&git).check(&base_input()).allowed);
}

#[test]
fn commit_target_ignores_tool_file_path() {
    let git = MockGit::default();
    let input = GuardInput {
        target: GuardTarget::Commit,
        tool_file_path: Some("/external/path/file.txt".to_string()),
        tool_command: Some("git commit -m \"test\"".to_string()),
        ..base_input()
    };
    assert!(!RealGuardService::new(&git).check(&input).allowed);
}

// ---------- is_inside_project_dir ----------

#[test]
fn inside_project_dir_nested_true() {
    assert!(is_inside_project_dir(
        "/home/user/project/src/file.ts",
        "/home/user/project"
    ));
}

#[test]
fn inside_project_dir_root_true() {
    assert!(is_inside_project_dir(
        "/home/user/project",
        "/home/user/project"
    ));
}

#[test]
fn inside_project_dir_outside_false() {
    assert!(!is_inside_project_dir(
        "/home/user/.claude/settings.json",
        "/home/user/project"
    ));
}

#[test]
fn inside_project_dir_similar_prefix_false() {
    assert!(!is_inside_project_dir(
        "/home/user/project-extra/file.ts",
        "/home/user/project"
    ));
}

#[test]
fn inside_project_dir_home_vs_project_false() {
    assert!(!is_inside_project_dir(
        "/Users/user/.claude/settings.json",
        "/Users/user/Documents/my-project"
    ));
}

// ---------- is_inside_any_git_repo ----------

#[test]
fn inside_any_git_repo_current_file_true() {
    assert!(is_inside_any_git_repo(&cwd_file()));
}

#[test]
fn inside_any_git_repo_deep_nonexistent_true() {
    let p = format!(
        "{}/some/deep/nonexistent/file.ts",
        std::env::current_dir().unwrap().display()
    );
    assert!(is_inside_any_git_repo(&p));
}

#[test]
fn inside_any_git_repo_tmp_root_false() {
    assert!(!is_inside_any_git_repo("/tmp/random-file.txt"));
}
