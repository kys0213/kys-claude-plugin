//! ImproveTask — PR 피드백 반영 Task 구현체.
//!
//! 기존 `pipeline::pr::improve_one()`의 로직을 Task trait으로 재구성한다.
//! before_invoke: worktree(head_branch) → 피드백 반영 프롬프트
//! after_invoke: exit_code 확인 → iteration++ → PushPr(IMPROVED)

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use super::AGENT_SYSTEM_PROMPT;
use crate::components::workspace::WorkspaceOps;
use crate::config::ConfigLoader;
use crate::daemon::task::{
    AgentRequest, AgentResponse, QueueOp, SkipReason, Task, TaskResult, TaskStatus,
};
use crate::domain::labels;
use crate::domain::models::NewConsumerLog;
use crate::infrastructure::claude::SessionOptions;
use crate::infrastructure::gh::Gh;
use crate::queue::task_queues::{pr_phase, PrItem};

/// PR 피드백 반영 Task.
///
/// `before_invoke`에서 worktree를 준비하고 피드백 반영 프롬프트를 구성한다.
/// `after_invoke`에서 성공 시 iteration을 증가시키고 IMPROVED 상태로 push한다.
pub struct ImproveTask {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    #[allow(dead_code)]
    config: Arc<dyn ConfigLoader>,
    item: PrItem,
    worker_id: String,
    task_id: String,
    wt_path: Option<PathBuf>,
    started_at: Option<String>,
}

impl ImproveTask {
    pub fn new(
        workspace: Arc<dyn WorkspaceOps>,
        gh: Arc<dyn Gh>,
        config: Arc<dyn ConfigLoader>,
        item: PrItem,
    ) -> Self {
        let task_id = format!("pr-{}", item.github_number);
        Self {
            workspace,
            gh,
            config,
            item,
            worker_id: Uuid::new_v4().to_string(),
            task_id,
            wt_path: None,
            started_at: None,
        }
    }

    async fn cleanup_worktree(&self) {
        let _ = self
            .workspace
            .remove_worktree(&self.item.repo_name, &self.task_id)
            .await;
    }
}

#[async_trait]
impl Task for ImproveTask {
    fn work_id(&self) -> &str {
        &self.item.work_id
    }

    fn repo_name(&self) -> &str {
        &self.item.repo_name
    }

    async fn before_invoke(&mut self) -> Result<AgentRequest, SkipReason> {
        // Workspace 준비
        self.workspace
            .ensure_cloned(&self.item.repo_url, &self.item.repo_name)
            .await
            .map_err(|e| {
                SkipReason::PreflightFailed(format!(
                    "clone failed for PR #{}: {e}",
                    self.item.github_number
                ))
            })?;

        let branch = if self.item.head_branch.is_empty() {
            None
        } else {
            Some(self.item.head_branch.as_str())
        };
        let wt_path = self
            .workspace
            .create_worktree(&self.item.repo_name, &self.task_id, branch)
            .await
            .map_err(|e| {
                SkipReason::PreflightFailed(format!(
                    "worktree failed for PR #{}: {e}",
                    self.item.github_number
                ))
            })?;
        self.wt_path = Some(wt_path.clone());

        let prompt = format!("[autodev] improve: PR #{}", self.item.github_number);

        self.started_at = Some(Utc::now().to_rfc3339());

        Ok(AgentRequest {
            working_dir: wt_path,
            prompt,
            session_opts: SessionOptions {
                append_system_prompt: Some(AGENT_SYSTEM_PROMPT.to_string()),
                ..Default::default()
            },
        })
    }

    async fn after_invoke(&mut self, response: AgentResponse) -> TaskResult {
        let gh_host = self.item.gh_host.as_deref();

        let started = self
            .started_at
            .take()
            .unwrap_or_else(|| Utc::now().to_rfc3339());
        let finished = Utc::now().to_rfc3339();

        let log = NewConsumerLog {
            repo_id: self.item.repo_id.clone(),
            queue_type: "pr".to_string(),
            queue_item_id: self.item.work_id.clone(),
            worker_id: self.worker_id.clone(),
            command: format!("implement review feedback PR #{}", self.item.github_number),
            stdout: response.stdout.clone(),
            stderr: response.stderr.clone(),
            exit_code: response.exit_code,
            started_at: started,
            finished_at: finished,
            duration_ms: response.duration.as_millis() as i64,
        };

        let mut ops = Vec::new();

        if response.exit_code == 0 {
            // Iteration 라벨 동기화
            if self.item.review_iteration > 0 {
                self.gh
                    .label_remove(
                        &self.item.repo_name,
                        self.item.github_number,
                        &labels::iteration_label(self.item.review_iteration),
                        gh_host,
                    )
                    .await;
            }
            let mut next_item = self.item.clone();
            next_item.review_iteration += 1;
            self.gh
                .label_add(
                    &self.item.repo_name,
                    self.item.github_number,
                    &labels::iteration_label(next_item.review_iteration),
                    gh_host,
                )
                .await;

            ops.push(QueueOp::Remove);
            ops.push(QueueOp::PushPr {
                phase: pr_phase::IMPROVED,
                item: Box::new(next_item),
            });
        } else {
            self.gh
                .label_remove(
                    &self.item.repo_name,
                    self.item.github_number,
                    labels::WIP,
                    gh_host,
                )
                .await;
            ops.push(QueueOp::Remove);
        }

        self.cleanup_worktree().await;

        TaskResult {
            work_id: self.item.work_id.clone(),
            repo_name: self.item.repo_name.clone(),
            queue_ops: ops,
            logs: vec![log],
            status: if response.exit_code == 0 {
                TaskStatus::Completed
            } else {
                TaskStatus::Failed(format!("improve exit_code={}", response.exit_code))
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::Duration;

    use crate::config::models::WorkflowConfig;
    use crate::infrastructure::gh::mock::MockGh;
    use crate::queue::task_queues::make_work_id;

    struct MockWorkspace;

    #[async_trait]
    impl WorkspaceOps for MockWorkspace {
        async fn ensure_cloned(&self, _: &str, _: &str) -> anyhow::Result<PathBuf> {
            Ok(PathBuf::from("/mock/main"))
        }
        async fn create_worktree(
            &self,
            _: &str,
            task_id: &str,
            _: Option<&str>,
        ) -> anyhow::Result<PathBuf> {
            Ok(PathBuf::from(format!("/mock/{task_id}")))
        }
        async fn remove_worktree(&self, _: &str, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct MockConfigLoader;
    impl ConfigLoader for MockConfigLoader {
        fn load(&self, _: Option<&Path>) -> WorkflowConfig {
            WorkflowConfig::default()
        }
    }

    fn make_test_pr() -> PrItem {
        PrItem {
            work_id: make_work_id("pr", "org/repo", 10),
            repo_id: "r1".to_string(),
            repo_name: "org/repo".to_string(),
            repo_url: "https://github.com/org/repo".to_string(),
            github_number: 10,
            title: "Fix bug".to_string(),
            head_branch: "autodev/issue-42".to_string(),
            base_branch: "main".to_string(),
            review_comment: Some("Fix error handling".to_string()),
            source_issue_number: Some(42),
            review_iteration: 0,
            gh_host: None,
        }
    }

    fn make_task(gh: Arc<MockGh>) -> ImproveTask {
        ImproveTask::new(
            Arc::new(MockWorkspace),
            gh,
            Arc::new(MockConfigLoader),
            make_test_pr(),
        )
    }

    #[tokio::test]
    async fn before_creates_worktree_with_pr_branch() {
        let gh = Arc::new(MockGh::new());
        let mut task = make_task(gh);

        let request = task.before_invoke().await.expect("should succeed");

        assert!(request.prompt.contains("improve: PR #10"));
    }

    #[tokio::test]
    async fn after_success_pushes_to_improved() {
        let gh = Arc::new(MockGh::new());
        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 0,
            stdout: "Changes applied".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(20),
        };

        let result = task.after_invoke(response).await;

        assert!(matches!(result.status, TaskStatus::Completed));
        assert!(result.queue_ops.iter().any(
            |op| matches!(op, QueueOp::PushPr { phase, item } if *phase == pr_phase::IMPROVED && item.review_iteration == 1)
        ));

        // Iteration label added
        let added = gh.added_labels.lock().unwrap();
        assert!(added
            .iter()
            .any(|(_, _, l)| l == &labels::iteration_label(1)));
    }

    #[tokio::test]
    async fn after_success_increments_iteration() {
        let gh = Arc::new(MockGh::new());
        let mut pr = make_test_pr();
        pr.review_iteration = 2;
        let mut task = ImproveTask::new(
            Arc::new(MockWorkspace),
            gh.clone(),
            Arc::new(MockConfigLoader),
            pr,
        );
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 0,
            stdout: "Done".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(10),
        };

        let result = task.after_invoke(response).await;

        // Should push with iteration 3
        assert!(result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::PushPr { item, .. } if item.review_iteration == 3)));

        // Old iteration label removed, new one added
        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed
            .iter()
            .any(|(_, _, l)| l == &labels::iteration_label(2)));
        let added = gh.added_labels.lock().unwrap();
        assert!(added
            .iter()
            .any(|(_, _, l)| l == &labels::iteration_label(3)));
    }

    #[tokio::test]
    async fn after_nonzero_exit_removes() {
        let gh = Arc::new(MockGh::new());
        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 1,
            stdout: String::new(),
            stderr: "error".to_string(),
            duration: Duration::from_secs(5),
        };

        let result = task.after_invoke(response).await;

        assert!(matches!(result.status, TaskStatus::Failed(_)));
        assert!(!result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::PushPr { .. })));

        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed.iter().any(|(_, n, l)| *n == 10 && l == labels::WIP));
    }
}
