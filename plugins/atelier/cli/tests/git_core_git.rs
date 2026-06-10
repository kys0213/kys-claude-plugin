//! Port of `git-utils/tests/core/git.test.ts` — integration tests against real
//! temporary git repositories (bare remote + local clone), exercising
//! `RealGitService` end-to-end.

use atelier::git::core::git::{create_git_service, BranchLocation, GitService};
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Runs a git/shell command in `cwd`, panicking on non-zero exit.
fn sh(cmd: &[&str], cwd: &Path) -> String {
    let out = Command::new(cmd[0])
        .args(&cmd[1..])
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("spawn {cmd:?}: {e}"));
    assert!(
        out.status.success(),
        "{cmd:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

/// Configures git identity + disables gpg signing on a fresh repo dir.
fn config_identity(dir: &Path) {
    sh(&["git", "config", "user.email", "test@test.com"], dir);
    sh(&["git", "config", "user.name", "Test"], dir);
    sh(&["git", "config", "commit.gpgsign", "false"], dir);
}

/// Bare remote + local clone on `main` with one pushed commit and origin/HEAD
/// pointing at main. Returns `(remote, local)` temp dirs (kept alive by caller).
fn setup() -> (TempDir, TempDir) {
    let remote = TempDir::new().unwrap();
    sh(&["git", "init", "--bare"], remote.path());

    let local = TempDir::new().unwrap();
    sh(&["git", "init", "-b", "main"], local.path());
    config_identity(local.path());
    std::fs::write(local.path().join("README.md"), "init").unwrap();
    sh(&["git", "add", "."], local.path());
    sh(&["git", "commit", "-m", "init"], local.path());
    sh(
        &[
            "git",
            "remote",
            "add",
            "origin",
            remote.path().to_str().unwrap(),
        ],
        local.path(),
    );
    sh(&["git", "push", "-u", "origin", "main"], local.path());
    sh(
        &[
            "git",
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
        local.path(),
    );
    (remote, local)
}

/// Helper to build a service rooted at a temp dir path.
fn svc(dir: &Path) -> impl GitService {
    create_git_service(Some(dir.to_string_lossy().to_string()))
}

#[test]
fn detect_default_branch_from_origin_head() {
    let (_r, local) = setup();
    assert_eq!(svc(local.path()).detect_default_branch().unwrap(), "main");
}

#[test]
fn detect_default_branch_fallback_main() {
    let (_r, local) = setup();
    sh(
        &[
            "git",
            "symbolic-ref",
            "--delete",
            "refs/remotes/origin/HEAD",
        ],
        local.path(),
    );
    assert_eq!(svc(local.path()).detect_default_branch().unwrap(), "main");
}

#[test]
fn detect_default_branch_master() {
    let remote = TempDir::new().unwrap();
    sh(&["git", "init", "--bare"], remote.path());
    let local = TempDir::new().unwrap();
    sh(&["git", "init", "-b", "master"], local.path());
    config_identity(local.path());
    std::fs::write(local.path().join("README.md"), "init").unwrap();
    sh(&["git", "add", "."], local.path());
    sh(&["git", "commit", "-m", "init"], local.path());
    sh(
        &[
            "git",
            "remote",
            "add",
            "origin",
            remote.path().to_str().unwrap(),
        ],
        local.path(),
    );
    sh(&["git", "push", "-u", "origin", "master"], local.path());
    assert_eq!(svc(local.path()).detect_default_branch().unwrap(), "master");
}

#[test]
fn detect_default_branch_develop() {
    let remote = TempDir::new().unwrap();
    sh(&["git", "init", "--bare"], remote.path());
    let local = TempDir::new().unwrap();
    sh(&["git", "init", "-b", "develop"], local.path());
    config_identity(local.path());
    std::fs::write(local.path().join("README.md"), "init").unwrap();
    sh(&["git", "add", "."], local.path());
    sh(&["git", "commit", "-m", "init"], local.path());
    sh(
        &[
            "git",
            "remote",
            "add",
            "origin",
            remote.path().to_str().unwrap(),
        ],
        local.path(),
    );
    sh(&["git", "push", "-u", "origin", "develop"], local.path());
    assert_eq!(
        svc(local.path()).detect_default_branch().unwrap(),
        "develop"
    );
}

#[test]
fn detect_default_branch_readonly_uses_cached_head() {
    let (_r, local) = setup();
    assert_eq!(
        svc(local.path()).detect_default_branch_readonly().unwrap(),
        "main"
    );
}

#[test]
fn detect_default_branch_readonly_does_not_write_origin_head() {
    let (_r, local) = setup();
    // Remove the cached symbolic ref so Method 1 misses; readonly must fall
    // back to Method 3 (probe common names) WITHOUT running `set-head`.
    sh(
        &[
            "git",
            "symbolic-ref",
            "--delete",
            "refs/remotes/origin/HEAD",
        ],
        local.path(),
    );

    assert_eq!(
        svc(local.path()).detect_default_branch_readonly().unwrap(),
        "main"
    );

    // The ref must still be absent — readonly detection has no side effects.
    let head_present = Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .current_dir(local.path())
        .output()
        .unwrap()
        .status
        .success();
    assert!(
        !head_present,
        "detect_default_branch_readonly must not recreate refs/remotes/origin/HEAD"
    );
}

#[test]
fn detect_default_branch_no_remote_errors() {
    let local = TempDir::new().unwrap();
    sh(&["git", "init"], local.path());
    config_identity(local.path());
    std::fs::write(local.path().join("README.md"), "init").unwrap();
    sh(&["git", "add", "."], local.path());
    sh(&["git", "commit", "-m", "init"], local.path());
    let err = svc(local.path()).detect_default_branch().unwrap_err();
    assert!(err.contains("Could not detect default branch"));
}

#[test]
fn current_branch_normal() {
    let (_r, local) = setup();
    assert_eq!(svc(local.path()).get_current_branch(), "main");
}

#[test]
fn current_branch_detached_is_empty() {
    let (_r, local) = setup();
    let hash = sh(&["git", "rev-parse", "HEAD"], local.path());
    sh(&["git", "checkout", &hash], local.path());
    assert_eq!(svc(local.path()).get_current_branch(), "");
}

#[test]
fn branch_exists_local() {
    let (_r, local) = setup();
    assert!(svc(local.path()).branch_exists("main", BranchLocation::Local));
}

#[test]
fn branch_not_exists_local() {
    let (_r, local) = setup();
    assert!(!svc(local.path()).branch_exists("nonexistent", BranchLocation::Local));
}

#[test]
fn branch_exists_remote() {
    let (_r, local) = setup();
    assert!(svc(local.path()).branch_exists("main", BranchLocation::Remote));
}

#[test]
fn branch_not_exists_any() {
    let (_r, local) = setup();
    assert!(!svc(local.path()).branch_exists("nonexistent", BranchLocation::Any));
}

#[test]
fn inside_work_tree_true() {
    let (_r, local) = setup();
    assert!(svc(local.path()).is_inside_work_tree());
}

#[test]
fn inside_work_tree_false() {
    let non_git = TempDir::new().unwrap();
    assert!(!svc(non_git.path()).is_inside_work_tree());
}

#[test]
fn uncommitted_none() {
    let (_r, local) = setup();
    assert!(!svc(local.path()).has_uncommitted_changes());
}

#[test]
fn uncommitted_unstaged() {
    let (_r, local) = setup();
    std::fs::write(local.path().join("README.md"), "modified").unwrap();
    assert!(svc(local.path()).has_uncommitted_changes());
}

#[test]
fn uncommitted_staged() {
    let (_r, local) = setup();
    std::fs::write(local.path().join("README.md"), "staged").unwrap();
    sh(&["git", "add", "README.md"], local.path());
    assert!(svc(local.path()).has_uncommitted_changes());
}

#[test]
fn uncommitted_untracked() {
    let (_r, local) = setup();
    std::fs::write(local.path().join("new-file.txt"), "new").unwrap();
    assert!(svc(local.path()).has_uncommitted_changes());
}

#[test]
fn special_state_normal() {
    let (_r, local) = setup();
    let state = svc(local.path()).get_special_state();
    assert!(!state.rebase && !state.merge && !state.detached);
}

#[test]
fn special_state_detached() {
    let (_r, local) = setup();
    let hash = sh(&["git", "rev-parse", "HEAD"], local.path());
    sh(&["git", "checkout", &hash], local.path());
    assert!(svc(local.path()).get_special_state().detached);
}

#[test]
fn add_tracked_stages_tracked() {
    let (_r, local) = setup();
    std::fs::write(local.path().join("README.md"), "tracked change").unwrap();
    svc(local.path()).add_tracked().unwrap();
    let staged = sh(&["git", "diff", "--cached", "--name-only"], local.path());
    assert!(staged.contains("README.md"));
}

#[test]
fn add_tracked_ignores_untracked() {
    let (_r, local) = setup();
    std::fs::write(local.path().join("untracked.txt"), "new file").unwrap();
    std::fs::write(local.path().join("README.md"), "tracked change").unwrap();
    svc(local.path()).add_tracked().unwrap();
    let staged = sh(&["git", "diff", "--cached", "--name-only"], local.path());
    assert!(!staged.contains("untracked.txt"));
    assert!(staged.contains("README.md"));
}
