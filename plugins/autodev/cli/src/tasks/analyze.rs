//! AnalyzeTask — 이슈 분석 Task 구현체.
//!
//! 기존 `pipeline::issue::analyze_one()`의 로직을 Task trait으로 재구성한다.
//! before_invoke: preflight(issue open?) → worktree 생성 → 분석 프롬프트 구성
//! after_invoke: verdict 파싱 → 라벨/코멘트 → QueueOp 생성

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use super::AGENT_SYSTEM_PROMPT;
use crate::components::verdict;
use crate::components::workspace::WorkspaceOps;
use crate::config::ConfigLoader;
use crate::daemon::task::{
    AgentRequest, AgentResponse, QueueOp, SkipReason, Task, TaskResult, TaskStatus,
};
use crate::domain::labels;
use crate::domain::models::NewConsumerLog;
use crate::infrastructure::claude::output::{self, AnalysisResult};
use crate::infrastructure::claude::SessionOptions;
use crate::infrastructure::gh::Gh;
use crate::queue::task_queues::IssueItem;

// ─── 분석 프롬프트 (JSON 응답 스키마 명시) ───

const ANALYSIS_PROMPT_TEMPLATE: &str = r#"Analyze the following GitHub issue and respond in JSON.

Issue #{number}: {title}

{body}

Respond with this exact JSON schema:
{{
  "verdict": "implement" | "needs_clarification" | "wontfix",
  "confidence": 0.0-1.0,
  "summary": "1-2 sentence summary of the issue",
  "questions": ["question1", ...],
  "reason": "reason if wontfix, null otherwise",
  "report": "full markdown analysis report with: affected files, implementation direction, checkpoints, risks"
}}

Rules:
- verdict "implement": the issue is clear enough to implement
- verdict "needs_clarification": the issue is ambiguous or missing critical details
- verdict "wontfix": the issue should not be implemented (duplicate, out of scope, invalid)
- confidence: how confident you are in the verdict (0.0 = no confidence, 1.0 = fully confident)
- questions: list of clarifying questions (required when verdict is "needs_clarification")
- reason: explanation (required when verdict is "wontfix")
- report: detailed analysis regardless of verdict"#;

/// 이슈 분석 Task.
///
/// `before_invoke`에서 이슈가 open인지 확인하고 worktree를 준비한 뒤,
/// 분석 프롬프트를 구성하여 `AgentRequest`를 반환한다.
/// `after_invoke`에서 분석 결과를 파싱하여 verdict에 따라 라벨/코멘트를 처리한다.
pub struct AnalyzeTask {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
    item: IssueItem,
    worker_id: String,
    task_id: String,
    wt_path: Option<PathBuf>,
    started_at: Option<String>,
}

impl AnalyzeTask {
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

    fn gh_host(&self) -> Option<&str> {
        self.item.gh_host.as_deref()
    }

    /// verdict 파싱 후 라벨/코멘트 처리하여 QueueOp 반환
    async fn handle_analysis(
        &self,
        analysis: &AnalysisResult,
        confidence_threshold: f64,
    ) -> Vec<QueueOp> {
        let gh_host = self.gh_host();

        if analysis.verdict == output::Verdict::Wontfix {
            let comment = verdict::format_wontfix_comment(analysis);
            self.gh
                .issue_comment(
                    &self.item.repo_name,
                    self.item.github_number,
                    &comment,
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
            tracing::info!("issue #{} → wontfix (skip)", self.item.github_number);
            return vec![QueueOp::Remove];
        }

        if analysis.verdict == output::Verdict::NeedsClarification
            || analysis.confidence < confidence_threshold
        {
            let comment = verdict::format_clarification_comment(analysis);
            self.gh
                .issue_comment(
                    &self.item.repo_name,
                    self.item.github_number,
                    &comment,
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
            tracing::info!(
                "issue #{} → skip (verdict={}, confidence={:.2})",
                self.item.github_number,
                analysis.verdict,
                analysis.confidence
            );
            return vec![QueueOp::Remove];
        }

        // implement verdict → analyzed 라벨 (HITL 게이트)
        let comment = verdict::format_analysis_comment(analysis);
        self.gh
            .issue_comment(
                &self.item.repo_name,
                self.item.github_number,
                &comment,
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
                labels::ANALYZED,
                gh_host,
            )
            .await;
        tracing::info!(
            "issue #{}: Analyzing → analyzed (awaiting human review, confidence={:.2})",
            self.item.github_number,
            analysis.confidence
        );
        vec![QueueOp::Remove]
    }

    /// 파싱 실패 시 fallback: 분석 결과를 raw text로 코멘트
    async fn handle_fallback(&self, stdout: &str) -> Vec<QueueOp> {
        let gh_host = self.gh_host();
        let report = output::parse_output(stdout);
        let comment = format!(
            "<!-- autodev:analysis -->\n\
             ## Autodev Analysis Report\n\n\
             {report}\n\n\
             ---\n\
             > 이 분석을 승인하려면 `autodev:approved-analysis` 라벨을 추가하세요.\n\
             > 수정이 필요하면 코멘트로 피드백을 남기고 `autodev:analyzed` 라벨을 제거하세요."
        );
        self.gh
            .issue_comment(
                &self.item.repo_name,
                self.item.github_number,
                &comment,
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
                labels::ANALYZED,
                gh_host,
            )
            .await;
        tracing::warn!(
            "issue #{}: analysis output not parseable, fallback → analyzed",
            self.item.github_number
        );
        vec![QueueOp::Remove]
    }

    async fn cleanup_worktree(&self) {
        let _ = self
            .workspace
            .remove_worktree(&self.item.repo_name, &self.task_id)
            .await;
    }
}

#[async_trait]
impl Task for AnalyzeTask {
    fn work_id(&self) -> &str {
        &self.item.work_id
    }

    fn repo_name(&self) -> &str {
        &self.item.repo_name
    }

    async fn before_invoke(&mut self) -> Result<AgentRequest, SkipReason> {
        let gh_host = self.gh_host();

        // Preflight: issue가 아직 open인지 확인
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

        // 프롬프트 구성
        let body_text = self.item.body.as_deref().unwrap_or("");
        let prompt = format!(
            "[autodev] analyze: issue #{} - {}\n\n{}",
            self.item.github_number,
            self.item.title,
            ANALYSIS_PROMPT_TEMPLATE
                .replace("{number}", &self.item.github_number.to_string())
                .replace("{title}", &self.item.title)
                .replace("{body}", body_text),
        );

        self.started_at = Some(Utc::now().to_rfc3339());

        Ok(AgentRequest {
            working_dir: wt_path,
            prompt,
            session_opts: SessionOptions {
                output_format: Some("json".into()),
                json_schema: Some(output::ANALYSIS_SCHEMA.clone()),
                append_system_prompt: Some(AGENT_SYSTEM_PROMPT.to_string()),
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
            queue_type: "issue".to_string(),
            queue_item_id: self.item.work_id.clone(),
            worker_id: self.worker_id.clone(),
            command: format!(
                "claude -p \"Analyze issue #{}...\"",
                self.item.github_number
            ),
            stdout: response.stdout.clone(),
            stderr: response.stderr.clone(),
            exit_code: response.exit_code,
            started_at: started,
            finished_at: finished,
            duration_ms: response.duration.as_millis() as i64,
        };

        // Agent 호출 실패 (exit_code != 0)
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
                status: TaskStatus::Failed(format!("analysis exit_code={}", response.exit_code)),
            };
        }

        // stdout 파싱
        let analysis = output::parse_analysis(&response.stdout);
        let ops = match analysis {
            Some(ref a) => {
                self.handle_analysis(a, cfg.consumer.confidence_threshold)
                    .await
            }
            None => self.handle_fallback(&response.stdout).await,
        };

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

    fn make_task(gh: Arc<MockGh>) -> AnalyzeTask {
        let ws = Arc::new(MockWorkspace::new());
        let cfg = Arc::new(MockConfigLoader);
        AnalyzeTask::new(ws, gh, cfg, make_test_issue())
    }

    fn make_implement_response() -> AgentResponse {
        let analysis = r#"{"verdict":"implement","confidence":0.9,"summary":"Clear bug","questions":[],"reason":null,"report":"Fix the login handler"}"#;
        let envelope = format!(
            r#"{{"result": {}}}"#,
            serde_json::to_string(analysis).unwrap()
        );
        AgentResponse {
            exit_code: 0,
            stdout: envelope,
            stderr: String::new(),
            duration: Duration::from_secs(5),
        }
    }

    fn make_wontfix_response() -> AgentResponse {
        let analysis = r#"{"verdict":"wontfix","confidence":0.95,"summary":"Duplicate","questions":[],"reason":"Already fixed in #10","report":""}"#;
        let envelope = format!(
            r#"{{"result": {}}}"#,
            serde_json::to_string(analysis).unwrap()
        );
        AgentResponse {
            exit_code: 0,
            stdout: envelope,
            stderr: String::new(),
            duration: Duration::from_secs(3),
        }
    }

    fn make_clarify_response() -> AgentResponse {
        let analysis = r#"{"verdict":"needs_clarification","confidence":0.4,"summary":"Unclear","questions":["Which API?"],"reason":null,"report":""}"#;
        let envelope = format!(
            r#"{{"result": {}}}"#,
            serde_json::to_string(analysis).unwrap()
        );
        AgentResponse {
            exit_code: 0,
            stdout: envelope,
            stderr: String::new(),
            duration: Duration::from_secs(3),
        }
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
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, _, l)| l == labels::DONE));
    }

    #[tokio::test]
    async fn before_creates_worktree_and_returns_request() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "issues/42", ".state", "open");

        let ws = Arc::new(MockWorkspace::new());
        let cfg = Arc::new(MockConfigLoader);
        let mut task = AnalyzeTask::new(ws.clone(), gh, cfg, make_test_issue());

        let request = task.before_invoke().await.expect("should succeed");

        assert!(request.prompt.contains("Fix login bug"));
        assert!(request.prompt.contains("Users cannot log in"));
        assert_eq!(request.session_opts.output_format.as_deref(), Some("json"));
        assert!(request.session_opts.json_schema.is_some());

        let cloned = ws.cloned.lock().unwrap();
        assert_eq!(cloned.len(), 1);
        let wts = ws.worktrees.lock().unwrap();
        assert_eq!(wts.len(), 1);
        assert_eq!(wts[0].1, "issue-42");
    }

    #[tokio::test]
    async fn before_skips_on_clone_failure() {
        struct FailCloneWorkspace;

        #[async_trait]
        impl WorkspaceOps for FailCloneWorkspace {
            async fn ensure_cloned(&self, _url: &str, _name: &str) -> anyhow::Result<PathBuf> {
                Err(anyhow::anyhow!("network error"))
            }
            async fn create_worktree(
                &self,
                _: &str,
                _: &str,
                _: Option<&str>,
            ) -> anyhow::Result<PathBuf> {
                unreachable!()
            }
            async fn remove_worktree(&self, _: &str, _: &str) -> anyhow::Result<()> {
                Ok(())
            }
            fn repo_base_path(&self, _: &str) -> PathBuf {
                PathBuf::new()
            }
            fn worktree_path(&self, _: &str, _: &str) -> PathBuf {
                PathBuf::new()
            }
        }

        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "issues/42", ".state", "open");

        let mut task = AnalyzeTask::new(
            Arc::new(FailCloneWorkspace),
            gh,
            Arc::new(MockConfigLoader),
            make_test_issue(),
        );

        let result = task.before_invoke().await;
        assert!(result.is_err());
        match result.unwrap_err() {
            SkipReason::PreflightFailed(msg) => assert!(msg.contains("clone failed")),
            _ => panic!("expected PreflightFailed"),
        }
    }

    // ═══════════════════════════════════════════════
    // after_invoke tests
    // ═══════════════════════════════════════════════

    #[tokio::test]
    async fn after_implement_verdict_posts_comment_and_marks_analyzed() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "issues/42", ".state", "open");

        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let result = task.after_invoke(make_implement_response()).await;

        assert!(matches!(result.status, TaskStatus::Completed));
        assert!(result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::Remove)));
        assert_eq!(result.logs.len(), 1);

        let comments = gh.posted_comments.lock().unwrap();
        assert_eq!(comments.len(), 1);
        assert!(comments[0].2.contains("autodev:analysis"));

        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, _, l)| l == labels::ANALYZED));

        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed.iter().any(|(_, _, l)| l == labels::WIP));
    }

    #[tokio::test]
    async fn after_wontfix_verdict_marks_skip() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "issues/42", ".state", "open");

        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let result = task.after_invoke(make_wontfix_response()).await;

        assert!(matches!(result.status, TaskStatus::Completed));
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, _, l)| l == labels::SKIP));

        let comments = gh.posted_comments.lock().unwrap();
        assert!(comments[0].2.contains("autodev:wontfix"));
    }

    #[tokio::test]
    async fn after_clarify_verdict_marks_skip() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "issues/42", ".state", "open");

        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let result = task.after_invoke(make_clarify_response()).await;

        assert!(matches!(result.status, TaskStatus::Completed));
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, _, l)| l == labels::SKIP));

        let comments = gh.posted_comments.lock().unwrap();
        assert!(comments[0].2.contains("autodev:waiting"));
    }

    #[tokio::test]
    async fn after_low_confidence_marks_skip() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "issues/42", ".state", "open");

        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        // implement verdict but very low confidence (default threshold = 0.7)
        let analysis = r#"{"verdict":"implement","confidence":0.3,"summary":"Maybe","questions":[],"reason":null,"report":"unclear"}"#;
        let envelope = format!(
            r#"{{"result": {}}}"#,
            serde_json::to_string(analysis).unwrap()
        );
        let response = AgentResponse {
            exit_code: 0,
            stdout: envelope,
            stderr: String::new(),
            duration: Duration::from_secs(3),
        };

        let _result = task.after_invoke(response).await;
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, _, l)| l == labels::SKIP));
    }

    #[tokio::test]
    async fn after_parse_failure_fallback_analyzed() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "issues/42", ".state", "open");

        let mut task = make_task(gh.clone());
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 0,
            stdout: "Not valid JSON at all".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(3),
        };

        let result = task.after_invoke(response).await;

        assert!(matches!(result.status, TaskStatus::Completed));
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, _, l)| l == labels::ANALYZED));

        let comments = gh.posted_comments.lock().unwrap();
        assert!(comments[0].2.contains("autodev:analysis"));
    }

    #[tokio::test]
    async fn after_nonzero_exit_fails_and_removes() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "issues/42", ".state", "open");

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
        assert_eq!(result.logs.len(), 1);

        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed.iter().any(|(_, _, l)| l == labels::WIP));
    }

    #[tokio::test]
    async fn after_cleans_up_worktree() {
        let gh = Arc::new(MockGh::new());
        gh.set_field("org/repo", "issues/42", ".state", "open");

        let ws = Arc::new(MockWorkspace::new());
        let cfg = Arc::new(MockConfigLoader);
        let mut task = AnalyzeTask::new(ws.clone(), gh, cfg, make_test_issue());
        let _ = task.before_invoke().await;

        let _ = task.after_invoke(make_implement_response()).await;

        let removed = ws.removed.lock().unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].1, "issue-42");
    }
}
