//! DefaultTaskRunner — TaskRunner trait의 기본 구현체.
//!
//! Task의 lifecycle을 실행한다:
//! on_enter → before_invoke → Agent.invoke → after_invoke → on_done/on_fail

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use super::agent::Agent;
use super::task_runner::TaskRunner;
use crate::core::lifecycle::{LifecycleContext, LifecyclePhase, LifecycleRunner};
use crate::core::task::{Task, TaskResult, TaskStatus};

/// Task 생명주기 실행기.
///
/// 1. `task.lifecycle_config()` → lifecycle 설정 로드
/// 2. `task.before_invoke()` → `AgentRequest` 또는 `SkipReason`
/// 3. `SkipReason`이면 → `TaskResult::skipped()` 반환
/// 4. `on_enter` lifecycle 실행
/// 5. `AgentRequest`면 → `self.agent.invoke()` 호출
/// 6. `task.after_invoke(response)` → `TaskResult`
/// 7. 결과에 따라 `on_done` 또는 `on_fail` lifecycle 실행
pub struct DefaultTaskRunner {
    agent: Arc<dyn Agent>,
    lifecycle_runner: Arc<dyn LifecycleRunner>,
}

impl DefaultTaskRunner {
    pub fn new(agent: Arc<dyn Agent>) -> Self {
        Self {
            agent,
            lifecycle_runner: Arc::new(crate::core::lifecycle::NoopLifecycleRunner),
        }
    }

    pub fn with_lifecycle_runner(mut self, runner: Arc<dyn LifecycleRunner>) -> Self {
        self.lifecycle_runner = runner;
        self
    }
}

#[async_trait]
impl TaskRunner for DefaultTaskRunner {
    async fn run(&self, mut task: Box<dyn Task>) -> TaskResult {
        let work_id = task.work_id().to_string();
        let repo_name = task.repo_name().to_string();
        let lifecycle = task.lifecycle_config();

        let request = match task.before_invoke().await {
            Ok(req) => req,
            Err(skip) => {
                return TaskResult::skipped(work_id, repo_name, skip);
            }
        };

        // Build lifecycle context from the AgentRequest's working_dir
        let ctx = LifecycleContext {
            work_id: work_id.clone(),
            worktree: request.working_dir.display().to_string(),
            extra_env: HashMap::new(),
        };

        // on_enter: Running 진입 후, handler 실행 전
        if !lifecycle.on_enter.is_empty() {
            if let Err(err) = self
                .lifecycle_runner
                .run_actions(&lifecycle.on_enter, LifecyclePhase::OnEnter, &ctx)
                .await
            {
                tracing::error!("on_enter failed for {work_id}: {err}");
                return TaskResult {
                    work_id,
                    repo_name,
                    queue_ops: vec![crate::core::task::QueueOp::Remove],
                    logs: vec![],
                    status: TaskStatus::Failed(format!("on_enter failed: {err}")),
                };
            }
        }

        // Handler 실행 (Agent 호출)
        let response = self.agent.invoke(request).await;
        let result = task.after_invoke(response).await;

        // on_done / on_fail: 결과에 따라 조건부 실행
        match &result.status {
            TaskStatus::Completed => {
                if !lifecycle.on_done.is_empty() {
                    if let Err(err) = self
                        .lifecycle_runner
                        .run_actions(&lifecycle.on_done, LifecyclePhase::OnDone, &ctx)
                        .await
                    {
                        // on_done script 실패 → Failed 상태로 전이
                        tracing::error!("on_done failed for {work_id}: {err}");
                        return TaskResult {
                            work_id: result.work_id,
                            repo_name: result.repo_name,
                            queue_ops: result.queue_ops,
                            logs: result.logs,
                            status: TaskStatus::Failed(format!("on_done failed: {err}")),
                        };
                    }
                }
            }
            TaskStatus::Failed(_) => {
                // on_fail은 escalation에서 조건부로 실행됨.
                // retry level에서는 on_fail을 실행하지 않음.
                // 여기서는 항상 실행하고, escalation 레벨 필터링은
                // daemon의 escalation 로직에서 처리.
                if !lifecycle.on_fail.is_empty() {
                    if let Err(err) = self
                        .lifecycle_runner
                        .run_actions(&lifecycle.on_fail, LifecyclePhase::OnFail, &ctx)
                        .await
                    {
                        tracing::warn!("on_fail script also failed for {work_id}: {err}");
                    }
                }
            }
            TaskStatus::Skipped(_) => {
                // Skipped 상태에서는 lifecycle 실행하지 않음
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::models::LifecycleAction;
    use crate::core::lifecycle::testing::MockLifecycleRunner;
    use crate::core::task::{
        AgentRequest, AgentResponse, LifecycleConfig, QueueOp, SkipReason, TaskStatus,
    };
    use crate::infra::claude::SessionOptions;
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

    // ─── Mock Task: succeeds with lifecycle ───

    struct SuccessTaskWithLifecycle;

    #[async_trait]
    impl Task for SuccessTaskWithLifecycle {
        fn work_id(&self) -> &str {
            "test:org/repo:1"
        }
        fn repo_name(&self) -> &str {
            "org/repo"
        }
        fn lifecycle_config(&self) -> LifecycleConfig {
            LifecycleConfig {
                on_enter: vec![LifecycleAction::Script {
                    script: "echo enter".into(),
                }],
                on_done: vec![LifecycleAction::Script {
                    script: "echo done".into(),
                }],
                on_fail: vec![LifecycleAction::Script {
                    script: "echo fail".into(),
                }],
            }
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

    fn success_agent() -> Arc<MockAgent> {
        Arc::new(MockAgent::new(AgentResponse {
            exit_code: 0,
            stdout: "done".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(1),
        }))
    }

    fn failure_agent() -> Arc<MockAgent> {
        Arc::new(MockAgent::new(AgentResponse {
            exit_code: 1,
            stdout: String::new(),
            stderr: "timeout".to_string(),
            duration: Duration::from_secs(60),
        }))
    }

    #[tokio::test]
    async fn runner_calls_agent_and_after_invoke() {
        let agent = success_agent();
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
        let agent = failure_agent();
        let runner = DefaultTaskRunner::new(agent);

        let result = runner.run(Box::new(SuccessTask)).await;

        assert!(matches!(result.status, TaskStatus::Failed(_)));
    }

    // ─── Lifecycle Tests ───

    #[tokio::test]
    async fn runner_executes_on_enter_and_on_done_on_success() {
        let agent = success_agent();
        let lifecycle = Arc::new(MockLifecycleRunner::new());
        let runner = DefaultTaskRunner::new(agent)
            .with_lifecycle_runner(Arc::clone(&lifecycle) as Arc<dyn LifecycleRunner>);

        let result = runner.run(Box::new(SuccessTaskWithLifecycle)).await;

        assert!(matches!(result.status, TaskStatus::Completed));

        let calls = lifecycle.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, LifecyclePhase::OnEnter);
        assert_eq!(calls[1].0, LifecyclePhase::OnDone);
    }

    #[tokio::test]
    async fn runner_executes_on_enter_and_on_fail_on_failure() {
        let agent = failure_agent();
        let lifecycle = Arc::new(MockLifecycleRunner::new());
        let runner = DefaultTaskRunner::new(agent)
            .with_lifecycle_runner(Arc::clone(&lifecycle) as Arc<dyn LifecycleRunner>);

        let result = runner.run(Box::new(SuccessTaskWithLifecycle)).await;

        assert!(matches!(result.status, TaskStatus::Failed(_)));

        let calls = lifecycle.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, LifecyclePhase::OnEnter);
        assert_eq!(calls[1].0, LifecyclePhase::OnFail);
    }

    #[tokio::test]
    async fn runner_fails_task_when_on_enter_fails() {
        let agent = success_agent();
        let lifecycle = Arc::new(MockLifecycleRunner::failing_on(LifecyclePhase::OnEnter));
        let runner = DefaultTaskRunner::new(agent.clone())
            .with_lifecycle_runner(Arc::clone(&lifecycle) as Arc<dyn LifecycleRunner>);

        let result = runner.run(Box::new(SuccessTaskWithLifecycle)).await;

        // on_enter failure should prevent agent invocation
        assert!(!agent.was_invoked());
        assert!(matches!(result.status, TaskStatus::Failed(_)));
        if let TaskStatus::Failed(msg) = &result.status {
            assert!(msg.contains("on_enter failed"));
        }
    }

    #[tokio::test]
    async fn runner_transitions_to_failed_when_on_done_fails() {
        let agent = success_agent();
        let lifecycle = Arc::new(MockLifecycleRunner::failing_on(LifecyclePhase::OnDone));
        let runner = DefaultTaskRunner::new(agent)
            .with_lifecycle_runner(Arc::clone(&lifecycle) as Arc<dyn LifecycleRunner>);

        let result = runner.run(Box::new(SuccessTaskWithLifecycle)).await;

        // on_done failure should transition Completed → Failed
        assert!(matches!(result.status, TaskStatus::Failed(_)));
        if let TaskStatus::Failed(msg) = &result.status {
            assert!(msg.contains("on_done failed"));
        }
    }

    #[tokio::test]
    async fn runner_skips_lifecycle_when_task_skipped() {
        let agent = success_agent();
        let lifecycle = Arc::new(MockLifecycleRunner::new());
        let runner = DefaultTaskRunner::new(agent)
            .with_lifecycle_runner(Arc::clone(&lifecycle) as Arc<dyn LifecycleRunner>);

        let result = runner.run(Box::new(SkipTask)).await;

        assert!(matches!(result.status, TaskStatus::Skipped(_)));
        let calls = lifecycle.calls.lock().unwrap();
        assert!(calls.is_empty());
    }

    #[tokio::test]
    async fn runner_skips_lifecycle_when_no_config() {
        let agent = success_agent();
        let lifecycle = Arc::new(MockLifecycleRunner::new());
        let runner = DefaultTaskRunner::new(agent)
            .with_lifecycle_runner(Arc::clone(&lifecycle) as Arc<dyn LifecycleRunner>);

        // SuccessTask has no lifecycle_config (default empty)
        let result = runner.run(Box::new(SuccessTask)).await;

        assert!(matches!(result.status, TaskStatus::Completed));
        let calls = lifecycle.calls.lock().unwrap();
        assert!(calls.is_empty());
    }
}
