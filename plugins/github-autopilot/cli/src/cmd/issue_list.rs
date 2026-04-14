use anyhow::Result;
use clap::ValueEnum;
use serde_json::Value;

use crate::gh::GhOps;

use super::labels;

#[derive(Clone, ValueEnum)]
pub enum Stage {
    /// No autopilot label, no analysis comment
    Unanalyzed,
    /// Has :ready, not :wip
    Ready,
    /// No :ready/:wip but has rework keyword in comments
    Rework,
    /// Has :wip
    Wip,
    /// Has escalation marker in comments
    Escalated,
}

/// List issues filtered by lifecycle stage.
pub fn list(
    gh: &dyn GhOps,
    stage: &Stage,
    label_prefix: &str,
    require_label: Option<&str>,
    limit: usize,
) -> Result<i32> {
    let issues = fetch_issues(gh, stage, label_prefix, limit)?;
    let filtered = filter_by_stage(&issues, stage, label_prefix);

    let result: Vec<Value> = filtered
        .into_iter()
        .filter(|issue| {
            if let Some(req) = require_label {
                let issue_labels = issue["labels"]
                    .as_array()
                    .map(|a| a.as_slice())
                    .unwrap_or(&[]);
                labels::has_exact_label(issue_labels, req)
            } else {
                true
            }
        })
        .map(|issue| slim_issue(&issue))
        .collect();

    println!("{}", serde_json::to_string(&result)?);
    Ok(0)
}

fn fetch_issues(
    gh: &dyn GhOps,
    stage: &Stage,
    label_prefix: &str,
    limit: usize,
) -> Result<Vec<Value>> {
    let limit_str = limit.to_string();

    match stage {
        Stage::Ready => {
            let ready_label = labels::with_prefix(label_prefix, labels::READY);
            gh.list_json(&[
                "issue",
                "list",
                "--label",
                &ready_label,
                "--state",
                "open",
                "--json",
                "number,title,labels",
                "--limit",
                &limit_str,
            ])
        }
        Stage::Wip => {
            let wip_label = labels::with_prefix(label_prefix, labels::WIP);
            gh.list_json(&[
                "issue",
                "list",
                "--label",
                &wip_label,
                "--state",
                "open",
                "--json",
                "number,title,labels",
                "--limit",
                &limit_str,
            ])
        }
        _ => gh.list_json(&[
            "issue",
            "list",
            "--state",
            "open",
            "--json",
            "number,title,body,labels,comments",
            "--limit",
            &limit_str,
        ]),
    }
}

/// Pure filtering logic — testable without gh calls.
pub fn filter_by_stage(issues: &[Value], stage: &Stage, label_prefix: &str) -> Vec<Value> {
    issues
        .iter()
        .filter(|issue| matches_stage(issue, stage, label_prefix))
        .cloned()
        .collect()
}

fn matches_stage(issue: &Value, stage: &Stage, prefix: &str) -> bool {
    let issue_labels = issue["labels"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[]);
    let comments = issue["comments"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    match stage {
        Stage::Unanalyzed => {
            !labels::has_prefixed_label(issue_labels, prefix)
                && !has_comment_containing(comments, "Autopilot 분석 결과")
                && !has_comment_containing(comments, "<!-- autopilot:false-positive -->")
        }
        Stage::Ready => {
            labels::has_label(issue_labels, prefix, labels::READY)
                && !labels::has_label(issue_labels, prefix, labels::WIP)
        }
        Stage::Rework => {
            !labels::has_label(issue_labels, prefix, labels::READY)
                && !labels::has_label(issue_labels, prefix, labels::WIP)
                && has_rework_request(comments)
        }
        Stage::Wip => labels::has_label(issue_labels, prefix, labels::WIP),
        Stage::Escalated => has_comment_containing(comments, "<!-- autopilot:escalated -->"),
    }
}

fn has_comment_containing(comments: &[Value], needle: &str) -> bool {
    comments
        .iter()
        .any(|c| c["body"].as_str().is_some_and(|b| b.contains(needle)))
}

fn has_rework_request(comments: &[Value]) -> bool {
    let mut has_keyword = false;
    let mut resolved = false;

    for comment in comments {
        let body = comment["body"].as_str().unwrap_or("");
        if super::issue::has_rework_keyword(body) {
            has_keyword = true;
        }
        if body.contains("<!-- autopilot:rework-resolved -->") {
            resolved = true;
        }
    }

    has_keyword && !resolved
}

fn slim_issue(issue: &Value) -> Value {
    serde_json::json!({
        "number": issue["number"],
        "title": issue["title"],
    })
}

/// Extract gap-fingerprint from issue body.
/// Returns JSON with fingerprint components + spec existence info.
pub fn extract_fingerprint(body: &str, check_path: Option<&dyn Fn(&str) -> bool>) -> Value {
    let fingerprint = extract_gap_fingerprint(body);

    match fingerprint {
        Some((full, spec_path, keyword)) => {
            let spec_exists = check_path.is_none_or(|f| f(&spec_path));
            serde_json::json!({
                "found": true,
                "fingerprint": full,
                "spec_path": spec_path,
                "keyword": keyword,
                "spec_exists": spec_exists,
            })
        }
        None => serde_json::json!({"found": false}),
    }
}

fn extract_gap_fingerprint(body: &str) -> Option<(String, String, String)> {
    for line in body.lines() {
        let trimmed = line.trim();
        // <!-- gap-fingerprint: gap:spec/auth.md:token-refresh -->
        // <!-- fingerprint: gap:spec/auth.md:token-refresh -->
        for prefix in &["<!-- gap-fingerprint:", "<!-- fingerprint:"] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                if let Some(content) = rest.strip_suffix("-->") {
                    let fp = content.trim();
                    if let Some(gap_content) = fp.strip_prefix("gap:") {
                        if let Some(colon) = gap_content.rfind(':') {
                            let spec_path = gap_content[..colon].to_string();
                            let keyword = gap_content[colon + 1..].to_string();
                            return Some((fp.to_string(), spec_path, keyword));
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_fingerprint_gap_format() {
        let body = "Some text\n\n<!-- gap-fingerprint: gap:spec/auth.md:token-refresh -->\n<!-- simhash: 0x123 -->";
        let result = extract_fingerprint(body, None);
        assert_eq!(result["found"], true);
        assert_eq!(result["fingerprint"], "gap:spec/auth.md:token-refresh");
        assert_eq!(result["spec_path"], "spec/auth.md");
        assert_eq!(result["keyword"], "token-refresh");
    }

    #[test]
    fn extract_fingerprint_standard_format() {
        let body = "`fingerprint: gap:spec/api.md:rate-limiting`\n<!-- fingerprint: gap:spec/api.md:rate-limiting -->";
        let result = extract_fingerprint(body, None);
        assert_eq!(result["found"], true);
        assert_eq!(result["spec_path"], "spec/api.md");
        assert_eq!(result["keyword"], "rate-limiting");
    }

    #[test]
    fn extract_fingerprint_not_found() {
        let body = "Just a regular issue body without fingerprint";
        let result = extract_fingerprint(body, None);
        assert_eq!(result["found"], false);
    }

    #[test]
    fn extract_fingerprint_non_gap() {
        let body = "<!-- fingerprint: ci-failure:workflow:main -->";
        let result = extract_fingerprint(body, None);
        assert_eq!(result["found"], false); // not a gap: prefix
    }

    #[test]
    fn extract_fingerprint_with_spec_check() {
        let body = "<!-- gap-fingerprint: gap:spec/missing.md:feature -->";
        let result = extract_fingerprint(body, Some(&|_path: &str| false));
        assert_eq!(result["found"], true);
        assert_eq!(result["spec_exists"], false);
    }

    #[test]
    fn filter_unanalyzed_excludes_labeled() {
        let issues = vec![
            serde_json::json!({
                "number": 1, "title": "Bug", "body": "",
                "labels": [{"name": "autopilot:ready"}],
                "comments": []
            }),
            serde_json::json!({
                "number": 2, "title": "Feature", "body": "",
                "labels": [],
                "comments": []
            }),
        ];
        let result = filter_by_stage(&issues, &Stage::Unanalyzed, "autopilot:");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["number"], 2);
    }

    #[test]
    fn filter_unanalyzed_excludes_analyzed() {
        let issues = vec![serde_json::json!({
            "number": 1, "title": "Bug", "body": "",
            "labels": [],
            "comments": [{"body": "Autopilot 분석 결과: skip"}]
        })];
        let result = filter_by_stage(&issues, &Stage::Unanalyzed, "autopilot:");
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn filter_unanalyzed_excludes_false_positive() {
        let issues = vec![serde_json::json!({
            "number": 1, "title": "Bug", "body": "",
            "labels": [],
            "comments": [{"body": "<!-- autopilot:false-positive -->"}]
        })];
        let result = filter_by_stage(&issues, &Stage::Unanalyzed, "autopilot:");
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn filter_ready_excludes_wip() {
        let issues = vec![
            serde_json::json!({
                "number": 1, "title": "Ready", "body": "",
                "labels": [{"name": "autopilot:ready"}],
                "comments": []
            }),
            serde_json::json!({
                "number": 2, "title": "WIP", "body": "",
                "labels": [{"name": "autopilot:ready"}, {"name": "autopilot:wip"}],
                "comments": []
            }),
        ];
        let result = filter_by_stage(&issues, &Stage::Ready, "autopilot:");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["number"], 1);
    }

    #[test]
    fn filter_rework_detects_keyword() {
        let issues = vec![serde_json::json!({
            "number": 1, "title": "Fix", "body": "",
            "labels": [],
            "comments": [{"body": "이 부분 재작업 필요합니다"}]
        })];
        let result = filter_by_stage(&issues, &Stage::Rework, "autopilot:");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn filter_rework_skips_resolved() {
        let issues = vec![serde_json::json!({
            "number": 1, "title": "Fix", "body": "",
            "labels": [],
            "comments": [
                {"body": "재작업 필요"},
                {"body": "<!-- autopilot:rework-resolved -->"}
            ]
        })];
        let result = filter_by_stage(&issues, &Stage::Rework, "autopilot:");
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn filter_ready_with_missing_labels_field() {
        // Issue JSON without a "labels" array — should not crash
        let issues = vec![serde_json::json!({
            "number": 1, "title": "No labels field", "body": "",
            "comments": []
        })];
        let result = filter_by_stage(&issues, &Stage::Ready, "autopilot:");
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn filter_with_malformed_label_object() {
        // Label without "name" field
        let issues = vec![serde_json::json!({
            "number": 1, "title": "Bad label", "body": "",
            "labels": [{"id": 123}],
            "comments": []
        })];
        let result = filter_by_stage(&issues, &Stage::Unanalyzed, "autopilot:");
        assert_eq!(result.len(), 1); // no prefixed label found → unanalyzed
    }

    #[test]
    fn filter_rework_excludes_ready_labeled() {
        // Issue with rework keyword BUT also has :ready — should NOT match rework stage
        let issues = vec![serde_json::json!({
            "number": 1, "title": "Fix", "body": "",
            "labels": [{"name": "autopilot:ready"}],
            "comments": [{"body": "rework 필요"}]
        })];
        let result = filter_by_stage(&issues, &Stage::Rework, "autopilot:");
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn filter_wip() {
        let issues = vec![
            serde_json::json!({
                "number": 1, "title": "In progress", "body": "",
                "labels": [{"name": "autopilot:wip"}],
                "comments": []
            }),
            serde_json::json!({
                "number": 2, "title": "Not wip", "body": "",
                "labels": [{"name": "autopilot:ready"}],
                "comments": []
            }),
        ];
        let result = filter_by_stage(&issues, &Stage::Wip, "autopilot:");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["number"], 1);
    }

    #[test]
    fn filter_escalated() {
        let issues = vec![
            serde_json::json!({
                "number": 1, "title": "Escalated", "body": "",
                "labels": [],
                "comments": [{"body": "## Autopilot Escalation Report\n\n<!-- autopilot:escalated -->"}]
            }),
            serde_json::json!({
                "number": 2, "title": "Normal", "body": "",
                "labels": [],
                "comments": []
            }),
        ];
        let result = filter_by_stage(&issues, &Stage::Escalated, "autopilot:");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["number"], 1);
    }
}
