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
use crate::core::config::ConfigLoader;
use crate::core::labels;
use crate::core::models::{NewConsumerLog, QueuePhase, QueueType};
use crate::core::phase::TaskKind;
use crate::core::queue_item::QueueItem;
use crate::core::task::{
    AgentRequest, AgentResponse, QueueOp, SkipReason, Task, TaskResult, TaskStatus,
};
use crate::infra::claude::output::{self, ReviewVerdict};
use crate::infra::claude::SessionOptions;
use crate::infra::gh::Gh;
use crate::tasks::helpers::workspace::WorkspaceOps;

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
    item: QueueItem,
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
        item: QueueItem,
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
                // source issue done 전이 (add-first)
                if let Some(issue_num) = self.item.source_issue_number() {
                    self.gh
                        .label_add(&self.item.repo_name, issue_num, labels::DONE, gh_host)
                        .await;
                    self.gh
                        .label_remove(
                            &self.item.repo_name,
                            issue_num,
                            labels::IMPLEMENTING,
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
                self.gh
                    .label_remove(
                        &self.item.repo_name,
                        self.item.github_number,
                        labels::WIP,
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

        // 레포별 config
        let repo_cfg = self.config.load(Some(&wt_path));
        let pr_prompt = format!("[autodev] review: PR #{}", self.item.github_number);
        let resolved = super::workflow_resolver::resolve_workflow_prompt(
            &repo_cfg.workflows.review.as_stage(),
            super::workflow_resolver::TaskType::Review,
        );
        let system_prompt = format!("{AGENT_SYSTEM_PROMPT}\n\n{resolved}");

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
            queue_type: QueueType::Pr.to_string(),
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
            // add-first: REVIEW_FAILED 추가 후 WIP 제거
            self.gh
                .label_add(
                    &self.item.repo_name,
                    self.item.github_number,
                    labels::REVIEW_FAILED,
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

            let fail_comment = format!(
                "<!-- autodev:review-failed -->\n\
                 ⚠️ Review agent failed (exit_code={}).\n\n\
                 **Branch**: `{}`\n\
                 Check the agent logs for details.",
                response.exit_code,
                self.item.head_branch().unwrap_or("")
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

                // Source issue → done (add-first)
                if let Some(issue_num) = self.item.source_issue_number() {
                    self.gh
                        .label_add(&self.item.repo_name, issue_num, labels::DONE, gh_host)
                        .await;
                    self.gh
                        .label_remove(
                            &self.item.repo_name,
                            issue_num,
                            labels::IMPLEMENTING,
                            gh_host,
                        )
                        .await;
                }

                // PR → done (add-first)
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
                        labels::WIP,
                        gh_host,
                    )
                    .await;
                // iteration 라벨 정리
                if self.item.review_iteration().unwrap_or(0) > 0 {
                    self.gh
                        .label_remove(
                            &self.item.repo_name,
                            self.item.github_number,
                            &labels::iteration_label(self.item.review_iteration().unwrap_or(0)),
                            gh_host,
                        )
                        .await;
                }

                ops.push(QueueOp::Remove);
                // Knowledge extraction은 scan_done_merged()가 merge 후 트리거
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

                // wip → changes-requested 라벨 전이 (add-first)
                self.gh
                    .label_add(
                        &self.item.repo_name,
                        self.item.github_number,
                        labels::CHANGES_REQUESTED,
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

                // 외부 PR (source_issue 없음): 리뷰 댓글만, 자동수정 안함
                if self.item.source_issue_number().is_none() {
                    ops.push(QueueOp::Remove);
                } else {
                    // Max iterations 확인 (re-review일 때만)
                    let max_iterations = cfg.workflows.review.max_iterations;
                    if self.item.review_iteration().unwrap_or(0) >= max_iterations {
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
                        // add-first: add SKIP before removing old labels
                        self.gh
                            .label_add(
                                &self.item.repo_name,
                                self.item.github_number,
                                labels::SKIP,
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
                            .label_remove(
                                &self.item.repo_name,
                                self.item.github_number,
                                labels::CHANGES_REQUESTED,
                                gh_host,
                            )
                            .await;
                        if self.item.review_iteration().unwrap_or(0) > 0 {
                            self.gh
                                .label_remove(
                                    &self.item.repo_name,
                                    self.item.github_number,
                                    &labels::iteration_label(
                                        self.item.review_iteration().unwrap_or(0),
                                    ),
                                    gh_host,
                                )
                                .await;
                        }
                        ops.push(QueueOp::Remove);
                    } else {
                        let mut next_item = self.item.clone();
                        next_item.set_review_comment(Some(review_text));
                        // In unified model, ReviewDone → Pending with TaskKind::Improve
                        next_item.task_kind = TaskKind::Improve;
                        ops.push(QueueOp::Remove);
                        ops.push(QueueOp::Push {
                            phase: QueuePhase::Pending,
                            item: Box::new(next_item),
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
    use std::time::Duration;

    use crate::core::config::models::WorkflowConfig;
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

    struct MockConfigLoader;
    impl ConfigLoader for MockConfigLoader {
        fn load(&self, _: Option<&Path>) -> WorkflowConfig {
            WorkflowConfig::default()
        }
    }

    fn make_test_pr(source_issue: Option<i64>) -> QueueItem {
        test_pr_with_source(10, TaskKind::Review, source_issue, 0)
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
    async fn after_approve_does_not_push_extracting() {
        // Knowledge extraction is now triggered by scan_done_merged, not ReviewTask
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "open");

        let mut task = make_task(gh.clone(), Some(42));
        let _ = task.before_invoke().await;

        let result = task.after_invoke(make_approve_response()).await;

        assert!(matches!(result.status, TaskStatus::Completed));
        // Only Remove, no Push
        assert!(result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::Remove)));
        assert!(!result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::Push { .. })));
        // PR marked done
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, n, l)| *n == 10 && l == labels::DONE));
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
            |op| matches!(op, QueueOp::Push { phase, item } if *phase == QueuePhase::Pending && item.review_comment().is_some())
        ));

        let reviews = gh.reviewed_prs.lock().unwrap();
        assert!(reviews
            .iter()
            .any(|(_, n, event, _)| *n == 10 && event == "REQUEST_CHANGES"));
    }

    #[tokio::test]
    async fn after_external_pr_request_changes_marks_changes_requested() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "open");

        // No source_issue — external PR
        let mut task = make_task(gh.clone(), None);
        let _ = task.before_invoke().await;

        let result = task.after_invoke(make_request_changes_response()).await;

        // Should NOT push to review_done (external PR: no auto-fix)
        assert!(!result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::Push { .. })));

        // wip → changes-requested 전이
        let added = gh.added_labels.lock().unwrap();
        assert!(added
            .iter()
            .any(|(_, n, l)| *n == 10 && l == labels::CHANGES_REQUESTED));
        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed.iter().any(|(_, n, l)| *n == 10 && l == labels::WIP));
    }

    #[tokio::test]
    async fn after_max_iterations_marks_skip() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "open");

        let pr = test_pr_with_source(10, TaskKind::Review, Some(42), 3);
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
            .any(|op| matches!(op, QueueOp::Push { .. })));

        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, n, l)| *n == 10 && l == labels::SKIP));

        // changes-requested should be removed when skip is applied
        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed
            .iter()
            .any(|(_, n, l)| *n == 10 && l == labels::CHANGES_REQUESTED));
    }

    // ═══════════════════════════════════════════════
    // DESIGN-v3: label add-first 순서 검증
    // ═══════════════════════════════════════════════

    #[tokio::test]
    async fn before_closed_pr_adds_done_before_removing_wip() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "closed");

        let mut task = make_task(gh.clone(), Some(42));
        let _ = task.before_invoke().await;

        // PR: done before wip removal
        gh.assert_add_before_remove(10, labels::DONE, labels::WIP);
        // Source issue: done before implementing removal
        gh.assert_add_before_remove(42, labels::DONE, labels::IMPLEMENTING);
    }

    #[tokio::test]
    async fn after_approve_adds_done_before_removing_wip() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "open");

        let mut task = make_task(gh.clone(), Some(42));
        let _ = task.before_invoke().await;
        let _ = task.after_invoke(make_approve_response()).await;

        // PR: done before wip removal
        gh.assert_add_before_remove(10, labels::DONE, labels::WIP);
        // Source issue: done before implementing removal
        gh.assert_add_before_remove(42, labels::DONE, labels::IMPLEMENTING);
    }

    #[tokio::test]
    async fn after_request_changes_adds_changes_requested_before_removing_wip() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "open");

        // external PR — triggers changes-requested label transition
        let mut task = make_task(gh.clone(), None);
        let _ = task.before_invoke().await;
        let _ = task.after_invoke(make_request_changes_response()).await;

        gh.assert_add_before_remove(10, labels::CHANGES_REQUESTED, labels::WIP);
    }

    #[tokio::test]
    async fn after_max_iterations_adds_skip_before_removing_changes_requested() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "pulls/10", ".state", "open");

        let pr = test_pr_with_source(10, TaskKind::Review, Some(42), 3);
        let mut task = ReviewTask::new(
            Arc::new(MockWorkspace),
            gh.clone(),
            Arc::new(MockConfigLoader),
            pr,
        );
        let _ = task.before_invoke().await;
        let _ = task.after_invoke(make_request_changes_response()).await;

        // max_iterations 경로에서는 wip→changes-requested 전이 후
        // skip 추가 → changes-requested 제거 순서로 진행.
        gh.assert_add_before_remove(10, labels::SKIP, labels::CHANGES_REQUESTED);
    }

    // ═══════════════════════════════════════════════
    // after_invoke: agent failure (exit_code != 0) tests
    // ═══════════════════════════════════════════════

    #[tokio::test]
    async fn after_nonzero_exit_adds_review_failed_label() {
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

        let added = gh.added_labels.lock().unwrap();
        assert!(
            added
                .iter()
                .any(|(_, n, l)| *n == 10 && l == labels::REVIEW_FAILED),
            "should add review-failed label on agent failure"
        );

        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed.iter().any(|(_, n, l)| *n == 10 && l == labels::WIP));
    }

    #[tokio::test]
    async fn after_nonzero_exit_uses_add_first_ordering() {
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
        let _ = task.after_invoke(response).await;

        gh.assert_add_before_remove(10, labels::REVIEW_FAILED, labels::WIP);
    }

    #[tokio::test]
    async fn after_nonzero_exit_posts_failure_comment() {
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
        let _ = task.after_invoke(response).await;

        let comments = gh.posted_comments.lock().unwrap();
        assert!(
            comments
                .iter()
                .any(|(_, n, body)| *n == 10 && body.contains("<!-- autodev:review-failed -->")),
            "should post review-failed comment with HTML marker"
        );
    }
}
