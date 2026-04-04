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
