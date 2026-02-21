use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub scan_interval_secs: u64,
    pub scan_targets: Vec<String>,
    pub issue_concurrency: u32,
    pub pr_concurrency: u32,
    pub merge_concurrency: u32,
    pub model: String,
    pub issue_workflow: String,
    pub pr_workflow: String,
    pub filter_labels: Option<Vec<String>>,
    pub ignore_authors: Vec<String>,
    pub workspace_strategy: String,
    /// GitHub Enterprise 호스트 (None이면 github.com)
    pub gh_host: Option<String>,
}

impl Default for RepoConfig {
    fn default() -> Self {
        Self {
            scan_interval_secs: 300,
            scan_targets: vec!["issues".into(), "pulls".into()],
            issue_concurrency: 1,
            pr_concurrency: 1,
            merge_concurrency: 1,
            model: "sonnet".into(),
            issue_workflow: "/develop-workflow:develop-auto".into(),
            pr_workflow: "/develop-workflow:multi-review".into(),
            filter_labels: None,
            ignore_authors: vec!["dependabot".into(), "renovate".into()],
            workspace_strategy: "worktree".into(),
            gh_host: None,
        }
    }
}
