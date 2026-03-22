//! v5 yaml-driven handler models.
//!
//! Replaces v4 Task trait's hardcoded lifecycle with declarative yaml definitions.
//! Each state in a workspace yaml defines handlers (prompt/script), lifecycle hooks
//! (on_enter, on_done, on_fail), trigger conditions, and escalation policies.
//!
//! See: `plugins/autodev/spec/draft/concerns/datasource.md`

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ─── Action — unified prompt/script type ───

/// A single executable action: either an LLM prompt or a shell script.
///
/// Used for handlers, on_enter, on_done, on_fail uniformly.
/// v5 spec: "handler는 prompt 또는 script 두 가지 타입. 동일한 통합 액션 타입을 사용."
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Action {
    /// LLM prompt — executed via AgentRuntime inside a worktree.
    Prompt {
        /// The prompt text to send to the LLM.
        prompt: String,
    },
    /// Shell script — executed via bash with WORK_ID + WORKTREE env vars.
    Script {
        /// The script content or path to execute.
        script: String,
    },
}

impl Action {
    /// Create a new prompt action.
    pub fn prompt(text: impl Into<String>) -> Self {
        Action::Prompt {
            prompt: text.into(),
        }
    }

    /// Create a new script action.
    pub fn script(text: impl Into<String>) -> Self {
        Action::Script {
            script: text.into(),
        }
    }

    /// Whether this action is a prompt (LLM invocation).
    pub fn is_prompt(&self) -> bool {
        matches!(self, Action::Prompt { .. })
    }

    /// Whether this action is a script (shell execution).
    pub fn is_script(&self) -> bool {
        matches!(self, Action::Script { .. })
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::Prompt { prompt } => write!(f, "prompt: {}", truncate(prompt, 50)),
            Action::Script { script } => write!(f, "script: {}", truncate(script, 50)),
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

// ─── Trigger — state entry condition ───

/// Trigger condition for a state. Defines when DataSource.collect() should
/// pick up items for this state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Trigger {
    /// GitHub label that triggers this state (e.g. "autodev:analyze").
    pub label: Option<String>,
    /// Jira status that triggers this state (v6+).
    pub status: Option<String>,
    /// Slack reaction that triggers this state (v6+).
    pub reaction: Option<String>,
}

// ─── EscalationAction — failure response level ───

/// What to do when a handler fails, based on failure count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationAction {
    /// Silently retry (worktree preserved, on_fail NOT executed).
    Retry,
    /// Execute on_fail + retry.
    RetryWithComment,
    /// Execute on_fail + create HITL event.
    Hitl,
    /// Execute on_fail + mark as Skipped.
    Skip,
    /// Execute on_fail + create HITL(replan) event.
    Replan,
}

impl fmt::Display for EscalationAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EscalationAction::Retry => write!(f, "retry"),
            EscalationAction::RetryWithComment => write!(f, "retry_with_comment"),
            EscalationAction::Hitl => write!(f, "hitl"),
            EscalationAction::Skip => write!(f, "skip"),
            EscalationAction::Replan => write!(f, "replan"),
        }
    }
}

impl EscalationAction {
    /// Whether on_fail script should be executed for this escalation level.
    /// Only `Retry` skips on_fail (silent retry).
    pub fn should_run_on_fail(&self) -> bool {
        !matches!(self, EscalationAction::Retry)
    }
}

// ─── StateConfig — single state definition ───

/// Configuration for a single pipeline state (e.g. "analyze", "implement", "review").
///
/// Defines trigger condition, handler actions, lifecycle hooks, and is parsed
/// from the `states` section of workspace yaml.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct StateConfig {
    /// Condition that triggers this state (label, status, etc.).
    pub trigger: Trigger,
    /// Actions to run when entering the Running phase (before handlers).
    #[serde(default)]
    pub on_enter: Vec<Action>,
    /// Actions to execute in sequence during the Running phase.
    #[serde(default)]
    pub handlers: Vec<Action>,
    /// Actions to run after successful completion (evaluate → Done).
    #[serde(default)]
    pub on_done: Vec<Action>,
    /// Actions to run on handler failure (before escalation).
    /// Not executed for `retry` escalation level.
    #[serde(default)]
    pub on_fail: Vec<Action>,
}

impl StateConfig {
    /// Whether this state has any prompt handlers (requires AgentRuntime).
    pub fn has_prompt_handlers(&self) -> bool {
        self.handlers.iter().any(|a| a.is_prompt())
    }

    /// Whether this state has any script handlers.
    pub fn has_script_handlers(&self) -> bool {
        self.handlers.iter().any(|a| a.is_script())
    }
}

// ─── SourceConfig — per-datasource workflow definition ───

/// Configuration for a single DataSource (e.g. GitHub).
///
/// Contains the URL, scan settings, state definitions, and escalation policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SourceConfig {
    /// Repository/project URL.
    pub url: Option<String>,
    /// Scan interval in seconds.
    pub scan_interval_secs: Option<u64>,
    /// Max concurrent items for this source.
    pub concurrency: Option<u32>,
    /// Pipeline state definitions (ordered map: state_name → config).
    #[serde(default)]
    pub states: HashMap<String, StateConfig>,
    /// Failure escalation policy (failure_count → action).
    #[serde(default)]
    pub escalation: HashMap<u32, EscalationAction>,
}

impl SourceConfig {
    /// Get escalation action for a given failure count.
    /// Returns the action for the highest matching level, or Retry as default.
    pub fn escalation_for(&self, failure_count: u32) -> &EscalationAction {
        // Find the exact match or the highest level that doesn't exceed failure_count
        self.escalation
            .get(&failure_count)
            .or_else(|| {
                self.escalation
                    .keys()
                    .filter(|&&k| k <= failure_count)
                    .max()
                    .and_then(|k| self.escalation.get(k))
            })
            .unwrap_or(&EscalationAction::Retry)
    }

    /// Get state config by name.
    pub fn state(&self, name: &str) -> Option<&StateConfig> {
        self.states.get(name)
    }

    /// List all state names.
    pub fn state_names(&self) -> Vec<&str> {
        self.states.keys().map(|s| s.as_str()).collect()
    }
}

// ─── WorkspaceSources — top-level sources section ───

/// Top-level `sources` section of workspace yaml.
///
/// Maps DataSource names to their configurations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct WorkspaceSources {
    /// GitHub DataSource configuration.
    pub github: Option<SourceConfig>,
}

// ─── HandlerResult — execution outcome ───

/// Result of executing a single action.
#[derive(Debug, Clone)]
pub struct HandlerResult {
    /// Exit code (0 = success).
    pub exit_code: i32,
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
    /// Working directory where the action was executed.
    pub working_dir: PathBuf,
}

impl HandlerResult {
    /// Whether the action succeeded (exit_code == 0).
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }
}

// ─── HandlerExecutor — action execution trait ───

/// Executor for handler actions. Abstracts LLM and script execution.
///
/// Replaces v4's Task.before_invoke/after_invoke lifecycle:
/// - Prompt actions → delegate to AgentRuntime (LLM)
/// - Script actions → delegate to shell executor (bash)
///
/// Both receive WORK_ID and WORKTREE as environment variables.
#[async_trait::async_trait]
pub trait HandlerExecutor: Send + Sync {
    /// Execute a single action in the given working directory.
    ///
    /// Environment variables `WORK_ID` and `WORKTREE` are injected automatically.
    async fn execute(
        &self,
        action: &Action,
        work_id: &str,
        worktree: &std::path::Path,
    ) -> HandlerResult;
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Action tests ───

    #[test]
    fn action_prompt_creation_and_display() {
        let action = Action::prompt("Analyze this issue");
        assert!(action.is_prompt());
        assert!(!action.is_script());
        assert!(action.to_string().contains("prompt:"));
    }

    #[test]
    fn action_script_creation_and_display() {
        let action = Action::script("echo hello");
        assert!(action.is_script());
        assert!(!action.is_prompt());
        assert!(action.to_string().contains("script:"));
    }

    #[test]
    fn action_display_truncates_long_text() {
        let long_text = "a".repeat(100);
        let action = Action::prompt(long_text);
        let display = action.to_string();
        assert!(display.contains("..."));
        assert!(display.len() < 70);
    }

    // ─── Trigger tests ───

    #[test]
    fn trigger_default_is_empty() {
        let trigger = Trigger::default();
        assert!(trigger.label.is_none());
        assert!(trigger.status.is_none());
        assert!(trigger.reaction.is_none());
    }

    // ─── EscalationAction tests ───

    #[test]
    fn escalation_retry_does_not_run_on_fail() {
        assert!(!EscalationAction::Retry.should_run_on_fail());
    }

    #[test]
    fn escalation_non_retry_runs_on_fail() {
        assert!(EscalationAction::RetryWithComment.should_run_on_fail());
        assert!(EscalationAction::Hitl.should_run_on_fail());
        assert!(EscalationAction::Skip.should_run_on_fail());
        assert!(EscalationAction::Replan.should_run_on_fail());
    }

    #[test]
    fn escalation_display() {
        assert_eq!(EscalationAction::Retry.to_string(), "retry");
        assert_eq!(
            EscalationAction::RetryWithComment.to_string(),
            "retry_with_comment"
        );
        assert_eq!(EscalationAction::Hitl.to_string(), "hitl");
        assert_eq!(EscalationAction::Skip.to_string(), "skip");
        assert_eq!(EscalationAction::Replan.to_string(), "replan");
    }

    // ─── StateConfig tests ───

    #[test]
    fn state_config_detects_prompt_and_script() {
        let state = StateConfig {
            handlers: vec![Action::prompt("analyze"), Action::script("lint.sh")],
            ..Default::default()
        };
        assert!(state.has_prompt_handlers());
        assert!(state.has_script_handlers());
    }

    #[test]
    fn state_config_no_handlers() {
        let state = StateConfig::default();
        assert!(!state.has_prompt_handlers());
        assert!(!state.has_script_handlers());
    }

    // ─── SourceConfig escalation tests ───

    #[test]
    fn source_config_escalation_exact_match() {
        let mut escalation = HashMap::new();
        escalation.insert(1, EscalationAction::Retry);
        escalation.insert(2, EscalationAction::RetryWithComment);
        escalation.insert(3, EscalationAction::Hitl);

        let config = SourceConfig {
            escalation,
            ..Default::default()
        };

        assert_eq!(config.escalation_for(1), &EscalationAction::Retry);
        assert_eq!(
            config.escalation_for(2),
            &EscalationAction::RetryWithComment
        );
        assert_eq!(config.escalation_for(3), &EscalationAction::Hitl);
    }

    #[test]
    fn source_config_escalation_fallback_to_highest() {
        let mut escalation = HashMap::new();
        escalation.insert(1, EscalationAction::Retry);
        escalation.insert(3, EscalationAction::Hitl);
        escalation.insert(5, EscalationAction::Replan);

        let config = SourceConfig {
            escalation,
            ..Default::default()
        };

        // failure_count=2 → falls back to level 1 (Retry)
        assert_eq!(config.escalation_for(2), &EscalationAction::Retry);
        // failure_count=4 → falls back to level 3 (Hitl)
        assert_eq!(config.escalation_for(4), &EscalationAction::Hitl);
        // failure_count=10 → falls back to level 5 (Replan)
        assert_eq!(config.escalation_for(10), &EscalationAction::Replan);
    }

    #[test]
    fn source_config_escalation_default_retry() {
        let config = SourceConfig::default();
        assert_eq!(config.escalation_for(1), &EscalationAction::Retry);
    }

    // ─── YAML parsing tests ───

    #[test]
    fn parse_action_prompt_from_yaml() {
        let yaml = r#"prompt: "Analyze this issue""#;
        let action: Action = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(action, Action::prompt("Analyze this issue"));
    }

    #[test]
    fn parse_action_script_from_yaml() {
        let yaml = r#"script: "echo hello""#;
        let action: Action = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(action, Action::script("echo hello"));
    }

    #[test]
    fn parse_state_config_from_yaml() {
        let yaml = r#"
trigger:
  label: "autodev:analyze"
handlers:
  - prompt: "Analyze the issue"
on_done:
  - script: |
      gh issue edit $ISSUE --add-label "autodev:implement"
on_fail:
  - script: "echo failed"
"#;
        let state: StateConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(state.trigger.label.as_deref(), Some("autodev:analyze"));
        assert_eq!(state.handlers.len(), 1);
        assert!(state.handlers[0].is_prompt());
        assert_eq!(state.on_done.len(), 1);
        assert!(state.on_done[0].is_script());
        assert_eq!(state.on_fail.len(), 1);
    }

    #[test]
    fn parse_full_source_config_from_yaml() {
        let yaml = r#"
url: https://github.com/org/repo
scan_interval_secs: 300
concurrency: 1
states:
  analyze:
    trigger:
      label: "autodev:analyze"
    handlers:
      - prompt: "Analyze the issue"
    on_done:
      - script: "gh issue edit $ISSUE --add-label implement"
  implement:
    trigger:
      label: "autodev:implement"
    handlers:
      - prompt: "Implement the issue"
    on_done:
      - script: "gh pr create"
escalation:
  1: retry
  2: retry_with_comment
  3: hitl
  4: skip
  5: replan
"#;
        let config: SourceConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.url.as_deref(), Some("https://github.com/org/repo"));
        assert_eq!(config.scan_interval_secs, Some(300));
        assert_eq!(config.concurrency, Some(1));
        assert_eq!(config.states.len(), 2);
        assert!(config.state("analyze").is_some());
        assert!(config.state("implement").is_some());
        assert!(config.state("review").is_none());

        assert_eq!(config.escalation.len(), 5);
        assert_eq!(config.escalation_for(1), &EscalationAction::Retry);
        assert_eq!(config.escalation_for(5), &EscalationAction::Replan);
    }

    #[test]
    fn parse_workspace_sources_from_yaml() {
        let yaml = r#"
github:
  url: https://github.com/org/repo
  states:
    analyze:
      trigger:
        label: "autodev:analyze"
      handlers:
        - prompt: "Analyze"
"#;
        let sources: WorkspaceSources = serde_yaml::from_str(yaml).unwrap();
        assert!(sources.github.is_some());
        let github = sources.github.unwrap();
        assert!(github.state("analyze").is_some());
    }

    #[test]
    fn parse_state_with_on_enter() {
        let yaml = r#"
trigger:
  label: "autodev:implement"
on_enter:
  - script: "echo starting"
handlers:
  - prompt: "Implement"
on_done:
  - script: "echo done"
"#;
        let state: StateConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(state.on_enter.len(), 1);
        assert!(state.on_enter[0].is_script());
    }

    #[test]
    fn parse_multiline_script_action() {
        let yaml = r#"
script: |
  CTX=$(autodev context $WORK_ID --json)
  ISSUE=$(echo $CTX | jq -r '.issue.number')
  gh issue edit $ISSUE --add-label "autodev:implement"
"#;
        let action: Action = serde_yaml::from_str(yaml).unwrap();
        match &action {
            Action::Script { script } => {
                assert!(script.contains("autodev context"));
                assert!(script.contains("jq"));
                assert!(script.contains("gh issue edit"));
            }
            _ => panic!("expected Script action"),
        }
    }

    // ─── HandlerResult tests ───

    #[test]
    fn handler_result_success() {
        let result = HandlerResult {
            exit_code: 0,
            stdout: "ok".into(),
            stderr: String::new(),
            working_dir: PathBuf::from("/tmp"),
        };
        assert!(result.is_success());
    }

    #[test]
    fn handler_result_failure() {
        let result = HandlerResult {
            exit_code: 1,
            stdout: String::new(),
            stderr: "error".into(),
            working_dir: PathBuf::from("/tmp"),
        };
        assert!(!result.is_success());
    }

    // ─── State name listing ───

    #[test]
    fn source_config_state_names() {
        let mut states = HashMap::new();
        states.insert("analyze".into(), StateConfig::default());
        states.insert("implement".into(), StateConfig::default());

        let config = SourceConfig {
            states,
            ..Default::default()
        };

        let mut names = config.state_names();
        names.sort();
        assert_eq!(names, vec!["analyze", "implement"]);
    }
}
