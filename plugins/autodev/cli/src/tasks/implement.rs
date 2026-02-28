//! ImplementTask — 이슈 구현 Task 구현체.
//!
//! 기존 `pipeline::issue::implement_one()`의 로직을 Task trait으로 재구성한다.
//! before_invoke: worktree 생성 → 구현 프롬프트 구성
//! after_invoke: PR 번호 추출 → PR queue push → 라벨/코멘트

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
use crate::infrastructure::claude::output;
use crate::infrastructure::claude::SessionOptions;
use crate::infrastructure::gh::Gh;
use crate::queue::task_queues::{make_work_id, pr_phase, IssueItem, PrItem};

/// head branch 이름으로 이미 생성된 PR을 조회하여 번호를 반환.
async fn find_existing_pr(
    gh: &dyn Gh,
    repo_name: &str,
    head_branch: &str,
    gh_host: Option<&str>,
) -> Option<i64> {
    let params = [("head", head_branch), ("state", "open"), ("per_page", "1")];
    let data = gh
        .api_paginate(repo_name, "pulls", &params, gh_host)
        .await
        .ok()?;
    let prs: Vec<serde_json::Value> = serde_json::from_slice(&data).ok()?;
    prs.first().and_then(|pr| pr["number"].as_i64())
}

/// 이슈 구현 Task.
///
/// `before_invoke`에서 worktree를 준비하고 구현 프롬프트를 구성한다.
/// `after_invoke`에서 PR 번호를 추출하여 PR queue에 push한다.
pub struct ImplementTask {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
    item: IssueItem,
    worker_id: String,
    task_id: String,
    wt_path: Option<PathBuf>,
    started_at: Option<String>,
}

impl ImplementTask {
    pub fn new(
        workspace: Arc<dyn WorkspaceOps>,
        gh: Arc<dyn Gh>,
        config: Arc<dyn ConfigLoader>,
        item: IssueItem,
    ) -> Self {
        let task_id = format!("issue-{}", item.github_number);
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
impl Task for ImplementTask {
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
                    "clone failed for issue #{}: {e}",
                    self.item.github_number
                ))
            })?;

        let wt_path = self
            .workspace
            .create_worktree(&self.item.repo_name, &self.task_id, None)
            .await
            .map_err(|e| {
                SkipReason::PreflightFailed(format!(
                    "worktree failed for issue #{}: {e}",
                    self.item.github_number
                ))
            })?;
        self.wt_path = Some(wt_path.clone());

        // 레포별 config에서 workflow 로드
        let repo_cfg = self.config.load(Some(&wt_path));
        let workflow = repo_cfg.workflow.issue.clone();
        let prompt = format!(
            "[autodev] implement: issue #{} in {}",
            self.item.github_number, self.item.repo_name
        );
        let system_prompt = format!("{AGENT_SYSTEM_PROMPT}\n\n{workflow}");

        self.started_at = Some(Utc::now().to_rfc3339());

        Ok(AgentRequest {
            working_dir: wt_path,
            prompt,
            session_opts: SessionOptions {
                append_system_prompt: Some(system_prompt),
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

        let repo_cfg = self.config.load(self.wt_path.as_deref());
        let workflow = &repo_cfg.workflow.issue;

        let log = NewConsumerLog {
            repo_id: self.item.repo_id.clone(),
            queue_type: "issue".to_string(),
            queue_item_id: self.item.work_id.clone(),
            worker_id: self.worker_id.clone(),
            command: format!(
                "claude -p \"{workflow} implement issue #{}\"",
                self.item.github_number
            ),
            stdout: response.stdout.clone(),
            stderr: response.stderr.clone(),
            exit_code: response.exit_code,
            started_at: started,
            finished_at: finished,
            duration_ms: response.duration.as_millis() as i64,
        };

        // Agent 호출 실패
        if response.exit_code != 0 {
            self.gh
                .label_remove(
                    &self.item.repo_name,
                    self.item.github_number,
                    labels::IMPLEMENTING,
                    gh_host,
                )
                .await;
            self.cleanup_worktree().await;
            return TaskResult {
                work_id: self.item.work_id.clone(),
                repo_name: self.item.repo_name.clone(),
                queue_ops: vec![QueueOp::Remove],
                logs: vec![log],
                status: TaskStatus::Failed(format!(
                    "implementation exit_code={}",
                    response.exit_code
                )),
            };
        }

        // PR 번호 추출 (stdout 파싱 + API fallback)
        let head_branch = format!("autodev/issue-{}", self.item.github_number);
        let pr_number = match output::extract_pr_number(&response.stdout) {
            Some(n) => Some(n),
            None => find_existing_pr(&*self.gh, &self.item.repo_name, &head_branch, gh_host).await,
        };

        let mut ops = Vec::new();

        match pr_number {
            Some(pr_num) => {
                let pr_work_id = make_work_id("pr", &self.item.repo_name, pr_num);
                self.gh
                    .label_add(&self.item.repo_name, pr_num, labels::WIP, gh_host)
                    .await;

                let pr_item = PrItem {
                    work_id: pr_work_id,
                    repo_id: self.item.repo_id.clone(),
                    repo_name: self.item.repo_name.clone(),
                    repo_url: self.item.repo_url.clone(),
                    github_number: pr_num,
                    title: format!("PR #{pr_num} (from issue #{})", self.item.github_number),
                    head_branch: String::new(),
                    base_branch: String::new(),
                    review_comment: None,
                    source_issue_number: Some(self.item.github_number),
                    review_iteration: 0,
                    gh_host: self.item.gh_host.clone(),
                };

                let pr_comment = format!(
                    "<!-- autodev:pr-link:{pr_num} -->\n\
                     Implementation PR #{pr_num} has been created and is awaiting review."
                );
                self.gh
                    .issue_comment(
                        &self.item.repo_name,
                        self.item.github_number,
                        &pr_comment,
                        gh_host,
                    )
                    .await;

                ops.push(QueueOp::Remove);
                ops.push(QueueOp::PushPr {
                    phase: pr_phase::PENDING,
                    item: pr_item,
                });
                tracing::info!(
                    "issue #{}: PR #{pr_num} created, pushed to PR queue",
                    self.item.github_number
                );
            }
            None => {
                self.gh
                    .label_remove(
                        &self.item.repo_name,
                        self.item.github_number,
                        labels::IMPLEMENTING,
                        gh_host,
                    )
                    .await;
                tracing::warn!(
                    "issue #{}: PR number extraction failed, implementing removed",
                    self.item.github_number
                );
                ops.push(QueueOp::Remove);
            }
        }

        self.cleanup_worktree().await;

        TaskResult {
            work_id: self.item.work_id.clone(),
            repo_name: self.item.repo_name.clone(),
            queue_ops: ops,
            logs: vec![log],
            status: TaskStatus::Completed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Mutex;
    use std::time::Duration;

    use crate::config::models::WorkflowConfig;
    use crate::infrastructure::gh::mock::MockGh;

    // ─── Mock WorkspaceOps ───

    struct MockWorkspace {
        cloned: Mutex<Vec<(String, String)>>,
        worktrees: Mutex<Vec<(String, String)>>,
        removed: Mutex<Vec<(String, String)>>,
    }

    impl MockWorkspace {
        fn new() -> Self {
            Self {
                cloned: Mutex::new(Vec::new()),
                worktrees: Mutex::new(Vec::new()),
                removed: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl WorkspaceOps for MockWorkspace {
        async fn ensure_cloned(&self, repo_url: &str, repo_name: &str) -> anyhow::Result<PathBuf> {
            self.cloned
                .lock()
                .unwrap()
                .push((repo_url.to_string(), repo_name.to_string()));
            Ok(PathBuf::from("/mock/workspaces/main"))
        }

        async fn create_worktree(
            &self,
            repo_name: &str,
            task_id: &str,
            _branch: Option<&str>,
        ) -> anyhow::Result<PathBuf> {
            self.worktrees
                .lock()
                .unwrap()
                .push((repo_name.to_string(), task_id.to_string()));
            Ok(PathBuf::from(format!("/mock/workspaces/{task_id}")))
        }

        async fn remove_worktree(&self, repo_name: &str, task_id: &str) -> anyhow::Result<()> {
            self.removed
                .lock()
                .unwrap()
                .push((repo_name.to_string(), task_id.to_string()));
            Ok(())
        }

        fn repo_base_path(&self, _repo_name: &str) -> PathBuf {
            PathBuf::from("/mock/workspaces/main")
        }

        fn worktree_path(&self, _repo_name: &str, task_id: &str) -> PathBuf {
            PathBuf::from(format!("/mock/workspaces/{task_id}"))
        }
    }

    // ─── Mock ConfigLoader ───

    struct MockConfigLoader;

    impl ConfigLoader for MockConfigLoader {
        fn load(&self, _workspace_path: Option<&Path>) -> WorkflowConfig {
            WorkflowConfig::default()
        }
    }

    // ─── Test helpers ───

    fn make_test_issue() -> IssueItem {
        IssueItem {
            work_id: make_work_id("issue", "org/repo", 42),
            repo_id: "r1".to_string(),
            repo_name: "org/repo".to_string(),
            repo_url: "https://github.com/org/repo".to_string(),
            github_number: 42,
            title: "Fix login bug".to_string(),
            body: Some("Users cannot log in".to_string()),
            labels: vec![],
            author: "user".to_string(),
            analysis_report: None,
            gh_host: None,
        }
    }

    fn make_task(gh: Arc<MockGh>) -> ImplementTask {
        let ws = Arc::new(MockWorkspace::new());
        let cfg = Arc::new(MockConfigLoader);
        ImplementTask::new(ws, gh, cfg, make_test_issue())
    }

    // ═══════════════════════════════════════════════
    // before_invoke tests
    // ═══════════════════════════════════════════════

    #[tokio::test]
    async fn before_creates_worktree_and_returns_request() {
        let gh = Arc::new(MockGh::new());
        let ws = Arc::new(MockWorkspace::new());
        let cfg = Arc::new(MockConfigLoader);
        let mut task = ImplementTask::new(ws.clone(), gh, cfg, make_test_issue());

        let request = task.before_invoke().await.expect("should succeed");

        assert!(request.prompt.contains("implement: issue #42"));
        assert!(request.prompt.contains("org/repo"));
        assert!(request
            .session_opts
            .append_system_prompt
            .as_ref()
            .unwrap()
            .contains(AGENT_SYSTEM_PROMPT));

        let wts = ws.worktrees.lock().unwrap();
        assert_eq!(wts.len(), 1);
        assert_eq!(wts[0].1, "issue-42");
    }

    // ═══════════════════════════════════════════════
    // after_invoke tests
    // ═══════════════════════════════════════════════

    #[tokio::test]
    async fn after_creates_pr_and_pushes_to_pr_queue() {
        let gh = Arc::new(MockGh::new());
        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 0,
            stdout: "Created PR at https://github.com/org/repo/pull/99".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(30),
        };

        let result = task.after_invoke(response).await;

        assert!(matches!(result.status, TaskStatus::Completed));

        // Should have Remove + PushPr
        assert!(result.queue_ops.len() >= 2);
        assert!(result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::Remove)));
        assert!(result.queue_ops.iter().any(
            |op| matches!(op, QueueOp::PushPr { phase, item } if *phase == pr_phase::PENDING && item.github_number == 99)
        ));

        // PR에 wip 라벨 추가됨
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, n, l)| *n == 99 && l == labels::WIP));

        // Issue에 pr-link 코멘트 작성됨
        let comments = gh.posted_comments.lock().unwrap();
        assert!(comments
            .iter()
            .any(|(_, n, body)| *n == 42 && body.contains("autodev:pr-link:99")));
    }

    #[tokio::test]
    async fn after_pr_extract_fail_removes_implementing() {
        let gh = Arc::new(MockGh::new());
        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 0,
            stdout: "Done implementing but no PR URL".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(30),
        };

        let result = task.after_invoke(response).await;

        assert!(matches!(result.status, TaskStatus::Completed));
        assert!(result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::Remove)));
        // No PushPr
        assert!(!result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::PushPr { .. })));

        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed
            .iter()
            .any(|(_, n, l)| *n == 42 && l == labels::IMPLEMENTING));
    }

    #[tokio::test]
    async fn after_nonzero_exit_removes_and_fails() {
        let gh = Arc::new(MockGh::new());
        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 1,
            stdout: String::new(),
            stderr: "timeout".to_string(),
            duration: Duration::from_secs(60),
        };

        let result = task.after_invoke(response).await;

        assert!(matches!(result.status, TaskStatus::Failed(_)));
        assert!(result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::Remove)));

        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed
            .iter()
            .any(|(_, n, l)| *n == 42 && l == labels::IMPLEMENTING));
    }

    #[tokio::test]
    async fn after_uses_find_existing_pr_fallback() {
        let gh = Arc::new(MockGh::new());
        // Set up mock paginate response for find_existing_pr
        gh.set_paginate(
            "org/repo",
            "pulls",
            serde_json::to_vec(&serde_json::json!([{"number": 55}])).unwrap(),
        );

        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        // stdout has no PR URL, but API will find one
        let response = AgentResponse {
            exit_code: 0,
            stdout: "Implementation complete.".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(30),
        };

        let result = task.after_invoke(response).await;

        assert!(result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::PushPr { item, .. } if item.github_number == 55)));
    }

    #[tokio::test]
    async fn after_cleans_up_worktree() {
        let gh = Arc::new(MockGh::new());
        let ws = Arc::new(MockWorkspace::new());
        let cfg = Arc::new(MockConfigLoader);
        let mut task = ImplementTask::new(ws.clone(), gh, cfg, make_test_issue());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 0,
            stdout: "https://github.com/org/repo/pull/10".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(10),
        };
        let _ = task.after_invoke(response).await;

        let removed = ws.removed.lock().unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].1, "issue-42");
    }
}
