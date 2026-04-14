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
    let hashes: Vec<u64> = issues
        .iter()
        .map(|issue| {
            let title = issue["title"].as_str().unwrap_or("");
            let body = issue["body"].as_str().unwrap_or("");
            let text = format!("{title}\n{body}");
            let tokens = super::simhash::tokenize_weighted(&text);
            super::simhash::weighted_simhash(&tokens)
        })
        .collect();

    let mut review_required: Vec<Value> = Vec::new();

    for i in 0..hashes.len() {
        for j in (i + 1)..hashes.len() {
            let distance = super::simhash::hamming_distance(hashes[i], hashes[j]);
            if distance <= threshold {
                review_required.push(serde_json::json!({
                    "pair": [issues[i]["number"], issues[j]["number"]],
                    "distance": distance,
                    "issues": [
                        {
                            "number": issues[i]["number"],
                            "title": issues[i]["title"],
                            "body": issues[i]["body"],
                        },
                        {
                            "number": issues[j]["number"],
                            "title": issues[j]["title"],
                            "body": issues[j]["body"],
                        }
                    ]
                }));
            }
        }
    }

    review_required.sort_by_key(|r| r["distance"].as_u64().unwrap_or(64));

    let total = issues.len();
    let pairs_checked = total * (total.saturating_sub(1)) / 2;

    serde_json::json!({
        "review_required": review_required,
        "total_issues": total,
        "pairs_checked": pairs_checked,
    })
}

/// Persona rotation order for repeated build failures (matches resilience skill).
const PERSONAS: &[&str] = &[
    "hacker",
    "researcher",
    "simplifier",
    "architect",
    "contrarian",
];

/// Filter issue comments for implementer agents and analyze failure patterns.
/// Reads comments from stdin as JSON array: `[{"body":"..."}]`
/// Outputs filtered comments + failure analysis with optional persona recommendation.
pub fn filter_comments() -> Result<i32> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("failed to read stdin")?;

    let comments: Vec<Value> =
        serde_json::from_str(&input).context("failed to parse stdin JSON")?;

    let result = compute_filtered_comments(&comments);
    println!("{result}");
    Ok(0)
}

/// Pure function for testability: filter comments and analyze failure patterns.
pub fn compute_filtered_comments(comments: &[Value]) -> Value {
    let mut analysis_comments = Vec::new();
    let mut failure_comments: Vec<(usize, Value)> = Vec::new(); // (attempt, comment)
    let mut rework_comments: Vec<(usize, Value)> = Vec::new(); // (index, comment)
    let mut other_comments = Vec::new();

    for (i, comment) in comments.iter().enumerate() {
        let body = comment["body"].as_str().unwrap_or("");
        let category = classify_comment(body);

        match category {
            CommentCategory::InternalMarker | CommentCategory::PrLink => {
                // excluded
            }
            CommentCategory::Analysis => {
                analysis_comments.push(comment.clone());
            }
            CommentCategory::FailureMarker => {
                let attempt = extract_failure_attempt(body).unwrap_or(0);
                failure_comments.push((attempt, comment.clone()));
            }
            CommentCategory::ReworkRequest => {
                rework_comments.push((i, comment.clone()));
            }
            CommentCategory::Other => {
                other_comments.push(comment.clone());
            }
        }
    }

    // Keep only latest failure and rework
    let latest_failure = failure_comments.iter().max_by_key(|(attempt, _)| *attempt);
    let latest_rework = rework_comments.last();

    let mut filtered = Vec::new();
    filtered.extend(analysis_comments);
    filtered.extend(other_comments);
    if let Some((_, comment)) = latest_rework {
        filtered.push(comment.clone());
    }
    if let Some((_, comment)) = latest_failure {
        filtered.push(comment.clone());
    }

    // Failure analysis
    let failure_analysis = analyze_failures(&failure_comments);

    serde_json::json!({
        "comments": filtered,
        "failure_analysis": failure_analysis,
    })
}

#[derive(Debug, PartialEq)]
enum CommentCategory {
    Analysis,
    FailureMarker,
    ReworkRequest,
    InternalMarker,
    PrLink,
    Other,
}

fn classify_comment(body: &str) -> CommentCategory {
    let trimmed = body.trim();

    // Internal markers: autopilot tracking comments not useful for implementer
    if trimmed.contains("<!-- autopilot:rework-detected -->")
        || trimmed.contains("<!-- autopilot:escalated -->")
        || is_marker_only(trimmed, "<!-- notified -->")
    {
        return CommentCategory::InternalMarker;
    }

    // PR link
    if trimmed.contains("PR created by autopilot") {
        return CommentCategory::PrLink;
    }

    // Failure marker
    if trimmed.contains("<!-- autopilot:failure:") {
        return CommentCategory::FailureMarker;
    }

    // Analysis comment
    if trimmed.contains("Autopilot 분석 결과") {
        return CommentCategory::Analysis;
    }

    // Rework request
    if has_rework_keyword(trimmed) {
        return CommentCategory::ReworkRequest;
    }

    CommentCategory::Other
}

fn is_marker_only(body: &str, marker: &str) -> bool {
    let without_marker = body.replace(marker, "");
    without_marker.trim().is_empty()
}

pub(crate) fn has_rework_keyword(body: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "재구현 필요",
        "재작업",
        "rework",
        "다시 구현",
        "re-implement",
    ];
    KEYWORDS.iter().any(|kw| body.contains(kw))
}

fn extract_failure_attempt(body: &str) -> Option<usize> {
    // Match <!-- autopilot:failure:N -->
    let marker = "<!-- autopilot:failure:";
    if let Some(start) = body.find(marker) {
        let rest = &body[start + marker.len()..];
        if let Some(end) = rest.find(" -->") {
            return rest[..end].parse().ok();
        }
    }
    None
}

fn extract_failure_category(body: &str) -> Option<String> {
    // Match **Category**: value or Category: value
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("**Category**:") {
            return Some(rest.trim().to_string());
        }
        if let Some(rest) = trimmed.strip_prefix("Category:") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

fn analyze_failures(failure_comments: &[(usize, Value)]) -> Value {
    if failure_comments.is_empty() {
        return serde_json::json!({
            "total_failures": 0,
            "repeated_category": false,
            "recommended_persona": null,
        });
    }

    let mut indices: Vec<usize> = (0..failure_comments.len()).collect();
    indices.sort_by_key(|&i| failure_comments[i].0);

    let categories: Vec<Option<String>> = indices
        .iter()
        .map(|&i| {
            let body = failure_comments[i].1["body"].as_str().unwrap_or("");
            extract_failure_category(body)
        })
        .collect();

    let latest = &failure_comments[*indices.last().unwrap()];
    let latest_attempt = latest.0;
    let latest_category = categories.last().cloned().flatten();

    // Check if the same category is repeated consecutively
    let repeated = if categories.len() >= 2 {
        let last = categories.last().unwrap();
        let second_last = &categories[categories.len() - 2];
        last.is_some() && last == second_last
    } else {
        false
    };

    let persona = if repeated {
        // Count consecutive same-category failures from the end
        let target = categories.last().unwrap();
        let consecutive = categories.iter().rev().take_while(|c| c == &target).count();
        // Pick persona: 2 consecutive → index 0 (hacker), 3 → index 1, etc.
        let idx = (consecutive - 2).min(PERSONAS.len() - 1);
        Some(PERSONAS[idx])
    } else {
        None
    };

    serde_json::json!({
        "total_failures": failure_comments.len(),
        "latest_attempt": latest_attempt,
        "latest_category": latest_category,
        "categories": categories,
        "repeated_category": repeated,
        "recommended_persona": persona,
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

    // --- filter_comments tests ---

    #[test]
    fn filter_excludes_internal_markers() {
        let comments = vec![
            serde_json::json!({"body": "<!-- notified -->"}),
            serde_json::json!({"body": "<!-- autopilot:rework-detected -->"}),
            serde_json::json!({"body": "## Autopilot Escalation Report\n\n<!-- autopilot:escalated -->"}),
            serde_json::json!({"body": "Autopilot 분석 결과: 영향 범위는 auth 모듈"}),
        ];
        let result = compute_filtered_comments(&comments);
        let filtered = result["comments"].as_array().unwrap();
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0]["body"].as_str().unwrap().contains("분석 결과"));
    }

    #[test]
    fn filter_excludes_pr_links() {
        let comments = vec![
            serde_json::json!({"body": "PR created by autopilot: #50"}),
            serde_json::json!({"body": "사용자 코멘트: 이 부분 수정해주세요"}),
        ];
        let result = compute_filtered_comments(&comments);
        let filtered = result["comments"].as_array().unwrap();
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0]["body"]
            .as_str()
            .unwrap()
            .contains("수정해주세요"));
    }

    #[test]
    fn filter_keeps_only_latest_failure() {
        let comments = vec![
            serde_json::json!({"body": "Autopilot 구현 실패 (attempt 1/3)\n\n**Category**: lint_failure\n**Reason**: clippy\n\n<!-- autopilot:failure:1 -->"}),
            serde_json::json!({"body": "Autopilot 구현 실패 (attempt 2/3)\n\n**Category**: test_failure\n**Reason**: assertion\n\n<!-- autopilot:failure:2 -->"}),
        ];
        let result = compute_filtered_comments(&comments);
        let filtered = result["comments"].as_array().unwrap();
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0]["body"].as_str().unwrap().contains("failure:2"));
    }

    #[test]
    fn filter_keeps_only_latest_rework() {
        let comments = vec![
            serde_json::json!({"body": "재구현 필요 — API 스펙 변경됨"}),
            serde_json::json!({"body": "재작업 — 새로운 요구사항 추가"}),
        ];
        let result = compute_filtered_comments(&comments);
        let filtered = result["comments"].as_array().unwrap();
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0]["body"]
            .as_str()
            .unwrap()
            .contains("새로운 요구사항"));
    }

    #[test]
    fn filter_preserves_all_analysis_and_user_comments() {
        let comments = vec![
            serde_json::json!({"body": "Autopilot 분석 결과: 첫 번째 분석"}),
            serde_json::json!({"body": "Autopilot 분석 결과: 두 번째 분석"}),
            serde_json::json!({"body": "사용자: 이건 중요한 코멘트"}),
        ];
        let result = compute_filtered_comments(&comments);
        let filtered = result["comments"].as_array().unwrap();
        assert_eq!(filtered.len(), 3);
    }

    #[test]
    fn filter_empty_comments() {
        let result = compute_filtered_comments(&[]);
        let filtered = result["comments"].as_array().unwrap();
        assert!(filtered.is_empty());
        assert_eq!(result["failure_analysis"]["total_failures"], 0);
    }

    #[test]
    fn failure_analysis_no_failures() {
        let comments = vec![serde_json::json!({"body": "Autopilot 분석 결과: 분석 내용"})];
        let result = compute_filtered_comments(&comments);
        let analysis = &result["failure_analysis"];
        assert_eq!(analysis["total_failures"], 0);
        assert_eq!(analysis["repeated_category"], false);
        assert!(analysis["recommended_persona"].is_null());
    }

    #[test]
    fn failure_analysis_detects_repeated_category() {
        let comments = vec![
            serde_json::json!({"body": "실패\n\n**Category**: lint_failure\n\n<!-- autopilot:failure:1 -->"}),
            serde_json::json!({"body": "실패\n\n**Category**: lint_failure\n\n<!-- autopilot:failure:2 -->"}),
        ];
        let result = compute_filtered_comments(&comments);
        let analysis = &result["failure_analysis"];
        assert_eq!(analysis["total_failures"], 2);
        assert_eq!(analysis["repeated_category"], true);
        assert_eq!(analysis["recommended_persona"], "hacker");
    }

    #[test]
    fn failure_analysis_no_persona_for_different_categories() {
        let comments = vec![
            serde_json::json!({"body": "실패\n\n**Category**: lint_failure\n\n<!-- autopilot:failure:1 -->"}),
            serde_json::json!({"body": "실패\n\n**Category**: test_failure\n\n<!-- autopilot:failure:2 -->"}),
        ];
        let result = compute_filtered_comments(&comments);
        let analysis = &result["failure_analysis"];
        assert_eq!(analysis["total_failures"], 2);
        assert_eq!(analysis["repeated_category"], false);
        assert!(analysis["recommended_persona"].is_null());
    }

    #[test]
    fn failure_analysis_persona_rotates_with_consecutive_failures() {
        let comments = vec![
            serde_json::json!({"body": "실패\n\n**Category**: test_failure\n\n<!-- autopilot:failure:1 -->"}),
            serde_json::json!({"body": "실패\n\n**Category**: test_failure\n\n<!-- autopilot:failure:2 -->"}),
            serde_json::json!({"body": "실패\n\n**Category**: test_failure\n\n<!-- autopilot:failure:3 -->"}),
        ];
        let result = compute_filtered_comments(&comments);
        let analysis = &result["failure_analysis"];
        assert_eq!(analysis["total_failures"], 3);
        // 3 consecutive same category → persona index 1 → "researcher"
        assert_eq!(analysis["recommended_persona"], "researcher");
    }

    #[test]
    fn classify_comment_categories() {
        assert_eq!(
            classify_comment("<!-- notified -->"),
            CommentCategory::InternalMarker
        );
        assert_eq!(
            classify_comment("<!-- autopilot:rework-detected -->"),
            CommentCategory::InternalMarker
        );
        assert_eq!(
            classify_comment("Report\n<!-- autopilot:escalated -->"),
            CommentCategory::InternalMarker
        );
        assert_eq!(
            classify_comment("PR created by autopilot: #50"),
            CommentCategory::PrLink
        );
        assert_eq!(
            classify_comment("실패\n<!-- autopilot:failure:1 -->"),
            CommentCategory::FailureMarker
        );
        assert_eq!(
            classify_comment("Autopilot 분석 결과: 영향 범위"),
            CommentCategory::Analysis
        );
        assert_eq!(
            classify_comment("재구현 필요"),
            CommentCategory::ReworkRequest
        );
        assert_eq!(
            classify_comment("일반 사용자 코멘트"),
            CommentCategory::Other
        );
    }

    #[test]
    fn extract_failure_attempt_parses_correctly() {
        assert_eq!(
            extract_failure_attempt("text <!-- autopilot:failure:3 --> more"),
            Some(3)
        );
        assert_eq!(extract_failure_attempt("no marker"), None);
        assert_eq!(
            extract_failure_attempt("<!-- autopilot:failure:12 -->"),
            Some(12)
        );
    }

    #[test]
    fn extract_failure_category_parses_both_formats() {
        assert_eq!(
            extract_failure_category("**Category**: lint_failure"),
            Some("lint_failure".to_string())
        );
        assert_eq!(
            extract_failure_category("Category: test_failure"),
            Some("test_failure".to_string())
        );
        assert_eq!(extract_failure_category("no category"), None);
    }

    #[test]
    fn filter_realistic_cycle_scenario() {
        // Simulates a real issue with 3 failed cycles + rework + analysis
        let comments = vec![
            serde_json::json!({"body": "Autopilot 분석 결과: auth 모듈에 refresh token 로직 추가 필요\n\n## 영향 범위\n- src/auth/mod.rs\n- src/auth/token.rs"}),
            serde_json::json!({"body": "Autopilot 구현 실패 (attempt 1/3)\n\n**Category**: lint_failure\n**Reason**: cargo clippy warnings\n\n<!-- autopilot:failure:1 -->"}),
            serde_json::json!({"body": "<!-- notified -->"}),
            serde_json::json!({"body": "PR created by autopilot: #50"}),
            serde_json::json!({"body": "재구현 필요 — API 스펙이 변경됨"}),
            serde_json::json!({"body": "Autopilot: 코멘트에서 재작업 요청 감지 — ready 라벨 재부여\n\n<!-- autopilot:rework-detected -->"}),
            serde_json::json!({"body": "Autopilot 구현 실패 (attempt 2/3)\n\n**Category**: lint_failure\n**Reason**: cargo clippy warnings\n\n<!-- autopilot:failure:2 -->"}),
            serde_json::json!({"body": "사용자: 이 함수의 리턴 타입을 Result로 바꿔주세요"}),
        ];
        let result = compute_filtered_comments(&comments);
        let filtered = result["comments"].as_array().unwrap();

        // Should keep: analysis(1) + user comment(1) + latest rework(1) + latest failure(1) = 4
        assert_eq!(filtered.len(), 4);

        // Verify excluded: notified, PR link, rework-detected marker, failure:1
        let all_bodies: Vec<&str> = filtered
            .iter()
            .map(|c| c["body"].as_str().unwrap())
            .collect();
        assert!(all_bodies.iter().any(|b| b.contains("분석 결과")));
        assert!(all_bodies.iter().any(|b| b.contains("리턴 타입")));
        assert!(all_bodies.iter().any(|b| b.contains("재구현 필요")));
        assert!(all_bodies.iter().any(|b| b.contains("failure:2")));
        assert!(!all_bodies.iter().any(|b| b.contains("failure:1")));
        assert!(!all_bodies.iter().any(|b| b.contains("notified")));
        assert!(!all_bodies.iter().any(|b| b.contains("PR created")));

        // Failure analysis: 2 consecutive lint_failure → hacker
        let analysis = &result["failure_analysis"];
        assert_eq!(analysis["total_failures"], 2);
        assert_eq!(analysis["repeated_category"], true);
        assert_eq!(analysis["recommended_persona"], "hacker");
    }

    #[test]
    fn persona_rotates_through_all_five() {
        let comments: Vec<Value> = (1..=6)
            .map(|i| {
                serde_json::json!({"body": format!(
                    "실패\n\n**Category**: test_failure\n\n<!-- autopilot:failure:{i} -->"
                )})
            })
            .collect();
        let result = compute_filtered_comments(&comments);
        let analysis = &result["failure_analysis"];

        // 6 consecutive → persona index min(6-2, 4) = 4 → "contrarian"
        assert_eq!(analysis["recommended_persona"], "contrarian");
    }

    #[test]
    fn persona_caps_at_contrarian_beyond_five() {
        let comments: Vec<Value> = (1..=10)
            .map(|i| {
                serde_json::json!({"body": format!(
                    "실패\n\n**Category**: lint_failure\n\n<!-- autopilot:failure:{i} -->"
                )})
            })
            .collect();
        let result = compute_filtered_comments(&comments);
        let analysis = &result["failure_analysis"];

        // 10 consecutive → capped at index 4 → "contrarian"
        assert_eq!(analysis["recommended_persona"], "contrarian");
    }

    #[test]
    fn category_break_resets_persona() {
        // A, A, B, B → last two are B, repeated=true, consecutive B=2 → hacker
        let comments = vec![
            serde_json::json!({"body": "실패\n\n**Category**: lint_failure\n\n<!-- autopilot:failure:1 -->"}),
            serde_json::json!({"body": "실패\n\n**Category**: lint_failure\n\n<!-- autopilot:failure:2 -->"}),
            serde_json::json!({"body": "실패\n\n**Category**: test_failure\n\n<!-- autopilot:failure:3 -->"}),
            serde_json::json!({"body": "실패\n\n**Category**: test_failure\n\n<!-- autopilot:failure:4 -->"}),
        ];
        let result = compute_filtered_comments(&comments);
        let analysis = &result["failure_analysis"];
        assert_eq!(analysis["repeated_category"], true);
        // Consecutive test_failure = 2 → index 0 → "hacker" (reset, not continuing from lint)
        assert_eq!(analysis["recommended_persona"], "hacker");
    }

    #[test]
    fn failure_markers_sorted_by_attempt_not_order() {
        // Comments arrive out of order (attempt 2 before attempt 1)
        let comments = vec![
            serde_json::json!({"body": "실패\n\n**Category**: lint_failure\n\n<!-- autopilot:failure:2 -->"}),
            serde_json::json!({"body": "실패\n\n**Category**: lint_failure\n\n<!-- autopilot:failure:1 -->"}),
        ];
        let result = compute_filtered_comments(&comments);
        let filtered = result["comments"].as_array().unwrap();

        // Latest by attempt number (2), not by array position
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0]["body"].as_str().unwrap().contains("failure:2"));

        let analysis = &result["failure_analysis"];
        assert_eq!(analysis["latest_attempt"], 2);
        assert_eq!(analysis["repeated_category"], true);
    }

    #[test]
    fn single_failure_no_persona() {
        let comments = vec![
            serde_json::json!({"body": "실패\n\n**Category**: lint_failure\n\n<!-- autopilot:failure:1 -->"}),
        ];
        let result = compute_filtered_comments(&comments);
        let analysis = &result["failure_analysis"];
        assert_eq!(analysis["total_failures"], 1);
        assert_eq!(analysis["repeated_category"], false);
        assert!(analysis["recommended_persona"].is_null());
    }

    #[test]
    fn empty_or_null_body_handled_without_panic() {
        let comments = vec![
            serde_json::json!({"body": ""}),
            serde_json::json!({"body": null}),
            serde_json::json!({"other_field": "no body"}),
            serde_json::json!({"body": "실제 사용자 코멘트"}),
        ];
        let result = compute_filtered_comments(&comments);
        // Should not panic; user comment is preserved
        let filtered = result["comments"].as_array().unwrap();
        assert!(filtered
            .iter()
            .any(|c| c["body"].as_str().unwrap_or("") == "실제 사용자 코멘트"));
    }

    #[test]
    fn rework_detected_with_meaningful_text_excluded() {
        // The marker comment has text before the marker — still excluded
        let comments = vec![
            serde_json::json!({"body": "Autopilot: 코멘트에서 재작업 요청 감지 — ready 라벨 재부여\n\n<!-- autopilot:rework-detected -->"}),
        ];
        let result = compute_filtered_comments(&comments);
        let filtered = result["comments"].as_array().unwrap();
        assert!(filtered.is_empty());
    }
}
