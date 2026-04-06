use anyhow::{Context, Result};
use clap::Args;
use serde_json::Value;

use crate::gh::GhOps;

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

    /// Optional simhash of gap analysis output (for stagnation tracking)
    #[arg(long)]
    pub simhash: Option<String>,
}

#[derive(Args)]
pub struct SearchSimilarArgs {
    /// Fingerprint to search for related issues
    #[arg(long)]
    pub fingerprint: String,

    /// Current simhash to compare against
    #[arg(long)]
    pub simhash: String,

    /// Maximum number of results
    #[arg(long, default_value = "5")]
    pub limit: usize,
}

/// Check if an open issue with the given fingerprint already exists.
/// Returns exit code: 0 = no duplicate, 1 = duplicate exists.
pub fn check_dup(gh: &dyn GhOps, fingerprint: &str) -> Result<i32> {
    match find_duplicate(gh, fingerprint)? {
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
pub fn create(gh: &dyn GhOps, args: &CreateArgs) -> Result<i32> {
    if let Some((number, title)) = find_duplicate(gh, &args.fingerprint)? {
        let out = serde_json::json!({
            "created": false,
            "duplicate": true,
            "issue_number": number,
            "issue_title": title,
        });
        println!("{out}");
        return Ok(1);
    }

    let body = append_metadata(&args.body, &args.fingerprint, args.simhash.as_deref());

    let mut gh_args: Vec<&str> = vec!["issue", "create", "--title", &args.title, "--body", &body];
    for label in &args.label {
        gh_args.push("--label");
        gh_args.push(label);
    }

    let output = gh.run(&gh_args).context("failed to create issue")?;

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
pub fn close_resolved(gh: &dyn GhOps, label_prefix: &str) -> Result<i32> {
    let ci_label = super::labels::with_prefix(label_prefix, super::labels::CI_FAILURE);

    let issues = gh.list_json(&[
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

        let prs = gh.list_json(&[
            "pr", "list", "--head", &branch, "--state", "merged", "--json", "number", "--limit",
            "1",
        ])?;

        if prs.is_empty() {
            continue;
        }

        let num_str = number.to_string();
        gh.run(&[
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

fn find_duplicate(gh: &dyn GhOps, fingerprint: &str) -> Result<Option<(u64, String)>> {
    let search_query = format!("\"{fingerprint}\" in:body");
    let items = gh.list_json(&[
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

/// Search for issues with the same fingerprint and rank by simhash similarity.
pub fn search_similar(gh: &dyn GhOps, args: &SearchSimilarArgs) -> Result<i32> {
    let query_hash = super::simhash::parse_simhash(&args.simhash)
        .ok_or_else(|| anyhow::anyhow!("invalid simhash: {}", args.simhash))?;

    let search_query = format!("\"{}\" in:body", args.fingerprint);
    let items = gh.list_json(&[
        "issue",
        "list",
        "--state",
        "all",
        "--search",
        &search_query,
        "--json",
        "number,title,state,body",
        "--limit",
        "50",
    ])?;

    let mut results: Vec<Value> = Vec::new();

    for item in &items {
        let number = item["number"].as_u64().unwrap_or(0);
        let title = item["title"].as_str().unwrap_or("");
        let state = item["state"].as_str().unwrap_or("OPEN");
        let body = item["body"].as_str().unwrap_or("");

        let issue_hash = extract_simhash_from_body(body);
        let distance = match issue_hash {
            Some(h) => super::simhash::hamming_distance(query_hash, h),
            None => 64, // max distance if no simhash found
        };

        results.push(serde_json::json!({
            "number": number,
            "distance": distance,
            "state": state,
            "title": title,
            "simhash": issue_hash.map(super::simhash::format_simhash),
        }));
    }

    // Sort by distance ascending
    results.sort_by_key(|r| r["distance"].as_u64().unwrap_or(64));
    results.truncate(args.limit);

    let out = serde_json::json!({
        "query_simhash": super::simhash::format_simhash(query_hash),
        "results": results,
    });
    println!("{out}");
    Ok(0)
}

/// Extract simhash from issue body HTML comment.
fn extract_simhash_from_body(body: &str) -> Option<u64> {
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("<!-- simhash:") {
            if let Some(hex) = rest.strip_suffix("-->") {
                return super::simhash::parse_simhash(hex.trim());
            }
        }
    }
    None
}

/// Append fingerprint (and optional simhash) to body.
/// Fingerprint is stored both as searchable plain text (backtick code span)
/// and as an HTML comment for structured extraction.
/// GitHub's `in:body` search does NOT index HTML comments, so the plain text
/// line is essential for `find_duplicate()` to work.
pub fn append_fingerprint(body: &str, fingerprint: &str) -> String {
    format!("{body}\n\n---\n`fingerprint: {fingerprint}`\n<!-- fingerprint: {fingerprint} -->")
}

/// Append fingerprint and optional simhash to body.
/// Delegates to `append_fingerprint` for the base format, then appends simhash if present.
fn append_metadata(body: &str, fingerprint: &str, simhash: Option<&str>) -> String {
    let mut result = append_fingerprint(body, fingerprint);
    if let Some(sh) = simhash {
        result.push_str(&format!("\n<!-- simhash: {sh} -->"));
    }
    result
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
        assert!(result.contains("`fingerprint: ci:validate.yml:main:test-failure`"));
        assert!(result.contains("<!-- fingerprint: ci:validate.yml:main:test-failure -->"));
    }

    #[test]
    fn test_append_metadata_with_simhash() {
        let body = "content";
        let result = append_metadata(body, "gap:spec:token", Some("0xABCD"));
        assert!(result.contains("<!-- fingerprint: gap:spec:token -->"));
        assert!(result.contains("<!-- simhash: 0xABCD -->"));
    }

    #[test]
    fn test_append_metadata_without_simhash() {
        let body = "content";
        let result = append_metadata(body, "gap:spec:token", None);
        assert!(result.contains("<!-- fingerprint: gap:spec:token -->"));
        assert!(!result.contains("simhash"));
    }

    #[test]
    fn test_extract_simhash_from_body() {
        let body =
            "content\n---\n<!-- fingerprint: gap:x -->\n<!-- simhash: 0xA3F2B81C4D5E6F1B -->";
        assert_eq!(extract_simhash_from_body(body), Some(0xA3F2B81C4D5E6F1B));
    }

    #[test]
    fn test_extract_simhash_missing() {
        let body = "content\n---\n<!-- fingerprint: gap:x -->";
        assert_eq!(extract_simhash_from_body(body), None);
    }

    #[test]
    fn test_extract_issue_number() {
        assert_eq!(
            extract_issue_number("https://github.com/owner/repo/issues/42"),
            42
        );
        assert_eq!(extract_issue_number("not-a-url"), 0);
    }

    #[test]
    fn test_extract_branch_from_ci_title() {
        assert_eq!(
            extract_branch_from_ci_title("fix: CI failure in validate.yml on feat/add-auth"),
            "feat/add-auth"
        );
        assert_eq!(extract_branch_from_ci_title("some other title"), "");
    }
}
