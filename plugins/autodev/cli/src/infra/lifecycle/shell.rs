//! ShellLifecycleRunner — LifecycleRunner trait의 shell 기반 구현체.
//!
//! `script` 액션: bash -c로 실행, WORK_ID + WORKTREE 환경변수 주입.
//! `prompt` 액션: 현재는 로그만 남기고 성공 반환 (v5 AgentRuntime 연동 시 확장).

use async_trait::async_trait;
use tokio::process::Command;

use crate::core::config::models::LifecycleAction;
use crate::core::lifecycle::{LifecycleContext, LifecycleResult, LifecycleRunner};

/// Shell 기반 lifecycle runner.
///
/// script 액션을 bash -c로 실행하고, WORK_ID + WORKTREE 환경변수를 주입한다.
#[derive(Default)]
pub struct ShellLifecycleRunner;

impl ShellLifecycleRunner {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl LifecycleRunner for ShellLifecycleRunner {
    async fn run_action(
        &self,
        action: &LifecycleAction,
        ctx: &LifecycleContext,
    ) -> LifecycleResult {
        match action {
            LifecycleAction::Script { script } => {
                let result = Command::new("bash")
                    .arg("-c")
                    .arg(script)
                    .env("WORK_ID", &ctx.work_id)
                    .env("WORKTREE", &ctx.worktree)
                    .envs(&ctx.extra_env)
                    .output()
                    .await;

                match result {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                        LifecycleResult {
                            success: output.status.success(),
                            output: stdout,
                            error: stderr,
                        }
                    }
                    Err(e) => LifecycleResult {
                        success: false,
                        output: String::new(),
                        error: format!("failed to execute script: {e}"),
                    },
                }
            }
            LifecycleAction::Prompt { prompt } => {
                // prompt 액션은 v5 AgentRuntime 연동 시 구현 예정.
                // 현재는 로그만 남기고 성공 반환.
                tracing::info!(
                    "lifecycle prompt action (not yet implemented): {}",
                    prompt.chars().take(80).collect::<String>()
                );
                LifecycleResult {
                    success: true,
                    output: String::new(),
                    error: String::new(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn shell_runner_executes_script_with_env_vars() {
        let runner = ShellLifecycleRunner::new();
        let ctx = LifecycleContext {
            work_id: "issue:org/repo:42".into(),
            worktree: "/tmp/test-wt".into(),
            extra_env: HashMap::new(),
        };

        let action = LifecycleAction::Script {
            script: "echo $WORK_ID".into(),
        };
        let result = runner.run_action(&action, &ctx).await;

        assert!(result.success);
        assert_eq!(result.output.trim(), "issue:org/repo:42");
    }

    #[tokio::test]
    async fn shell_runner_returns_failure_on_exit_code() {
        let runner = ShellLifecycleRunner::new();
        let ctx = LifecycleContext {
            work_id: "test".into(),
            worktree: "/tmp".into(),
            extra_env: HashMap::new(),
        };

        let action = LifecycleAction::Script {
            script: "exit 1".into(),
        };
        let result = runner.run_action(&action, &ctx).await;

        assert!(!result.success);
    }

    #[tokio::test]
    async fn shell_runner_injects_worktree_env() {
        let runner = ShellLifecycleRunner::new();
        let ctx = LifecycleContext {
            work_id: "test".into(),
            worktree: "/my/worktree/path".into(),
            extra_env: HashMap::new(),
        };

        let action = LifecycleAction::Script {
            script: "echo $WORKTREE".into(),
        };
        let result = runner.run_action(&action, &ctx).await;

        assert!(result.success);
        assert_eq!(result.output.trim(), "/my/worktree/path");
    }

    #[tokio::test]
    async fn shell_runner_injects_extra_env() {
        let runner = ShellLifecycleRunner::new();
        let mut extra = HashMap::new();
        extra.insert("CUSTOM_VAR".into(), "custom_value".into());
        let ctx = LifecycleContext {
            work_id: "test".into(),
            worktree: "/tmp".into(),
            extra_env: extra,
        };

        let action = LifecycleAction::Script {
            script: "echo $CUSTOM_VAR".into(),
        };
        let result = runner.run_action(&action, &ctx).await;

        assert!(result.success);
        assert_eq!(result.output.trim(), "custom_value");
    }

    #[tokio::test]
    async fn shell_runner_prompt_action_succeeds() {
        let runner = ShellLifecycleRunner::new();
        let ctx = LifecycleContext {
            work_id: "test".into(),
            worktree: "/tmp".into(),
            extra_env: HashMap::new(),
        };

        let action = LifecycleAction::Prompt {
            prompt: "do something".into(),
        };
        let result = runner.run_action(&action, &ctx).await;

        assert!(result.success);
    }
}
