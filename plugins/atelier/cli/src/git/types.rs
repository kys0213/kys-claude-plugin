//! Type definitions for the git subsystem — a faithful Rust port of
//! `git-utils/src/types.ts`. The `Result<T>` discriminated union and all
//! Input/Output structs preserve the original JSON shapes so `atelier git`
//! emits byte-identical output to the legacy `git-utils` CLI.

use serde::Serialize;

/// Mirror of the TS `Result<T> = {ok:true,data} | {ok:false,error}`. Commands
/// return this; the CLI layer serializes `data` on success and prints
/// `Error: <error>` to stderr (exit 1) on failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdResult<T> {
    Ok(T),
    Err(String),
}

impl<T> CmdResult<T> {
    pub fn is_ok(&self) -> bool {
        matches!(self, CmdResult::Ok(_))
    }
}

// ---------------------------------------------------------------------------
// Commit
// ---------------------------------------------------------------------------

pub const COMMIT_TYPES: [&str; 8] = [
    "feat", "fix", "docs", "style", "refactor", "test", "chore", "perf",
];

#[derive(Debug, Clone)]
pub struct CommitInput {
    pub commit_type: String,
    pub description: String,
    pub scope: Option<String>,
    pub body: Option<String>,
    pub skip_add: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CommitOutput {
    pub subject: String,
    #[serde(rename = "jiraTicket", skip_serializing_if = "Option::is_none")]
    pub jira_ticket: Option<String>,
}

// ---------------------------------------------------------------------------
// Branch
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BranchInput {
    pub branch_name: String,
    pub base_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BranchOutput {
    #[serde(rename = "branchName")]
    pub branch_name: String,
    #[serde(rename = "baseBranch")]
    pub base_branch: String,
}

// ---------------------------------------------------------------------------
// PR
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PrInput {
    pub title: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PrOutput {
    pub url: String,
    pub title: String,
    #[serde(rename = "baseBranch")]
    pub base_branch: String,
    #[serde(rename = "jiraTicket", skip_serializing_if = "Option::is_none")]
    pub jira_ticket: Option<String>,
}

// ---------------------------------------------------------------------------
// Reviews
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ReviewsInput {
    pub pr_number: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReviewComment {
    pub author: String,
    pub body: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReviewThread {
    #[serde(rename = "isResolved")]
    pub is_resolved: bool,
    #[serde(rename = "isOutdated")]
    pub is_outdated: bool,
    pub path: String,
    pub line: i64,
    pub comments: Vec<ReviewComment>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReviewsOutput {
    #[serde(rename = "prTitle")]
    pub pr_title: String,
    #[serde(rename = "prUrl")]
    pub pr_url: String,
    pub threads: Vec<ReviewThread>,
}

// ---------------------------------------------------------------------------
// Guard
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardTarget {
    Write,
    Commit,
}

#[derive(Debug, Clone)]
pub struct GuardInput {
    pub target: GuardTarget,
    pub project_dir: String,
    pub create_branch_script: String,
    pub default_branch: Option<String>,
    pub protected_branches: Option<Vec<String>>,
    pub tool_command: Option<String>,
    pub tool_file_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardOutput {
    pub allowed: bool,
    pub reason: Option<String>,
    pub current_branch: Option<String>,
    pub default_branch: Option<String>,
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct HookRegisterInput {
    pub hook_type: String,
    pub matcher: String,
    pub command: String,
    pub timeout: Option<i64>,
    pub project_dir: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HookUnregisterInput {
    pub hook_type: String,
    pub command: String,
    pub project_dir: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HookListInput {
    pub hook_type: Option<String>,
    pub project_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HookRegisterOutput {
    pub action: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HookUnregisterOutput {
    pub command: String,
}

// ---------------------------------------------------------------------------
// Jira
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JiraTicket {
    pub raw: String,
    pub normalized: String,
}

// ---------------------------------------------------------------------------
// PR Guard
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct PrGuardInput {
    pub tool_command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrGuardOutput {
    pub allowed: bool,
    pub reason: Option<String>,
    pub pr_number: Option<i64>,
}

// ---------------------------------------------------------------------------
// Git special state — shared by core modules
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitSpecialState {
    pub rebase: bool,
    pub merge: bool,
    pub detached: bool,
}
