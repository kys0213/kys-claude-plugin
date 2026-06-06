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

/// Input to the `reviews` command.
#[derive(Debug, Clone, Default)]
pub struct ReviewsInput {
    /// Explicit PR number; when absent the current branch's PR is detected.
    pub pr_number: Option<i64>,
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

/// Which protected-branch guard is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardTarget {
    Write,
    Commit,
}

/// Input to the default-branch guard.
#[derive(Debug, Clone)]
pub struct GuardInput {
    pub target: GuardTarget,
    pub project_dir: String,
    pub create_branch_script: String,
    pub default_branch: Option<String>,
    /// Extra protected branches beyond the default branch (and `develop`).
    pub protected_branches: Option<Vec<String>>,
    /// commit guard: the `tool_input.command` from stdin.
    pub tool_command: Option<String>,
    /// write guard: the `tool_input.file_path` from stdin.
    pub tool_file_path: Option<String>,
}

/// Decision of the default-branch guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardOutput {
    pub allowed: bool,
    pub reason: Option<String>,
    pub current_branch: Option<String>,
    pub default_branch: Option<String>,
}

/// Input to the `branch` command.
#[derive(Debug, Clone)]
pub struct BranchInput {
    pub branch_name: String,
    pub base_branch: Option<String>,
}

/// Output of the `branch` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchOutput {
    pub branch_name: String,
    pub base_branch: String,
}

/// Conventional-commit types accepted by the `commit` command.
pub const COMMIT_TYPES: [&str; 8] = [
    "feat", "fix", "docs", "style", "refactor", "test", "chore", "perf",
];

/// Input to the `commit` command. `commit_type` is a raw string so an invalid
/// type surfaces as a command error (matching the TS contract).
#[derive(Debug, Clone)]
pub struct CommitInput {
    pub commit_type: String,
    pub description: String,
    pub scope: Option<String>,
    pub body: Option<String>,
    pub skip_add: bool,
}

/// Output of the `commit` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitOutput {
    pub subject: String,
    pub jira_ticket: Option<String>,
}

/// Input to the `pr` command.
#[derive(Debug, Clone)]
pub struct PrInput {
    pub title: String,
    pub description: Option<String>,
}

/// Output of the `pr` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrOutput {
    pub url: String,
    pub title: String,
    pub base_branch: String,
    pub jira_ticket: Option<String>,
}
