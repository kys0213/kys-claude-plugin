//! Shared type contracts for `atelier git`, ported from git-utils `types.ts`.
//! Commands return `Result<T, String>` (the `{ ok, data } | { ok, error }`
//! union in TypeScript maps onto Rust's `Ok`/`Err`).

/// A single review comment on a PR thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewComment {
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub url: String,
}

/// A PR review thread with its comments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewThread {
    pub is_resolved: bool,
    pub is_outdated: bool,
    pub path: String,
    pub line: i64,
    pub comments: Vec<ReviewComment>,
}

/// Result of fetching review threads for a PR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewsOutput {
    pub pr_title: String,
    pub pr_url: String,
    pub threads: Vec<ReviewThread>,
}

/// Input to the PR-creation guard.
#[derive(Debug, Clone, Default)]
pub struct PrGuardInput {
    /// The `tool_input.command` from a PreToolUse hook, used to match the
    /// `gh pr create` pattern. `None` skips the pattern filter.
    pub tool_command: Option<String>,
}

/// Decision of the PR-creation guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrGuardOutput {
    pub allowed: bool,
    pub reason: Option<String>,
    /// Existing open PR number (set when blocked).
    pub pr_number: Option<i64>,
}
