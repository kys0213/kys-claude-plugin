use anyhow::{bail, Context, Result};
use std::process::Command;

/// Abstraction over git CLI operations for testability.
pub trait GitOps: Send + Sync {
    /// Return the current HEAD commit hash.
    fn rev_parse_head(&self) -> Result<String>;

    /// List files changed between two commits (--name-only).
    fn diff_name_only(&self, from: &str, to: &str) -> Result<Vec<String>>;

    /// Check whether a commit hash exists in the repository.
    fn commit_exists(&self, hash: &str) -> Result<bool>;

    /// Return the URL of a named remote (e.g. "origin").
    fn remote_url(&self, name: &str) -> Result<String>;

    /// Return the basename of the repository root directory.
    fn repo_name(&self) -> Result<String>;

    /// Fetch a specific branch from a remote.
    fn fetch_remote(&self, remote: &str, branch: &str) -> Result<()>;

    /// Resolve an arbitrary ref to a commit hash.
    fn rev_parse_ref(&self, refname: &str) -> Result<String>;

    /// Count commits in a range (from..to).
    fn rev_list_count(&self, from: &str, to: &str) -> Result<u64>;

    /// List all worktrees and their associated branches.
    fn worktree_list(&self) -> Result<Vec<WorktreeEntry>>;

    /// Force-remove a worktree at the given path.
    fn worktree_remove(&self, path: &str) -> Result<()>;

    /// Prune stale worktree metadata.
    fn worktree_prune(&self) -> Result<()>;

    /// Delete a local branch.
    fn branch_delete(&self, name: &str) -> Result<()>;
}

/// A worktree entry from `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub path: String,
    pub branch: Option<String>,
}

/// Real implementation that shells out to `git`.
pub struct RealGit;

impl GitOps for RealGit {
    fn rev_parse_head(&self) -> Result<String> {
        run_git(&["rev-parse", "HEAD"])
    }

    fn diff_name_only(&self, from: &str, to: &str) -> Result<Vec<String>> {
        let range = format!("{from}..{to}");
        let output = run_git(&["diff", "--name-only", &range])?;
        Ok(output
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect())
    }

    fn commit_exists(&self, hash: &str) -> Result<bool> {
        let output = Command::new("git")
            .args(["cat-file", "-e", hash])
            .output()
            .context("git not found")?;
        Ok(output.status.success())
    }

    fn remote_url(&self, name: &str) -> Result<String> {
        run_git(&["remote", "get-url", name])
    }

    fn repo_name(&self) -> Result<String> {
        let root = run_git(&["rev-parse", "--show-toplevel"])?;
        let name = root.rsplit('/').next().unwrap_or("unknown").to_string();
        Ok(name)
    }

    fn fetch_remote(&self, remote: &str, branch: &str) -> Result<()> {
        let _ = Command::new("git")
            .args(["fetch", remote, branch, "--quiet"])
            .output()
            .context("git not found")?;
        Ok(())
    }

    fn rev_parse_ref(&self, refname: &str) -> Result<String> {
        run_git(&["rev-parse", refname])
    }

    fn rev_list_count(&self, from: &str, to: &str) -> Result<u64> {
        let range = format!("{from}..{to}");
        let output = run_git(&["rev-list", "--count", &range])?;
        output.parse().context("failed to parse rev-list count")
    }

    fn worktree_list(&self) -> Result<Vec<WorktreeEntry>> {
        let output = run_git(&["worktree", "list", "--porcelain"])?;
        let mut entries = Vec::new();
        let mut current_path: Option<String> = None;
        let mut current_branch: Option<String> = None;

        for line in output.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                // Flush previous entry
                if let Some(p) = current_path.take() {
                    entries.push(WorktreeEntry {
                        path: p,
                        branch: current_branch.take(),
                    });
                }
                current_path = Some(path.to_string());
                current_branch = None;
            } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
                current_branch = Some(branch.to_string());
            }
        }
        // Flush last entry
        if let Some(p) = current_path {
            entries.push(WorktreeEntry {
                path: p,
                branch: current_branch,
            });
        }
        Ok(entries)
    }

    fn worktree_remove(&self, path: &str) -> Result<()> {
        run_git(&["worktree", "remove", path, "--force"])?;
        Ok(())
    }

    fn worktree_prune(&self) -> Result<()> {
        run_git(&["worktree", "prune"])?;
        Ok(())
    }

    fn branch_delete(&self, name: &str) -> Result<()> {
        run_git(&["branch", "-D", name])?;
        Ok(())
    }
}

fn run_git(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .context("git not found")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Convenience: create a boxed real client.
pub fn real() -> Box<dyn GitOps> {
    Box::new(RealGit)
}
