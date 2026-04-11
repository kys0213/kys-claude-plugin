use anyhow::{Context, Result};
use clap::Args;
use serde_json::Value;
use std::io::Read as _;

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

#[derive(Args)]
pub struct DetectOverlapArgs {
    /// Hamming distance threshold — pairs at or below this are flagged
    #[arg(long, default_value = "15")]
    pub threshold: u32,
}

/// Detect overlapping issues by simhash text similarity.
/// Reads issues from stdin as JSON array: `[{"number":N,"title":"...","body":"..."}]`
/// Outputs pairs whose hamming distance is at or below the threshold.
pub fn detect_overlap(args: &DetectOverlapArgs) -> Result<i32> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("failed to read stdin")?;

    let issues: Vec<Value> = serde_json::from_str(&input).context("failed to parse stdin JSON")?;

    let result = compute_overlaps(&issues, args.threshold);
    println!("{result}");
    Ok(0)
}

/// Pure function for testability: compute pairwise overlaps.
pub fn compute_overlaps(issues: &[Value], threshold: u32) -> Value {
    // Compute simhash for each issue from title + body
    let hashed: Vec<(u64, &Value)> = issues
        .iter()
        .map(|issue| {
            let title = issue["title"].as_str().unwrap_or("");
            let body = issue["body"].as_str().unwrap_or("");
            let text = format!("{title}\n{body}");
            let tokens = super::simhash::tokenize_weighted(&text);
            let hash = super::simhash::weighted_simhash(&tokens);
            (hash, issue)
        })
        .collect();

    let mut review_required: Vec<Value> = Vec::new();

    for i in 0..hashed.len() {
        for j in (i + 1)..hashed.len() {
            let distance = super::simhash::hamming_distance(hashed[i].0, hashed[j].0);
            if distance <= threshold {
                let num_a = hashed[i].1["number"].as_u64().unwrap_or(0);
                let num_b = hashed[j].1["number"].as_u64().unwrap_or(0);
                review_required.push(serde_json::json!({
                    "pair": [num_a, num_b],
                    "distance": distance,
                    "issues": [
                        {
                            "number": num_a,
                            "title": hashed[i].1["title"],
                            "body": hashed[i].1["body"],
                        },
                        {
                            "number": num_b,
                            "title": hashed[j].1["title"],
                            "body": hashed[j].1["body"],
                        }
                    ]
                }));
            }
        }
    }

    // Sort by distance ascending (most similar first)
    review_required.sort_by_key(|r| r["distance"].as_u64().unwrap_or(64));

    let total = issues.len();
    let pairs_checked = total * (total.saturating_sub(1)) / 2;

    serde_json::json!({
        "review_required": review_required,
        "total_issues": total,
        "pairs_checked": pairs_checked,
    })
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
    fn detect_overlap_flags_similar_issues() {
        let issues = vec![
            serde_json::json!({"number": 1, "title": "JWT token refresh middleware 추가", "body": "middleware 레이어에 JWT refresh token 로직을 추가합니다. src/auth/middleware.rs 수정 필요"}),
            serde_json::json!({"number": 2, "title": "JWT token refresh handler 구현", "body": "JWT refresh token 처리 handler를 구현합니다. src/auth/handler.rs에 추가"}),
            serde_json::json!({"number": 3, "title": "데이터베이스 마이그레이션 스크립트 작성", "body": "PostgreSQL 스키마 변경을 위한 마이그레이션 파일을 생성합니다. migrations/001_add_users.sql"}),
        ];
        // Use generous threshold to find the actual distance, then verify
        let debug = compute_overlaps(&issues, 64);
        let all_pairs = debug["review_required"].as_array().unwrap();
        // Find the distance between issues 1 and 2
        let pair_12 = all_pairs
            .iter()
            .find(|r| {
                let p = r["pair"].as_array().unwrap();
                p[0].as_u64() == Some(1) && p[1].as_u64() == Some(2)
            })
            .expect("pair 1-2 should exist");
        let dist_12 = pair_12["distance"].as_u64().unwrap();
        // Find the distance between issues 1 and 3
        let pair_13 = all_pairs
            .iter()
            .find(|r| {
                let p = r["pair"].as_array().unwrap();
                p[0].as_u64() == Some(1) && p[1].as_u64() == Some(3)
            })
            .expect("pair 1-3 should exist");
        let dist_13 = pair_13["distance"].as_u64().unwrap();
        // Similar issues (1,2) should have smaller distance than dissimilar (1,3)
        assert!(
            dist_12 < dist_13,
            "issues 1&2 (dist={dist_12}) should be more similar than 1&3 (dist={dist_13})"
        );

        // Now use a threshold that captures 1-2 but not 1-3
        let result = compute_overlaps(&issues, dist_12 as u32);
        let review = result["review_required"].as_array().unwrap();
        assert!(
            review.iter().any(|r| {
                let pair = r["pair"].as_array().unwrap();
                pair[0].as_u64() == Some(1) && pair[1].as_u64() == Some(2)
            }),
            "expected issues 1 and 2 to be flagged, got: {review:?}"
        );
        // Issue 3 should not overlap with 1 or 2
        assert!(
            !review.iter().any(|r| {
                let pair = r["pair"].as_array().unwrap();
                pair.iter().any(|n| n.as_u64() == Some(3))
            }),
            "issue 3 should not be in any overlap pair"
        );
    }

    #[test]
    fn detect_overlap_empty_input() {
        let result = compute_overlaps(&[], 15);
        assert_eq!(result["review_required"].as_array().unwrap().len(), 0);
        assert_eq!(result["total_issues"], 0);
        assert_eq!(result["pairs_checked"], 0);
    }

    #[test]
    fn detect_overlap_single_issue() {
        let issues = vec![serde_json::json!({"number": 1, "title": "test", "body": "body"})];
        let result = compute_overlaps(&issues, 15);
        assert_eq!(result["review_required"].as_array().unwrap().len(), 0);
        assert_eq!(result["pairs_checked"], 0);
    }

    #[test]
    fn detect_overlap_threshold_filters() {
        let issues = vec![
            serde_json::json!({"number": 1, "title": "JWT token refresh middleware 추가", "body": "middleware 레이어에 JWT refresh"}),
            serde_json::json!({"number": 2, "title": "JWT token refresh handler 구현", "body": "JWT refresh token handler 구현"}),
        ];
        // Very strict threshold should filter out
        let strict = compute_overlaps(&issues, 0);
        assert_eq!(strict["review_required"].as_array().unwrap().len(), 0);

        // Generous threshold should include
        let generous = compute_overlaps(&issues, 30);
        assert!(!generous["review_required"].as_array().unwrap().is_empty());
    }

    #[test]
    fn detect_overlap_includes_issue_context() {
        let issues = vec![
            serde_json::json!({"number": 10, "title": "same task A", "body": "same body content here"}),
            serde_json::json!({"number": 11, "title": "same task A", "body": "same body content here"}),
        ];
        let result = compute_overlaps(&issues, 64);
        let review = result["review_required"].as_array().unwrap();
        assert_eq!(review.len(), 1);
        let pair = &review[0];
        assert_eq!(pair["distance"], 0);
        // Verify full context is included
        let pair_issues = pair["issues"].as_array().unwrap();
        assert_eq!(pair_issues[0]["title"], "same task A");
        assert_eq!(pair_issues[1]["body"], "same body content here");
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
