//! ReviewTask — PR 리뷰 Task 구현체.
//!
//! 기존 `pipeline::pr::review_one()` + `re_review_one()`의 로직을 통합하여
//! Task trait으로 재구성한다. review_iteration에 따라 initial/re-review를 구분.
//! before_invoke: preflight(PR reviewable?) → worktree(head_branch) → 리뷰 프롬프트
//! after_invoke: verdict 파싱 → approve/request_changes 처리

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
use crate::infrastructure::claude::output::{self, ReviewVerdict};
use crate::infrastructure::claude::SessionOptions;
use crate::infrastructure::gh::Gh;
use crate::queue::task_queues::{pr_phase, PrItem};

/// PR 리뷰 결과를 GitHub 댓글로 포맷
fn format_review_comment(review: &str, pr_number: i64, verdict: Option<&ReviewVerdict>) -> String {
    let verdict_label = match verdict {
        Some(ReviewVerdict::Approve) => " — **Approved**",
        Some(ReviewVerdict::RequestChanges) => " — **Changes Requested**",
        None => "",
    };
    format!(
        "<!-- autodev:review -->\n\
         ## Autodev Code Review (PR #{pr_number}){verdict_label}\n\n\
         {review}"
    )
}

/// PR 리뷰 Task (initial + re-review 통합).
///
/// `review_iteration == 0`이면 initial review, `>= 1`이면 re-review.
pub struct ReviewTask {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
    item: PrItem,
    worker_id: String,
    task_id: String,
    wt_path: Option<PathBuf>,
    started_at: Option<String>,
}

impl ReviewTask {
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
impl Task for ReviewTask {
    fn work_id(&self) -> &str {
        &self.item.work_id
    }

    fn repo_name(&self) -> &str {
        &self.item.repo_name
    }

    async fn before_invoke(&mut self) -> Result<AgentRequest, SkipReason> {
        let gh_host = self.item.gh_host.as_deref();

        // Preflight: PR이 리뷰 대상인지 확인 (open + no APPROVED)
        let state = self
            .gh
            .api_get_field(
                &self.item.repo_name,
                &format!("pulls/{}", self.item.github_number),
                ".state",
                gh_host,
            )
            .await;
        if let Some(ref s) = state {
            if s != "open" {
                // source issue done 전이
                if let Some(issue_num) = self.item.source_issue_number {
                    self.gh
                        .label_remove(
                            &self.item.repo_name,
                            issue_num,
                            labels::IMPLEMENTING,
                            gh_host,
                        )
                        .await;
                    self.gh
                        .label_add(&self.item.repo_name, issue_num, labels::DONE, gh_host)
                        .await;
                }
                self.gh
                    .label_remove(
                        &self.item.repo_name,
                        self.item.github_number,
                        labels::WIP,
                        gh_host,
                    )
                    .await;
                self.gh
                    .label_add(
                        &self.item.repo_name,
                        self.item.github_number,
                        labels::DONE,
                        gh_host,
                    )
                    .await;
                return Err(SkipReason::PreflightFailed(format!(
                    "PR #{} is closed",
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

        // 레포별 config
        let repo_cfg = self.config.load(Some(&wt_path));
        let pr_prompt = format!("[autodev] review: PR #{}", self.item.github_number);
        let system_prompt = format!("{AGENT_SYSTEM_PROMPT}\n\n{}", repo_cfg.workflow.pr);

        self.started_at = Some(Utc::now().to_rfc3339());

        Ok(AgentRequest {
            working_dir: wt_path,
            prompt: pr_prompt,
            session_opts: SessionOptions {
                output_format: Some("json".into()),
                json_schema: Some(output::REVIEW_SCHEMA.clone()),
                append_system_prompt: Some(system_prompt),
            },
        })
    }

    async fn after_invoke(&mut self, response: AgentResponse) -> TaskResult {
        let cfg = self.config.load(self.wt_path.as_deref());
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
            command: format!("[autodev] review: PR #{}", self.item.github_number),
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
                    labels::WIP,
                    gh_host,
                )
                .await;
            self.cleanup_worktree().await;
            return TaskResult {
                work_id: self.item.work_id.clone(),
                repo_name: self.item.repo_name.clone(),
                queue_ops: vec![QueueOp::Remove],
                logs: vec![log],
                status: TaskStatus::Failed(format!("review exit_code={}", response.exit_code)),
            };
        }

        let review_result = output::parse_review(&response.stdout);
        let (review_text, verdict) = match review_result {
            Some(ref r) => (r.summary.clone(), Some(r.verdict.clone())),
            None => (output::parse_output(&response.stdout), None),
        };

        let mut ops = Vec::new();

        match verdict.as_ref() {
            Some(ReviewVerdict::Approve) => {
                // GitHub Review API: APPROVE
                self.gh
                    .pr_review(
                        &self.item.repo_name,
                        self.item.github_number,
                        "APPROVE",
                        &review_text,
                        gh_host,
                    )
                    .await;

                // Review comment
                let comment = format_review_comment(
                    &review_text,
                    self.item.github_number,
                    Some(&ReviewVerdict::Approve),
                );
                self.gh
                    .issue_comment(
                        &self.item.repo_name,
                        self.item.github_number,
                        &comment,
                        gh_host,
                    )
                    .await;

                // Source issue → done
                if let Some(issue_num) = self.item.source_issue_number {
                    self.gh
                        .label_remove(
                            &self.item.repo_name,
                            issue_num,
                            labels::IMPLEMENTING,
                            gh_host,
                        )
                        .await;
                    self.gh
                        .label_add(&self.item.repo_name, issue_num, labels::DONE, gh_host)
                        .await;
                }

                // PR → done
                self.gh
                    .label_remove(
                        &self.item.repo_name,
                        self.item.github_number,
                        labels::WIP,
                        gh_host,
                    )
                    .await;
                // iteration 라벨 정리
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
                self.gh
                    .label_add(
                        &self.item.repo_name,
                        self.item.github_number,
                        labels::DONE,
                        gh_host,
                    )
                    .await;

                ops.push(QueueOp::Remove);

                // Knowledge extraction (best-effort, config gated)
                if cfg.consumer.knowledge_extraction {
                    ops.push(QueueOp::PushPr {
                        phase: pr_phase::EXTRACTING,
                        item: self.item.clone(),
                    });
                }
            }
            Some(ReviewVerdict::RequestChanges) | None => {
                // GitHub Review API: REQUEST_CHANGES
                if matches!(verdict, Some(ReviewVerdict::RequestChanges)) {
                    self.gh
                        .pr_review(
                            &self.item.repo_name,
                            self.item.github_number,
                            "REQUEST_CHANGES",
                            &review_text,
                            gh_host,
                        )
                        .await;
                }

                let comment =
                    format_review_comment(&review_text, self.item.github_number, verdict.as_ref());
                self.gh
                    .issue_comment(
                        &self.item.repo_name,
                        self.item.github_number,
                        &comment,
                        gh_host,
                    )
                    .await;

                // 외부 PR (source_issue 없음): 리뷰 댓글만, 자동수정 안함
                if self.item.source_issue_number.is_none() {
                    self.gh
                        .label_remove(
                            &self.item.repo_name,
                            self.item.github_number,
                            labels::WIP,
                            gh_host,
                        )
                        .await;
                    self.gh
                        .label_add(
                            &self.item.repo_name,
                            self.item.github_number,
                            labels::DONE,
                            gh_host,
                        )
                        .await;
                    ops.push(QueueOp::Remove);
                } else {
                    // Max iterations 확인 (re-review일 때만)
                    let max_iterations = cfg.develop.review.max_iterations;
                    if self.item.review_iteration >= max_iterations {
                        let limit_comment = format!(
                            "<!-- autodev:skip -->\n\
                             ## Autodev: Review iteration limit reached\n\n\
                             Reached maximum review iterations ({max_iterations}). \
                             Marking as `autodev:skip`. Manual intervention required."
                        );
                        self.gh
                            .issue_comment(
                                &self.item.repo_name,
                                self.item.github_number,
                                &limit_comment,
                                gh_host,
                            )
                            .await;
                        self.gh
                            .label_remove(
                                &self.item.repo_name,
                                self.item.github_number,
                                labels::WIP,
                                gh_host,
                            )
                            .await;
                        self.gh
                            .label_add(
                                &self.item.repo_name,
                                self.item.github_number,
                                labels::SKIP,
                                gh_host,
                            )
                            .await;
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
                        ops.push(QueueOp::Remove);
                    } else {
                        let mut next_item = self.item.clone();
                        next_item.review_comment = Some(review_text);
                        ops.push(QueueOp::Remove);
                        ops.push(QueueOp::PushPr {
                            phase: pr_phase::REVIEW_DONE,
                            item: next_item,
                        });
                    }
                }
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

    fn make_test_pr(source_issue: Option<i64>) -> PrItem {
        PrItem {
            work_id: make_work_id("pr", "org/repo", 10),
            repo_id: "r1".to_string(),
            repo_name: "org/repo".to_string(),
            repo_url: "https://github.com/org/repo".to_string(),
            github_number: 10,
            title: "Fix bug".to_string(),
            head_branch: "autodev/issue-42".to_string(),
            base_branch: "main".to_string(),
            review_comment: None,
            source_issue_number: source_issue,
            review_iteration: 0,
            gh_host: None,
        }
    }

    fn make_task(gh: Arc<MockGh>, source_issue: Option<i64>) -> ReviewTask {
        ReviewTask::new(
            Arc::new(MockWorkspace),
            gh,
            Arc::new(MockConfigLoader),
            make_test_pr(source_issue),
        )
    }

    fn make_approve_response() -> AgentResponse {
        let review = r#"{"verdict":"approve","summary":"LGTM"}"#;
        let envelope = format!(
            r#"{{"result": {}}}"#,
            serde_json::to_string(review).unwrap()
        );
        AgentResponse {
            exit_code: 0,
            stdout: envelope,
            stderr: String::new(),
            duration: Duration::from_secs(10),
        }
    }

    fn make_request_changes_response() -> AgentResponse {
        let review = r#"{"verdict":"request_changes","summary":"Fix error handling"}"#;
        let envelope = format!(
            r#"{{"result": {}}}"#,
            serde_json::to_string(review).unwrap()
        );
        AgentResponse {
            exit_code: 0,
            stdout: envelope,
            stderr: String::new(),
            duration: Duration::from_secs(10),
        }
    }

    #[tokio::test]
    async fn before_skips_closed_pr() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "closed");

        let mut task = make_task(gh.clone(), Some(42));
        let result = task.before_invoke().await;

        assert!(result.is_err());
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, n, l)| *n == 10 && l == labels::DONE));
        // Source issue should also be marked done
        assert!(added.iter().any(|(_, n, l)| *n == 42 && l == labels::DONE));
    }

    #[tokio::test]
    async fn before_creates_worktree_with_pr_branch() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "open");

        let mut task = make_task(gh, None);
        let request = task.before_invoke().await.expect("should succeed");

        assert!(request.prompt.contains("review: PR #10"));
        assert_eq!(request.session_opts.output_format.as_deref(), Some("json"));
    }

    #[tokio::test]
    async fn after_approve_transitions_to_done() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "open");

        let mut task = make_task(gh.clone(), Some(42));
        let _ = task.before_invoke().await;

        let result = task.after_invoke(make_approve_response()).await;

        assert!(matches!(result.status, TaskStatus::Completed));
        assert!(result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::Remove)));

        // PR marked done
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, n, l)| *n == 10 && l == labels::DONE));
        // Source issue marked done
        assert!(added.iter().any(|(_, n, l)| *n == 42 && l == labels::DONE));
        // APPROVE review posted
        let reviews = gh.reviewed_prs.lock().unwrap();
        assert!(reviews
            .iter()
            .any(|(_, n, event, _)| *n == 10 && event == "APPROVE"));
    }

    #[tokio::test]
    async fn after_request_changes_pushes_to_review_done() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "open");

        let mut task = make_task(gh.clone(), Some(42));
        let _ = task.before_invoke().await;

        let result = task.after_invoke(make_request_changes_response()).await;

        assert!(matches!(result.status, TaskStatus::Completed));
        assert!(result.queue_ops.iter().any(
            |op| matches!(op, QueueOp::PushPr { phase, item } if *phase == pr_phase::REVIEW_DONE && item.review_comment.is_some())
        ));

        let reviews = gh.reviewed_prs.lock().unwrap();
        assert!(reviews
            .iter()
            .any(|(_, n, event, _)| *n == 10 && event == "REQUEST_CHANGES"));
    }

    #[tokio::test]
    async fn after_external_pr_request_changes_marks_done() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "open");

        // No source_issue — external PR
        let mut task = make_task(gh.clone(), None);
        let _ = task.before_invoke().await;

        let result = task.after_invoke(make_request_changes_response()).await;

        // Should NOT push to review_done
        assert!(!result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::PushPr { .. })));

        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, n, l)| *n == 10 && l == labels::DONE));
    }

    #[tokio::test]
    async fn after_max_iterations_marks_skip() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "open");

        let mut pr = make_test_pr(Some(42));
        pr.review_iteration = 3; // default max is 3
        let mut task = ReviewTask::new(
            Arc::new(MockWorkspace),
            gh.clone(),
            Arc::new(MockConfigLoader),
            pr,
        );
        let _ = task.before_invoke().await;

        let result = task.after_invoke(make_request_changes_response()).await;

        assert!(result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::Remove)));
        assert!(!result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::PushPr { .. })));

        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, n, l)| *n == 10 && l == labels::SKIP));
    }

    #[tokio::test]
    async fn after_nonzero_exit_removes() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "open");

        let mut task = make_task(gh.clone(), None);
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 1,
            stdout: String::new(),
            stderr: "error".to_string(),
            duration: Duration::from_secs(5),
        };

        let result = task.after_invoke(response).await;

        assert!(matches!(result.status, TaskStatus::Failed(_)));
        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed.iter().any(|(_, n, l)| *n == 10 && l == labels::WIP));
    }
}
