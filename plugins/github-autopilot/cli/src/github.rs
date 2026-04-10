use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::process::Command;
use std::sync::Arc;

// ── Domain Types ──

/// A completed workflow run from `gh run list`.
#[derive(Debug, Clone)]
pub struct CompletedRun {
    pub id: u64,
    pub name: String,
    pub branch: String,
    pub conclusion: String,
}

/// An open issue from `gh issue list`.
#[derive(Debug, Clone)]
pub struct OpenIssue {
    pub number: u64,
    pub title: String,
    pub labels: Vec<String>,
}

// ── Trait ──

/// Domain-level GitHub operations.
///
/// Encapsulates GitHub API interactions behind a testable interface.
/// The real implementation uses `gh` CLI; tests use MockGitHub.
pub trait GitHub: Send + Sync {
    /// Get the default branch name for the repository.
    fn default_branch(&self) -> Result<String>;

    /// List recently completed workflow runs.
    fn list_completed_runs(&self, limit: u64) -> Result<Vec<CompletedRun>>;

    /// List open issues.
    fn list_open_issues(&self, limit: u64) -> Result<Vec<OpenIssue>>;
}

// ── Real Implementation ──

/// GitHub trait implementation using the `gh` CLI.
pub struct GhCliGitHub;

#[derive(Deserialize)]
struct RawRun {
    #[serde(rename = "databaseId")]
    database_id: u64,
    name: String,
    #[serde(rename = "headBranch")]
    head_branch: String,
    conclusion: String,
}

#[derive(Deserialize)]
struct RawIssue {
    number: u64,
    title: String,
    labels: Vec<RawLabel>,
}

#[derive(Deserialize)]
struct RawLabel {
    name: String,
}

fn run_gh(args: &[&str]) -> Result<String> {
    let output = Command::new("gh")
        .args(args)
        .output()
        .context("gh CLI not found")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh {} failed: {}", args.join(" "), stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

impl GitHub for GhCliGitHub {
    fn default_branch(&self) -> Result<String> {
        run_gh(&[
            "repo",
            "view",
            "--json",
            "defaultBranchRef",
            "--jq",
            ".defaultBranchRef.name",
        ])
    }

    fn list_completed_runs(&self, limit: u64) -> Result<Vec<CompletedRun>> {
        let limit_str = limit.to_string();
        let raw = run_gh(&[
            "run",
            "list",
            "--status",
            "completed",
            "--limit",
            &limit_str,
            "--json",
            "databaseId,name,headBranch,conclusion",
        ])?;

        let runs: Vec<RawRun> = serde_json::from_str(&raw).context("failed to parse run list")?;

        Ok(runs
            .into_iter()
            .map(|r| CompletedRun {
                id: r.database_id,
                name: r.name,
                branch: r.head_branch,
                conclusion: r.conclusion,
            })
            .collect())
    }

    fn list_open_issues(&self, limit: u64) -> Result<Vec<OpenIssue>> {
        let limit_str = limit.to_string();
        let raw = run_gh(&[
            "issue",
            "list",
            "--state",
            "open",
            "--limit",
            &limit_str,
            "--json",
            "number,title,labels",
        ])?;

        let issues: Vec<RawIssue> =
            serde_json::from_str(&raw).context("failed to parse issue list")?;

        Ok(issues
            .into_iter()
            .map(|i| OpenIssue {
                number: i.number,
                title: i.title,
                labels: i.labels.into_iter().map(|l| l.name).collect(),
            })
            .collect())
    }
}

/// Convenience: create a shared real client.
pub fn real() -> Arc<dyn GitHub> {
    Arc::new(GhCliGitHub)
}
