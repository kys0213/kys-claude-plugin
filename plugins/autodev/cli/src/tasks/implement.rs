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
use crate::core::config::ConfigLoader;
use crate::core::labels;
use crate::core::models::NewConsumerLog;
use crate::core::task::{
    AgentRequest, AgentResponse, QueueOp, SkipReason, Task, TaskResult, TaskStatus,
};
use crate::core::task_queues::{make_work_id, pr_phase, IssueItem, PrItem};
use crate::infra::claude::output;
use crate::infra::claude::SessionOptions;
use crate::infra::gh::Gh;
use crate::tasks::helpers::workspace::WorkspaceOps;

/// head branch 이름으로 이미 생성된 PR을 조회하여 번호를 반환.
///
/// GitHub API의 `head` 파라미터는 `owner:branch` 형식을 요구한다.
/// 반환된 PR의 head branch가 예상과 일치하는지 추가 검증한다.
async fn find_existing_pr(
    gh: &dyn Gh,
    repo_name: &str,
    head_branch: &str,
    gh_host: Option<&str>,
) -> Option<i64> {
    // GitHub API requires "owner:branch" format for the head parameter
    let owner = repo_name.split('/').next()?;
    let head_filter = format!("{owner}:{head_branch}");
    let params = [
        ("head", head_filter.as_str()),
        ("state", "open"),
        ("per_page", "1"),
    ];
    let data = gh
        .api_paginate(repo_name, "pulls", &params, gh_host)
        .await
        .ok()?;
    let prs: Vec<serde_json::Value> = serde_json::from_slice(&data).ok()?;
    let pr = prs.first()?;

    // Validate: returned PR's head branch must match expected branch
    let actual_head = pr
        .get("head")
        .and_then(|h| h.get("ref"))
        .and_then(|r| r.as_str());
    if actual_head != Some(head_branch) {
        tracing::warn!(
            "find_existing_pr: expected head={head_branch}, got {:?}, ignoring PR #{}",
            actual_head,
            pr["number"]
        );
        return None;
    }

    pr["number"].as_i64()
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

    fn head_branch(&self) -> String {
        format!("autodev/issue-{}", self.item.github_number)
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
        let gh_host = self.item.gh_host.as_deref();

        // Preflight: issue가 아직 open인지 확인 (AnalyzeTask 패턴 동일)
        let state = self
            .gh
            .api_get_field(
                &self.item.repo_name,
                &format!("issues/{}", self.item.github_number),
                ".state",
                gh_host,
            )
            .await;
        if let Some(ref s) = state {
            if s != "open" {
                // add-first: DONE 먼저, IMPLEMENTING 제거
                self.gh
                    .label_add(
                        &self.item.repo_name,
                        self.item.github_number,
                        labels::DONE,
                        gh_host,
                    )
                    .await;
                self.gh
                    .label_remove(
                        &self.item.repo_name,
                        self.item.github_number,
                        labels::IMPLEMENTING,
                        gh_host,
                    )
                    .await;
                return Err(SkipReason::PreflightFailed(format!(
                    "issue #{} is closed",
                    self.item.github_number
                )));
            }
        }

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

        let branch_name = self.head_branch();
        let wt_path = self
            .workspace
            .create_worktree(&self.item.repo_name, &self.task_id, Some(&branch_name))
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
        let resolved = super::workflow_resolver::resolve_workflow_prompt(
            &repo_cfg.workflows.implement,
            super::workflow_resolver::TaskType::Implement,
        );
        let prompt = format!(
            "[autodev] implement: issue #{} in {}",
            self.item.github_number, self.item.repo_name
        );
        let system_prompt = format!("{AGENT_SYSTEM_PROMPT}\n\n{resolved}");

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
        let workflow = repo_cfg
            .workflows
            .implement
            .command
            .as_deref()
            .unwrap_or("builtin");

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
            // add-first: IMPL_FAILED 추가 후 IMPLEMENTING 제거
            self.gh
                .label_add(
                    &self.item.repo_name,
                    self.item.github_number,
                    labels::IMPL_FAILED,
                    gh_host,
                )
                .await;
            self.gh
                .label_remove(
                    &self.item.repo_name,
                    self.item.github_number,
                    labels::IMPLEMENTING,
                    gh_host,
                )
                .await;

            let head_branch = self.head_branch();
            let fail_comment = format!(
                "<!-- autodev:impl-failed -->\n\
                 ⚠️ Implementation agent failed (exit_code={}).\n\n\
                 **Branch**: `{head_branch}`\n\
                 Check the agent logs for details.",
                response.exit_code
            );
            self.gh
                .issue_comment(
                    &self.item.repo_name,
                    self.item.github_number,
                    &fail_comment,
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
        let head_branch = self.head_branch();
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
                    item: Box::new(pr_item),
                });
                tracing::info!(
                    "issue #{}: PR #{pr_num} created, pushed to PR queue",
                    self.item.github_number
                );
                self.cleanup_worktree().await;
            }
            None => {
                // PR extraction failed but agent succeeded — preserve worktree for manual recovery
                self.gh
                    .label_add(
                        &self.item.repo_name,
                        self.item.github_number,
                        labels::IMPL_FAILED,
                        gh_host,
                    )
                    .await;
                self.gh
                    .label_remove(
                        &self.item.repo_name,
                        self.item.github_number,
                        labels::IMPLEMENTING,
                        gh_host,
                    )
                    .await;

                let fail_comment = format!(
                    "<!-- autodev:impl-failed -->\n\
                     ⚠️ Implementation completed but PR creation/detection failed.\n\n\
                     **Branch**: `{head_branch}`\n\
                     Worktree has been preserved. You can manually create a PR:\n\
                     ```\ngh pr create --head {head_branch}\n```"
                );
                self.gh
                    .issue_comment(
                        &self.item.repo_name,
                        self.item.github_number,
                        &fail_comment,
                        gh_host,
                    )
                    .await;

                tracing::warn!(
                    "issue #{}: PR extraction failed, worktree preserved for manual recovery",
                    self.item.github_number
                );
                ops.push(QueueOp::Remove);
            }
        }

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

    use crate::core::config::models::WorkflowConfig;
    use crate::infra::gh::mock::MockGh;

    // ─── Mock WorkspaceOps ───

    struct MockWorkspace {
        cloned: Mutex<Vec<(String, String)>>,
        worktrees: Mutex<Vec<(String, String, Option<String>)>>,
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
            branch: Option<&str>,
        ) -> anyhow::Result<PathBuf> {
            self.worktrees.lock().unwrap().push((
                repo_name.to_string(),
                task_id.to_string(),
                branch.map(|b| b.to_string()),
            ));
            Ok(PathBuf::from(format!("/mock/workspaces/{task_id}")))
        }

        async fn remove_worktree(&self, repo_name: &str, task_id: &str) -> anyhow::Result<()> {
            self.removed
                .lock()
                .unwrap()
                .push((repo_name.to_string(), task_id.to_string()));
            Ok(())
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

    fn make_task_with_issue(
        gh: Arc<MockGh>,
        ws: Arc<MockWorkspace>,
        item: IssueItem,
    ) -> ImplementTask {
        let cfg = Arc::new(MockConfigLoader);
        ImplementTask::new(ws, gh, cfg, item)
    }

    // ═══════════════════════════════════════════════
    // before_invoke tests
    // ═══════════════════════════════════════════════

    #[tokio::test]
    async fn before_skips_closed_issue() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "issues/42", ".state", "closed");

        let mut task = make_task(gh.clone());
        let result = task.before_invoke().await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SkipReason::PreflightFailed(ref msg) if msg.contains("is closed")
        ));
        // add-first: DONE added
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, n, l)| *n == 42 && l == labels::DONE));
        // IMPLEMENTING removed
        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed
            .iter()
            .any(|(_, n, l)| *n == 42 && l == labels::IMPLEMENTING));
    }

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
        assert_eq!(wts[0].2, Some("autodev/issue-42".to_string()));
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
    async fn after_pr_extract_fail_preserves_worktree_and_adds_failed_label() {
        let gh = Arc::new(MockGh::new());
        let ws = Arc::new(MockWorkspace::new());
        let cfg = Arc::new(MockConfigLoader);
        let mut task = ImplementTask::new(ws.clone(), gh.clone(), cfg, make_test_issue());
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

        // implementing label removed
        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed
            .iter()
            .any(|(_, n, l)| *n == 42 && l == labels::IMPLEMENTING));

        // impl-failed label added
        let added = gh.added_labels.lock().unwrap();
        assert!(added
            .iter()
            .any(|(_, n, l)| *n == 42 && l == labels::IMPL_FAILED));

        // Recovery comment posted
        let comments = gh.posted_comments.lock().unwrap();
        assert!(comments
            .iter()
            .any(|(_, n, body)| *n == 42 && body.contains("autodev:impl-failed")));

        // Worktree NOT cleaned up (preserved for manual recovery)
        let removed_wts = ws.removed.lock().unwrap();
        assert!(removed_wts.is_empty());
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

        // implementing label removed
        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed
            .iter()
            .any(|(_, n, l)| *n == 42 && l == labels::IMPLEMENTING));

        // impl-failed label added
        let added = gh.added_labels.lock().unwrap();
        assert!(added
            .iter()
            .any(|(_, n, l)| *n == 42 && l == labels::IMPL_FAILED));

        // failure comment posted
        let comments = gh.posted_comments.lock().unwrap();
        assert!(comments
            .iter()
            .any(|(_, n, body)| *n == 42 && body.contains("<!-- autodev:impl-failed -->")));
    }

    #[tokio::test]
    async fn after_uses_find_existing_pr_fallback() {
        let gh = Arc::new(MockGh::new());
        // Set up mock paginate response for find_existing_pr
        gh.set_paginate(
            "org/repo",
            "pulls",
            serde_json::to_vec(&serde_json::json!([{
                "number": 55,
                "head": {"ref": "autodev/issue-42"}
            }]))
            .unwrap(),
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

    // ═══════════════════════════════════════════════
    // find_existing_pr unit tests
    // ═══════════════════════════════════════════════

    #[tokio::test]
    async fn find_existing_pr_uses_owner_prefix_in_head_param() {
        let gh = MockGh::new();
        gh.set_paginate(
            "org/repo",
            "pulls",
            serde_json::to_vec(&serde_json::json!([{
                "number": 10,
                "head": {"ref": "autodev/issue-5"}
            }]))
            .unwrap(),
        );

        let result = find_existing_pr(&gh, "org/repo", "autodev/issue-5", None).await;
        assert_eq!(result, Some(10));

        // Verify params contain "owner:branch" format
        let calls = gh.paginate_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let head_param = calls[0]
            .2
            .iter()
            .find(|(k, _)| k == "head")
            .expect("head param should exist");
        assert_eq!(head_param.1, "org:autodev/issue-5");
    }

    #[tokio::test]
    async fn find_existing_pr_validates_head_branch() {
        let gh = MockGh::new();
        gh.set_paginate(
            "org/repo",
            "pulls",
            serde_json::to_vec(&serde_json::json!([{
                "number": 20,
                "head": {"ref": "autodev/issue-7"}
            }]))
            .unwrap(),
        );

        let result = find_existing_pr(&gh, "org/repo", "autodev/issue-7", None).await;
        assert_eq!(result, Some(20));
    }

    #[tokio::test]
    async fn find_existing_pr_rejects_mismatched_branch() {
        let gh = MockGh::new();
        // API returns PR with a different head branch
        gh.set_paginate(
            "org/repo",
            "pulls",
            serde_json::to_vec(&serde_json::json!([{
                "number": 75,
                "head": {"ref": "feature/scaffold-boilerplate"}
            }]))
            .unwrap(),
        );

        let result = find_existing_pr(&gh, "org/repo", "autodev/issue-131", None).await;
        assert_eq!(result, None, "should reject PR with mismatched head branch");
    }

    #[tokio::test]
    async fn find_existing_pr_returns_none_on_empty_response() {
        let gh = MockGh::new();
        gh.set_paginate(
            "org/repo",
            "pulls",
            serde_json::to_vec(&serde_json::json!([])).unwrap(),
        );

        let result = find_existing_pr(&gh, "org/repo", "autodev/issue-99", None).await;
        assert_eq!(result, None);
    }

    // ═══════════════════════════════════════════════
    // after_invoke: agent failure (exit_code != 0) tests
    // ═══════════════════════════════════════════════

    #[tokio::test]
    async fn after_nonzero_exit_adds_impl_failed_label() {
        let gh = Arc::new(MockGh::new());
        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 1,
            stdout: String::new(),
            stderr: "error".to_string(),
            duration: Duration::from_secs(10),
        };
        let result = task.after_invoke(response).await;

        assert!(matches!(result.status, TaskStatus::Failed(_)));

        let added = gh.added_labels.lock().unwrap();
        assert!(
            added
                .iter()
                .any(|(_, n, l)| *n == 42 && l == labels::IMPL_FAILED),
            "should add impl-failed label on agent failure"
        );
    }

    #[tokio::test]
    async fn after_nonzero_exit_posts_failure_comment() {
        let gh = Arc::new(MockGh::new());
        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 1,
            stdout: String::new(),
            stderr: "crash".to_string(),
            duration: Duration::from_secs(10),
        };
        let _ = task.after_invoke(response).await;

        let comments = gh.posted_comments.lock().unwrap();
        assert!(
            comments
                .iter()
                .any(|(_, n, body)| *n == 42 && body.contains("<!-- autodev:impl-failed -->")),
            "should post impl-failed comment marker"
        );
    }

    #[tokio::test]
    async fn after_nonzero_exit_comment_includes_exit_code() {
        let gh = Arc::new(MockGh::new());
        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 137,
            stdout: String::new(),
            stderr: "killed".to_string(),
            duration: Duration::from_secs(10),
        };
        let _ = task.after_invoke(response).await;

        let comments = gh.posted_comments.lock().unwrap();
        assert!(
            comments
                .iter()
                .any(|(_, n, body)| *n == 42 && body.contains("exit_code=137")),
            "comment should include the exit code"
        );
    }

    // ═══════════════════════════════════════════════
    // after_invoke: fallback PR rejection/acceptance tests
    // ═══════════════════════════════════════════════

    #[tokio::test]
    async fn after_fallback_rejects_wrong_branch_pr() {
        let gh = Arc::new(MockGh::new());
        // API returns a PR with a different branch (reproducing issue #218)
        gh.set_paginate(
            "org/repo",
            "pulls",
            serde_json::to_vec(&serde_json::json!([{
                "number": 75,
                "head": {"ref": "feature/scaffold-boilerplate"}
            }]))
            .unwrap(),
        );

        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 0,
            stdout: "Implementation complete.".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(30),
        };
        let result = task.after_invoke(response).await;

        // Should NOT push PR #75 to queue
        assert!(
            !result
                .queue_ops
                .iter()
                .any(|op| matches!(op, QueueOp::PushPr { .. })),
            "should not push mismatched PR to queue"
        );

        // Should add impl-failed label
        let added = gh.added_labels.lock().unwrap();
        assert!(
            added
                .iter()
                .any(|(_, n, l)| *n == 42 && l == labels::IMPL_FAILED),
            "should add impl-failed label when fallback PR is rejected"
        );
    }

    #[tokio::test]
    async fn after_fallback_accepts_correct_branch_pr() {
        let gh = Arc::new(MockGh::new());
        gh.set_paginate(
            "org/repo",
            "pulls",
            serde_json::to_vec(&serde_json::json!([{
                "number": 88,
                "head": {"ref": "autodev/issue-42"}
            }]))
            .unwrap(),
        );

        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 0,
            stdout: "Implementation complete.".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(30),
        };
        let result = task.after_invoke(response).await;

        // Should push PR #88 to queue
        assert!(
            result
                .queue_ops
                .iter()
                .any(|op| matches!(op, QueueOp::PushPr { item, .. } if item.github_number == 88)),
            "should push correct-branch PR to queue"
        );
    }

    // ═══════════════════════════════════════════════
    // Regression test: issue #218 exact reproduction
    // ═══════════════════════════════════════════════

    /// issue #218 재현 테스트.
    ///
    /// 시나리오 (실제 발생한 버그):
    ///   1. issue #131 "feat(testing): setup 시 .claude/rules에 테스트 관련 rules 설치 지원" 등록
    ///   2. implementing 단계 진입, agent가 exit_code=0으로 완료하지만 실제 PR을 생성하지 않음
    ///   3. extract_pr_number() → None (stdout에 PR URL 없음)
    ///   4. find_existing_pr() fallback → 관련 없는 PR #75 (feature/scaffold-boilerplate) 반환
    ///   5. PR #75를 issue에 잘못 링크 → 라벨 제거 → 파이프라인 이탈
    ///
    /// 기대 동작:
    ///   - find_existing_pr()가 head branch 불일치 PR을 거부
    ///   - IMPL_FAILED 라벨 추가 + 에러 코멘트 작성
    ///   - 잘못된 PR이 큐에 push되지 않음
    #[tokio::test]
    async fn regression_issue_218_wrong_pr_link_and_pipeline_escape() {
        let gh = Arc::new(MockGh::new());
        let ws = Arc::new(MockWorkspace::new());

        // issue #131 시뮬레이션
        let issue_131 = IssueItem {
            work_id: make_work_id("issue", "tosspayments/node-claude-code-plugin", 131),
            repo_id: "r1".to_string(),
            repo_name: "tosspayments/node-claude-code-plugin".to_string(),
            repo_url: "https://github.com/tosspayments/node-claude-code-plugin".to_string(),
            github_number: 131,
            title: "feat(testing): setup 시 .claude/rules에 테스트 관련 rules 설치 지원"
                .to_string(),
            body: Some("테스트 rules 설치 지원".to_string()),
            labels: vec!["autodev:implementing".to_string()],
            author: "user".to_string(),
            analysis_report: None,
            gh_host: None,
        };

        // GitHub API가 관련 없는 PR #75를 반환하는 상황 재현
        gh.set_paginate(
            "tosspayments/node-claude-code-plugin",
            "pulls",
            serde_json::to_vec(&serde_json::json!([{
                "number": 75,
                "title": "feat(stack-installer): add boilerplate scaffolding",
                "head": {"ref": "feature/scaffold-boilerplate"}
            }]))
            .unwrap(),
        );

        let mut task = make_task_with_issue(gh.clone(), ws.clone(), issue_131);
        let _ = task.before_invoke().await;

        // Agent가 exit_code=0이지만 실제 PR을 생성하지 않은 상황
        let response = AgentResponse {
            exit_code: 0,
            stdout: "Implementation complete. All changes have been committed.".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(120),
        };
        let result = task.after_invoke(response).await;

        // 검증 1: 잘못된 PR #75가 큐에 push되지 않아야 함
        assert!(
            !result
                .queue_ops
                .iter()
                .any(|op| matches!(op, QueueOp::PushPr { .. })),
            "must NOT link unrelated PR #75 to issue #131"
        );

        // 검증 2: autodev:pr-link:75 코멘트가 작성되지 않아야 함
        let comments = gh.posted_comments.lock().unwrap();
        assert!(
            !comments
                .iter()
                .any(|(_, _, body)| body.contains("autodev:pr-link:75")),
            "must NOT post pr-link comment for unrelated PR #75"
        );

        // 검증 3: IMPL_FAILED 라벨이 추가되어야 함 (파이프라인 이탈 방지)
        let added = gh.added_labels.lock().unwrap();
        assert!(
            added
                .iter()
                .any(|(_, n, l)| *n == 131 && l == labels::IMPL_FAILED),
            "must add impl-failed label to prevent pipeline escape"
        );

        // 검증 4: impl-failed 코멘트가 작성되어야 함
        assert!(
            comments
                .iter()
                .any(|(_, n, body)| *n == 131 && body.contains("<!-- autodev:impl-failed -->")),
            "must post impl-failed comment for recovery"
        );

        // 검증 5: find_existing_pr가 owner:branch 형식으로 호출되었는지
        let calls = gh.paginate_calls.lock().unwrap();
        let head_param = calls[0]
            .2
            .iter()
            .find(|(k, _)| k == "head")
            .expect("head param should exist");
        assert_eq!(
            head_param.1, "tosspayments:autodev/issue-131",
            "must use owner:branch format in GitHub API head parameter"
        );
    }

    // ═══════════════════════════════════════════════
    // DESIGN-v3: label add-first 순서 검증
    // ═══════════════════════════════════════════════

    #[tokio::test]
    async fn before_closed_issue_adds_done_before_removing_implementing() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "issues/42", ".state", "closed");

        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        gh.assert_add_before_remove(42, labels::DONE, labels::IMPLEMENTING);
    }

    #[tokio::test]
    async fn after_no_pr_adds_impl_failed_before_removing_implementing() {
        let gh = Arc::new(MockGh::new());
        let ws = Arc::new(MockWorkspace::new());
        let cfg = Arc::new(MockConfigLoader);
        let mut task = ImplementTask::new(ws, gh.clone(), cfg, make_test_issue());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 0,
            stdout: "Done but no PR URL".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(30),
        };
        let _ = task.after_invoke(response).await;

        gh.assert_add_before_remove(42, labels::IMPL_FAILED, labels::IMPLEMENTING);
    }

    #[tokio::test]
    async fn after_nonzero_exit_uses_add_first_ordering() {
        let gh = Arc::new(MockGh::new());
        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 1,
            stdout: String::new(),
            stderr: "agent crashed".to_string(),
            duration: Duration::from_secs(5),
        };
        let _ = task.after_invoke(response).await;

        gh.assert_add_before_remove(42, labels::IMPL_FAILED, labels::IMPLEMENTING);
    }

    // ═══════════════════════════════════════════════
    // existing tests (continued)
    // ═══════════════════════════════════════════════

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
