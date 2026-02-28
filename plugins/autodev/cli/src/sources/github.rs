//! GitHubTaskSource — GitHub 이슈/PR 스캔 기반 TaskSource 구현체.
//!
//! daemon의 per-repo 큐, 스캔, 복구 로직을 캡슐화한다.
//! `poll()`: repo sync → recovery → scan → queue drain → Task 생성
//! `apply()`: TaskResult의 QueueOp를 per-repo 큐에 적용

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::components::workspace::WorkspaceOps;
use crate::config::{self, ConfigLoader, Env};
use crate::daemon::task::{QueueOp, Task, TaskResult};
use crate::daemon::task_source::TaskSource;
use crate::domain::git_repository::GitRepository;
use crate::domain::git_repository_factory::GitRepositoryFactory;
use crate::domain::repository::{RepoRepository, ScanCursorRepository};
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::infrastructure::suggest_workflow::SuggestWorkflow;
use crate::queue::task_queues::{issue_phase, merge_phase, pr_phase};
use crate::tasks::analyze::AnalyzeTask;
use crate::tasks::extract::ExtractTask;
use crate::tasks::implement::ImplementTask;
use crate::tasks::improve::ImproveTask;
use crate::tasks::merge::MergeTask;
use crate::tasks::review::ReviewTask;

/// GitHub 이슈/PR 스캔 기반 TaskSource.
///
/// per-repo 큐를 소유하고, 스캔 → Task 생성 → 큐 적용 생명주기를 관리한다.
pub struct GitHubTaskSource<DB: RepoRepository + ScanCursorRepository> {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
    env: Arc<dyn Env>,
    git: Arc<dyn Git>,
    sw: Arc<dyn SuggestWorkflow>,
    db: DB,
    repos: HashMap<String, GitRepository>,
}

impl<DB: RepoRepository + ScanCursorRepository + Send> GitHubTaskSource<DB> {
    pub fn new(
        workspace: Arc<dyn WorkspaceOps>,
        gh: Arc<dyn Gh>,
        config: Arc<dyn ConfigLoader>,
        env: Arc<dyn Env>,
        git: Arc<dyn Git>,
        sw: Arc<dyn SuggestWorkflow>,
        db: DB,
    ) -> Self {
        Self {
            workspace,
            gh,
            config,
            env,
            git,
            sw,
            db,
            repos: HashMap::new(),
        }
    }

    /// 외부에서 repos를 주입한다 (startup reconcile 후 사용).
    pub fn set_repos(&mut self, repos: HashMap<String, GitRepository>) {
        self.repos = repos;
    }

    /// 상태 조회용: repos 참조 반환 (status heartbeat에서 사용).
    pub fn repos(&self) -> &HashMap<String, GitRepository> {
        &self.repos
    }

    /// DB에서 enabled repos를 동기화한다 (추가/제거).
    async fn sync_repos(&mut self) {
        let enabled = match self.db.repo_find_enabled() {
            Ok(e) => e,
            Err(e) => {
                tracing::error!("repo_find_enabled failed: {e}");
                return;
            }
        };

        // 새 레포 추가
        for er in &enabled {
            if !self.repos.contains_key(&er.name) {
                let git_repo = GitRepositoryFactory::create(er, &*self.env, &*self.gh).await;
                self.repos.insert(er.name.clone(), git_repo);
                tracing::info!("added new repo: {}", er.name);
            }
        }

        // 비활성 레포 제거
        let enabled_names: std::collections::HashSet<&str> =
            enabled.iter().map(|r| r.name.as_str()).collect();
        let to_remove: Vec<String> = self
            .repos
            .keys()
            .filter(|k| !enabled_names.contains(k.as_str()))
            .cloned()
            .collect();
        for name in to_remove {
            self.repos.remove(&name);
            tracing::info!("removed disabled repo: {name}");
        }
    }

    /// per-repo refresh + orphan recovery.
    async fn run_recovery(&mut self) {
        for repo in self.repos.values_mut() {
            repo.refresh(&*self.gh).await;
            let n = repo.recover_orphan_wip(&*self.gh).await;
            if n > 0 {
                tracing::info!("recovered {n} orphan wip items in {}", repo.name());
            }
            let n = repo.recover_orphan_implementing(&*self.gh).await;
            if n > 0 {
                tracing::info!("recovered {n} orphan implementing items in {}", repo.name());
            }
        }
    }

    /// per-repo config 기반 스캔.
    async fn run_scans(&mut self) {
        let repo_names: Vec<String> = self.repos.keys().cloned().collect();

        for repo_name in &repo_names {
            let ws_path =
                config::workspaces_path(&*self.env).join(config::sanitize_repo_name(repo_name));
            let repo_cfg = config::loader::load_merged(
                &*self.env,
                if ws_path.exists() {
                    Some(ws_path.as_path())
                } else {
                    None
                },
            );

            let repo = match self.repos.get(repo_name) {
                Some(r) => r,
                None => continue,
            };

            let should_scan = self
                .db
                .cursor_should_scan(repo.id(), repo_cfg.consumer.scan_interval_secs as i64)
                .unwrap_or(false);
            if !should_scan {
                continue;
            }

            tracing::info!("scanning {}...", repo_name);

            let repo = match self.repos.get_mut(repo_name) {
                Some(r) => r,
                None => continue,
            };

            for target in &repo_cfg.consumer.scan_targets {
                match target.as_str() {
                    "issues" => {
                        if let Err(e) = repo
                            .scan_issues(
                                &*self.gh,
                                &self.db,
                                &repo_cfg.consumer.ignore_authors,
                                &repo_cfg.consumer.filter_labels,
                            )
                            .await
                        {
                            tracing::error!("issue scan error for {repo_name}: {e}");
                        }

                        if let Err(e) = repo.scan_approved_issues(&*self.gh).await {
                            tracing::error!("approved scan error for {repo_name}: {e}");
                        }
                    }
                    "pulls" => {
                        if let Err(e) = repo
                            .scan_pulls(&*self.gh, &self.db, &repo_cfg.consumer.ignore_authors)
                            .await
                        {
                            tracing::error!("PR scan error for {repo_name}: {e}");
                        }
                    }
                    "merges" => {
                        if repo_cfg.consumer.auto_merge {
                            if let Err(e) = repo.scan_merges(&*self.gh).await {
                                tracing::error!("merge scan error for {repo_name}: {e}");
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// 모든 repo의 큐에서 ready 아이템을 pop → working phase 전이 → Task 생성.
    fn drain_queue_items(&mut self) -> Vec<Box<dyn Task>> {
        let mut tasks: Vec<Box<dyn Task>> = Vec::new();

        for repo in self.repos.values_mut() {
            // Issue: Pending → Analyzing
            while let Some(item) = repo.issue_queue.pop(issue_phase::PENDING) {
                repo.issue_queue.push(issue_phase::ANALYZING, item.clone());
                tracing::debug!("issue #{}: creating AnalyzeTask", item.github_number);
                tasks.push(Box::new(AnalyzeTask::new(
                    Arc::clone(&self.workspace),
                    Arc::clone(&self.gh),
                    Arc::clone(&self.config),
                    item,
                )));
            }

            // Issue: Ready → Implementing
            while let Some(item) = repo.issue_queue.pop(issue_phase::READY) {
                repo.issue_queue
                    .push(issue_phase::IMPLEMENTING, item.clone());
                tracing::debug!("issue #{}: creating ImplementTask", item.github_number);
                tasks.push(Box::new(ImplementTask::new(
                    Arc::clone(&self.workspace),
                    Arc::clone(&self.gh),
                    Arc::clone(&self.config),
                    item,
                )));
            }

            // PR: Pending → Reviewing
            while let Some(item) = repo.pr_queue.pop(pr_phase::PENDING) {
                repo.pr_queue.push(pr_phase::REVIEWING, item.clone());
                tracing::debug!("PR #{}: creating ReviewTask", item.github_number);
                tasks.push(Box::new(ReviewTask::new(
                    Arc::clone(&self.workspace),
                    Arc::clone(&self.gh),
                    Arc::clone(&self.config),
                    item,
                )));
            }

            // PR: ReviewDone → Improving
            while let Some(item) = repo.pr_queue.pop(pr_phase::REVIEW_DONE) {
                repo.pr_queue.push(pr_phase::IMPROVING, item.clone());
                tracing::debug!("PR #{}: creating ImproveTask", item.github_number);
                tasks.push(Box::new(ImproveTask::new(
                    Arc::clone(&self.workspace),
                    Arc::clone(&self.gh),
                    Arc::clone(&self.config),
                    item,
                )));
            }

            // PR: Improved → Reviewing (re-review)
            while let Some(item) = repo.pr_queue.pop(pr_phase::IMPROVED) {
                repo.pr_queue.push(pr_phase::REVIEWING, item.clone());
                tracing::debug!(
                    "PR #{}: creating ReviewTask (re-review)",
                    item.github_number
                );
                tasks.push(Box::new(ReviewTask::new(
                    Arc::clone(&self.workspace),
                    Arc::clone(&self.gh),
                    Arc::clone(&self.config),
                    item,
                )));
            }

            // PR: Extracting → knowledge extraction (best-effort)
            while let Some(item) = repo.pr_queue.pop(pr_phase::EXTRACTING) {
                tracing::debug!(
                    "PR #{}: creating ExtractTask (knowledge)",
                    item.github_number
                );
                tasks.push(Box::new(ExtractTask::new(
                    Arc::clone(&self.workspace),
                    Arc::clone(&self.gh),
                    Arc::clone(&self.config),
                    Arc::clone(&self.sw),
                    Arc::clone(&self.git),
                    Arc::clone(&self.env),
                    item,
                )));
            }

            // Merge: Pending → Merging
            while let Some(item) = repo.merge_queue.pop(merge_phase::PENDING) {
                repo.merge_queue.push(merge_phase::MERGING, item.clone());
                tracing::debug!("merge PR #{}: creating MergeTask", item.pr_number);
                tasks.push(Box::new(MergeTask::new(
                    Arc::clone(&self.workspace),
                    Arc::clone(&self.gh),
                    Arc::clone(&self.config),
                    item,
                )));
            }
        }

        tasks
    }

    /// TaskResult의 QueueOp를 해당 repo의 per-repo 큐에 적용한다.
    fn apply_queue_ops(&mut self, result: &TaskResult) {
        let repo = match self.repos.get_mut(&result.repo_name) {
            Some(r) => r,
            None => {
                tracing::warn!(
                    "task output for unknown repo {}: {}",
                    result.repo_name,
                    result.work_id
                );
                return;
            }
        };

        for op in &result.queue_ops {
            match op {
                QueueOp::Remove => {
                    repo.issue_queue.remove(&result.work_id);
                    repo.pr_queue.remove(&result.work_id);
                    repo.merge_queue.remove(&result.work_id);
                }
                QueueOp::PushPr { phase, item } => {
                    repo.pr_queue.push(phase, *item.clone());
                }
            }
        }
    }
}

#[async_trait(?Send)]
impl<DB: RepoRepository + ScanCursorRepository + Send> TaskSource for GitHubTaskSource<DB> {
    async fn poll(&mut self) -> Vec<Box<dyn Task>> {
        self.sync_repos().await;
        self.run_recovery().await;
        self.run_scans().await;
        self.drain_queue_items()
    }

    fn apply(&mut self, result: &TaskResult) {
        self.apply_queue_ops(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::WorkflowConfig;
    use crate::domain::models::EnabledRepo;
    use crate::infrastructure::gh::mock::MockGh;
    use crate::infrastructure::git::Git;
    use crate::infrastructure::suggest_workflow::SuggestWorkflow;
    use crate::knowledge::models::{RepetitionEntry, SessionEntry, ToolFrequencyEntry};
    use crate::queue::task_queues::{make_work_id, IssueItem, MergeItem, PrItem};
    use std::path::{Path, PathBuf};

    // ─── Mock dependencies ───

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

    struct MockEnv;
    impl Env for MockEnv {
        fn var(&self, key: &str) -> Result<String, std::env::VarError> {
            match key {
                "AUTODEV_HOME" => Ok("/tmp/autodev-test".to_string()),
                _ => Err(std::env::VarError::NotPresent),
            }
        }
    }

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

    struct MockSuggestWorkflow;

    #[async_trait]
    impl SuggestWorkflow for MockSuggestWorkflow {
        async fn query_tool_frequency(
            &self,
            _: Option<&str>,
        ) -> anyhow::Result<Vec<ToolFrequencyEntry>> {
            Ok(vec![])
        }
        async fn query_filtered_sessions(
            &self,
            _: &str,
            _: Option<&str>,
            _: Option<u32>,
        ) -> anyhow::Result<Vec<SessionEntry>> {
            Ok(vec![])
        }
        async fn query_repetition(&self, _: Option<&str>) -> anyhow::Result<Vec<RepetitionEntry>> {
            Ok(vec![])
        }
    }

    /// Minimal DB mock for tests — only provides repo_find_enabled.
    struct MockDb {
        repos: Vec<EnabledRepo>,
    }

    impl MockDb {
        fn empty() -> Self {
            Self { repos: vec![] }
        }
    }

    impl RepoRepository for MockDb {
        fn repo_add(&self, _: &str, _: &str) -> anyhow::Result<String> {
            Ok("r1".to_string())
        }
        fn repo_remove(&self, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn repo_list(&self) -> anyhow::Result<Vec<crate::domain::models::RepoInfo>> {
            Ok(vec![])
        }
        fn repo_find_enabled(&self) -> anyhow::Result<Vec<EnabledRepo>> {
            Ok(self.repos.clone())
        }
        fn repo_status_summary(&self) -> anyhow::Result<Vec<crate::domain::models::RepoStatusRow>> {
            Ok(vec![])
        }
    }

    impl ScanCursorRepository for MockDb {
        fn cursor_get_last_seen(&self, _: &str, _: &str) -> anyhow::Result<Option<String>> {
            Ok(None)
        }
        fn cursor_upsert(&self, _: &str, _: &str, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn cursor_should_scan(&self, _: &str, _: i64) -> anyhow::Result<bool> {
            Ok(false)
        }
    }

    fn make_source(gh: Arc<MockGh>) -> GitHubTaskSource<MockDb> {
        GitHubTaskSource::new(
            Arc::new(MockWorkspace),
            gh,
            Arc::new(MockConfigLoader),
            Arc::new(MockEnv),
            Arc::new(MockGit),
            Arc::new(MockSuggestWorkflow),
            MockDb::empty(),
        )
    }

    fn make_test_issue(repo_name: &str, number: i64) -> IssueItem {
        IssueItem {
            work_id: make_work_id("issue", repo_name, number),
            repo_id: "r1".to_string(),
            repo_name: repo_name.to_string(),
            repo_url: format!("https://github.com/{repo_name}"),
            github_number: number,
            title: format!("Issue #{number}"),
            body: None,
            labels: vec![],
            author: "user".to_string(),
            analysis_report: None,
            gh_host: None,
        }
    }

    fn make_test_pr(repo_name: &str, number: i64) -> PrItem {
        PrItem {
            work_id: make_work_id("pr", repo_name, number),
            repo_id: "r1".to_string(),
            repo_name: repo_name.to_string(),
            repo_url: format!("https://github.com/{repo_name}"),
            github_number: number,
            title: format!("PR #{number}"),
            head_branch: "feat".to_string(),
            base_branch: "main".to_string(),
            review_comment: None,
            source_issue_number: None,
            review_iteration: 0,
            gh_host: None,
        }
    }

    fn make_test_merge(repo_name: &str, number: i64) -> MergeItem {
        MergeItem {
            work_id: make_work_id("merge", repo_name, number),
            repo_id: "r1".to_string(),
            repo_name: repo_name.to_string(),
            repo_url: format!("https://github.com/{repo_name}"),
            pr_number: number,
            title: format!("PR #{number}"),
            head_branch: "feat".to_string(),
            base_branch: "main".to_string(),
            gh_host: None,
        }
    }

    // ─── drain_queue_items tests ───

    #[test]
    fn drain_creates_analyze_task_from_pending_issue() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.issue_queue
            .push(issue_phase::PENDING, make_test_issue("org/repo", 1));
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].work_id(), "issue:org/repo:1");

        // Item should be moved to ANALYZING
        assert!(source.repos["org/repo"]
            .issue_queue
            .contains("issue:org/repo:1"));
    }

    #[test]
    fn drain_creates_implement_task_from_ready_issue() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.issue_queue
            .push(issue_phase::READY, make_test_issue("org/repo", 2));
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].work_id(), "issue:org/repo:2");
    }

    #[test]
    fn drain_creates_review_task_from_pending_pr() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.pr_queue
            .push(pr_phase::PENDING, make_test_pr("org/repo", 10));
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].work_id(), "pr:org/repo:10");
    }

    #[test]
    fn drain_creates_improve_task_from_review_done_pr() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.pr_queue
            .push(pr_phase::REVIEW_DONE, make_test_pr("org/repo", 10));
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].work_id(), "pr:org/repo:10");
    }

    #[test]
    fn drain_creates_merge_task_from_pending_merge() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.merge_queue
            .push(merge_phase::PENDING, make_test_merge("org/repo", 20));
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].work_id(), "merge:org/repo:20");
    }

    // ─── apply_queue_ops tests ───

    #[test]
    fn apply_remove_clears_item() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.issue_queue
            .push(issue_phase::ANALYZING, make_test_issue("org/repo", 1));
        source.repos.insert("org/repo".to_string(), repo);

        let result = TaskResult {
            work_id: "issue:org/repo:1".to_string(),
            repo_name: "org/repo".to_string(),
            queue_ops: vec![QueueOp::Remove],
            logs: vec![],
            status: crate::daemon::task::TaskStatus::Completed,
        };

        source.apply(&result);
        assert!(!source.repos["org/repo"].contains("issue:org/repo:1"));
    }

    #[test]
    fn apply_remove_then_push_pr() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.issue_queue
            .push(issue_phase::IMPLEMENTING, make_test_issue("org/repo", 1));
        source.repos.insert("org/repo".to_string(), repo);

        let result = TaskResult {
            work_id: "issue:org/repo:1".to_string(),
            repo_name: "org/repo".to_string(),
            queue_ops: vec![
                QueueOp::Remove,
                QueueOp::PushPr {
                    phase: pr_phase::PENDING,
                    item: Box::new(make_test_pr("org/repo", 10)),
                },
            ],
            logs: vec![],
            status: crate::daemon::task::TaskStatus::Completed,
        };

        source.apply(&result);
        assert!(!source.repos["org/repo"].contains("issue:org/repo:1"));
        assert!(source.repos["org/repo"].contains("pr:org/repo:10"));
    }

    #[test]
    fn apply_unknown_repo_is_noop() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let result = TaskResult {
            work_id: "issue:unknown/repo:1".to_string(),
            repo_name: "unknown/repo".to_string(),
            queue_ops: vec![QueueOp::Remove],
            logs: vec![],
            status: crate::daemon::task::TaskStatus::Completed,
        };

        // Should not panic
        source.apply(&result);
    }

    #[test]
    fn drain_handles_multiple_repos_and_phases() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo1 = GitRepository::new(
            "r1".to_string(),
            "org/repo1".to_string(),
            "https://github.com/org/repo1".to_string(),
            None,
        );
        repo1
            .issue_queue
            .push(issue_phase::PENDING, make_test_issue("org/repo1", 1));
        repo1
            .pr_queue
            .push(pr_phase::PENDING, make_test_pr("org/repo1", 10));

        let mut repo2 = GitRepository::new(
            "r2".to_string(),
            "org/repo2".to_string(),
            "https://github.com/org/repo2".to_string(),
            None,
        );
        repo2
            .merge_queue
            .push(merge_phase::PENDING, make_test_merge("org/repo2", 20));

        source.repos.insert("org/repo1".to_string(), repo1);
        source.repos.insert("org/repo2".to_string(), repo2);

        let tasks = source.drain_queue_items();
        assert_eq!(tasks.len(), 3);
    }

    #[test]
    fn drain_empty_queues_returns_nothing() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();
        assert!(tasks.is_empty());
    }
}
