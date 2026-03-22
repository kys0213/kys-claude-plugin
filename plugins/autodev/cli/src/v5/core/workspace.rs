use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::escalation::EscalationPolicy;

/// 워크스페이스 설정. `.autodev.yaml` 또는 `workspace.yaml`에서 로드.
///
/// 하나의 워크스페이스는 하나의 레포지토리(외부 시스템)에 대응한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub name: String,
    #[serde(default)]
    pub sources: HashMap<String, SourceConfig>,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub v5: V5FeatureConfig,
}

/// v5 feature flag.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct V5FeatureConfig {
    #[serde(default)]
    pub enabled: bool,
}

/// DataSource별 설정 (e.g. github).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    pub url: String,
    #[serde(default = "default_scan_interval")]
    pub scan_interval_secs: u64,
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
    #[serde(default)]
    pub states: HashMap<String, StateConfig>,
    #[serde(default)]
    pub escalation: EscalationPolicy,
}

fn default_scan_interval() -> u64 {
    300
}

fn default_concurrency() -> u32 {
    1
}

/// 워크플로우 상태 설정.
///
/// 각 state는 trigger 조건, handler 배열, on_done/on_fail/on_enter script를 정의한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateConfig {
    #[serde(default)]
    pub trigger: TriggerConfig,
    #[serde(default)]
    pub handlers: Vec<HandlerConfig>,
    #[serde(default)]
    pub on_enter: Vec<ScriptAction>,
    #[serde(default)]
    pub on_done: Vec<ScriptAction>,
    #[serde(default)]
    pub on_fail: Vec<ScriptAction>,
}

/// Trigger 조건. DataSource.collect()에서 이 조건을 검사한다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TriggerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Handler (prompt 또는 script).
///
/// handler는 순차 실행되며, 하나라도 실패하면 중단한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HandlerConfig {
    Prompt {
        prompt: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        runtime: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    Script {
        script: String,
    },
}

/// on_done/on_fail/on_enter에 사용되는 script action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptAction {
    pub script: String,
}

/// Runtime 설정.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default = "default_runtime_name")]
    pub default: String,
    #[serde(flatten)]
    pub runtimes: HashMap<String, RuntimeInstanceConfig>,
}

fn default_runtime_name() -> String {
    "claude".to_string()
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            default: default_runtime_name(),
            runtimes: HashMap::new(),
        }
    }
}

/// 개별 런타임 인스턴스 설정.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInstanceConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// 워크스페이스 참조 (경량 식별 정보).
#[derive(Debug, Clone)]
pub struct WorkspaceRef {
    pub id: String,
    pub name: String,
    pub url: String,
    pub concurrency: u32,
}

impl WorkspaceRef {
    pub fn from_config(id: &str, config: &WorkspaceConfig, source_name: &str) -> Option<Self> {
        let source = config.sources.get(source_name)?;
        Some(Self {
            id: id.to_string(),
            name: config.name.clone(),
            url: source.url.clone(),
            concurrency: source.concurrency,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORKSPACE_YAML: &str = r#"
name: auth-project
sources:
  github:
    url: https://github.com/org/repo
    scan_interval_secs: 300
    concurrency: 2
    states:
      analyze:
        trigger:
          label: "autodev:analyze"
        handlers:
          - prompt: "이슈를 분석하고 구현 가능 여부를 판단해줘"
        on_done:
          - script: |
              gh issue edit $ISSUE --remove-label "autodev:analyze"
      implement:
        trigger:
          label: "autodev:implement"
        handlers:
          - prompt: "이슈를 구현해줘"
            runtime: claude
            model: sonnet
          - script: "cargo test"
        on_done:
          - script: "gh pr create --title $TITLE"
        on_fail:
          - script: "gh issue comment $ISSUE --body 'failed'"
    escalation:
      1: retry
      2: retry_with_comment
      3: hitl
      4: skip
      5: replan
runtime:
  default: claude
"#;

    #[test]
    fn parse_full_workspace_yaml() {
        let config: WorkspaceConfig = serde_yaml::from_str(WORKSPACE_YAML).unwrap();
        assert_eq!(config.name, "auth-project");
        assert!(!config.v5.enabled);

        let github = config.sources.get("github").unwrap();
        assert_eq!(github.url, "https://github.com/org/repo");
        assert_eq!(github.scan_interval_secs, 300);
        assert_eq!(github.concurrency, 2);
    }

    #[test]
    fn parse_states() {
        let config: WorkspaceConfig = serde_yaml::from_str(WORKSPACE_YAML).unwrap();
        let github = config.sources.get("github").unwrap();

        let analyze = github.states.get("analyze").unwrap();
        assert_eq!(analyze.trigger.label.as_deref(), Some("autodev:analyze"));
        assert_eq!(analyze.handlers.len(), 1);
        assert_eq!(analyze.on_done.len(), 1);
        assert!(analyze.on_fail.is_empty());

        let implement = github.states.get("implement").unwrap();
        assert_eq!(implement.handlers.len(), 2);
        assert_eq!(implement.on_done.len(), 1);
        assert_eq!(implement.on_fail.len(), 1);
    }

    #[test]
    fn parse_handler_types() {
        let config: WorkspaceConfig = serde_yaml::from_str(WORKSPACE_YAML).unwrap();
        let github = config.sources.get("github").unwrap();
        let implement = github.states.get("implement").unwrap();

        match &implement.handlers[0] {
            HandlerConfig::Prompt {
                prompt,
                runtime,
                model,
            } => {
                assert!(prompt.contains("구현"));
                assert_eq!(runtime.as_deref(), Some("claude"));
                assert_eq!(model.as_deref(), Some("sonnet"));
            }
            _ => panic!("expected Prompt handler"),
        }

        match &implement.handlers[1] {
            HandlerConfig::Script { script } => {
                assert_eq!(script, "cargo test");
            }
            _ => panic!("expected Script handler"),
        }
    }

    #[test]
    fn parse_escalation() {
        let config: WorkspaceConfig = serde_yaml::from_str(WORKSPACE_YAML).unwrap();
        let github = config.sources.get("github").unwrap();

        use super::super::escalation::EscalationAction;
        assert_eq!(github.escalation.resolve(1), EscalationAction::Retry);
        assert_eq!(
            github.escalation.resolve(2),
            EscalationAction::RetryWithComment
        );
        assert_eq!(github.escalation.resolve(5), EscalationAction::Replan);
    }

    #[test]
    fn defaults() {
        let yaml = r#"
name: minimal
sources:
  github:
    url: https://github.com/org/repo
"#;
        let config: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
        let github = config.sources.get("github").unwrap();
        assert_eq!(github.scan_interval_secs, 300);
        assert_eq!(github.concurrency, 1);
        assert!(github.states.is_empty());
        assert!(github.escalation.is_empty());
        assert_eq!(config.runtime.default, "claude");
        assert!(!config.v5.enabled);
    }

    #[test]
    fn v5_enabled() {
        let yaml = r#"
name: test
v5:
  enabled: true
sources: {}
"#;
        let config: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.v5.enabled);
    }

    #[test]
    fn workspace_ref_from_config() {
        let config: WorkspaceConfig = serde_yaml::from_str(WORKSPACE_YAML).unwrap();
        let ws_ref = WorkspaceRef::from_config("ws-1", &config, "github").unwrap();
        assert_eq!(ws_ref.id, "ws-1");
        assert_eq!(ws_ref.name, "auth-project");
        assert_eq!(ws_ref.url, "https://github.com/org/repo");
        assert_eq!(ws_ref.concurrency, 2);
    }

    #[test]
    fn workspace_ref_nonexistent_source() {
        let config: WorkspaceConfig = serde_yaml::from_str(WORKSPACE_YAML).unwrap();
        assert!(WorkspaceRef::from_config("ws-1", &config, "gitlab").is_none());
    }
}
