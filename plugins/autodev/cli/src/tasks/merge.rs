//! MergeTask — PR 머지 Task 구현체.
//!
//! 기존 `pipeline::merge::merge_one()`의 로직을 Task trait으로 재구성한다.
//! before_invoke: preflight(PR mergeable?) → worktree → 머지 프롬프트
//! after_invoke: MergeOutcome 파싱 → 라벨 처리

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::components::workspace::WorkspaceOps;
use crate::config::ConfigLoader;
use crate::daemon::task::{
    AgentRequest, AgentResponse, QueueOp, SkipReason, Task, TaskResult, TaskStatus,
};
use crate::domain::labels;
use crate::domain::models::NewConsumerLog;
use crate::infrastructure::claude::SessionOptions;
use crate::infrastructure::gh::Gh;
use crate::queue::task_queues::MergeItem;

/// Merge 결과 분류
enum MergeOutcome {
    Success,
    Failed,
}

fn classify_merge(exit_code: i32, _stdout: &str, _stderr: &str) -> MergeOutcome {
    if exit_code == 0 {
        MergeOutcome::Success
    } else {
        MergeOutcome::Failed
    }
}

/// PR 머지 Task.
///
/// 단일 Agent 호출로 머지 + 충돌 해결을 처리한다.
/// 프롬프트에 머지 명령과 충돌 해결 지침을 함께 포함한다.
pub struct MergeTask {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    #[allow(dead_code)]
    config: Arc<dyn ConfigLoader>,
    item: MergeItem,
    worker_id: String,
    task_id: String,
    wt_path: Option<PathBuf>,
    started_at: Option<String>,
}

impl MergeTask {
    pub fn new(
        workspace: Arc<dyn WorkspaceOps>,
        gh: Arc<dyn Gh>,
        config: Arc<dyn ConfigLoader>,
        item: MergeItem,
    ) -> Self {
        let task_id = format!("merge-pr-{}", item.pr_number);
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
impl Task for MergeTask {
    fn work_id(&self) -> &str {
        &self.item.work_id
    }

    fn repo_name(&self) -> &str {
        &self.item.repo_name
    }

    async fn before_invoke(&mut self) -> Result<AgentRequest, SkipReason> {
        let gh_host = self.item.gh_host.as_deref();

        // Preflight: PR이 머지 가능한지 확인
        let state = self
            .gh
            .api_get_field(
                &self.item.repo_name,
                &format!("pulls/{}", self.item.pr_number),
                ".state",
                gh_host,
            )
            .await;
        if let Some(ref s) = state {
            if s != "open" {
                self.gh
                    .label_remove(
                        &self.item.repo_name,
                        self.item.pr_number,
                        labels::WIP,
                        gh_host,
                    )
                    .await;
                self.gh
                    .label_add(
                        &self.item.repo_name,
                        self.item.pr_number,
                        labels::DONE,
                        gh_host,
                    )
                    .await;
                return Err(SkipReason::PreflightFailed(format!(
                    "PR #{} is not open",
                    self.item.pr_number
                )));
            }
        }

        // Workspace 준비
        self.workspace
            .ensure_cloned(&self.item.repo_url, &self.item.repo_name)
            .await
            .map_err(|e| {
                SkipReason::PreflightFailed(format!(
                    "clone failed for merge PR #{}: {e}",
                    self.item.pr_number
                ))
            })?;

        let wt_path = self
            .workspace
            .create_worktree(&self.item.repo_name, &self.task_id, None)
            .await
            .map_err(|e| {
                SkipReason::PreflightFailed(format!(
                    "worktree failed for merge PR #{}: {e}",
                    self.item.pr_number
                ))
            })?;
        self.wt_path = Some(wt_path.clone());

        // 머지 + 충돌 해결을 포함한 프롬프트
        let prompt = format!(
            "[autodev] merge: PR #{pr}\n\n\
             /git-utils:merge-pr {pr}\n\n\
             If there are merge conflicts, resolve them:\n\
             1. Run `git status` to find conflicting files.\n\
             2. For each file with conflict markers (<<<<<<< / ======= / >>>>>>>), \
             resolve by choosing the correct version or combining both changes.\n\
             3. `git add` each resolved file.\n\
             4. Run the project's tests to verify.\n\
             5. `git commit` the merge resolution.",
            pr = self.item.pr_number
        );

        self.started_at = Some(Utc::now().to_rfc3339());

        Ok(AgentRequest {
            working_dir: wt_path,
            prompt,
            session_opts: SessionOptions::default(),
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
            queue_type: "merge".to_string(),
            queue_item_id: self.item.work_id.clone(),
            worker_id: self.worker_id.clone(),
            command: format!("claude -p \"/git-utils:merge-pr {}\"", self.item.pr_number),
            stdout: response.stdout.clone(),
            stderr: response.stderr.clone(),
            exit_code: response.exit_code,
            started_at: started,
            finished_at: finished,
            duration_ms: response.duration.as_millis() as i64,
        };

        let outcome = classify_merge(response.exit_code, &response.stdout, &response.stderr);

        match outcome {
            MergeOutcome::Success => {
                self.gh
                    .label_remove(
                        &self.item.repo_name,
                        self.item.pr_number,
                        labels::WIP,
                        gh_host,
                    )
                    .await;
                self.gh
                    .label_add(
                        &self.item.repo_name,
                        self.item.pr_number,
                        labels::DONE,
                        gh_host,
                    )
                    .await;
            }
            MergeOutcome::Failed => {
                self.gh
                    .label_remove(
                        &self.item.repo_name,
                        self.item.pr_number,
                        labels::WIP,
                        gh_host,
                    )
                    .await;
            }
        }

        self.cleanup_worktree().await;

        let status = match outcome {
            MergeOutcome::Success => TaskStatus::Completed,
            MergeOutcome::Failed => {
                TaskStatus::Failed(format!("merge exit_code={}", response.exit_code))
            }
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
        fn repo_base_path(&self, _: &str) -> PathBuf {
            PathBuf::from("/mock/main")
        }
        fn worktree_path(&self, _: &str, task_id: &str) -> PathBuf {
            PathBuf::from(format!("/mock/{task_id}"))
        }
    }

    struct MockConfigLoader;
    impl ConfigLoader for MockConfigLoader {
        fn load(&self, _: Option<&Path>) -> WorkflowConfig {
            WorkflowConfig::default()
        }
    }

    fn make_test_merge() -> MergeItem {
        MergeItem {
            work_id: make_work_id("merge", "org/repo", 10),
            repo_id: "r1".to_string(),
            repo_name: "org/repo".to_string(),
            repo_url: "https://github.com/org/repo".to_string(),
            pr_number: 10,
            title: "Fix bug".to_string(),
            head_branch: "autodev/issue-42".to_string(),
            base_branch: "main".to_string(),
            gh_host: None,
        }
    }

    fn make_task(gh: Arc<MockGh>) -> MergeTask {
        MergeTask::new(
            Arc::new(MockWorkspace),
            gh,
            Arc::new(MockConfigLoader),
            make_test_merge(),
        )
    }

    #[tokio::test]
    async fn before_skips_non_open_pr() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "merged");

        let mut task = make_task(gh.clone());
        let result = task.before_invoke().await;

        assert!(result.is_err());
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, n, l)| *n == 10 && l == labels::DONE));
    }

    #[tokio::test]
    async fn before_creates_worktree() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "open");

        let mut task = make_task(gh);
        let request = task.before_invoke().await.expect("should succeed");

        assert!(request.prompt.contains("/git-utils:merge-pr 10"));
        assert!(request.prompt.contains("merge conflicts"));
    }

    #[tokio::test]
    async fn after_success_marks_done() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "open");

        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 0,
            stdout: "Merged successfully".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(10),
        };

        let result = task.after_invoke(response).await;

        assert!(matches!(result.status, TaskStatus::Completed));
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, n, l)| *n == 10 && l == labels::DONE));
    }

    #[tokio::test]
    async fn after_failure_removes_wip() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "open");

        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 1,
            stdout: String::new(),
            stderr: "merge failed".to_string(),
            duration: Duration::from_secs(5),
        };

        let result = task.after_invoke(response).await;

        assert!(matches!(result.status, TaskStatus::Failed(_)));
        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed.iter().any(|(_, n, l)| *n == 10 && l == labels::WIP));
        // Should NOT add DONE on failure
        let added = gh.added_labels.lock().unwrap();
        assert!(!added.iter().any(|(_, n, l)| *n == 10 && l == labels::DONE));
    }
}
