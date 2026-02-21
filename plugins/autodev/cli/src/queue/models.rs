use serde::{Deserialize, Serialize};

// ─── Full models (SELECT *) ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub id: String,
    pub url: String,
    pub name: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueQueueItem {
    pub id: String,
    pub repo_id: String,
    pub github_number: i64,
    pub title: String,
    pub body: Option<String>,
    pub labels: Option<String>,
    pub author: String,
    pub analysis_report: Option<String>,
    pub status: String,
    pub worker_id: Option<String>,
    pub branch_name: Option<String>,
    pub pr_number: Option<i64>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrQueueItem {
    pub id: String,
    pub repo_id: String,
    pub github_number: i64,
    pub title: String,
    pub body: Option<String>,
    pub author: String,
    pub head_branch: String,
    pub base_branch: String,
    pub review_comment: Option<String>,
    pub status: String,
    pub worker_id: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeQueueItem {
    pub id: String,
    pub repo_id: String,
    pub pr_number: i64,
    pub title: String,
    pub head_branch: String,
    pub base_branch: String,
    pub status: String,
    pub conflict_files: Option<String>,
    pub worker_id: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerLog {
    pub id: String,
    pub repo_id: String,
    pub queue_type: String,
    pub queue_item_id: String,
    pub worker_id: String,
    pub command: String,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
}

// ─── Input models (INSERT) ───

pub struct NewIssueItem {
    pub repo_id: String,
    pub github_number: i64,
    pub title: String,
    pub body: Option<String>,
    pub labels: String,
    pub author: String,
}

pub struct NewPrItem {
    pub repo_id: String,
    pub github_number: i64,
    pub title: String,
    pub body: Option<String>,
    pub author: String,
    pub head_branch: String,
    pub base_branch: String,
}

pub struct NewMergeItem {
    pub repo_id: String,
    pub pr_number: i64,
    pub title: String,
    pub head_branch: String,
    pub base_branch: String,
}

pub struct NewConsumerLog {
    pub repo_id: String,
    pub queue_type: String,
    pub queue_item_id: String,
    pub worker_id: String,
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: i64,
}

// ─── Query result models (projections) ───

pub struct EnabledRepo {
    pub id: String,
    pub url: String,
    pub name: String,
}

pub struct PendingIssue {
    pub id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub github_number: i64,
    pub title: String,
    pub body: Option<String>,
    pub repo_url: String,
}

/// ready 상태 이슈 (구현 단계용 — analysis_report 포함)
pub struct ReadyIssue {
    pub id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub github_number: i64,
    pub title: String,
    pub analysis_report: Option<String>,
    pub repo_url: String,
}

pub struct PendingPr {
    pub id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub github_number: i64,
    pub title: String,
    pub head_branch: String,
    pub base_branch: String,
    pub repo_url: String,
}

pub struct PendingMerge {
    pub id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub pr_number: i64,
    pub head_branch: String,
    pub base_branch: String,
    pub repo_url: String,
}

pub struct RepoStatusRow {
    pub name: String,
    pub enabled: bool,
    pub issue_pending: i64,
    pub pr_pending: i64,
    pub merge_pending: i64,
}

pub struct RepoInfo {
    pub name: String,
    pub url: String,
    pub enabled: bool,
}

pub struct QueueListItem {
    pub github_number: i64,
    pub title: String,
    pub status: String,
}

pub struct LogEntry {
    pub started_at: String,
    pub queue_type: String,
    pub command: String,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
}

/// 큐 항목 상태 업데이트용 optional fields
#[derive(Default)]
pub struct StatusFields {
    pub worker_id: Option<String>,
    pub analysis_report: Option<String>,
    pub review_comment: Option<String>,
    pub error_message: Option<String>,
}
