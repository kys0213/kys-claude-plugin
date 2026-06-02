//! Jira ticket detection — port of `git-utils/src/core/jira.ts`. Three ordered
//! regex patterns; the first match wins, preserving the exact priority and
//! normalization (uppercasing) of the TS implementation.

use crate::git::types::JiraTicket;
use regex::Regex;
use std::sync::LazyLock;

// Pattern 1: prefix/TICKET-123 or prefix-TICKET-123
static PREFIXED_TICKET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z]+[-/]([A-Za-z]{2,}-\d+)").unwrap());
// Pattern 2: UPPERCASE TICKET-123 (anywhere)
static UPPERCASE_TICKET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([A-Z]{2,}-\d+)").unwrap());
// Pattern 3: lowercase ticket-123 (anywhere, >=2 char project key)
static LOWERCASE_TICKET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([a-z]{2,}-\d+)").unwrap());

/// Service interface so commands can be unit-tested with a mock detector.
pub trait JiraService {
    fn detect_ticket(&self, branch_name: &str) -> Option<JiraTicket>;
}

/// Real implementation backed by the three regex patterns.
pub struct RealJiraService;

impl JiraService for RealJiraService {
    fn detect_ticket(&self, branch_name: &str) -> Option<JiraTicket> {
        detect_ticket(branch_name)
    }
}

/// Constructs the real Jira service.
pub fn create_jira_service() -> RealJiraService {
    RealJiraService
}

/// Free-function detection, exposed for direct (black-box) testing.
pub fn detect_ticket(branch_name: &str) -> Option<JiraTicket> {
    if branch_name.is_empty() {
        return None;
    }

    if let Some(caps) = PREFIXED_TICKET.captures(branch_name) {
        let raw = caps.get(1).unwrap().as_str().to_string();
        let normalized = raw.to_uppercase();
        return Some(JiraTicket { raw, normalized });
    }

    if let Some(caps) = UPPERCASE_TICKET.captures(branch_name) {
        let raw = caps.get(1).unwrap().as_str().to_string();
        return Some(JiraTicket {
            normalized: raw.clone(),
            raw,
        });
    }

    if let Some(caps) = LOWERCASE_TICKET.captures(branch_name) {
        let raw = caps.get(1).unwrap().as_str().to_string();
        let normalized = raw.to_uppercase();
        return Some(JiraTicket { raw, normalized });
    }

    None
}
