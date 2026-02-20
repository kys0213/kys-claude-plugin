use serde::{Deserialize, Serialize};

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
