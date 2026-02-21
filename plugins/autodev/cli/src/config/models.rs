use serde::{Deserialize, Serialize};

/// .develop-workflow.yaml의 전체 스키마
/// 글로벌(~/) + 레포별 오버라이드를 딥머지하여 최종 설정 생성
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct WorkflowConfig {
    pub consumer: ConsumerConfig,
    pub workflow: WorkflowRouting,
    pub commands: CommandsConfig,
    pub develop: DevelopConfig,
}

/// Consumer 인프라 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConsumerConfig {
    pub scan_interval_secs: u64,
    pub scan_targets: Vec<String>,
    pub issue_concurrency: u32,
    pub pr_concurrency: u32,
    pub merge_concurrency: u32,
    pub stuck_threshold_secs: u64,
    pub model: String,
    pub workspace_strategy: String,
    pub filter_labels: Option<Vec<String>>,
    pub ignore_authors: Vec<String>,
    pub gh_host: Option<String>,
    pub confidence_threshold: f64,
}

impl Default for ConsumerConfig {
    fn default() -> Self {
        Self {
            scan_interval_secs: 300,
            scan_targets: vec!["issues".into(), "pulls".into()],
            issue_concurrency: 1,
            pr_concurrency: 1,
            merge_concurrency: 1,
            stuck_threshold_secs: 1800,
            model: "sonnet".into(),
            workspace_strategy: "worktree".into(),
            filter_labels: None,
            ignore_authors: vec!["dependabot".into(), "renovate".into()],
            gh_host: None,
            confidence_threshold: 0.7,
        }
    }
}

/// 워크플로우 라우팅 — Consumer가 어떤 워크플로우를 실행할지
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkflowRouting {
    pub issue: String,
    pub pr: String,
}

impl Default for WorkflowRouting {
    fn default() -> Self {
        Self {
            issue: "/develop-workflow:develop-auto".into(),
            pr: "/develop-workflow:multi-review".into(),
        }
    }
}

/// 워크플로우 내부 커맨드 매핑
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandsConfig {
    pub design: String,
    pub review: String,
    pub branch: String,
    pub branch_status: String,
    pub code_review: String,
    pub commit_and_pr: String,
}

impl Default for CommandsConfig {
    fn default() -> Self {
        Self {
            design: "/multi-llm-design".into(),
            review: "/multi-review".into(),
            branch: "/git-branch".into(),
            branch_status: "/branch-status".into(),
            code_review: "/multi-review".into(),
            commit_and_pr: "/commit-and-pr".into(),
        }
    }
}

/// 워크플로우 옵션
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DevelopConfig {
    pub review: ReviewConfig,
    pub implement: ImplementConfig,
    pub pr: PrConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReviewConfig {
    pub multi_llm: bool,
    pub auto_feedback: bool,
    pub max_iterations: u32,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            multi_llm: true,
            auto_feedback: true,
            max_iterations: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ImplementConfig {
    pub strategy: String,
    pub max_retries: u32,
    pub validate_each: bool,
}

impl Default for ImplementConfig {
    fn default() -> Self {
        Self {
            strategy: "auto".into(),
            max_retries: 3,
            validate_each: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PrConfig {
    pub code_review: bool,
}

impl Default for PrConfig {
    fn default() -> Self {
        Self { code_review: true }
    }
}
