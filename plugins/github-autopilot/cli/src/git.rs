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

    /// Check whether a worktree has uncommitted changes (staged or unstaged).
    fn has_uncommitted_changes(&self, worktree_path: &str) -> Result<bool>;

    /// Stage all changes and commit in a specific worktree.
    fn commit_all_in_worktree(&self, worktree_path: &str, message: &str) -> Result<()>;
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
        Ok(parse_worktree_porcelain(&output))
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

    fn has_uncommitted_changes(&self, worktree_path: &str) -> Result<bool> {
        let output = run_git(&["-C", worktree_path, "status", "--porcelain"])?;
        Ok(!output.is_empty())
    }

    fn commit_all_in_worktree(&self, worktree_path: &str, message: &str) -> Result<()> {
        run_git(&["-C", worktree_path, "add", "-A"])?;
        run_git(&["-C", worktree_path, "commit", "-m", message])?;
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

/// Parse `git worktree list --porcelain` output into structured entries.
pub fn parse_worktree_porcelain(output: &str) -> Vec<WorktreeEntry> {
    let mut entries = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_branch: Option<String> = None;

    for line in output.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
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
    if let Some(p) = current_path {
        entries.push(WorktreeEntry {
            path: p,
            branch: current_branch,
        });
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_main_worktree() {
        let output = "\
worktree /repo
HEAD abc1234
branch refs/heads/main
";
        let entries = parse_worktree_porcelain(output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "/repo");
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
    }

    #[test]
    fn parse_multiple_worktrees() {
        let output = "\
worktree /repo
HEAD abc1234
branch refs/heads/main

worktree /repo/.claude/worktrees/agent-1
HEAD def5678
branch refs/heads/feature/issue-42

worktree /repo/.claude/worktrees/agent-2
HEAD 9ab0123
branch refs/heads/draft/issue-99
";
        let entries = parse_worktree_porcelain(output);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[1].path, "/repo/.claude/worktrees/agent-1");
        assert_eq!(entries[1].branch.as_deref(), Some("feature/issue-42"));
        assert_eq!(entries[2].branch.as_deref(), Some("draft/issue-99"));
    }

    #[test]
    fn parse_detached_head_worktree() {
        let output = "\
worktree /repo
HEAD abc1234
branch refs/heads/main

worktree /repo/.claude/worktrees/agent-1
HEAD def5678
detached
";
        let entries = parse_worktree_porcelain(output);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].path, "/repo/.claude/worktrees/agent-1");
        assert!(entries[1].branch.is_none());
    }

    #[test]
    fn parse_bare_worktree() {
        let output = "\
worktree /repo.git
bare
";
        let entries = parse_worktree_porcelain(output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "/repo.git");
        assert!(entries[0].branch.is_none());
    }

    #[test]
    fn parse_empty_output() {
        let entries = parse_worktree_porcelain("");
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_path_with_spaces() {
        let output = "\
worktree /home/user/my project/repo
HEAD abc1234
branch refs/heads/main
";
        let entries = parse_worktree_porcelain(output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "/home/user/my project/repo");
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
    }

    #[test]
    fn parse_nested_branch_name() {
        let output = "\
worktree /repo/.claude/worktrees/agent-1
HEAD abc1234
branch refs/heads/feat/autopilot/deep-nested
";
        let entries = parse_worktree_porcelain(output);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].branch.as_deref(),
            Some("feat/autopilot/deep-nested")
        );
    }
}
