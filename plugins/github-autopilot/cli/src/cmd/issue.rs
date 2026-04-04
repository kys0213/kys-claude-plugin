use anyhow::{Context, Result};
use clap::Args;
use serde_json::Value;

use crate::gh;

#[derive(Args)]
pub struct CreateArgs {
    /// Issue title
    #[arg(long)]
    pub title: String,

    /// Labels (can be specified multiple times)
    #[arg(long)]
    pub label: Vec<String>,

    /// Fingerprint for dedup
    #[arg(long)]
    pub fingerprint: String,

    /// Issue body (without fingerprint — it is appended automatically)
    #[arg(long)]
    pub body: String,
}

/// Check if an open issue with the given fingerprint already exists.
/// Returns exit code: 0 = no duplicate, 1 = duplicate exists.
pub fn check_dup(fingerprint: &str) -> Result<i32> {
    match find_duplicate(fingerprint)? {
        Some((number, title)) => {
            let out = serde_json::json!({
                "duplicate": true,
                "issue_number": number,
                "issue_title": title,
            });
            println!("{out}");
            Ok(1)
        }
        None => {
            println!(r#"{{"duplicate": false}}"#);
            Ok(0)
        }
    }
}

/// Create an issue with fingerprint-based dedup.
/// Returns exit code: 0 = created, 1 = duplicate (skipped).
pub fn create(args: &CreateArgs) -> Result<i32> {
    if let Some((number, title)) = find_duplicate(&args.fingerprint)? {
        let out = serde_json::json!({
            "created": false,
            "duplicate": true,
            "issue_number": number,
            "issue_title": title,
        });
        println!("{out}");
        return Ok(1);
    }

    let body = append_fingerprint(&args.body, &args.fingerprint);

    let mut gh_args: Vec<&str> = vec!["issue", "create", "--title", &args.title, "--body", &body];
    for label in &args.label {
        gh_args.push("--label");
        gh_args.push(label);
    }

    let output = gh::run(&gh_args).context("failed to create issue")?;

    let number = extract_issue_number(&output);
    let out = serde_json::json!({
        "created": true,
        "issue_number": number,
        "url": output,
    });
    println!("{out}");
    Ok(0)
}

/// Close CI-failure issues whose related branch PR has been merged.
pub fn close_resolved(label_prefix: &str) -> Result<i32> {
    let ci_label = super::labels::with_prefix(label_prefix, super::labels::CI_FAILURE);

    let issues = gh::list_json(&[
        "issue",
        "list",
        "--label",
        &ci_label,
        "--state",
        "open",
        "--json",
        "number,title",
        "--limit",
        "50",
    ])?;

    if issues.is_empty() {
        println!(r#"{{"closed": []}}"#);
        return Ok(0);
    }

    let mut closed: Vec<Value> = Vec::new();

    for issue in &issues {
        let number = issue["number"].as_u64().unwrap_or(0);
        let title = issue["title"].as_str().unwrap_or("");

        let branch = extract_branch_from_ci_title(title);
        if branch.is_empty() {
            continue;
        }

        let prs = gh::list_json(&[
            "pr", "list", "--head", &branch, "--state", "merged", "--json", "number", "--limit",
            "1",
        ])?;

        if prs.is_empty() {
            continue;
        }

        let num_str = number.to_string();
        gh::run(&[
            "issue",
            "close",
            &num_str,
            "--comment",
            "Resolved: related branch PR has been merged.",
        ])?;

        closed.push(serde_json::json!({
            "number": number,
            "title": title,
            "branch": branch,
        }));
    }

    let out = serde_json::json!({ "closed": closed });
    println!("{out}");
    Ok(0)
}

fn find_duplicate(fingerprint: &str) -> Result<Option<(u64, String)>> {
    let search_query = format!("\"{fingerprint}\" in:body");
    let items = gh::list_json(&[
        "issue",
        "list",
        "--state",
        "open",
        "--search",
        &search_query,
        "--json",
        "number,title",
        "--limit",
        "1",
    ])?;

    if let Some(item) = items.first() {
        let number = item["number"].as_u64().unwrap_or(0);
        let title = item["title"].as_str().unwrap_or("").to_string();
        Ok(Some((number, title)))
    } else {
        Ok(None)
    }
}

/// Append fingerprint HTML comment to body.
pub fn append_fingerprint(body: &str, fingerprint: &str) -> String {
    format!("{body}\n\n---\n<!-- fingerprint: {fingerprint} -->")
}

fn extract_issue_number(url: &str) -> u64 {
    url.rsplit('/')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Extract branch name from CI failure issue title.
/// Expected format: "fix: CI failure in {workflow} on {branch}"
pub fn extract_branch_from_ci_title(title: &str) -> String {
    if let Some(pos) = title.rfind(" on ") {
        title[pos + 4..].trim().to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_fingerprint() {
        let body = "## Summary\n\nSome content";
        let fp = "ci:validate.yml:main:test-failure";
        let result = append_fingerprint(body, fp);
        assert!(result.ends_with("<!-- fingerprint: ci:validate.yml:main:test-failure -->"));
        assert!(result.contains("---\n<!-- fingerprint:"));
    }

    #[test]
    fn test_extract_issue_number() {
        assert_eq!(
            extract_issue_number("https://github.com/owner/repo/issues/42"),
            42
        );
        assert_eq!(
            extract_issue_number("https://github.com/owner/repo/issues/999"),
            999
        );
        assert_eq!(extract_issue_number("not-a-url"), 0);
    }

    #[test]
    fn test_extract_branch_from_ci_title() {
        assert_eq!(
            extract_branch_from_ci_title("fix: CI failure in validate.yml on feat/add-auth"),
            "feat/add-auth"
        );
        assert_eq!(
            extract_branch_from_ci_title("fix: CI failure in build.yml on main"),
            "main"
        );
        assert_eq!(extract_branch_from_ci_title("some other title"), "");
    }
}
