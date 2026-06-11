//! GitHub CLI (`gh`) wrapper — port of `git-utils/src/core/github.ts`. The
//! `GitHubService` trait abstracts the gh calls for mockability; the real
//! implementation reads `GH_HOST` from `~/.git-workflow-env`, calls `gh`, and
//! parses the review-threads GraphQL response into the same shapes as the TS.

use crate::git::core::shell::{exec, exec_or_throw, ExecOptions};
use crate::git::types::{ReviewComment, ReviewThread};
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

pub struct ReviewThreadsResult {
    pub pr_title: String,
    pub pr_url: String,
    pub threads: Vec<ReviewThread>,
}

pub struct CreatePrOptions<'a> {
    pub base: &'a str,
    pub title: &'a str,
    pub body: &'a str,
}

pub trait GitHubService {
    fn is_authenticated(&self) -> bool;
    fn create_pr(&self, options: &CreatePrOptions) -> Result<String, String>;
    fn get_review_threads(&self, pr_number: i64) -> Result<ReviewThreadsResult, String>;
    fn detect_current_pr_number(&self) -> Result<Option<i64>, String>;
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

/// Matches `export GH_HOST="<value>"` with flexible inter-token whitespace,
/// after the TS `loadGhHost` regex — but with `[^"]+` instead of the TS
/// greedy `.+`, which would capture up to the LAST quote on the line (e.g.
/// a trailing `# see "docs"` comment would corrupt the host).
static GH_HOST_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?m)^export\s+GH_HOST="([^"]+)""#).unwrap());

/// Reads `GH_HOST` from `~/.git-workflow-env` (matching the TS loadGhHost).
fn load_gh_host() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let env_path = std::path::Path::new(&home).join(".git-workflow-env");
    let content = std::fs::read_to_string(env_path).ok()?;
    let val = GH_HOST_PATTERN.captures(&content)?.get(1)?.as_str();
    (!val.is_empty()).then(|| val.to_string())
}

pub struct RealGitHubService {
    cwd: Option<String>,
    // Lazy: the env-file read only happens on the first gh invocation, so
    // constructing the service (e.g. for a guard target that never consults
    // gh) costs no I/O. OnceLock (not cell::OnceCell) keeps the service Sync.
    gh_host: std::sync::OnceLock<Option<String>>,
}

/// Constructs the real GitHub service bound to an optional working directory.
pub fn create_github_service(cwd: Option<String>) -> RealGitHubService {
    RealGitHubService {
        cwd,
        gh_host: std::sync::OnceLock::new(),
    }
}

impl RealGitHubService {
    fn gh_host(&self) -> &Option<String> {
        self.gh_host.get_or_init(load_gh_host)
    }
    fn opts(&self) -> Option<ExecOptions> {
        let gh_host = self.gh_host();
        if self.cwd.is_none() && gh_host.is_none() {
            return None;
        }
        let env = gh_host
            .as_ref()
            .map(|host| HashMap::from([("GH_HOST".to_string(), host.clone())]));
        Some(ExecOptions {
            cwd: self.cwd.clone(),
            env,
        })
    }

    fn gh(&self, args: &[&str]) -> Result<String, String> {
        let mut full = vec!["gh"];
        full.extend_from_slice(args);
        exec_or_throw(&full, self.opts().as_ref())
    }

    fn gh_safe(&self, args: &[&str]) -> (String, i32) {
        let mut full = vec!["gh"];
        full.extend_from_slice(args);
        let r = exec(&full, self.opts().as_ref());
        (r.stdout, r.exit_code)
    }
}

impl GitHubService for RealGitHubService {
    fn is_authenticated(&self) -> bool {
        let (_, exit) = self.gh_safe(&["auth", "status"]);
        exit == 0
    }

    fn create_pr(&self, options: &CreatePrOptions) -> Result<String, String> {
        let url = self.gh(&[
            "pr",
            "create",
            "--base",
            options.base,
            "--title",
            options.title,
            "--body",
            options.body,
        ])?;
        Ok(url.trim().to_string())
    }

    fn get_review_threads(&self, pr_number: i64) -> Result<ReviewThreadsResult, String> {
        let repo_info = self.gh(&["repo", "view", "--json", "owner,name"])?;
        let repo_json: serde_json::Value =
            serde_json::from_str(&repo_info).map_err(|e| e.to_string())?;
        let owner = repo_json["owner"]["login"]
            .as_str()
            .ok_or("missing repo owner")?;
        let repo = repo_json["name"].as_str().ok_or("missing repo name")?;

        let result = self.gh(&[
            "api",
            "graphql",
            "-f",
            &format!("query={REVIEW_THREADS_QUERY}"),
            "-f",
            &format!("owner={owner}"),
            "-f",
            &format!("repo={repo}"),
            "-F",
            &format!("number={pr_number}"),
        ])?;

        let data: serde_json::Value = serde_json::from_str(&result).map_err(|e| e.to_string())?;
        let pr = &data["data"]["repository"]["pullRequest"];

        let mut threads = Vec::new();
        if let Some(nodes) = pr["reviewThreads"]["nodes"].as_array() {
            for node in nodes {
                let comments = node["comments"]["nodes"]
                    .as_array()
                    .map(|cs| {
                        cs.iter()
                            .map(|c| ReviewComment {
                                author: c["author"]["login"]
                                    .as_str()
                                    .unwrap_or("ghost")
                                    .to_string(),
                                body: c["body"].as_str().unwrap_or("").to_string(),
                                created_at: c["createdAt"].as_str().unwrap_or("").to_string(),
                                url: c["url"].as_str().unwrap_or("").to_string(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                threads.push(ReviewThread {
                    is_resolved: node["isResolved"].as_bool().unwrap_or(false),
                    is_outdated: node["isOutdated"].as_bool().unwrap_or(false),
                    path: node["path"].as_str().unwrap_or("").to_string(),
                    line: node["line"].as_i64().unwrap_or(0),
                    comments,
                });
            }
        }

        Ok(ReviewThreadsResult {
            pr_title: pr["title"].as_str().unwrap_or("").to_string(),
            pr_url: pr["url"].as_str().unwrap_or("").to_string(),
            threads,
        })
    }

    fn detect_current_pr_number(&self) -> Result<Option<i64>, String> {
        let (stdout, exit) = self.gh_safe(&["pr", "view", "--json", "number,state"]);
        if exit != 0 {
            return Ok(None);
        }
        let parsed: serde_json::Value = match serde_json::from_str(&stdout) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };
        if parsed["state"].as_str() != Some("OPEN") {
            return Ok(None);
        }
        Ok(parsed["number"].as_i64())
    }
}

#[cfg(test)]
mod tests {
    use super::GH_HOST_PATTERN;

    /// Extracts the captured GH_HOST value, mirroring `load_gh_host`'s parse.
    fn parse(content: &str) -> Option<&str> {
        GH_HOST_PATTERN
            .captures(content)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
    }

    #[test]
    fn single_space_form() {
        assert_eq!(
            parse(r#"export GH_HOST="github.example.com""#),
            Some("github.example.com")
        );
    }

    #[test]
    fn trailing_quoted_text_does_not_extend_capture() {
        // Greedy `.+` would capture up to the last quote on the line; the
        // value must stop at the closing quote of the assignment.
        assert_eq!(
            parse(r#"export GH_HOST="ghe.corp.com" # see "docs""#),
            Some("ghe.corp.com")
        );
    }

    #[test]
    fn flexible_whitespace_between_export_and_var() {
        // The TS regex uses `\s+`; tabs and multiple spaces must still match.
        assert_eq!(
            parse("export\tGH_HOST=\"ghe.tab.com\""),
            Some("ghe.tab.com")
        );
        assert_eq!(
            parse(r#"export   GH_HOST="ghe.spaces.com""#),
            Some("ghe.spaces.com")
        );
    }

    #[test]
    fn matches_amid_other_lines() {
        let content = "# config\nexport FOO=\"bar\"\nexport GH_HOST=\"ghe.multi.com\"\n";
        assert_eq!(parse(content), Some("ghe.multi.com"));
    }

    #[test]
    fn no_match_when_absent() {
        assert_eq!(parse("export FOO=\"bar\"\n"), None);
    }
}
