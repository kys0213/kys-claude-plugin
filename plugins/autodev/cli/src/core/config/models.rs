use serde::{Deserialize, Serialize};

/// .autodev.yaml의 전체 스키마 (v2)
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
    pub claw: ClawConfig,
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
    /// Webhook URL for notifications (e.g. Slack incoming webhook).
    /// When set, HITL timeout events and other notifications are sent here.
    pub webhook_url: Option<String>,
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
            webhook_url: None,
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
    /// 분석 완료 후 자동 구현 전환 활성화 (기본: false)
    pub auto_approve: bool,
    /// 자동 전환을 위한 최소 confidence 임계값 (기본: 0.8)
    pub auto_approve_threshold: f64,
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
            auto_approve: false,
            auto_approve_threshold: 0.8,
        }
    }
}

// ═══════════════════════════════════════════════
// claw — Claw 레이어 설정
// ═══════════════════════════════════════════════

/// Claw 레이어 설정.
///
/// `enabled: true`이면 daemon은 Ready→Running drain만 수행하고,
/// Pending→Ready 전이는 Claw(CLI)가 담당한다.
/// `enabled: false`(기본)이면 기존 Pending→Running 직행 동작을 유지한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClawConfig {
    pub enabled: bool,
    pub recovery_interval_secs: u64,
    /// claw-evaluate cron 주기 (초). 기본 60초.
    pub schedule_interval_secs: u64,
    /// gap-detection cron 주기 (초). 기본 3600초 (1시간).
    pub gap_detection_interval_secs: u64,
}

impl Default for ClawConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            recovery_interval_secs: 120,
            schedule_interval_secs: 60,
            gap_detection_interval_secs: 3600,
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
/// 각 단계는 `command`로 커스텀 슬래시 커맨드를 지정할 수 있다.
/// 미지정 시 task_type별 기본 출력 스펙이 system prompt로 사용된다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Workflows {
    pub analyze: WorkflowStage,
    pub implement: WorkflowStage,
    pub review: ReviewStage,
}

/// 워크플로우 단계 공통 설정.
///
/// `command`가 지정되면 해당 슬래시 커맨드를 system prompt로 사용한다.
/// 미지정 시 task_type별 기본 출력 스펙이 적용된다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct WorkflowStage {
    pub command: Option<String>,
}

/// 리뷰 단계 설정 — WorkflowStage + max_iterations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReviewStage {
    pub command: Option<String>,
    pub max_iterations: u32,
}

impl Default for ReviewStage {
    fn default() -> Self {
        Self {
            command: None,
            max_iterations: 2,
        }
    }
}

impl ReviewStage {
    /// 워크플로우 라우팅에 필요한 command 부분만 추출.
    pub fn as_stage(&self) -> WorkflowStage {
        WorkflowStage {
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
    fn workflows_default() {
        let cfg = Workflows::default();
        assert!(cfg.analyze.command.is_none());
        assert!(cfg.implement.command.is_none());
        assert!(cfg.review.command.is_none());
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
        assert_eq!(cfg.workflows.review.max_iterations, 5);
        assert!(cfg.workflows.review.command.is_none());
    }

    #[test]
    fn workflows_custom_command() {
        let yaml = r#"
workflows:
  analyze:
    command: /review:multi-analyze
  review:
    command: /review:multi-review
    max_iterations: 3
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            cfg.workflows.analyze.command.as_deref(),
            Some("/review:multi-analyze")
        );
        assert_eq!(
            cfg.workflows.review.command.as_deref(),
            Some("/review:multi-review")
        );
        assert_eq!(cfg.workflows.review.max_iterations, 3);
    }

    #[test]
    fn deprecated_v1_keys_are_silently_ignored() {
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
        assert_eq!(cfg.sources.github.model, "opus");
        // workflows uses defaults (v1 keys ignored)
        assert!(cfg.workflows.analyze.command.is_none());
        assert!(cfg.workflows.review.command.is_none());
    }

    #[test]
    fn review_stage_as_stage() {
        let review = ReviewStage {
            command: Some("/custom-review".into()),
            max_iterations: 3,
        };
        let stage = review.as_stage();
        assert_eq!(stage.command.as_deref(), Some("/custom-review"));
    }

    #[test]
    fn deprecated_agent_field_is_silently_ignored() {
        // 기존 YAML에 agent 필드가 있어도 파싱 실패 없이 무시
        let yaml = r#"
workflows:
  analyze:
    agent: autodev:issue-analyzer
    command: /custom-analyze
  review:
    agent: autodev:pr-reviewer
    max_iterations: 3
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            cfg.workflows.analyze.command.as_deref(),
            Some("/custom-analyze")
        );
        assert_eq!(cfg.workflows.review.max_iterations, 3);
    }

    #[test]
    fn claw_config_defaults() {
        let cfg = ClawConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.recovery_interval_secs, 120);
        assert_eq!(cfg.schedule_interval_secs, 60);
        assert_eq!(cfg.gap_detection_interval_secs, 3600);
    }

    #[test]
    fn claw_config_from_yaml() {
        let yaml = r#"
claw:
  enabled: true
  recovery_interval_secs: 60
  schedule_interval_secs: 30
  gap_detection_interval_secs: 1800
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.claw.enabled);
        assert_eq!(cfg.claw.recovery_interval_secs, 60);
        assert_eq!(cfg.claw.schedule_interval_secs, 30);
        assert_eq!(cfg.claw.gap_detection_interval_secs, 1800);
    }

    #[test]
    fn claw_config_defaults_when_omitted() {
        let yaml = r#"
daemon:
  log_level: "info"
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!cfg.claw.enabled);
        assert_eq!(cfg.claw.recovery_interval_secs, 120);
        assert_eq!(cfg.claw.schedule_interval_secs, 60);
        assert_eq!(cfg.claw.gap_detection_interval_secs, 3600);
    }

    #[test]
    fn claw_config_partial_override_preserves_defaults() {
        let yaml = r#"
claw:
  enabled: true
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.claw.enabled);
        assert_eq!(cfg.claw.recovery_interval_secs, 120);
        assert_eq!(cfg.claw.schedule_interval_secs, 60);
        assert_eq!(cfg.claw.gap_detection_interval_secs, 3600);
    }
}
