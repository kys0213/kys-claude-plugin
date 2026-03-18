//! GitHub comment reply scanner for HITL auto-response.
//!
//! Scans GitHub issue/PR comments for replies to autodev HITL marker comments.
//! When a reply is found, automatically saves it as a HITL response.

use std::sync::Arc;

use crate::core::models::*;
use crate::core::repository::HitlRepository;
use crate::infra::db::Database;
use crate::infra::gh::Gh;

/// Marker prefix used in HITL GitHub comments.
const HITL_MARKER_PREFIX: &str = "<!-- autodev:hitl:";

/// Scan GitHub comments for replies to HITL marker comments.
///
/// For each pending HITL event with a work_id:
/// 1. Fetch recent comments on the linked issue/PR
/// 2. Find the autodev HITL marker comment
/// 3. Look for replies posted after the marker
/// 4. Parse reply content as choice number or free text
/// 5. Save as HITL response
pub async fn scan_replies(db: &Database, gh: &Arc<dyn Gh>, gh_host: Option<&str>) -> Vec<String> {
    let mut responses = Vec::new();

    let pending = match db.hitl_list(None) {
        Ok(events) => events
            .into_iter()
            .filter(|e| matches!(e.status, HitlStatus::Pending))
            .collect::<Vec<_>>(),
        Err(_) => return responses,
    };

    for event in &pending {
        let work_id = match &event.work_id {
            Some(id) => id,
            None => continue,
        };

        let (repo_name, number) = match parse_work_id(work_id) {
            Some(parsed) => parsed,
            None => continue,
        };

        // Fetch comments on the issue/PR
        let endpoint = format!("repos/{repo_name}/issues/{number}/comments");
        let comments_json = match gh.api_paginate(&repo_name, &endpoint, &[], gh_host).await {
            Ok(data) => data,
            Err(_) => continue,
        };

        let comments: Vec<serde_json::Value> = match serde_json::from_slice(&comments_json) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Find the marker comment and any reply after it
        let mut marker_found = false;
        let marker_id = format!("{HITL_MARKER_PREFIX}{} -->", event.id);

        for comment in &comments {
            let body = comment["body"].as_str().unwrap_or("");

            if body.contains(&marker_id) {
                marker_found = true;
                continue;
            }

            // Reply must come after the marker comment and not be from autodev
            if marker_found {
                let author = comment["user"]["login"].as_str().unwrap_or("");
                if author == "github-actions[bot]" || author.is_empty() {
                    continue;
                }

                // Parse choice or free text
                let trimmed = body.trim();
                let (choice, _message) = parse_reply_content(trimmed);

                let response = NewHitlResponse {
                    event_id: event.id.clone(),
                    choice,
                    message: Some(trimmed.to_string()),
                    source: format!("github:{author}"),
                };

                if let Err(e) = db.hitl_respond(&response) {
                    tracing::warn!("failed to save reply for HITL {}: {e}", event.id);
                } else {
                    responses.push(format!(
                        "HITL {} responded via GitHub comment by @{author}",
                        event.id
                    ));
                }

                break; // Only process first reply
            }
        }
    }

    responses
}

/// Parse work_id into (repo_name, number).
fn parse_work_id(work_id: &str) -> Option<(String, i64)> {
    let parts: Vec<&str> = work_id.splitn(3, ':').collect();
    if parts.len() != 3 {
        return None;
    }
    let number = parts[2].parse::<i64>().ok()?;
    Some((parts[1].to_string(), number))
}

/// Parse reply content: if it starts with a number, use as choice.
fn parse_reply_content(text: &str) -> (Option<i32>, Option<String>) {
    let first_word = text.split_whitespace().next().unwrap_or("");
    if let Ok(n) = first_word.parse::<i32>() {
        (
            Some(n),
            if text.len() > first_word.len() {
                Some(text.to_string())
            } else {
                None
            },
        )
    } else {
        (None, Some(text.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_work_id_valid() {
        assert_eq!(
            parse_work_id("issue:org/repo:42"),
            Some(("org/repo".to_string(), 42))
        );
        assert_eq!(
            parse_work_id("pr:org/repo:15"),
            Some(("org/repo".to_string(), 15))
        );
    }

    #[test]
    fn parse_work_id_invalid() {
        assert!(parse_work_id("invalid").is_none());
        assert!(parse_work_id("issue:repo").is_none());
    }

    #[test]
    fn parse_reply_choice_number() {
        let (choice, msg) = parse_reply_content("1");
        assert_eq!(choice, Some(1));
        assert!(msg.is_none());
    }

    #[test]
    fn parse_reply_choice_with_text() {
        let (choice, msg) = parse_reply_content("2 skip this one");
        assert_eq!(choice, Some(2));
        assert!(msg.is_some());
    }

    #[test]
    fn parse_reply_free_text() {
        let (choice, msg) = parse_reply_content("please retry with different approach");
        assert!(choice.is_none());
        assert!(msg.is_some());
    }
}
