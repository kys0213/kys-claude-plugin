//! Git operations — port of `git-utils/src/core/git.ts`. The `GitService`
//! trait abstracts the git CLI so commands can be unit-tested with mocks
//! (constructor injection); `RealGitService` shells out via `core::shell`.
//! Method semantics (argument order, fallback chains, error propagation)
//! match the TS factory exactly.

use crate::git::core::shell::{exec, exec_or_throw, ExecOptions};
use crate::git::types::GitSpecialState;

/// Options for `checkout`.
#[derive(Debug, Clone, Default)]
pub struct CheckoutOptions {
    pub create: bool,
    pub track: Option<String>,
}

/// Options for `push`.
#[derive(Debug, Clone, Default)]
pub struct PushOptions {
    pub set_upstream: bool,
}

pub trait GitService {
    fn detect_default_branch(&self) -> Result<String, String>;
    fn get_current_branch(&self) -> String;
    fn branch_exists(&self, name: &str, location: BranchLocation) -> bool;
    fn is_inside_work_tree(&self) -> bool;
    fn has_uncommitted_changes(&self) -> bool;
    fn get_special_state(&self) -> GitSpecialState;
    fn fetch(&self, remote: Option<&str>) -> Result<(), String>;
    fn checkout(&self, branch: &str, options: Option<&CheckoutOptions>) -> Result<(), String>;
    fn commit(&self, message: &str) -> Result<(), String>;
    fn push(&self, branch: &str, options: Option<&PushOptions>) -> Result<(), String>;
    fn pull(&self, branch: &str) -> Result<(), String>;
    fn add_tracked(&self) -> Result<(), String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchLocation {
    Local,
    Remote,
    Any,
}

/// Real `GitService` bound to an optional working directory.
pub struct RealGitService {
    cwd: Option<String>,
}

/// Constructs the real git service, optionally pinned to `cwd`.
pub fn create_git_service(cwd: Option<String>) -> RealGitService {
    RealGitService { cwd }
}

impl RealGitService {
    fn opts(&self) -> Option<ExecOptions> {
        self.cwd.as_ref().map(|cwd| ExecOptions {
            cwd: Some(cwd.clone()),
            env: None,
        })
    }

    /// `execOrThrow(['git', ...args])`.
    fn git(&self, args: &[&str]) -> Result<String, String> {
        let mut full = vec!["git"];
        full.extend_from_slice(args);
        exec_or_throw(&full, self.opts().as_ref())
    }

    /// `exec(['git', ...args])` returning (stdout, exit_code).
    fn git_safe(&self, args: &[&str]) -> (String, i32) {
        let mut full = vec!["git"];
        full.extend_from_slice(args);
        let r = exec(&full, self.opts().as_ref());
        (r.stdout, r.exit_code)
    }
}

impl GitService for RealGitService {
    fn detect_default_branch(&self) -> Result<String, String> {
        // Method 1: cached origin/HEAD
        let (head, head_exit) = self.git_safe(&["symbolic-ref", "refs/remotes/origin/HEAD"]);
        if head_exit == 0 && !head.is_empty() {
            return Ok(head.replace("refs/remotes/origin/", ""));
        }

        // Method 2: auto-detect from remote
        let _ = exec(
            &["git", "remote", "set-head", "origin", "--auto"],
            self.opts().as_ref(),
        );
        let (head2, head_exit2) = self.git_safe(&["symbolic-ref", "refs/remotes/origin/HEAD"]);
        if head_exit2 == 0 && !head2.is_empty() {
            return Ok(head2.replace("refs/remotes/origin/", ""));
        }

        // Method 3: fallback to common names
        for name in ["main", "develop", "master"] {
            let (_, exit) = self.git_safe(&[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/remotes/origin/{name}"),
            ]);
            if exit == 0 {
                return Ok(name.to_string());
            }
        }

        Err("Could not detect default branch. Make sure you have a remote configured.".to_string())
    }

    fn get_current_branch(&self) -> String {
        let (stdout, exit) = self.git_safe(&["branch", "--show-current"]);
        if exit != 0 {
            return String::new();
        }
        stdout
    }

    fn branch_exists(&self, name: &str, location: BranchLocation) -> bool {
        if matches!(location, BranchLocation::Local | BranchLocation::Any) {
            let (_, exit) = self.git_safe(&[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{name}"),
            ]);
            if exit == 0 {
                return true;
            }
        }
        if matches!(location, BranchLocation::Remote | BranchLocation::Any) {
            let (_, exit) = self.git_safe(&[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/remotes/origin/{name}"),
            ]);
            if exit == 0 {
                return true;
            }
        }
        false
    }

    fn is_inside_work_tree(&self) -> bool {
        let (_, exit) = self.git_safe(&["rev-parse", "--is-inside-work-tree"]);
        exit == 0
    }

    fn has_uncommitted_changes(&self) -> bool {
        let (stdout, _) = self.git_safe(&["status", "--porcelain"]);
        !stdout.is_empty()
    }

    fn get_special_state(&self) -> GitSpecialState {
        let (git_dir, _) = self.git_safe(&["rev-parse", "--git-dir"]);
        // git-dir is relative to cwd; resolve against the configured cwd so
        // `exists` checks hit the right path regardless of process cwd.
        let base = self.cwd.clone();
        let resolve = |suffix: &str| -> std::path::PathBuf {
            let p = std::path::Path::new(&git_dir).join(suffix);
            if p.is_absolute() {
                p
            } else if let Some(base) = &base {
                std::path::Path::new(base).join(&p)
            } else {
                p
            }
        };
        let rebase = resolve("rebase-merge").exists() || resolve("rebase-apply").exists();
        let merge = resolve("MERGE_HEAD").exists();
        let detached = self.get_current_branch().is_empty();
        GitSpecialState {
            rebase,
            merge,
            detached,
        }
    }

    fn fetch(&self, remote: Option<&str>) -> Result<(), String> {
        let remote = remote.unwrap_or("origin");
        // Mirrors TS: uses `exec` (ignores failures at the call site).
        let _ = exec(&["git", "fetch", remote, "--prune"], self.opts().as_ref());
        Ok(())
    }

    fn checkout(&self, branch: &str, options: Option<&CheckoutOptions>) -> Result<(), String> {
        let mut args: Vec<String> = vec!["checkout".to_string()];
        if options.map(|o| o.create).unwrap_or(false) {
            args.push("-b".to_string());
        }
        args.push(branch.to_string());
        if let Some(track) = options.and_then(|o| o.track.as_ref()) {
            args.push("--track".to_string());
            args.push(track.clone());
        }
        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.git(&arg_refs).map(|_| ())
    }

    fn commit(&self, message: &str) -> Result<(), String> {
        self.git(&["commit", "-m", message]).map(|_| ())
    }

    fn push(&self, branch: &str, options: Option<&PushOptions>) -> Result<(), String> {
        let mut args: Vec<&str> = vec!["push"];
        if options.map(|o| o.set_upstream).unwrap_or(false) {
            args.push("-u");
        }
        args.push("origin");
        args.push(branch);
        self.git(&args).map(|_| ())
    }

    fn pull(&self, branch: &str) -> Result<(), String> {
        // Mirrors TS: uses `exec` (failures ignored at call site).
        let _ = exec(&["git", "pull", "origin", branch], self.opts().as_ref());
        Ok(())
    }

    fn add_tracked(&self) -> Result<(), String> {
        self.git(&["add", "-u"]).map(|_| ())
    }
}
