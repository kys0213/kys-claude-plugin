//! Black-box integration port of git-utils `tests/core/git.test.ts`.
//! Exercises `RealGitService` against real temporary git repositories.

use std::path::Path;
use std::process::Command;

use atelier::git::core::git::{BranchLocation, GitService, GitSpecialState, RealGitService};
use tempfile::TempDir;

/// Run a git command in `dir`, asserting success and returning trimmed stdout.
fn run(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

struct Repo {
    local: TempDir,
    _remote: TempDir,
}

/// Build a bare remote + local repo on `branch` with one initial commit
/// pushed, optionally setting `origin/HEAD`.
fn mk_repo(branch: &str, set_origin_head: bool) -> Repo {
    let remote = TempDir::new().unwrap();
    run(remote.path(), &["init", "--bare"]);

    let local = TempDir::new().unwrap();
    run(local.path(), &["init", "-b", branch]);
    run(local.path(), &["config", "user.email", "test@test.com"]);
    run(local.path(), &["config", "user.name", "Test"]);
    run(local.path(), &["config", "commit.gpgsign", "false"]);
    std::fs::write(local.path().join("README.md"), "init").unwrap();
    run(local.path(), &["add", "."]);
    run(local.path(), &["commit", "-m", "init"]);
    run(
        local.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    run(local.path(), &["push", "-u", "origin", branch]);
    if set_origin_head {
        run(
            local.path(),
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                &format!("refs/remotes/origin/{branch}"),
            ],
        );
    }
    Repo {
        local,
        _remote: remote,
    }
}

fn svc(repo: &Repo) -> RealGitService {
    RealGitService::new(Some(repo.local.path().to_path_buf()))
}

// ---------- detect_default_branch ----------

#[test]
fn detect_default_branch_from_origin_head() {
    let repo = mk_repo("main", true);
    assert_eq!(svc(&repo).detect_default_branch().unwrap(), "main");
}

#[test]
fn detect_default_branch_falls_back_to_main() {
    let repo = mk_repo("main", true);
    run(
        repo.local.path(),
        &["symbolic-ref", "--delete", "refs/remotes/origin/HEAD"],
    );
    assert_eq!(svc(&repo).detect_default_branch().unwrap(), "main");
}

#[test]
fn detect_default_branch_master_only() {
    let repo = mk_repo("master", false);
    assert_eq!(svc(&repo).detect_default_branch().unwrap(), "master");
}

#[test]
fn detect_default_branch_develop_only() {
    let repo = mk_repo("develop", false);
    assert_eq!(svc(&repo).detect_default_branch().unwrap(), "develop");
}

#[test]
fn detect_default_branch_no_remote_errors() {
    let dir = TempDir::new().unwrap();
    run(dir.path(), &["init"]);
    run(dir.path(), &["config", "user.email", "test@test.com"]);
    run(dir.path(), &["config", "user.name", "Test"]);
    run(dir.path(), &["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.path().join("README.md"), "init").unwrap();
    run(dir.path(), &["add", "."]);
    run(dir.path(), &["commit", "-m", "init"]);

    let s = RealGitService::new(Some(dir.path().to_path_buf()));
    let err = s.detect_default_branch().unwrap_err();
    assert!(err.to_string().contains("Could not detect default branch"));
}

// ---------- get_current_branch ----------

#[test]
fn current_branch_normal() {
    let repo = mk_repo("main", true);
    assert_eq!(svc(&repo).get_current_branch().unwrap(), "main");
}

#[test]
fn current_branch_detached_is_empty() {
    let repo = mk_repo("main", true);
    let hash = run(repo.local.path(), &["rev-parse", "HEAD"]);
    run(repo.local.path(), &["checkout", &hash]);
    assert_eq!(svc(&repo).get_current_branch().unwrap(), "");
}

// ---------- branch_exists ----------

#[test]
fn branch_exists_local_true() {
    let repo = mk_repo("main", true);
    assert!(svc(&repo)
        .branch_exists("main", BranchLocation::Local)
        .unwrap());
}

#[test]
fn branch_exists_local_false() {
    let repo = mk_repo("main", true);
    assert!(!svc(&repo)
        .branch_exists("nonexistent", BranchLocation::Local)
        .unwrap());
}

#[test]
fn branch_exists_remote_true() {
    let repo = mk_repo("main", true);
    assert!(svc(&repo)
        .branch_exists("main", BranchLocation::Remote)
        .unwrap());
}

#[test]
fn branch_exists_any_false() {
    let repo = mk_repo("main", true);
    assert!(!svc(&repo)
        .branch_exists("nonexistent", BranchLocation::Any)
        .unwrap());
}

// ---------- is_inside_work_tree ----------

#[test]
fn inside_work_tree_true() {
    let repo = mk_repo("main", true);
    assert!(svc(&repo).is_inside_work_tree().unwrap());
}

#[test]
fn inside_work_tree_false_outside_repo() {
    let non_git = TempDir::new().unwrap();
    let s = RealGitService::new(Some(non_git.path().to_path_buf()));
    assert!(!s.is_inside_work_tree().unwrap());
}

// ---------- has_uncommitted_changes ----------

#[test]
fn uncommitted_none_false() {
    let repo = mk_repo("main", true);
    assert!(!svc(&repo).has_uncommitted_changes().unwrap());
}

#[test]
fn uncommitted_unstaged_true() {
    let repo = mk_repo("main", true);
    std::fs::write(repo.local.path().join("README.md"), "modified").unwrap();
    assert!(svc(&repo).has_uncommitted_changes().unwrap());
}

#[test]
fn uncommitted_staged_true() {
    let repo = mk_repo("main", true);
    std::fs::write(repo.local.path().join("README.md"), "staged").unwrap();
    run(repo.local.path(), &["add", "README.md"]);
    assert!(svc(&repo).has_uncommitted_changes().unwrap());
}

#[test]
fn uncommitted_untracked_true() {
    let repo = mk_repo("main", true);
    std::fs::write(repo.local.path().join("new-file.txt"), "new").unwrap();
    assert!(svc(&repo).has_uncommitted_changes().unwrap());
}

// ---------- get_special_state ----------

#[test]
fn special_state_normal() {
    let repo = mk_repo("main", true);
    assert_eq!(
        svc(&repo).get_special_state().unwrap(),
        GitSpecialState {
            rebase: false,
            merge: false,
            detached: false,
        }
    );
}

#[test]
fn special_state_detached() {
    let repo = mk_repo("main", true);
    let hash = run(repo.local.path(), &["rev-parse", "HEAD"]);
    run(repo.local.path(), &["checkout", &hash]);
    assert!(svc(&repo).get_special_state().unwrap().detached);
}

// ---------- add_tracked ----------

#[test]
fn add_tracked_stages_tracked_change() {
    let repo = mk_repo("main", true);
    std::fs::write(repo.local.path().join("README.md"), "tracked change").unwrap();
    svc(&repo).add_tracked().unwrap();
    let staged = run(repo.local.path(), &["diff", "--cached", "--name-only"]);
    assert!(staged.contains("README.md"));
}

#[test]
fn add_tracked_ignores_untracked() {
    let repo = mk_repo("main", true);
    std::fs::write(repo.local.path().join("untracked.txt"), "new file").unwrap();
    std::fs::write(repo.local.path().join("README.md"), "tracked change").unwrap();
    svc(&repo).add_tracked().unwrap();
    let staged = run(repo.local.path(), &["diff", "--cached", "--name-only"]);
    assert!(!staged.contains("untracked.txt"));
    assert!(staged.contains("README.md"));
}
