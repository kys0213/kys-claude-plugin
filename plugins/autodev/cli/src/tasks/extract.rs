//! ExtractTask — Knowledge Extraction Task 구현체.
//!
//! PR 승인 후 완료된 task의 세션을 분석하여 knowledge 개선 제안을 추출한다.
//! ReviewTask approve → PushPr(EXTRACTING) → ExtractTask 순서로 실행.
//!
//! before_invoke: worktree(head_branch) → suggest-workflow 조회 → 기존 지식 수집 → 프롬프트
//! after_invoke: 결과 파싱 → 코멘트 게시 → knowledge PR 생성 → Remove
//!
//! Best-effort: extraction 실패해도 PR은 이미 done 상태이므로 메인 플로우에 영향 없음.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use super::AGENT_SYSTEM_PROMPT;
use crate::components::workspace::{Workspace, WorkspaceOps};
use crate::config::{ConfigLoader, Env};
use crate::daemon::task::{
    AgentRequest, AgentResponse, QueueOp, SkipReason, Task, TaskResult, TaskStatus,
};
use crate::domain::models::NewConsumerLog;
use crate::infrastructure::claude::SessionOptions;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::infrastructure::suggest_workflow::SuggestWorkflow;
use crate::knowledge::extractor::{
    build_suggest_workflow_section, collect_existing_knowledge, create_task_knowledge_prs,
    format_knowledge_comment, parse_knowledge_suggestion,
};
use crate::queue::task_queues::PrItem;

/// Knowledge Extraction Task.
///
/// PR 리뷰 승인 후 해당 작업에서 학습할 수 있는 개선 사항을 추출한다.
/// 제안이 있으면 GitHub 코멘트로 게시하고, 제안별 actionable PR을 생성한다.
pub struct ExtractTask {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    #[allow(dead_code)]
    config: Arc<dyn ConfigLoader>,
    sw: Arc<dyn SuggestWorkflow>,
    git: Arc<dyn Git>,
    env: Arc<dyn Env>,
    item: PrItem,
    worker_id: String,
    task_id: String,
    wt_path: Option<PathBuf>,
    started_at: Option<String>,
}

impl ExtractTask {
    pub fn new(
        workspace: Arc<dyn WorkspaceOps>,
        gh: Arc<dyn Gh>,
        config: Arc<dyn ConfigLoader>,
        sw: Arc<dyn SuggestWorkflow>,
        git: Arc<dyn Git>,
        env: Arc<dyn Env>,
        item: PrItem,
    ) -> Self {
        let task_id = format!("extract-pr-{}", item.github_number);
        Self {
            workspace,
            gh,
            config,
            sw,
            git,
            env,
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
impl Task for ExtractTask {
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

        let task_type = "pr";
        let number = self.item.github_number;

        // suggest-workflow 도구 사용 패턴 조회 (best-effort)
        let sw_section = build_suggest_workflow_section(&*self.sw, task_type, number).await;

        // 기존 knowledge 수집 (delta check)
        let existing = collect_existing_knowledge(&wt_path);
        let delta_section = if existing.is_empty() {
            String::new()
        } else {
            format!(
                "\n\n--- Existing Knowledge Base ---\n\
                 The following knowledge already exists in this repository. \
                 Do NOT suggest anything that is already covered below. \
                 Only suggest genuinely NEW improvements.\n\n{existing}"
            )
        };

        let prompt = format!(
            "[autodev] knowledge: per-task {task_type} #{number}\n\n\
             Analyze the completed {task_type} task (#{number}) in this workspace. \
             Review the changes made, any issues encountered, and lessons learned.\
             {sw_section}{delta_section}\n\n\
             Respond with a JSON object matching this schema:\n\
             {{\n  \"suggestions\": [\n    {{\n      \
             \"type\": \"rule | claude_md | hook | skill | subagent\",\n      \
             \"target_file\": \".claude/rules/...\",\n      \
             \"content\": \"specific recommendation\",\n      \
             \"reason\": \"why this matters\"\n    }}\n  ]\n}}\n\n\
             Only include suggestions if there are genuine improvements to propose. \
             If none, return {{\"suggestions\": []}}."
        );

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
        let task_type = "pr";
        let number = self.item.github_number;

        let started = self
            .started_at
            .take()
            .unwrap_or_else(|| Utc::now().to_rfc3339());
        let finished = Utc::now().to_rfc3339();

        let log = NewConsumerLog {
            repo_id: self.item.repo_id.clone(),
            queue_type: "knowledge".to_string(),
            queue_item_id: self.item.work_id.clone(),
            worker_id: self.worker_id.clone(),
            command: format!("[autodev] knowledge: per-task {task_type} #{number}"),
            stdout: response.stdout.clone(),
            stderr: response.stderr.clone(),
            exit_code: response.exit_code,
            started_at: started,
            finished_at: finished,
            duration_ms: response.duration.as_millis() as i64,
        };

        // Best-effort: extraction 실패해도 task는 completed로 처리
        if response.exit_code == 0 {
            let suggestion = parse_knowledge_suggestion(&response.stdout)
                .filter(|ks| !ks.suggestions.is_empty());

            if let Some(ref ks) = suggestion {
                // GitHub 코멘트 게시
                let comment = format_knowledge_comment(ks, task_type, number);
                self.gh
                    .issue_comment(&self.item.repo_name, number, &comment, gh_host)
                    .await;

                // per-suggestion actionable PR 생성
                let ws = Workspace::new(&*self.git, &*self.env);
                create_task_knowledge_prs(
                    &*self.gh,
                    &ws,
                    &self.item.repo_name,
                    ks,
                    task_type,
                    number,
                    gh_host,
                )
                .await;
            }
        } else {
            tracing::warn!(
                "knowledge extraction exited with {} for {task_type} #{number}",
                response.exit_code
            );
        }

        self.cleanup_worktree().await;

        TaskResult {
            work_id: self.item.work_id.clone(),
            repo_name: self.item.repo_name.clone(),
            queue_ops: vec![QueueOp::Remove],
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

    use crate::config::models::WorkflowConfig;
    use crate::infrastructure::gh::mock::MockGh;
    use crate::infrastructure::suggest_workflow::SuggestWorkflow;
    use crate::knowledge::models::ToolFrequencyEntry;
    use crate::queue::task_queues::make_work_id;

    // ─── Mock Workspace ───

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

    // ─── Mock ConfigLoader ───

    struct MockConfigLoader;
    impl ConfigLoader for MockConfigLoader {
        fn load(&self, _: Option<&Path>) -> WorkflowConfig {
            WorkflowConfig::default()
        }
    }

    // ─── Mock SuggestWorkflow ───

    struct MockSuggestWorkflow {
        entries: Vec<ToolFrequencyEntry>,
    }

    impl MockSuggestWorkflow {
        fn empty() -> Self {
            Self { entries: vec![] }
        }

        fn with_entries(entries: Vec<ToolFrequencyEntry>) -> Self {
            Self { entries }
        }
    }

    #[async_trait]
    impl SuggestWorkflow for MockSuggestWorkflow {
        async fn query_tool_frequency(
            &self,
            _session_filter: Option<&str>,
        ) -> anyhow::Result<Vec<ToolFrequencyEntry>> {
            Ok(self.entries.clone())
        }
        async fn query_filtered_sessions(
            &self,
            _: &str,
            _: Option<&str>,
            _: Option<u32>,
        ) -> anyhow::Result<Vec<crate::knowledge::models::SessionEntry>> {
            Ok(vec![])
        }
        async fn query_repetition(
            &self,
            _: Option<&str>,
        ) -> anyhow::Result<Vec<crate::knowledge::models::RepetitionEntry>> {
            Ok(vec![])
        }
    }

    // ─── Mock Git ───

    struct MockGit;

    #[async_trait]
    impl Git for MockGit {
        async fn clone(&self, _: &str, _: &Path) -> anyhow::Result<()> {
            Ok(())
        }
        async fn pull_ff_only(&self, _: &Path) -> anyhow::Result<bool> {
            Ok(true)
        }
        async fn worktree_add(&self, _: &Path, _: &Path, _: Option<&str>) -> anyhow::Result<()> {
            Ok(())
        }
        async fn worktree_remove(&self, _: &Path, _: &Path) -> anyhow::Result<()> {
            Ok(())
        }
        async fn checkout_new_branch(&self, _: &Path, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn add_commit_push(
            &self,
            _: &Path,
            _: &[&str],
            _: &str,
            _: &str,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    // ─── Mock Env ───

    struct MockEnv;
    impl Env for MockEnv {
        fn var(&self, key: &str) -> Result<String, std::env::VarError> {
            match key {
                "AUTODEV_HOME" => Ok("/tmp/autodev-test".to_string()),
                _ => Err(std::env::VarError::NotPresent),
            }
        }
    }

    // ─── Helpers ───

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
            review_comment: None,
            source_issue_number: Some(42),
            review_iteration: 1,
            gh_host: None,
        }
    }

    fn make_task(gh: Arc<MockGh>, sw: Arc<dyn SuggestWorkflow>) -> ExtractTask {
        ExtractTask::new(
            Arc::new(MockWorkspace),
            gh,
            Arc::new(MockConfigLoader),
            sw,
            Arc::new(MockGit),
            Arc::new(MockEnv),
            make_test_pr(),
        )
    }

    // ─── Tests ───

    #[tokio::test]
    async fn before_invoke_builds_extraction_prompt() {
        let gh = Arc::new(MockGh::new());
        let sw: Arc<dyn SuggestWorkflow> = Arc::new(MockSuggestWorkflow::empty());

        let mut task = make_task(gh, sw);
        let request = task.before_invoke().await.expect("should succeed");

        assert!(request.prompt.contains("knowledge: per-task pr #10"));
        assert!(request.prompt.contains("suggestions"));
        assert_eq!(request.working_dir, PathBuf::from("/mock/extract-pr-10"));
    }

    #[tokio::test]
    async fn before_invoke_includes_sw_section_when_available() {
        let gh = Arc::new(MockGh::new());
        let sw: Arc<dyn SuggestWorkflow> = Arc::new(MockSuggestWorkflow::with_entries(vec![
            ToolFrequencyEntry {
                tool: "bash:test".to_string(),
                frequency: 12,
                sessions: 1,
            },
        ]));

        let mut task = make_task(gh, sw);
        let request = task.before_invoke().await.expect("should succeed");

        assert!(request.prompt.contains("suggest-workflow session data"));
        assert!(request.prompt.contains("bash:test"));
    }

    #[tokio::test]
    async fn after_invoke_posts_comment_on_suggestions() {
        let gh = Arc::new(MockGh::new());
        let sw: Arc<dyn SuggestWorkflow> = Arc::new(MockSuggestWorkflow::empty());

        let mut task = make_task(gh.clone(), sw);
        let _ = task.before_invoke().await;

        let json = r#"{"suggestions":[{"type":"rule","target_file":".claude/rules/test.md","content":"Always run tests","reason":"Tests caught 3 bugs"}]}"#;
        let response = AgentResponse {
            exit_code: 0,
            stdout: json.to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(5),
        };

        let result = task.after_invoke(response).await;

        assert!(matches!(result.status, TaskStatus::Completed));
        assert!(result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::Remove)));

        // Comment posted
        let comments = gh.posted_comments.lock().unwrap();
        assert!(comments
            .iter()
            .any(|(_, n, body)| *n == 10 && body.contains("autodev:knowledge")));
    }

    #[tokio::test]
    async fn after_invoke_no_comment_on_empty_suggestions() {
        let gh = Arc::new(MockGh::new());
        let sw: Arc<dyn SuggestWorkflow> = Arc::new(MockSuggestWorkflow::empty());

        let mut task = make_task(gh.clone(), sw);
        let _ = task.before_invoke().await;

        let json = r#"{"suggestions":[]}"#;
        let response = AgentResponse {
            exit_code: 0,
            stdout: json.to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(5),
        };

        let result = task.after_invoke(response).await;

        assert!(matches!(result.status, TaskStatus::Completed));
        // No comment posted
        let comments = gh.posted_comments.lock().unwrap();
        assert!(comments.is_empty());
    }

    #[tokio::test]
    async fn after_invoke_handles_agent_failure_gracefully() {
        let gh = Arc::new(MockGh::new());
        let sw: Arc<dyn SuggestWorkflow> = Arc::new(MockSuggestWorkflow::empty());

        let mut task = make_task(gh.clone(), sw);
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 1,
            stdout: String::new(),
            stderr: "timeout".to_string(),
            duration: Duration::from_secs(60),
        };

        let result = task.after_invoke(response).await;

        // Still completes (best-effort)
        assert!(matches!(result.status, TaskStatus::Completed));
        assert!(result
            .queue_ops
            .iter()
            .any(|op| matches!(op, QueueOp::Remove)));

        // No comment posted
        let comments = gh.posted_comments.lock().unwrap();
        assert!(comments.is_empty());
    }

    #[tokio::test]
    async fn after_invoke_logs_with_knowledge_queue_type() {
        let gh = Arc::new(MockGh::new());
        let sw: Arc<dyn SuggestWorkflow> = Arc::new(MockSuggestWorkflow::empty());

        let mut task = make_task(gh, sw);
        let _ = task.before_invoke().await;

        let response = AgentResponse {
            exit_code: 0,
            stdout: r#"{"suggestions":[]}"#.to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(2),
        };

        let result = task.after_invoke(response).await;

        assert_eq!(result.logs.len(), 1);
        assert_eq!(result.logs[0].queue_type, "knowledge");
        assert!(result.logs[0]
            .command
            .contains("knowledge: per-task pr #10"));
    }
}
