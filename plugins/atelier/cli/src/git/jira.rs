//! Jira ticket detection, ported from git-utils `core/jira.ts`
//! (originally the 3-stage regex matching of `detect-jira-ticket.sh`).
//!
//! Supported patterns:
//!   - `feat/WAD-0212`     → `WAD-0212`
//!   - `feat/wad-0212`     → `WAD-0212` (uppercased)
//!   - `WAD-0212`          → `WAD-0212`
//!   - `fix/wad-2223/desc` → `WAD-2223`
//!
//! The project key requires at least two letters to avoid mis-matches.

use regex::Regex;
use std::sync::LazyLock;

/// A detected Jira ticket: the `raw` substring that matched and its
/// `normalized` (uppercased) form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JiraTicket {
    pub raw: String,
    pub normalized: String,
}

// Pattern 1: prefix/TICKET-123 or prefix-TICKET-123 (lowercase prefix).
static PREFIXED_TICKET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z]+[-/]([A-Za-z]{2,}-\d+)").unwrap());
// Pattern 2: UPPERCASE TICKET-123 anywhere in the branch name.
static UPPERCASE_TICKET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([A-Z]{2,}-\d+)").unwrap());
// Pattern 3: lowercase ticket-123 anywhere (≥2-letter project key).
static LOWERCASE_TICKET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([a-z]{2,}-\d+)").unwrap());

/// Detects a Jira ticket in `branch_name`, returning `None` when no pattern
/// matches. Mirrors `JiraService.detectTicket` from git-utils.
pub fn detect_ticket(branch_name: &str) -> Option<JiraTicket> {
    if branch_name.is_empty() {
        return None;
    }

    // Pattern 1: prefix/TICKET-123 → normalize to uppercase.
    if let Some(caps) = PREFIXED_TICKET.captures(branch_name) {
        let raw = caps[1].to_string();
        let normalized = raw.to_uppercase();
        return Some(JiraTicket { raw, normalized });
    }

    // Pattern 2: already-uppercase TICKET-123 → raw == normalized.
    if let Some(caps) = UPPERCASE_TICKET.captures(branch_name) {
        let raw = caps[1].to_string();
        return Some(JiraTicket {
            normalized: raw.clone(),
            raw,
        });
    }

    // Pattern 3: lowercase ticket-123 → normalize to uppercase.
    if let Some(caps) = LOWERCASE_TICKET.captures(branch_name) {
        let raw = caps[1].to_string();
        let normalized = raw.to_uppercase();
        return Some(JiraTicket { raw, normalized });
    }

    None
}
