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
    pub escalation: EscalationConfig,
    pub v5: V5Config,
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
    /// Webhook URL for notifications (backward compat, prefer notifications.channels).
    pub webhook_url: Option<String>,
    /// Multi-channel notification configuration.
    pub notifications: NotificationConfig,
    /// Claude CLI 프로세스 타임아웃 (초). 기본 1800초 (30분).
    /// 이 시간 초과 시 프로세스를 강제 종료한다.
    pub task_timeout_secs: u64,
    /// Shutdown drain 타임아웃 (초). 기본 30초.
    /// SIGINT 후 in-flight 태스크 완료 대기 상한.
    pub shutdown_drain_timeout_secs: u64,
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
            notifications: NotificationConfig::default(),
            task_timeout_secs: 1800,
            shutdown_drain_timeout_secs: 30,
        }
    }
}

/// Multi-channel notification configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationConfig {
    pub channels: Vec<NotificationChannel>,
}

/// A single notification channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannel {
    /// Channel type: "webhook" or "github_comment"
    #[serde(rename = "type")]
    pub channel_type: String,
    /// Channel-specific configuration.
    #[serde(default)]
    pub config: ChannelConfig,
}

/// Channel-specific configuration fields.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelConfig {
    /// Webhook URL (for webhook type).
    pub url: Option<String>,
    /// @mention target (for github_comment type).
    pub mention: Option<String>,
    /// Severity filter — only send notifications matching these levels.
    pub severity_filter: Vec<String>,
}

/// GitHub 소스 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GitHubSourceConfig {
    pub scan_interval_secs: u64,
    pub scan_targets: Vec<String>,
    /// 워크스페이스(레포)당 동시 Running 아이템 상한.
    /// issue_concurrency + pr_concurrency 합산과 별개로, 워크스페이스 전체의 상한을 지정한다.
    /// 0이면 제한 없음 (issue_concurrency + pr_concurrency 한도만 적용).
    pub concurrency: u32,
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
            concurrency: 0,
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
// escalation — 5-Level failure escalation 정책
// ═══════════════════════════════════════════════

/// 5단계 에스컬레이션 정책 설정.
///
/// workspace yaml에서 failure_count → action 매핑과 on_fail script를 정의한다.
///
/// ```yaml
/// escalation:
///   levels:
///     1: retry
///     2: retry_with_comment
///     3: hitl
///     4: skip
///     5: replan
///   on_fail:
///     - "echo 'task failed'"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EscalationConfig {
    /// failure_count → EscalationAction 매핑.
    /// 키는 1~5, 값은 retry | retry_with_comment | hitl | skip | replan.
    pub levels: std::collections::BTreeMap<u32, EscalationAction>,
    /// 실패 시 실행할 on_fail script 목록.
    /// retry 레벨에서는 실행하지 않고, 나머지 레벨에서 순차 실행한다.
    pub on_fail: Vec<String>,
}

impl Default for EscalationConfig {
    fn default() -> Self {
        let mut levels = std::collections::BTreeMap::new();
        levels.insert(1, EscalationAction::Retry);
        levels.insert(2, EscalationAction::RetryWithComment);
        levels.insert(3, EscalationAction::Hitl);
        levels.insert(4, EscalationAction::Skip);
        levels.insert(5, EscalationAction::Replan);
        Self {
            levels,
            on_fail: Vec::new(),
        }
    }
}

impl EscalationConfig {
    /// failure_count에 대응하는 EscalationAction을 반환한다.
    /// 정의된 범위를 초과하면 가장 높은 레벨의 action을 반환한다.
    pub fn action_for(&self, failure_count: u32) -> EscalationAction {
        if let Some(action) = self.levels.get(&failure_count) {
            return *action;
        }
        // failure_count가 정의된 최대 레벨을 초과하면 최고 레벨 action 사용
        self.levels
            .values()
            .last()
            .copied()
            .unwrap_or(EscalationAction::Replan)
    }

    /// 해당 action에서 on_fail script를 실행해야 하는지 여부.
    /// retry만 on_fail을 실행하지 않는다.
    pub fn should_run_on_fail(&self, action: EscalationAction) -> bool {
        action != EscalationAction::Retry
    }
}

/// 에스컬레이션 액션 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationAction {
    /// 조용한 재시도 (on_fail 실행 안 함, worktree 보존)
    Retry,
    /// on_fail 실행 + 재시도
    RetryWithComment,
    /// on_fail 실행 + HITL 이벤트 생성
    Hitl,
    /// on_fail 실행 + Skipped 상태 전이
    Skip,
    /// on_fail 실행 + HITL(replan) 생성
    Replan,
}

impl std::fmt::Display for EscalationAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EscalationAction::Retry => write!(f, "retry"),
            EscalationAction::RetryWithComment => write!(f, "retry_with_comment"),
            EscalationAction::Hitl => write!(f, "hitl"),
            EscalationAction::Skip => write!(f, "skip"),
            EscalationAction::Replan => write!(f, "replan"),
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
// v5 — v5 daemon feature flag
// ═══════════════════════════════════════════════

/// v5 daemon 기능 플래그.
///
/// `enabled: true`이면 v5 daemon 루프가 시작된다.
/// `enabled: false`(기본)이면 기존 v4 daemon이 그대로 동작한다.
/// v4와 v5는 동일한 PID 파일을 공유하므로 동시 실행되지 않는다.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct V5Config {
    pub enabled: bool,
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

// ═══════════════════════════════════════════════
// lifecycle — yaml state lifecycle actions
// ═══════════════════════════════════════════════

/// yaml state의 on_enter/on_done/on_fail에서 사용하는 액션.
///
/// handler, on_enter, on_done, on_fail 전부 같은 두 가지 타입:
/// - `script`: bash 실행 (결정적, WORK_ID + WORKTREE 주입)
/// - `prompt`: AgentRuntime.invoke() (LLM, worktree 안에서)
///
/// YAML 형식: `- script: "..."` 또는 `- prompt: "..."`
/// serde(untagged)를 사용하여 `{ script: "..." }` map을 자연스럽게 파싱.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LifecycleAction {
    /// bash 스크립트 실행 (WORK_ID + WORKTREE 환경변수 주입)
    Script { script: String },
    /// LLM prompt 실행 (worktree 안에서)
    Prompt { prompt: String },
}

impl LifecycleAction {
    /// script 값을 반환 (Script variant일 때만).
    pub fn as_script(&self) -> Option<&str> {
        match self {
            LifecycleAction::Script { script } => Some(script),
            LifecycleAction::Prompt { .. } => None,
        }
    }

    /// prompt 값을 반환 (Prompt variant일 때만).
    pub fn as_prompt(&self) -> Option<&str> {
        match self {
            LifecycleAction::Prompt { prompt } => Some(prompt),
            LifecycleAction::Script { .. } => None,
        }
    }
}

/// 워크플로우 단계 공통 설정.
///
/// `command`가 지정되면 해당 슬래시 커맨드를 system prompt로 사용한다.
/// 미지정 시 task_type별 기본 출력 스펙이 적용된다.
///
/// lifecycle scripts:
/// - `on_enter`: Running 진입 후, handler 실행 전
/// - `on_done`: 성공적 완료 시 (evaluate가 Done 판정 후)
/// - `on_fail`: 실패 시 (escalation level에 따라 조건부)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct WorkflowStage {
    pub command: Option<String>,
    pub on_enter: Vec<LifecycleAction>,
    pub on_done: Vec<LifecycleAction>,
    pub on_fail: Vec<LifecycleAction>,
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
            on_enter: Vec::new(),
            on_done: Vec::new(),
            on_fail: Vec::new(),
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
    fn daemon_config_default_timeout_values() {
        let cfg = DaemonConfig::default();
        assert_eq!(cfg.task_timeout_secs, 1800);
        assert_eq!(cfg.shutdown_drain_timeout_secs, 30);
    }

    #[test]
    fn daemon_config_timeout_from_yaml() {
        let yaml = r#"
daemon:
  task_timeout_secs: 3600
  shutdown_drain_timeout_secs: 60
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.daemon.task_timeout_secs, 3600);
        assert_eq!(cfg.daemon.shutdown_drain_timeout_secs, 60);
    }

    #[test]
    fn daemon_config_timeout_defaults_when_omitted() {
        let yaml = r#"
daemon:
  log_level: "info"
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.daemon.task_timeout_secs, 1800);
        assert_eq!(cfg.daemon.shutdown_drain_timeout_secs, 30);
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
    fn workspace_concurrency_from_yaml() {
        let yaml = r#"
sources:
  github:
    concurrency: 3
    issue_concurrency: 2
    pr_concurrency: 1
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.sources.github.concurrency, 3);
        assert_eq!(cfg.sources.github.issue_concurrency, 2);
        assert_eq!(cfg.sources.github.pr_concurrency, 1);
    }

    #[test]
    fn workspace_concurrency_defaults_to_zero() {
        let yaml = r#"
sources:
  github:
    model: opus
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.sources.github.concurrency, 0);
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

    // ═══════════════════════════════════════════════
    // EscalationConfig 테스트
    // ═══════════════════════════════════════════════

    #[test]
    fn escalation_config_defaults() {
        let cfg = EscalationConfig::default();
        assert_eq!(cfg.levels.len(), 5);
        assert_eq!(cfg.action_for(1), EscalationAction::Retry);
        assert_eq!(cfg.action_for(2), EscalationAction::RetryWithComment);
        assert_eq!(cfg.action_for(3), EscalationAction::Hitl);
        assert_eq!(cfg.action_for(4), EscalationAction::Skip);
        assert_eq!(cfg.action_for(5), EscalationAction::Replan);
        assert!(cfg.on_fail.is_empty());
    }

    #[test]
    fn escalation_config_defaults_when_omitted() {
        let yaml = r#"
daemon:
  log_level: "info"
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.escalation.levels.len(), 5);
        assert_eq!(cfg.escalation.action_for(1), EscalationAction::Retry);
        assert!(cfg.escalation.on_fail.is_empty());
    }

    #[test]
    fn escalation_config_from_yaml() {
        let yaml = r#"
escalation:
  levels:
    1: retry
    2: retry_with_comment
    3: hitl
    4: skip
    5: replan
  on_fail:
    - "gh issue comment $ISSUE --body 'task failed'"
    - "echo 'notification sent'"
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.escalation.levels.len(), 5);
        assert_eq!(cfg.escalation.action_for(1), EscalationAction::Retry);
        assert_eq!(
            cfg.escalation.action_for(2),
            EscalationAction::RetryWithComment
        );
        assert_eq!(cfg.escalation.action_for(3), EscalationAction::Hitl);
        assert_eq!(cfg.escalation.action_for(4), EscalationAction::Skip);
        assert_eq!(cfg.escalation.action_for(5), EscalationAction::Replan);
        assert_eq!(cfg.escalation.on_fail.len(), 2);
    }

    #[test]
    fn escalation_config_custom_levels() {
        let yaml = r#"
escalation:
  levels:
    1: retry
    2: retry
    3: skip
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.escalation.levels.len(), 3);
        assert_eq!(cfg.escalation.action_for(1), EscalationAction::Retry);
        assert_eq!(cfg.escalation.action_for(2), EscalationAction::Retry);
        assert_eq!(cfg.escalation.action_for(3), EscalationAction::Skip);
        // Beyond max → last defined action
        assert_eq!(cfg.escalation.action_for(99), EscalationAction::Skip);
    }

    #[test]
    fn escalation_config_partial_override_preserves_defaults() {
        let yaml = r#"
escalation:
  on_fail:
    - "echo fail"
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        // levels should use defaults
        assert_eq!(cfg.escalation.levels.len(), 5);
        assert_eq!(cfg.escalation.on_fail.len(), 1);
        assert_eq!(cfg.escalation.on_fail[0], "echo fail");
    }

    #[test]
    fn escalation_should_run_on_fail() {
        let cfg = EscalationConfig::default();
        assert!(!cfg.should_run_on_fail(EscalationAction::Retry));
        assert!(cfg.should_run_on_fail(EscalationAction::RetryWithComment));
        assert!(cfg.should_run_on_fail(EscalationAction::Hitl));
        assert!(cfg.should_run_on_fail(EscalationAction::Skip));
        assert!(cfg.should_run_on_fail(EscalationAction::Replan));
    }

    #[test]
    fn escalation_action_display() {
        assert_eq!(EscalationAction::Retry.to_string(), "retry");
        assert_eq!(
            EscalationAction::RetryWithComment.to_string(),
            "retry_with_comment"
        );
        assert_eq!(EscalationAction::Hitl.to_string(), "hitl");
        assert_eq!(EscalationAction::Skip.to_string(), "skip");
        assert_eq!(EscalationAction::Replan.to_string(), "replan");
    }

    // ═══════════════════════════════════════════════
    // V5Config 테스트
    // ═══════════════════════════════════════════════

    #[test]
    fn v5_config_defaults() {
        let cfg = V5Config::default();
        assert!(!cfg.enabled);
    }

    #[test]
    fn v5_config_from_yaml() {
        let yaml = r#"
v5:
  enabled: true
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.v5.enabled);
    }

    #[test]
    fn v5_config_defaults_when_omitted() {
        let yaml = r#"
daemon:
  log_level: "info"
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!cfg.v5.enabled);
    }

    // ═══════════════════════════════════════════════
    // lifecycle — on_enter / on_done / on_fail 파싱
    // ═══════════════════════════════════════════════

    #[test]
    fn workflow_stage_lifecycle_defaults_empty() {
        let stage = WorkflowStage::default();
        assert!(stage.on_enter.is_empty());
        assert!(stage.on_done.is_empty());
        assert!(stage.on_fail.is_empty());
    }

    #[test]
    fn workflow_stage_lifecycle_from_yaml() {
        let yaml = r#"
workflows:
  implement:
    on_enter:
      - script: "echo entering"
    on_done:
      - script: "echo done"
      - prompt: "summarize changes"
    on_fail:
      - script: "echo failed"
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.workflows.implement.on_enter.len(), 1);
        assert_eq!(cfg.workflows.implement.on_done.len(), 2);
        assert_eq!(cfg.workflows.implement.on_fail.len(), 1);

        match &cfg.workflows.implement.on_enter[0] {
            LifecycleAction::Script { script: s } => assert_eq!(s, "echo entering"),
            LifecycleAction::Prompt { prompt: _ } => panic!("expected Script"),
        }
        match &cfg.workflows.implement.on_done[1] {
            LifecycleAction::Prompt { prompt: p } => assert_eq!(p, "summarize changes"),
            LifecycleAction::Script { script: _ } => panic!("expected Prompt"),
        }
    }

    #[test]
    fn workflow_stage_lifecycle_omitted_defaults_empty() {
        let yaml = r#"
workflows:
  implement:
    command: /custom-implement
"#;
        let cfg: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            cfg.workflows.implement.command.as_deref(),
            Some("/custom-implement")
        );
        assert!(cfg.workflows.implement.on_enter.is_empty());
        assert!(cfg.workflows.implement.on_done.is_empty());
        assert!(cfg.workflows.implement.on_fail.is_empty());
    }

    #[test]
    fn lifecycle_action_script_serde_roundtrip() {
        let action = LifecycleAction::Script {
            script: "echo hello".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let parsed: LifecycleAction = serde_json::from_str(&json).unwrap();
        match parsed {
            LifecycleAction::Script { script: s } => assert_eq!(s, "echo hello"),
            LifecycleAction::Prompt { prompt: _ } => panic!("expected Script"),
        }
    }

    #[test]
    fn lifecycle_action_prompt_serde_roundtrip() {
        let action = LifecycleAction::Prompt {
            prompt: "do something".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let parsed: LifecycleAction = serde_json::from_str(&json).unwrap();
        match parsed {
            LifecycleAction::Prompt { prompt: p } => assert_eq!(p, "do something"),
            LifecycleAction::Script { script: _ } => panic!("expected Prompt"),
        }
    }
}
