//! DefaultTaskRunner — TaskRunner trait의 기본 구현체.
//!
//! Task의 before_invoke → Agent.invoke → after_invoke 생명주기를 실행한다.

use std::sync::Arc;

use async_trait::async_trait;

use super::agent::Agent;
use super::task::{Task, TaskResult};
use super::task_runner::TaskRunner;

/// Task 생명주기 실행기.
///
/// 1. `task.before_invoke()` → `AgentRequest` 또는 `SkipReason`
/// 2. `SkipReason`이면 → `TaskResult::skipped()` 반환
/// 3. `AgentRequest`면 → `self.agent.invoke()` 호출
/// 4. `task.after_invoke(response)` → `TaskResult` 반환
pub struct DefaultTaskRunner {
    agent: Arc<dyn Agent>,
}

impl DefaultTaskRunner {
    pub fn new(agent: Arc<dyn Agent>) -> Self {
        Self { agent }
    }
}

#[async_trait]
impl TaskRunner for DefaultTaskRunner {
    async fn run(&self, mut task: Box<dyn Task>) -> TaskResult {
        let request = match task.before_invoke().await {
            Ok(req) => req,
            Err(skip) => {
                return TaskResult::skipped(
                    task.work_id().to_string(),
                    task.repo_name().to_string(),
                    skip,
                );
            }
        };

        let response = self.agent.invoke(request).await;
        task.after_invoke(response).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::task::{AgentRequest, AgentResponse, QueueOp, SkipReason, TaskStatus};
    use crate::infrastructure::claude::SessionOptions;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::Duration;

    // ─── Mock Agent ───

    struct MockAgent {
        responses: Mutex<Vec<AgentResponse>>,
        invoked: Mutex<bool>,
    }

    impl MockAgent {
        fn new(response: AgentResponse) -> Self {
            Self {
                responses: Mutex::new(vec![response]),
                invoked: Mutex::new(false),
            }
        }

        fn was_invoked(&self) -> bool {
            *self.invoked.lock().unwrap()
        }
    }

    #[async_trait]
    impl Agent for MockAgent {
        async fn invoke(&self, _request: AgentRequest) -> AgentResponse {
            *self.invoked.lock().unwrap() = true;
            self.responses.lock().unwrap().remove(0)
        }
    }

    // ─── Mock Task: succeeds ───

    struct SuccessTask;

    #[async_trait]
    impl Task for SuccessTask {
        fn work_id(&self) -> &str {
            "test:org/repo:1"
        }
        fn repo_name(&self) -> &str {
            "org/repo"
        }
        async fn before_invoke(&mut self) -> Result<AgentRequest, SkipReason> {
            Ok(AgentRequest {
                working_dir: PathBuf::from("/tmp"),
                prompt: "test".to_string(),
                session_opts: SessionOptions::default(),
            })
        }
        async fn after_invoke(&mut self, response: AgentResponse) -> TaskResult {
            TaskResult {
                work_id: "test:org/repo:1".to_string(),
                repo_name: "org/repo".to_string(),
                queue_ops: vec![QueueOp::Remove],
                logs: vec![],
                status: if response.exit_code == 0 {
                    TaskStatus::Completed
                } else {
                    TaskStatus::Failed("error".to_string())
                },
            }
        }
    }

    // ─── Mock Task: skips ───

    struct SkipTask;

    #[async_trait]
    impl Task for SkipTask {
        fn work_id(&self) -> &str {
            "test:org/repo:2"
        }
        fn repo_name(&self) -> &str {
            "org/repo"
        }
        async fn before_invoke(&mut self) -> Result<AgentRequest, SkipReason> {
            Err(SkipReason::PreflightFailed("issue closed".to_string()))
        }
        async fn after_invoke(&mut self, _response: AgentResponse) -> TaskResult {
            unreachable!("should not be called when skipped")
        }
    }

    #[tokio::test]
    async fn runner_calls_agent_and_after_invoke() {
        let agent = Arc::new(MockAgent::new(AgentResponse {
            exit_code: 0,
            stdout: "done".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(1),
        }));
        let runner = DefaultTaskRunner::new(agent.clone());

        let result = runner.run(Box::new(SuccessTask)).await;

        assert!(agent.was_invoked());
        assert!(matches!(result.status, TaskStatus::Completed));
        assert_eq!(result.work_id, "test:org/repo:1");
    }

    #[tokio::test]
    async fn runner_skips_without_calling_agent() {
        let agent = Arc::new(MockAgent::new(AgentResponse {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
            duration: Duration::ZERO,
        }));
        let runner = DefaultTaskRunner::new(agent.clone());

        let result = runner.run(Box::new(SkipTask)).await;

        assert!(!agent.was_invoked());
        assert!(matches!(result.status, TaskStatus::Skipped(_)));
        assert_eq!(result.work_id, "test:org/repo:2");
    }

    #[tokio::test]
    async fn runner_returns_failed_on_agent_error() {
        let agent = Arc::new(MockAgent::new(AgentResponse {
            exit_code: 1,
            stdout: String::new(),
            stderr: "timeout".to_string(),
            duration: Duration::from_secs(60),
        }));
        let runner = DefaultTaskRunner::new(agent);

        let result = runner.run(Box::new(SuccessTask)).await;

        assert!(matches!(result.status, TaskStatus::Failed(_)));
    }
}
