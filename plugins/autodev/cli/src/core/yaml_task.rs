//! YamlDrivenTask — bridges v5 yaml handler definitions to v4 Task trait.
//!
//! This compatibility layer allows the existing TaskRunner/TaskManager/Collector
//! infrastructure to execute yaml-defined handlers without modification.
//!
//! In v5, the Task lifecycle is driven by yaml state definitions:
//! - `on_enter` scripts run before handlers (replaces part of before_invoke)
//! - `handlers` (prompt/script) run during the task (replaces Agent invocation)
//! - `on_done` scripts run after success (replaces part of after_invoke)
//! - `on_fail` scripts run on failure (replaces error handling in after_invoke)
//!
//! YamlDrivenTask wraps a StateConfig + QueueItem and implements the v4 Task trait,
//! so the existing daemon loop can execute it without changes.

use std::path::PathBuf;

use async_trait::async_trait;

use super::handler::{Action, StateConfig};
use super::queue_item::QueueItem;
use super::task::{AgentRequest, AgentResponse, QueueOp, SkipReason, Task, TaskResult, TaskStatus};
use crate::infra::claude::SessionOptions;

/// A Task implementation driven by yaml state configuration.
///
/// Bridges the v5 yaml handler model to the v4 Task trait interface.
/// The first prompt handler becomes the AgentRequest prompt;
/// remaining handlers and lifecycle hooks are tracked for future execution.
pub struct YamlDrivenTask {
    /// The queue item being processed.
    item: QueueItem,
    /// The yaml state configuration for this task.
    state_config: StateConfig,
    /// The state name (e.g. "analyze", "implement").
    state_name: String,
    /// Working directory (worktree path).
    working_dir: PathBuf,
}

impl YamlDrivenTask {
    /// Create a new YamlDrivenTask.
    ///
    /// # Arguments
    /// * `item` - The queue item to process
    /// * `state_config` - The yaml state configuration
    /// * `state_name` - The state name (for logging/identification)
    /// * `working_dir` - The worktree path
    pub fn new(
        item: QueueItem,
        state_config: StateConfig,
        state_name: impl Into<String>,
        working_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            item,
            state_config,
            state_name: state_name.into(),
            working_dir: working_dir.into(),
        }
    }

    /// Extract the first prompt from handlers.
    /// Falls back to a generic prompt based on the state name.
    fn primary_prompt(&self) -> String {
        for action in &self.state_config.handlers {
            if let Action::Prompt { prompt } = action {
                return prompt.clone();
            }
        }
        // Fallback: generate a generic prompt from the state name
        format!("Execute the '{}' step for this work item.", self.state_name)
    }

    /// Build script commands from on_done actions for logging purposes.
    fn on_done_scripts(&self) -> Vec<String> {
        self.state_config
            .on_done
            .iter()
            .filter_map(|a| match a {
                Action::Script { script } => Some(script.clone()),
                Action::Prompt { .. } => None,
            })
            .collect()
    }
}

#[async_trait]
impl Task for YamlDrivenTask {
    fn work_id(&self) -> &str {
        &self.item.work_id
    }

    fn repo_name(&self) -> &str {
        &self.item.repo_name
    }

    /// Prepare the agent request from yaml handler definitions.
    ///
    /// The primary prompt handler becomes the AgentRequest.
    /// If no prompt handlers exist (script-only state), returns SkipReason
    /// since the current TaskRunner requires an Agent invocation.
    async fn before_invoke(&mut self) -> Result<AgentRequest, SkipReason> {
        if !self.state_config.has_prompt_handlers() {
            return Err(SkipReason::PreflightFailed(format!(
                "state '{}' has no prompt handlers — script-only states not yet supported via Task trait",
                self.state_name
            )));
        }

        Ok(AgentRequest {
            working_dir: self.working_dir.clone(),
            prompt: self.primary_prompt(),
            session_opts: SessionOptions {
                append_system_prompt: Some(format!(
                    "You are executing the '{}' step of an automated development pipeline.",
                    self.state_name
                )),
                ..Default::default()
            },
        })
    }

    /// Process the agent response.
    ///
    /// On success: creates QueueOp::Remove (the daemon will handle on_done scripts).
    /// On failure: creates a Failed TaskResult.
    async fn after_invoke(&mut self, response: AgentResponse) -> TaskResult {
        let status = if response.exit_code == 0 {
            TaskStatus::Completed
        } else {
            TaskStatus::Failed(format!(
                "state '{}' handler failed: {}",
                self.state_name,
                if response.stderr.is_empty() {
                    "non-zero exit code".to_string()
                } else {
                    response.stderr.clone()
                }
            ))
        };

        // Include on_done script paths in logs for traceability
        let on_done = self.on_done_scripts();
        let command = if on_done.is_empty() {
            format!("yaml:{}", self.state_name)
        } else {
            format!(
                "yaml:{} (on_done: {} scripts)",
                self.state_name,
                on_done.len()
            )
        };

        let now = chrono::Utc::now().to_rfc3339();
        let log = crate::core::models::NewConsumerLog {
            repo_id: self.item.repo_id.clone(),
            queue_type: self.item.queue_type.as_str().to_string(),
            queue_item_id: self.item.work_id.clone(),
            worker_id: format!("yaml-handler:{}", self.state_name),
            command,
            stdout: response.stdout,
            stderr: response.stderr,
            exit_code: response.exit_code,
            started_at: now.clone(),
            finished_at: now,
            duration_ms: response.duration.as_millis() as i64,
        };

        TaskResult {
            work_id: self.item.work_id.clone(),
            repo_name: self.item.repo_name.clone(),
            queue_ops: vec![QueueOp::Remove],
            logs: vec![log],
            status,
        }
    }
}

/// Build a YamlDrivenTask from a QueueItem and a SourceConfig.
///
/// Looks up the state configuration by name from the source config.
/// Returns None if the state is not defined in the source config.
pub fn build_yaml_task(
    item: QueueItem,
    state_name: &str,
    source_config: &super::handler::SourceConfig,
    working_dir: impl Into<PathBuf>,
) -> Option<YamlDrivenTask> {
    let state_config = source_config.state(state_name)?.clone();
    Some(YamlDrivenTask::new(
        item,
        state_config,
        state_name,
        working_dir,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::handler::{Action, SourceConfig, StateConfig, Trigger};
    use crate::core::phase::TaskKind;
    use crate::core::queue_item::testing::{test_issue, test_pr};
    use std::collections::HashMap;
    use std::time::Duration;

    fn make_analyze_state() -> StateConfig {
        StateConfig {
            trigger: Trigger {
                label: Some("autodev:analyze".into()),
                ..Default::default()
            },
            handlers: vec![Action::prompt(
                "Analyze the issue and determine feasibility",
            )],
            on_done: vec![Action::script(
                "gh issue edit $ISSUE --add-label autodev:implement",
            )],
            on_fail: vec![Action::script("echo 'Analysis failed'")],
            ..Default::default()
        }
    }

    fn make_script_only_state() -> StateConfig {
        StateConfig {
            trigger: Trigger {
                label: Some("autodev:lint".into()),
                ..Default::default()
            },
            handlers: vec![Action::script("cargo clippy")],
            ..Default::default()
        }
    }

    fn make_source_config() -> SourceConfig {
        let mut states = HashMap::new();
        states.insert("analyze".into(), make_analyze_state());
        states.insert("lint".into(), make_script_only_state());

        let mut escalation = HashMap::new();
        escalation.insert(1, crate::core::handler::EscalationAction::Retry);
        escalation.insert(3, crate::core::handler::EscalationAction::Hitl);

        SourceConfig {
            url: Some("https://github.com/org/repo".into()),
            states,
            escalation,
            ..Default::default()
        }
    }

    // ─── YamlDrivenTask creation ───

    #[test]
    fn yaml_task_work_id_and_repo_name() {
        let item = test_issue(42, TaskKind::Analyze);
        let task = YamlDrivenTask::new(
            item,
            make_analyze_state(),
            "analyze",
            PathBuf::from("/tmp/worktree"),
        );
        assert_eq!(task.work_id(), "github:org/repo#42:analyze");
        assert_eq!(task.repo_name(), "org/repo");
    }

    // ─── before_invoke ───

    #[tokio::test]
    async fn before_invoke_returns_prompt_from_handlers() {
        let item = test_issue(42, TaskKind::Analyze);
        let mut task = YamlDrivenTask::new(
            item,
            make_analyze_state(),
            "analyze",
            PathBuf::from("/tmp/worktree"),
        );

        let request = task.before_invoke().await.unwrap();
        assert!(request
            .prompt
            .contains("Analyze the issue and determine feasibility"));
        assert_eq!(request.working_dir, PathBuf::from("/tmp/worktree"));
    }

    #[tokio::test]
    async fn before_invoke_skips_script_only_state() {
        let item = test_issue(42, TaskKind::Analyze);
        let mut task = YamlDrivenTask::new(
            item,
            make_script_only_state(),
            "lint",
            PathBuf::from("/tmp/worktree"),
        );

        let result = task.before_invoke().await;
        assert!(result.is_err());
        match result.unwrap_err() {
            SkipReason::PreflightFailed(msg) => {
                assert!(msg.contains("no prompt handlers"));
            }
            other => panic!("expected PreflightFailed, got: {:?}", other),
        }
    }

    // ─── after_invoke ───

    #[tokio::test]
    async fn after_invoke_success() {
        let item = test_issue(42, TaskKind::Analyze);
        let mut task = YamlDrivenTask::new(
            item,
            make_analyze_state(),
            "analyze",
            PathBuf::from("/tmp/worktree"),
        );

        let response = AgentResponse {
            exit_code: 0,
            stdout: "analysis complete".into(),
            stderr: String::new(),
            duration: Duration::from_secs(5),
        };

        let result = task.after_invoke(response).await;
        assert!(matches!(result.status, TaskStatus::Completed));
        assert_eq!(result.work_id, "github:org/repo#42:analyze");
        assert_eq!(result.logs.len(), 1);
        assert!(result.logs[0].command.contains("yaml:analyze"));
        assert!(result.logs[0].command.contains("on_done: 1 scripts"));
    }

    #[tokio::test]
    async fn after_invoke_failure() {
        let item = test_issue(42, TaskKind::Analyze);
        let mut task = YamlDrivenTask::new(
            item,
            make_analyze_state(),
            "analyze",
            PathBuf::from("/tmp/worktree"),
        );

        let response = AgentResponse {
            exit_code: 1,
            stdout: String::new(),
            stderr: "timeout".into(),
            duration: Duration::from_secs(60),
        };

        let result = task.after_invoke(response).await;
        match &result.status {
            TaskStatus::Failed(msg) => {
                assert!(msg.contains("analyze"));
                assert!(msg.contains("timeout"));
            }
            other => panic!("expected Failed, got: {other}"),
        }
    }

    // ─── build_yaml_task ───

    #[test]
    fn build_yaml_task_existing_state() {
        let item = test_issue(42, TaskKind::Analyze);
        let config = make_source_config();
        let task = build_yaml_task(item, "analyze", &config, "/tmp/worktree");
        assert!(task.is_some());
    }

    #[test]
    fn build_yaml_task_missing_state() {
        let item = test_issue(42, TaskKind::Analyze);
        let config = make_source_config();
        let task = build_yaml_task(item, "nonexistent", &config, "/tmp/worktree");
        assert!(task.is_none());
    }

    // ─── PR item with yaml task ───

    #[tokio::test]
    async fn yaml_task_with_pr_item() {
        let item = test_pr(10, TaskKind::Review);
        let state = StateConfig {
            trigger: Trigger {
                label: Some("autodev:review".into()),
                ..Default::default()
            },
            handlers: vec![Action::prompt("Review this PR for code quality")],
            on_done: vec![Action::script(
                "gh issue edit $ISSUE --add-label autodev:done",
            )],
            ..Default::default()
        };

        let mut task =
            YamlDrivenTask::new(item, state, "review", PathBuf::from("/tmp/worktree-pr"));

        assert_eq!(task.work_id(), "github:org/repo#10:review");
        assert_eq!(task.repo_name(), "org/repo");

        let request = task.before_invoke().await.unwrap();
        assert!(request.prompt.contains("Review this PR"));
    }

    // ─── Multiple handlers — first prompt is used ───

    #[tokio::test]
    async fn yaml_task_uses_first_prompt_handler() {
        let item = test_issue(1, TaskKind::Analyze);
        let state = StateConfig {
            handlers: vec![
                Action::script("echo pre-check"),
                Action::prompt("First prompt"),
                Action::prompt("Second prompt"),
            ],
            ..Default::default()
        };

        let mut task = YamlDrivenTask::new(item, state, "test", PathBuf::from("/tmp"));
        let request = task.before_invoke().await.unwrap();
        assert_eq!(request.prompt, "First prompt");
    }

    // ─── State with on_enter ───

    #[test]
    fn yaml_task_state_with_on_enter_detected() {
        let state = StateConfig {
            on_enter: vec![Action::script("echo entering")],
            handlers: vec![Action::prompt("do work")],
            ..Default::default()
        };
        assert!(!state.on_enter.is_empty());
        assert!(state.has_prompt_handlers());
    }
}
