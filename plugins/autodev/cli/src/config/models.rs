use serde::{Deserialize, Serialize};

/// .develop-workflow.yaml의 전체 스키마
/// 글로벌(~/) + 레포별 오버라이드를 딥머지하여 최종 설정 생성
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct WorkflowConfig {
    pub sources: SourcesConfig,
    pub daemon: DaemonConfig,
    pub workflow: WorkflowRouting,
    pub commands: CommandsConfig,
    pub develop: DevelopConfig,
}

/// 태스크 소스 설정 — 소스 종류별 하위 키
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SourcesConfig {
    pub github: GitHubSourceConfig,
}

/// 데몬 루프 전용 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    pub tick_interval_secs: u64,
    pub daily_report_hour: u32,
    pub log_dir: String,
    /// 로그 레벨 (trace, debug, info, warn, error). RUST_LOG 환경변수가 우선.
    pub log_level: String,
    pub log_retention_days: u32,
    /// 전체 동시 실행 가능한 파이프라인 태스크 상한 (Claude 세션 수)
    pub max_concurrent_tasks: u32,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            tick_interval_secs: 10,
            daily_report_hour: 6,
            log_dir: "logs".into(),
            log_level: "info".into(),
            log_retention_days: 30,
            max_concurrent_tasks: 3,
        }
    }
}

/// GitHub 소스 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GitHubSourceConfig {
    pub scan_interval_secs: u64,
    pub scan_targets: Vec<String>,
    pub issue_concurrency: u32,
    pub pr_concurrency: u32,
    pub model: String,
    pub workspace_strategy: String,
    pub filter_labels: Option<Vec<String>>,
    pub ignore_authors: Vec<String>,
    pub gh_host: Option<String>,
    pub confidence_threshold: f64,
    pub knowledge_extraction: bool,
}

impl Default for GitHubSourceConfig {
    fn default() -> Self {
        Self {
            scan_interval_secs: 300,
            scan_targets: vec!["issues".into(), "pulls".into()],
            issue_concurrency: 1,
            pr_concurrency: 1,
            model: "sonnet".into(),
            workspace_strategy: "worktree".into(),
            filter_labels: None,
            ignore_authors: vec!["dependabot".into(), "renovate".into()],
            gh_host: None,
            confidence_threshold: 0.7,
            knowledge_extraction: true,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_config_default_log_level_is_info() {
        let cfg = DaemonConfig::default();
        assert_eq!(cfg.log_level, "info");
    }

    #[test]
    fn daemon_config_log_level_from_yaml() {
        let yaml = r#"
daemon:
  log_level: "debug"
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.daemon.log_level, "debug");
        // 나머지 필드는 기본값 유지
        assert_eq!(cfg.daemon.log_dir, "logs");
    }

    #[test]
    fn daemon_config_log_level_defaults_when_omitted() {
        let yaml = r#"
daemon:
  log_dir: "/var/log/autodev"
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.daemon.log_level, "info");
        assert_eq!(cfg.daemon.log_dir, "/var/log/autodev");
    }
}
