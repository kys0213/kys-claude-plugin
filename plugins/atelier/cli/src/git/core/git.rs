//! Git reads consumed by the branch guard — a trimmed port of
//! `git-utils/src/core/git.ts`. After the git CLI was narrowed to its
//! mechanical surface (guard/hook/reviews), the guard is the only consumer of
//! `GitService`, so the trait exposes just the three reads it needs;
//! `RealGitService` shells out via `core::shell`. Commit/branch/PR flows now
//! run as plain git/gh under the `git` skill's conventions, not through here.

use crate::git::core::shell::{exec, ExecOptions};
use crate::git::types::GitSpecialState;

pub trait GitService {
    /// Detects the repository's default branch WITHOUT mutating repo state
    /// (no `git remote set-head`), so it is safe to call from a PreToolUse
    /// guard on every tool invocation (#779). Method 1 reads the cached
    /// `refs/remotes/origin/HEAD`; Method 3 probes common branch names.
    fn detect_default_branch_readonly(&self) -> Result<String, String>;
    fn is_inside_work_tree(&self) -> bool;
    fn get_special_state(&self) -> GitSpecialState;
}

/// Real `GitService` bound to an optional working directory.
pub struct RealGitService {
    cwd: Option<String>,
}

/// Constructs the real git service, optionally pinned to `cwd`.
pub fn create_git_service(cwd: Option<String>) -> RealGitService {
    RealGitService { cwd }
}

const NO_DEFAULT_BRANCH: &str =
    "Could not detect default branch. Make sure you have a remote configured.";

impl RealGitService {
    fn opts(&self) -> Option<ExecOptions> {
        self.cwd.as_ref().map(|cwd| ExecOptions {
            cwd: Some(cwd.clone()),
            env: None,
        })
    }

    /// `exec(['git', ...args])` returning (stdout, exit_code).
    fn git_safe(&self, args: &[&str]) -> (String, i32) {
        let mut full = vec!["git"];
        full.extend_from_slice(args);
        let r = exec(&full, self.opts().as_ref());
        (r.stdout, r.exit_code)
    }

    /// Method 1: read the cached `refs/remotes/origin/HEAD` symbolic ref.
    /// Pure read — never mutates repo state.
    fn read_origin_head(&self) -> Option<String> {
        let (head, exit) = self.git_safe(&["symbolic-ref", "refs/remotes/origin/HEAD"]);
        if exit == 0 && !head.is_empty() {
            Some(head.replace("refs/remotes/origin/", ""))
        } else {
            None
        }
    }

    /// Method 3: probe common default-branch names on the remote. Pure read.
    fn probe_common_default(&self) -> Option<String> {
        for name in ["main", "develop", "master"] {
            let (_, exit) = self.git_safe(&[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/remotes/origin/{name}"),
            ]);
            if exit == 0 {
                return Some(name.to_string());
            }
        }
        None
    }

    /// Current branch name, empty on detached HEAD. Private helper for
    /// `get_special_state` (the guard reads the branch off the state snapshot).
    fn current_branch(&self) -> String {
        let (stdout, exit) = self.git_safe(&["branch", "--show-current"]);
        if exit != 0 {
            return String::new();
        }
        stdout
    }
}

impl GitService for RealGitService {
    fn detect_default_branch_readonly(&self) -> Result<String, String> {
        // Method 1 + Method 3 only — no `set-head` write (see #779).
        self.read_origin_head()
            .or_else(|| self.probe_common_default())
            .ok_or_else(|| NO_DEFAULT_BRANCH.to_string())
    }

    fn is_inside_work_tree(&self) -> bool {
        let (_, exit) = self.git_safe(&["rev-parse", "--is-inside-work-tree"]);
        exit == 0
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
        GitSpecialState {
            rebase,
            merge,
            current_branch: self.current_branch(),
        }
    }
}
