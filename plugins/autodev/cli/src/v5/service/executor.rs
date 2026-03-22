use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{bail, Result};

use crate::v5::core::action::Action;
use crate::v5::core::runtime::{RuntimeRegistry, RuntimeRequest};

/// Action 실행 결과.
#[derive(Debug, Clone)]
pub struct ActionResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: std::time::Duration,
}

impl ActionResult {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Action 배열을 순차 실행하는 실행기.
///
/// Prompt → RuntimeRegistry에서 resolve한 AgentRuntime.invoke()
/// Script → bash (WORK_ID + WORKTREE 환경변수 주입)
///
/// 하나라도 실패하면 즉시 중단한다.
pub struct ActionExecutor {
    registry: Arc<RuntimeRegistry>,
}

impl ActionExecutor {
    pub fn new(registry: Arc<RuntimeRegistry>) -> Self {
        Self { registry }
    }

    /// Action 배열을 순차 실행한다.
    ///
    /// 각 Action 성공 시 다음으로 진행, 실패 시 즉시 중단.
    /// 반환: 마지막으로 실행된 ActionResult (또는 빈 배열이면 Ok(None)).
    pub async fn execute_all(
        &self,
        actions: &[Action],
        env: &ActionEnv,
    ) -> Result<Option<ActionResult>> {
        let mut last_result = None;
        for action in actions {
            let result = self.execute_one(action, env).await?;
            if !result.success() {
                return Ok(Some(result));
            }
            last_result = Some(result);
        }
        Ok(last_result)
    }

    /// 단일 Action을 실행한다.
    pub async fn execute_one(&self, action: &Action, env: &ActionEnv) -> Result<ActionResult> {
        match action {
            Action::Prompt {
                text,
                runtime,
                model,
            } => {
                self.execute_prompt(text, runtime.as_deref(), model.clone(), env)
                    .await
            }
            Action::Script { command } => self.execute_script(command, env).await,
        }
    }

    async fn execute_prompt(
        &self,
        text: &str,
        runtime_name: Option<&str>,
        model: Option<String>,
        env: &ActionEnv,
    ) -> Result<ActionResult> {
        let name = runtime_name.unwrap_or(self.registry.default_name());
        let runtime = self
            .registry
            .resolve(name)
            .ok_or_else(|| anyhow::anyhow!("runtime not found: {name}"))?;

        let request = RuntimeRequest {
            working_dir: env.worktree.clone(),
            prompt: text.to_string(),
            model,
            system_prompt: None,
            session_id: None,
        };

        let response = runtime.invoke(request).await;
        Ok(ActionResult {
            exit_code: response.exit_code,
            stdout: response.stdout,
            stderr: response.stderr,
            duration: response.duration,
        })
    }

    async fn execute_script(&self, command: &str, env: &ActionEnv) -> Result<ActionResult> {
        let start = Instant::now();
        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg("-c").arg(command);
        cmd.current_dir(&env.worktree);

        // 환경변수 주입
        cmd.env("WORK_ID", &env.work_id);
        cmd.env("WORKTREE", env.worktree.to_string_lossy().as_ref());
        for (k, v) in &env.extra_vars {
            cmd.env(k, v);
        }

        let output = cmd.output().await;
        let duration = start.elapsed();

        match output {
            Ok(output) => Ok(ActionResult {
                exit_code: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                duration,
            }),
            Err(e) => bail!("script execution failed: {e}"),
        }
    }
}

/// Action 실행 시 주입되는 환경.
#[derive(Debug, Clone)]
pub struct ActionEnv {
    pub work_id: String,
    pub worktree: PathBuf,
    pub extra_vars: HashMap<String, String>,
}

impl ActionEnv {
    pub fn new(work_id: &str, worktree: &Path) -> Self {
        Self {
            work_id: work_id.to_string(),
            worktree: worktree.to_path_buf(),
            extra_vars: HashMap::new(),
        }
    }

    pub fn with_var(mut self, key: &str, value: &str) -> Self {
        self.extra_vars.insert(key.to_string(), value.to_string());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v5::infra::runtimes::mock::MockRuntime;

    fn setup_registry() -> Arc<RuntimeRegistry> {
        let mut registry = RuntimeRegistry::new("mock".to_string());
        registry.register(Arc::new(MockRuntime::new("mock", vec![0])));
        Arc::new(registry)
    }

    fn test_env() -> ActionEnv {
        ActionEnv::new("test-work-id", Path::new("/tmp"))
    }

    #[tokio::test]
    async fn execute_prompt_success() {
        let executor = ActionExecutor::new(setup_registry());
        let action = Action::prompt("analyze this");
        let result = executor.execute_one(&action, &test_env()).await.unwrap();
        assert!(result.success());
    }

    #[tokio::test]
    async fn execute_prompt_with_runtime() {
        let mut registry = RuntimeRegistry::new("mock".to_string());
        registry.register(Arc::new(MockRuntime::new("mock", vec![0])));
        registry.register(Arc::new(MockRuntime::new("other", vec![42])));
        let executor = ActionExecutor::new(Arc::new(registry));

        let action = Action::prompt_with_runtime("test", "other", None);
        let result = executor.execute_one(&action, &test_env()).await.unwrap();
        assert_eq!(result.exit_code, 42);
    }

    #[tokio::test]
    async fn execute_script_with_env_vars() {
        let executor = ActionExecutor::new(setup_registry());
        let action = Action::script("echo $WORK_ID");
        let env = ActionEnv::new("my-work-id", Path::new("/tmp"));
        let result = executor.execute_one(&action, &env).await.unwrap();
        assert!(result.success());
        assert!(result.stdout.contains("my-work-id"));
    }

    #[tokio::test]
    async fn execute_script_failure() {
        let executor = ActionExecutor::new(setup_registry());
        let action = Action::script("exit 1");
        let result = executor.execute_one(&action, &test_env()).await.unwrap();
        assert!(!result.success());
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn execute_all_stops_on_failure() {
        let mut registry = RuntimeRegistry::new("mock".to_string());
        // 첫 번째 성공, 두 번째 실패
        registry.register(Arc::new(MockRuntime::new("mock", vec![0, 1])));
        let executor = ActionExecutor::new(Arc::new(registry));

        let actions = vec![Action::prompt("first"), Action::prompt("second")];
        let result = executor.execute_all(&actions, &test_env()).await.unwrap();
        // 두 번째에서 실패로 중단
        let r = result.unwrap();
        assert!(!r.success());
    }

    #[tokio::test]
    async fn execute_all_empty() {
        let executor = ActionExecutor::new(setup_registry());
        let result = executor.execute_all(&[], &test_env()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn execute_all_all_success() {
        let mut registry = RuntimeRegistry::new("mock".to_string());
        registry.register(Arc::new(MockRuntime::new("mock", vec![0, 0, 0])));
        let executor = ActionExecutor::new(Arc::new(registry));

        let actions = vec![
            Action::prompt("a"),
            Action::prompt("b"),
            Action::prompt("c"),
        ];
        let result = executor.execute_all(&actions, &test_env()).await.unwrap();
        assert!(result.unwrap().success());
    }

    #[tokio::test]
    async fn execute_script_extra_vars() {
        let executor = ActionExecutor::new(setup_registry());
        let action = Action::script("echo $MY_VAR");
        let env = ActionEnv::new("wid", Path::new("/tmp")).with_var("MY_VAR", "hello");
        let result = executor.execute_one(&action, &env).await.unwrap();
        assert!(result.stdout.contains("hello"));
    }

    #[tokio::test]
    async fn execute_prompt_unknown_runtime() {
        let registry = RuntimeRegistry::new("nonexistent".to_string());
        let executor = ActionExecutor::new(Arc::new(registry));
        let action = Action::prompt_with_runtime("test", "unknown", None);
        let result = executor.execute_one(&action, &test_env()).await;
        assert!(result.is_err());
    }
}
