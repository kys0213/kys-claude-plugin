use serde::{Deserialize, Serialize};

/// .develop-workflow.yaml의 전체 스키마 (v2)
/// 글로벌(~/) + 레포별 오버라이드를 딥머지하여 최종 설정 생성
///
/// v2에서 `commands`, `develop`, `workflow` 섹션을 제거하고
/// `workflows` 섹션으로 파이프라인 3단계(analyze, implement, review)를
/// 1급 개념으로 표현한다.
///
/// `deny_unknown_fields` 제거: v1 YAML에 deprecated 키가 있어도
/// 파싱 실패 없이 무시하고 유효한 키만 역직렬화한다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct WorkflowConfig {
    pub sources: SourcesConfig,
    pub daemon: DaemonConfig,
    pub workflows: Workflows,
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

// ═══════════════════════════════════════════════
// workflows — 파이프라인 단계별 실행 방식 (v2)
// ═══════════════════════════════════════════════

/// 파이프라인 단계별 워크플로우 설정.
///
/// ```text
/// analyze → implement → review
/// ```
///
/// 각 단계는 `agent`(builtin) 또는 `command`(커스텀 슬래시 커맨드) 중
/// 하나로 실행 방식을 지정한다. 둘 다 미지정 시 task_type별 기본 agent 사용.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Workflows {
    pub analyze: WorkflowStage,
    pub implement: WorkflowStage,
    pub review: ReviewStage,
}

impl Default for Workflows {
    fn default() -> Self {
        Self {
            analyze: WorkflowStage {
                agent: Some("autodev:issue-analyzer".into()),
                command: None,
            },
            implement: WorkflowStage {
                agent: Some("autodev:issue-analyzer".into()),
                command: None,
            },
            review: ReviewStage {
                agent: Some("autodev:pr-reviewer".into()),
                command: None,
                max_iterations: 2,
            },
        }
    }
}

/// 워크플로우 단계 공통 설정.
///
/// `agent`와 `command`는 상호 배타적이다.
/// - `agent`: autodev builtin agent에 위임 (예: `autodev:issue-analyzer`)
/// - `command`: 커스텀 슬래시 커맨드 실행 (예: `/develop-workflow:multi-review`)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct WorkflowStage {
    pub agent: Option<String>,
    pub command: Option<String>,
}

/// 리뷰 단계 설정 — WorkflowStage + max_iterations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReviewStage {
    pub agent: Option<String>,
    pub command: Option<String>,
    pub max_iterations: u32,
}

impl Default for ReviewStage {
    fn default() -> Self {
        Self {
            agent: Some("autodev:pr-reviewer".into()),
            command: None,
            max_iterations: 2,
        }
    }
}

impl ReviewStage {
    /// 워크플로우 라우팅에 필요한 agent/command 부분만 추출.
    pub fn as_stage(&self) -> WorkflowStage {
        WorkflowStage {
            agent: self.agent.clone(),
            command: self.command.clone(),
        }
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

    #[test]
    fn workflows_default_agents() {
        let cfg = Workflows::default();
        assert_eq!(cfg.analyze.agent.as_deref(), Some("autodev:issue-analyzer"));
        assert_eq!(
            cfg.implement.agent.as_deref(),
            Some("autodev:issue-analyzer")
        );
        assert_eq!(cfg.review.agent.as_deref(), Some("autodev:pr-reviewer"));
        assert_eq!(cfg.review.max_iterations, 2);
    }

    #[test]
    fn workflows_from_yaml_partial_override() {
        let yaml = r#"
workflows:
  review:
    max_iterations: 5
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        // review.max_iterations overridden
        assert_eq!(cfg.workflows.review.max_iterations, 5);
        // review.agent retains default
        assert_eq!(
            cfg.workflows.review.agent.as_deref(),
            Some("autodev:pr-reviewer")
        );
        // analyze/implement retain defaults from Workflows::default()
        assert_eq!(
            cfg.workflows.analyze.agent.as_deref(),
            Some("autodev:issue-analyzer")
        );
    }

    #[test]
    fn workflows_custom_command_overrides_agent() {
        let yaml = r#"
workflows:
  analyze:
    command: /develop-workflow:multi-analyze
    agent: null
  review:
    command: /develop-workflow:multi-review
    agent: null
    max_iterations: 3
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            cfg.workflows.analyze.command.as_deref(),
            Some("/develop-workflow:multi-analyze")
        );
        assert!(cfg.workflows.analyze.agent.is_none());
        assert_eq!(
            cfg.workflows.review.command.as_deref(),
            Some("/develop-workflow:multi-review")
        );
        assert!(cfg.workflows.review.agent.is_none());
        assert_eq!(cfg.workflows.review.max_iterations, 3);
    }

    #[test]
    fn deprecated_v1_keys_are_silently_ignored() {
        // v1 YAML with commands, develop, workflow keys
        // With deny_unknown_fields removed, these should be ignored
        let yaml = r#"
sources:
  github:
    model: opus
commands:
  design: /old-design
develop:
  review:
    multi_llm: true
workflow:
  issue: builtin
  pr: builtin
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        // Valid keys are parsed
        assert_eq!(cfg.sources.github.model, "opus");
        // workflows uses defaults (v1 keys ignored)
        assert_eq!(
            cfg.workflows.analyze.agent.as_deref(),
            Some("autodev:issue-analyzer")
        );
        assert_eq!(
            cfg.workflows.review.agent.as_deref(),
            Some("autodev:pr-reviewer")
        );
    }

    #[test]
    fn review_stage_as_stage() {
        let review = ReviewStage::default();
        let stage = review.as_stage();
        assert_eq!(stage.agent.as_deref(), Some("autodev:pr-reviewer"));
        assert!(stage.command.is_none());
    }
}
