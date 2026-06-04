//! GitHub CLI (`gh`) wrapper, ported from git-utils `core/github.ts`.
//!
//! [`GitHubService`] is the injectable boundary (command-layer tests mock it);
//! [`RealGitHubService`] shells out to `gh`. There is no dedicated test for the
//! real implementation — mirroring the TypeScript, which also has none — so it
//! is exercised only through the real CLI.

use anyhow::{Context, Result};
use regex::Regex;
use std::path::{Path, PathBuf};

use crate::git::types::{ReviewComment, ReviewThread, ReviewsOutput};

use super::shell::{exec_env, ExecResult};

/// GitHub operations used by the command layer. Injectable for testing.
pub trait GitHubService {
    /// `gh auth status` succeeded.
    fn is_authenticated(&self) -> Result<bool>;
    /// Create a PR (`gh pr create`); returns the PR URL.
    fn create_pr(&self, base: &str, title: &str, body: &str) -> Result<String>;
    /// Fetch review threads for `pr_number` via `gh api graphql`.
    fn get_review_threads(&self, pr_number: i64) -> Result<ReviewsOutput>;
    /// Detect the current branch's open PR number, or `None`.
    fn detect_current_pr_number(&self) -> Result<Option<i64>>;
}

const REVIEW_THREADS_QUERY: &str = r#"
query($owner: String!, $repo: String!, $number: Int!) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $number) {
      title
      url
      reviewThreads(first: 100) {
        nodes {
          isResolved
          isOutdated
          path
          line
          comments(first: 100) {
            nodes {
              author { login }
              body
              createdAt
              url
            }
          }
        }
      }
    }
  }
}
"#;

/// Reads `GH_HOST` from `~/.git-workflow-env`, if present.
fn load_gh_host() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let env_path = PathBuf::from(home).join(".git-workflow-env");
    let content = std::fs::read_to_string(env_path).ok()?;
    let re = Regex::new(r#"(?m)^export\s+GH_HOST="(.+)""#).unwrap();
    re.captures(&content)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// Real implementation shelling out to `gh`, optionally scoped to `cwd` and
/// honouring `GH_HOST` from `~/.git-workflow-env`.
pub struct RealGitHubService {
    cwd: Option<PathBuf>,
    gh_host: Option<String>,
}

impl RealGitHubService {
    pub fn new(cwd: Option<PathBuf>) -> Self {
        Self {
            cwd,
            gh_host: load_gh_host(),
        }
    }

    fn cwd(&self) -> Option<&Path> {
        self.cwd.as_deref()
    }

    fn env(&self) -> Vec<(&str, &str)> {
        match &self.gh_host {
            Some(host) => vec![("GH_HOST", host.as_str())],
            None => vec![],
        }
    }

    fn gh_safe(&self, args: &[&str]) -> Result<ExecResult> {
        let mut command = vec!["gh"];
        command.extend_from_slice(args);
        exec_env(&command, self.cwd(), &self.env())
    }

    fn gh(&self, args: &[&str]) -> Result<String> {
        let result = self.gh_safe(args)?;
        if result.exit_code != 0 {
            anyhow::bail!(
                "Command failed (exit {}): gh {}\n{}",
                result.exit_code,
                args.join(" "),
                result.stderr
            );
        }
        Ok(result.stdout)
    }
}

impl GitHubService for RealGitHubService {
    fn is_authenticated(&self) -> Result<bool> {
        Ok(self.gh_safe(&["auth", "status"])?.exit_code == 0)
    }

    fn create_pr(&self, base: &str, title: &str, body: &str) -> Result<String> {
        let url = self.gh(&[
            "pr", "create", "--base", base, "--title", title, "--body", body,
        ])?;
        Ok(url.trim().to_string())
    }

    fn get_review_threads(&self, pr_number: i64) -> Result<ReviewsOutput> {
        let repo_info = self.gh(&["repo", "view", "--json", "owner,name"])?;
        let repo: serde_json::Value =
            serde_json::from_str(&repo_info).context("parse `gh repo view` JSON")?;
        let owner = repo["owner"]["login"].as_str().unwrap_or_default();
        let name = repo["name"].as_str().unwrap_or_default();

        let number_arg = format!("number={pr_number}");
        let query_arg = format!("query={REVIEW_THREADS_QUERY}");
        let owner_arg = format!("owner={owner}");
        let repo_arg = format!("repo={name}");
        let result = self.gh(&[
            "api",
            "graphql",
            "-f",
            &query_arg,
            "-f",
            &owner_arg,
            "-f",
            &repo_arg,
            "-F",
            &number_arg,
        ])?;

        let data: serde_json::Value =
            serde_json::from_str(&result).context("parse `gh api graphql` JSON")?;
        let pr = &data["data"]["repository"]["pullRequest"];

        let threads = pr["reviewThreads"]["nodes"]
            .as_array()
            .map(|nodes| {
                nodes
                    .iter()
                    .map(|node| ReviewThread {
                        is_resolved: node["isResolved"].as_bool().unwrap_or(false),
                        is_outdated: node["isOutdated"].as_bool().unwrap_or(false),
                        path: node["path"].as_str().unwrap_or_default().to_string(),
                        line: node["line"].as_i64().unwrap_or(0),
                        comments: node["comments"]["nodes"]
                            .as_array()
                            .map(|cs| {
                                cs.iter()
                                    .map(|c| ReviewComment {
                                        author: c["author"]["login"]
                                            .as_str()
                                            .unwrap_or("ghost")
                                            .to_string(),
                                        body: c["body"].as_str().unwrap_or_default().to_string(),
                                        created_at: c["createdAt"]
                                            .as_str()
                                            .unwrap_or_default()
                                            .to_string(),
                                        url: c["url"].as_str().unwrap_or_default().to_string(),
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(ReviewsOutput {
            pr_title: pr["title"].as_str().unwrap_or_default().to_string(),
            pr_url: pr["url"].as_str().unwrap_or_default().to_string(),
            threads,
        })
    }

    fn detect_current_pr_number(&self) -> Result<Option<i64>> {
        let result = self.gh_safe(&["pr", "view", "--json", "number,state"])?;
        if result.exit_code != 0 {
            return Ok(None);
        }
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&result.stdout) else {
            return Ok(None);
        };
        if parsed["state"].as_str() != Some("OPEN") {
            return Ok(None);
        }
        Ok(parsed["number"].as_i64())
    }
}
