//! Git operations, ported from git-utils `core/git.ts`.
//!
//! [`GitService`] is the injectable boundary: command-layer code depends on
//! the trait so tests can supply mocks, while [`RealGitService`] shells out to
//! the `git` binary. Methods are synchronous (blocking subprocess calls), the
//! idiomatic shape for a CLI.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

use super::shell::{exec, exec_or_throw};

/// Special in-progress repository states (rebase / merge / detached HEAD).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSpecialState {
    pub rebase: bool,
    pub merge: bool,
    pub detached: bool,
}

/// Where to look when checking whether a branch exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchLocation {
    Local,
    Remote,
    Any,
}

/// Git operations used by the command layer. Injectable for testing.
pub trait GitService {
    fn detect_default_branch(&self) -> Result<String>;
    fn get_current_branch(&self) -> Result<String>;
    fn branch_exists(&self, name: &str, location: BranchLocation) -> Result<bool>;
    fn is_inside_work_tree(&self) -> Result<bool>;
    fn has_uncommitted_changes(&self) -> Result<bool>;
    fn get_special_state(&self) -> Result<GitSpecialState>;
    fn fetch(&self, remote: Option<&str>) -> Result<()>;
    fn checkout(&self, branch: &str, create: bool, track: Option<&str>) -> Result<()>;
    fn commit(&self, message: &str) -> Result<()>;
    fn push(&self, branch: &str, set_upstream: bool) -> Result<()>;
    fn pull(&self, branch: &str) -> Result<()>;
    fn add_tracked(&self) -> Result<()>;
}

/// Real implementation that shells out to `git`, optionally scoped to `cwd`.
pub struct RealGitService {
    cwd: Option<PathBuf>,
}

impl RealGitService {
    pub fn new(cwd: Option<PathBuf>) -> Self {
        Self { cwd }
    }

    fn cwd(&self) -> Option<&Path> {
        self.cwd.as_deref()
    }

    /// `git <args>` strictly (errors on non-zero exit).
    fn git(&self, args: &[&str]) -> Result<String> {
        let mut command = vec!["git"];
        command.extend_from_slice(args);
        exec_or_throw(&command, self.cwd())
    }

    /// `git <args>` returning `(stdout, exit_code)` without failing.
    fn git_safe(&self, args: &[&str]) -> Result<(String, i32)> {
        let mut command = vec!["git"];
        command.extend_from_slice(args);
        let r = exec(&command, self.cwd())?;
        Ok((r.stdout, r.exit_code))
    }
}

impl GitService for RealGitService {
    fn detect_default_branch(&self) -> Result<String> {
        // Method 1: cached origin/HEAD.
        let (head, head_exit) = self.git_safe(&["symbolic-ref", "refs/remotes/origin/HEAD"])?;
        if head_exit == 0 && !head.is_empty() {
            return Ok(head.replace("refs/remotes/origin/", ""));
        }

        // Method 2: auto-detect from the remote.
        let _ = self.git_safe(&["remote", "set-head", "origin", "--auto"]);
        let (head2, head_exit2) = self.git_safe(&["symbolic-ref", "refs/remotes/origin/HEAD"])?;
        if head_exit2 == 0 && !head2.is_empty() {
            return Ok(head2.replace("refs/remotes/origin/", ""));
        }

        // Method 3: fall back to common names.
        for name in ["main", "develop", "master"] {
            let r = format!("refs/remotes/origin/{name}");
            let (_, exit) = self.git_safe(&["show-ref", "--verify", "--quiet", &r])?;
            if exit == 0 {
                return Ok(name.to_string());
            }
        }

        bail!("Could not detect default branch. Make sure you have a remote configured.")
    }

    fn get_current_branch(&self) -> Result<String> {
        let (stdout, exit) = self.git_safe(&["branch", "--show-current"])?;
        if exit != 0 {
            return Ok(String::new());
        }
        Ok(stdout)
    }

    fn branch_exists(&self, name: &str, location: BranchLocation) -> Result<bool> {
        if matches!(location, BranchLocation::Local | BranchLocation::Any) {
            let r = format!("refs/heads/{name}");
            let (_, exit) = self.git_safe(&["show-ref", "--verify", "--quiet", &r])?;
            if exit == 0 {
                return Ok(true);
            }
        }
        if matches!(location, BranchLocation::Remote | BranchLocation::Any) {
            let r = format!("refs/remotes/origin/{name}");
            let (_, exit) = self.git_safe(&["show-ref", "--verify", "--quiet", &r])?;
            if exit == 0 {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn is_inside_work_tree(&self) -> Result<bool> {
        let (_, exit) = self.git_safe(&["rev-parse", "--is-inside-work-tree"])?;
        Ok(exit == 0)
    }

    fn has_uncommitted_changes(&self) -> Result<bool> {
        let (stdout, _) = self.git_safe(&["status", "--porcelain"])?;
        Ok(!stdout.is_empty())
    }

    fn get_special_state(&self) -> Result<GitSpecialState> {
        let (git_dir, _) = self.git_safe(&["rev-parse", "--git-dir"])?;
        // `git rev-parse --git-dir` is relative to the service cwd; resolve it
        // against cwd so existence checks point at the real repo.
        let base = self.cwd().unwrap_or_else(|| Path::new("."));
        let git_dir_path = {
            let p = Path::new(&git_dir);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                base.join(p)
            }
        };
        let rebase = git_dir_path.join("rebase-merge").exists()
            || git_dir_path.join("rebase-apply").exists();
        let merge = git_dir_path.join("MERGE_HEAD").exists();
        let detached = self.get_current_branch()?.is_empty();
        Ok(GitSpecialState {
            rebase,
            merge,
            detached,
        })
    }

    fn fetch(&self, remote: Option<&str>) -> Result<()> {
        let remote = remote.unwrap_or("origin");
        exec(&["git", "fetch", remote, "--prune"], self.cwd())?;
        Ok(())
    }

    fn checkout(&self, branch: &str, create: bool, track: Option<&str>) -> Result<()> {
        let mut args: Vec<&str> = vec!["checkout"];
        if create {
            args.push("-b");
        }
        args.push(branch);
        if let Some(t) = track {
            args.push("--track");
            args.push(t);
        }
        self.git(&args)?;
        Ok(())
    }

    fn commit(&self, message: &str) -> Result<()> {
        self.git(&["commit", "-m", message])?;
        Ok(())
    }

    fn push(&self, branch: &str, set_upstream: bool) -> Result<()> {
        let mut args: Vec<&str> = vec!["push"];
        if set_upstream {
            args.push("-u");
        }
        args.push("origin");
        args.push(branch);
        self.git(&args)?;
        Ok(())
    }

    fn pull(&self, branch: &str) -> Result<()> {
        exec(&["git", "pull", "origin", branch], self.cwd())?;
        Ok(())
    }

    fn add_tracked(&self) -> Result<()> {
        self.git(&["add", "-u"])?;
        Ok(())
    }
}
