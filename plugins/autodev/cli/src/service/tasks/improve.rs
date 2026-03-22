//! ImproveTask — PR 피드백 반영 Task 구현체.
//!
//! 기존 `pipeline::pr::improve_one()`의 로직을 Task trait으로 재구성한다.
//! before_invoke: worktree(head_branch) → 피드백 반영 프롬프트
//! after_invoke: exit_code 확인 → iteration++ → Push(Pending) with TaskKind::Review

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use super::AGENT_SYSTEM_PROMPT;
use crate::core::labels;
use crate::core::models::{NewConsumerLog, QueuePhase, QueueType};
use crate::core::queue_item::QueueItem;
use crate::core::task::{
    AgentRequest, AgentResponse, QueueOp, SkipReason, Task, TaskResult, TaskStatus,
};
use crate::infra::claude::SessionOptions;
use crate::infra::gh::Gh;
use crate::service::tasks::helpers::workspace::WorkspaceOps;

/// PR 피드백 반영 Task.
///
/// `before_invoke`에서 worktree를 준비하고 피드백 반영 프롬프트를 구성한다.
/// `after_invoke`에서 성공 시 iteration을 증가시키고 Pending 상태로 push한다.
pub struct ImproveTask {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    item: QueueItem,
    worker_id: String,
    task_id: String,
    wt_path: Option<PathBuf>,
    started_at: Option<String>,
}

impl ImproveTask {
    pub fn new(workspace: Arc<dyn WorkspaceOps>, gh: Arc<dyn Gh>, item: QueueItem) -> Self {
        let task_id = format!("pr-{}", item.github_number);
        Self {
            workspace,
            gh,
            item,
            worker_id: Uuid::new_v4().to_string(),
            task_id,
            wt_path: None,
            started_at: None,
        }
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

        let branch = self.item.head_branch();
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
            queue_type: QueueType::Pr.to_string(),
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
            // changes-requested → wip 라벨 전이 (add-first)
            self.gh
                .label_add(
                    &self.item.repo_name,
                    self.item.github_number,
                    labels::WIP,
                    gh_host,
                )
                .await;
            self.gh
                .label_remove(
                    &self.item.repo_name,
                    self.item.github_number,
                    labels::CHANGES_REQUESTED,
                    gh_host,
                )
                .await;

            // Iteration 라벨 동기화
            if self.item.review_iteration_or_zero() > 0 {
                self.gh
                    .label_remove(
                        &self.item.repo_name,
                        self.item.github_number,
                        &labels::iteration_label(self.item.review_iteration_or_zero()),
                        gh_host,
                    )
                    .await;
            }
            let mut next_item = self.item.clone();
            let new_iteration = next_item.increment_review_iteration();
            // In unified model, Improved → directly to Pending with TaskKind::Review
            next_item.transition_to_review();
            self.gh
                .label_add(
                    &self.item.repo_name,
                    self.item.github_number,
                    &labels::iteration_label(new_iteration),
                    gh_host,
                )
                .await;

            ops.push(QueueOp::Remove);
            ops.push(QueueOp::Push {
                phase: QueuePhase::Pending,
                item: Box::new(next_item),
            });
        } else {
            // add-first: IMPROVE_FAILED 추가 후 CHANGES_REQUESTED 제거
            self.gh
                .label_add(
                    &self.item.repo_name,
                    self.item.github_number,
                    labels::IMPROVE_FAILED,
                    gh_host,
                )
                .await;
            self.gh
                .label_remove(
                    &self.item.repo_name,
                    self.item.github_number,
                    labels::CHANGES_REQUESTED,
                    gh_host,
                )
                .await;

            let head_branch = self.item.head_branch().unwrap_or("").to_string();
            let fail_comment = format!(
                "<!-- autodev:improve-failed -->\n\
                 ⚠️ Improve agent failed (exit_code={}).\n\n\
                 **Branch**: `{head_branch}`\n\
                 Worktree has been preserved for debugging:\n\
                 ```\ngit worktree list | grep '{head_branch}'\n```",
                response.exit_code,
            );
            self.gh
                .issue_comment(
                    &self.item.repo_name,
                    self.item.github_number,
                    &fail_comment,
                    gh_host,
                )
                .await;

            ops.push(QueueOp::Remove);
        }

        let status = if response.exit_code == 0 {
            TaskStatus::Completed
        } else {
            TaskStatus::Failed(format!("improve exit_code={}", response.exit_code))
        };
        crate::service::tasks::helpers::workspace::maybe_cleanup_worktree(
            &*self.workspace,
            &self.item.repo_name,
            &self.task_id,
            &status,
        )
        .await;

        TaskResult {
            work_id: self.item.work_id.clone(),
            repo_name: self.item.repo_name.clone(),
            queue_ops: ops,
            logs: vec![log],
            status,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::core::phase::TaskKind;
    use crate::core::queue_item::testing::*;
    use crate::infra::gh::mock::MockGh;

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

    fn make_test_pr() -> QueueItem {
        test_pr_with_source(10, TaskKind::Improve, Some(42), 0)
    }

    fn make_task(gh: Arc<MockGh>) -> ImproveTask {
        ImproveTask::new(Arc::new(MockWorkspace), gh, make_test_pr())
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
        // Improved → Pending with TaskKind::Review (re-review 경로)
        assert!(result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::Push { phase, item }
                if *phase == QueuePhase::Pending
                && item.task_kind == TaskKind::Review
                && item.review_iteration() == Some(1))));

        // changes-requested → wip 전이
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, n, l)| *n == 10 && l == labels::WIP));
        // Iteration label added
        assert!(added
            .iter()
            .any(|(_, _, l)| l == &labels::iteration_label(1)));
        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed
            .iter()
            .any(|(_, n, l)| *n == 10 && l == labels::CHANGES_REQUESTED));
    }

    #[tokio::test]
    async fn after_success_increments_iteration() {
        let gh = Arc::new(MockGh::new());
        let pr = test_pr_with_source(10, TaskKind::Improve, Some(42), 2);
        let mut task = ImproveTask::new(Arc::new(MockWorkspace), gh.clone(), pr);
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 0,
            stdout: "Done".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(10),
        };

        let result = task.after_invoke(response).await;

        // Should push with iteration 3
        assert!(result.queue_ops.iter().any(
            |op| matches!(op, QueueOp::Push { item, .. } if item.review_iteration() == Some(3))
        ));

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

    // ═══════════════════════════════════════════════
    // DESIGN-v3: label add-first 순서 검증
    // ═══════════════════════════════════════════════

    #[tokio::test]
    async fn after_success_adds_wip_before_removing_changes_requested() {
        let gh = Arc::new(MockGh::new());
        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 0,
            stdout: "Changes applied".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(20),
        };
        let _ = task.after_invoke(response).await;

        gh.assert_add_before_remove(10, labels::WIP, labels::CHANGES_REQUESTED);
    }

    // ═══════════════════════════════════════════════
    // after_invoke: agent failure (exit_code != 0) tests
    // ═══════════════════════════════════════════════

    #[tokio::test]
    async fn after_nonzero_exit_adds_improve_failed_label() {
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
            .any(|op| matches!(op, QueueOp::Push { .. })));

        let added = gh.added_labels.lock().unwrap();
        assert!(
            added
                .iter()
                .any(|(_, n, l)| *n == 10 && l == labels::IMPROVE_FAILED),
            "should add improve-failed label on agent failure"
        );

        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed
            .iter()
            .any(|(_, n, l)| *n == 10 && l == labels::CHANGES_REQUESTED));
    }

    #[tokio::test]
    async fn after_nonzero_exit_uses_add_first_ordering() {
        let gh = Arc::new(MockGh::new());
        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 1,
            stdout: String::new(),
            stderr: "error".to_string(),
            duration: Duration::from_secs(5),
        };
        let _ = task.after_invoke(response).await;

        gh.assert_add_before_remove(10, labels::IMPROVE_FAILED, labels::CHANGES_REQUESTED);
    }

    #[tokio::test]
    async fn after_nonzero_exit_posts_failure_comment() {
        let gh = Arc::new(MockGh::new());
        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 1,
            stdout: String::new(),
            stderr: "error".to_string(),
            duration: Duration::from_secs(5),
        };
        let _ = task.after_invoke(response).await;

        let comments = gh.posted_comments.lock().unwrap();
        assert!(
            comments
                .iter()
                .any(|(_, n, body)| *n == 10 && body.contains("<!-- autodev:improve-failed -->")),
            "should post improve-failed comment with HTML marker"
        );
    }
}
