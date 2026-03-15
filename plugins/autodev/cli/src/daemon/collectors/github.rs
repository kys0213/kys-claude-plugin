//! GitHubTaskSource — GitHub 이슈/PR 스캔 기반 Collector 구현체.
//!
//! daemon의 per-repo 큐, 스캔, 복구 로직을 캡슐화한다.
//! `poll()`: repo sync → recovery → scan → queue drain → Task 생성
//! `apply()`: TaskResult의 QueueOp를 per-repo 큐에 적용

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::core::collector::Collector;
use crate::core::config::{self, ConfigLoader, Env};
use crate::core::models::{QueuePhase, QueueType};
use crate::core::phase::TaskKind;
use crate::core::repository::{RepoRepository, ScanCursorRepository};
use crate::core::task::{QueueOp, Task, TaskResult};
use crate::infra::gh::Gh;
use crate::infra::git::Git;
use crate::infra::suggest_workflow::SuggestWorkflow;
use crate::tasks::analyze::AnalyzeTask;
use crate::tasks::extract::ExtractTask;
use crate::tasks::helpers::git_ops::GitRepository;
use crate::tasks::helpers::git_ops_factory::GitRepositoryFactory;
use crate::tasks::helpers::workspace::WorkspaceOps;
use crate::tasks::implement::ImplementTask;
use crate::tasks::improve::ImproveTask;
use crate::tasks::review::ReviewTask;

/// GitHub 이슈/PR 스캔 기반 Collector.
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

            let repo = match self.repos.get_mut(repo_name) {
                Some(r) => r,
                None => continue,
            };

            // Always cache concurrency limits (even when scan is skipped)
            repo.issue_concurrency = repo_cfg.sources.github.issue_concurrency as usize;
            repo.pr_concurrency = repo_cfg.sources.github.pr_concurrency as usize;

            let should_scan = self
                .db
                .cursor_should_scan(repo.id(), repo_cfg.sources.github.scan_interval_secs as i64)
                .unwrap_or(false);
            if !should_scan {
                continue;
            }

            tracing::info!("scanning {}...", repo_name);

            for target in &repo_cfg.sources.github.scan_targets {
                match target.as_str() {
                    "issues" => {
                        if let Err(e) = repo
                            .scan_issues(
                                &*self.gh,
                                &self.db,
                                &repo_cfg.sources.github.ignore_authors,
                                &repo_cfg.sources.github.filter_labels,
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
                            .scan_pulls(&*self.gh, &repo_cfg.sources.github.ignore_authors)
                            .await
                        {
                            tracing::error!("PR scan error for {repo_name}: {e}");
                        }

                        // done + merged + NOT extracted → knowledge extraction
                        if repo_cfg.sources.github.knowledge_extraction {
                            if let Err(e) = repo.scan_done_merged(&*self.gh).await {
                                tracing::error!("done_merged scan error for {repo_name}: {e}");
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// 모든 repo의 큐에서 ready 아이템을 pop → working phase 전이 → Task 생성.
    ///
    /// per-repo `issue_concurrency` / `pr_concurrency` 제한을 적용하여,
    /// in-flight 태스크 수를 초과하지 않도록 bounded drain을 수행한다.
    fn drain_queue_items(&mut self) -> Vec<Box<dyn Task>> {
        let mut tasks: Vec<Box<dyn Task>> = Vec::new();

        for repo in self.repos.values_mut() {
            // ─── Issue concurrency: Analyze + Implement running 합산 제한 ───
            let issue_running = repo
                .queue
                .iter(QueuePhase::Running)
                .filter(|i| i.is_type(QueueType::Issue))
                .count();
            let mut issue_slots = repo.issue_concurrency.saturating_sub(issue_running);

            // Issue: Pending(Analyze) → Running
            let drained = repo.queue.drain_to_filtered(
                QueuePhase::Pending,
                QueuePhase::Running,
                issue_slots,
                |i| i.is(QueueType::Issue, TaskKind::Analyze),
            );
            issue_slots -= drained.len();
            for item in drained {
                tracing::debug!("issue #{}: creating AnalyzeTask", item.github_number);
                tasks.push(Box::new(AnalyzeTask::new(
                    Arc::clone(&self.workspace),
                    Arc::clone(&self.gh),
                    Arc::clone(&self.config),
                    item,
                )));
            }

            // Issue: Pending(Implement) → Running
            for item in repo.queue.drain_to_filtered(
                QueuePhase::Pending,
                QueuePhase::Running,
                issue_slots,
                |i| i.is(QueueType::Issue, TaskKind::Implement),
            ) {
                tracing::debug!("issue #{}: creating ImplementTask", item.github_number);
                tasks.push(Box::new(ImplementTask::new(
                    Arc::clone(&self.workspace),
                    Arc::clone(&self.gh),
                    Arc::clone(&self.config),
                    item,
                )));
            }

            // ─── PR concurrency: Review + Improve running 합산 제한 ───
            let pr_running = repo
                .queue
                .iter(QueuePhase::Running)
                .filter(|i| i.is_pr_concurrent())
                .count();
            let mut pr_slots = repo.pr_concurrency.saturating_sub(pr_running);

            // PR: Pending(Review) → Running
            let drained = repo.queue.drain_to_filtered(
                QueuePhase::Pending,
                QueuePhase::Running,
                pr_slots,
                |i| i.is(QueueType::Pr, TaskKind::Review),
            );
            pr_slots -= drained.len();
            for item in drained {
                tracing::debug!("PR #{}: creating ReviewTask", item.github_number);
                tasks.push(Box::new(ReviewTask::new(
                    Arc::clone(&self.workspace),
                    Arc::clone(&self.gh),
                    Arc::clone(&self.config),
                    item,
                )));
            }

            // PR: Pending(Improve) → Running
            let drained = repo.queue.drain_to_filtered(
                QueuePhase::Pending,
                QueuePhase::Running,
                pr_slots,
                |i| i.is(QueueType::Pr, TaskKind::Improve),
            );
            pr_slots -= drained.len();
            for item in drained {
                tracing::debug!("PR #{}: creating ImproveTask", item.github_number);
                tasks.push(Box::new(ImproveTask::new(
                    Arc::clone(&self.workspace),
                    Arc::clone(&self.gh),
                    item,
                )));
            }

            // PR: Pending(Extract) → fire-and-forget (batch drain, immediate removal)
            let extract_items = repo.queue.drain_to_filtered(
                QueuePhase::Pending,
                QueuePhase::Running,
                pr_slots,
                |i| i.is(QueueType::Pr, TaskKind::Extract),
            );
            for item in extract_items {
                tracing::debug!(
                    "PR #{}: creating ExtractTask (knowledge)",
                    item.github_number
                );
                repo.queue.remove(&item.work_id);
                tasks.push(Box::new(ExtractTask::new(
                    Arc::clone(&self.workspace),
                    Arc::clone(&self.gh),
                    Arc::clone(&self.sw),
                    Arc::clone(&self.git),
                    Arc::clone(&self.env),
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
                    repo.queue.remove(&result.work_id);
                }
                QueueOp::Push { phase, item } => {
                    repo.queue.push(*phase, *item.clone());
                }
            }
        }
    }
}

#[async_trait(?Send)]
impl<DB: RepoRepository + ScanCursorRepository + Send> Collector for GitHubTaskSource<DB> {
    async fn poll(&mut self) -> Vec<Box<dyn Task>> {
        self.sync_repos().await;
        self.run_recovery().await;
        self.run_scans().await;
        self.drain_queue_items()
    }

    fn apply(&mut self, result: &TaskResult) {
        self.apply_queue_ops(result);
    }

    fn active_items(&self) -> Vec<crate::daemon::status::StatusItem> {
        let mut items = Vec::new();
        for repo in self.repos.values() {
            for (phase, item) in repo.queue.iter_all() {
                items.push(crate::daemon::status::StatusItem {
                    work_id: item.work_id.clone(),
                    queue_type: item.queue_type.clone(),
                    repo_name: item.repo_name.clone(),
                    number: item.github_number,
                    title: item.title.clone(),
                    phase: phase.to_string(),
                });
            }
        }
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::models::WorkflowConfig;
    use crate::core::models::EnabledRepo;
    use crate::core::queue_item::testing::test_repo_named;
    use crate::core::queue_item::{ItemMetadata, QueueItem};
    use crate::infra::gh::mock::MockGh;
    use crate::infra::git::Git;
    use crate::infra::suggest_workflow::SuggestWorkflow;
    use crate::tasks::knowledge::models::{RepetitionEntry, SessionEntry, ToolFrequencyEntry};
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
        async fn sync_default_branch(&self, _: &Path) -> anyhow::Result<bool> {
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
        fn repo_list(&self) -> anyhow::Result<Vec<crate::core::models::RepoInfo>> {
            Ok(vec![])
        }
        fn repo_find_enabled(&self) -> anyhow::Result<Vec<EnabledRepo>> {
            Ok(self.repos.clone())
        }
        fn repo_status_summary(&self) -> anyhow::Result<Vec<crate::core::models::RepoStatusRow>> {
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

    fn make_test_queue_item(
        queue_type: QueueType,
        task_kind: TaskKind,
        repo_name: &str,
        number: i64,
    ) -> QueueItem {
        let repo = test_repo_named(repo_name);
        match queue_type {
            QueueType::Issue => QueueItem::new_issue(
                &repo,
                number,
                task_kind,
                format!("Issue #{number}"),
                None,
                vec![],
                "user".into(),
            ),
            _ => QueueItem::new_pr(
                &repo,
                number,
                task_kind,
                format!("PR #{number}"),
                ItemMetadata::Pr {
                    head_branch: "feat".into(),
                    base_branch: "main".into(),
                    review_comment: None,
                    source_issue_number: None,
                    review_iteration: 0,
                },
            ),
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
        repo.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", 1),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].work_id(), "issue:org/repo:1");

        // Item should be moved to Running
        assert!(source.repos["org/repo"].contains("issue:org/repo:1"));
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
        repo.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Issue, TaskKind::Implement, "org/repo", 2),
        );
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
        repo.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Pr, TaskKind::Review, "org/repo", 10),
        );
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
        repo.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Pr, TaskKind::Improve, "org/repo", 10),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].work_id(), "pr:org/repo:10");
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
        repo.queue.push(
            QueuePhase::Running,
            make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", 1),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let result = TaskResult {
            work_id: "issue:org/repo:1".to_string(),
            repo_name: "org/repo".to_string(),
            queue_ops: vec![QueueOp::Remove],
            logs: vec![],
            status: crate::core::task::TaskStatus::Completed,
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
        repo.queue.push(
            QueuePhase::Running,
            make_test_queue_item(QueueType::Issue, TaskKind::Implement, "org/repo", 1),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let result = TaskResult {
            work_id: "issue:org/repo:1".to_string(),
            repo_name: "org/repo".to_string(),
            queue_ops: vec![
                QueueOp::Remove,
                QueueOp::Push {
                    phase: QueuePhase::Pending,
                    item: Box::new(make_test_queue_item(
                        QueueType::Pr,
                        TaskKind::Review,
                        "org/repo",
                        10,
                    )),
                },
            ],
            logs: vec![],
            status: crate::core::task::TaskStatus::Completed,
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
            status: crate::core::task::TaskStatus::Completed,
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
        repo1.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo1", 1),
        );
        repo1.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Pr, TaskKind::Review, "org/repo1", 10),
        );

        let mut repo2 = GitRepository::new(
            "r2".to_string(),
            "org/repo2".to_string(),
            "https://github.com/org/repo2".to_string(),
            None,
        );
        repo2.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Pr, TaskKind::Improve, "org/repo2", 20),
        );

        source.repos.insert("org/repo1".to_string(), repo1);
        source.repos.insert("org/repo2".to_string(), repo2);

        let tasks = source.drain_queue_items();
        assert_eq!(tasks.len(), 3);
    }

    // ─── concurrency limit tests ───

    #[test]
    fn drain_respects_issue_concurrency_limit() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.issue_concurrency = 2;

        // 5 pending issues, but concurrency = 2
        for i in 1..=5 {
            repo.queue.push(
                QueuePhase::Pending,
                make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", i),
            );
        }
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();
        assert_eq!(tasks.len(), 2);

        // 3 items should remain in Pending
        assert_eq!(
            source.repos["org/repo"]
                .queue
                .iter(QueuePhase::Pending)
                .filter(|i| i.is(QueueType::Issue, TaskKind::Analyze))
                .count(),
            3
        );
        assert_eq!(
            source.repos["org/repo"]
                .queue
                .iter(QueuePhase::Running)
                .filter(|i| i.is(QueueType::Issue, TaskKind::Analyze))
                .count(),
            2
        );
    }

    #[test]
    fn drain_issue_concurrency_counts_in_flight() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.issue_concurrency = 2;

        // 1 already running (Analyze)
        repo.queue.push(
            QueuePhase::Running,
            make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", 1),
        );
        // 3 pending
        for i in 2..=4 {
            repo.queue.push(
                QueuePhase::Pending,
                make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", i),
            );
        }
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();
        // Only 1 slot available (2 - 1 in-flight)
        assert_eq!(tasks.len(), 1);
        assert_eq!(
            source.repos["org/repo"]
                .queue
                .iter(QueuePhase::Running)
                .filter(|i| i.is(QueueType::Issue, TaskKind::Analyze))
                .count(),
            2
        );
        assert_eq!(
            source.repos["org/repo"]
                .queue
                .iter(QueuePhase::Pending)
                .filter(|i| i.is(QueueType::Issue, TaskKind::Analyze))
                .count(),
            2
        );
    }

    #[test]
    fn drain_issue_concurrency_includes_implementing() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.issue_concurrency = 1;

        // 1 already running (Implement) → no slots
        repo.queue.push(
            QueuePhase::Running,
            make_test_queue_item(QueueType::Issue, TaskKind::Implement, "org/repo", 1),
        );
        repo.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", 2),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();
        assert_eq!(tasks.len(), 0);
    }

    #[test]
    fn drain_respects_pr_concurrency_limit() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.pr_concurrency = 1;

        // 3 pending PRs, but concurrency = 1
        for i in 1..=3 {
            repo.queue.push(
                QueuePhase::Pending,
                make_test_queue_item(QueueType::Pr, TaskKind::Review, "org/repo", i),
            );
        }
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();
        assert_eq!(tasks.len(), 1);
        assert_eq!(
            source.repos["org/repo"]
                .queue
                .iter(QueuePhase::Pending)
                .filter(|i| i.is(QueueType::Pr, TaskKind::Review))
                .count(),
            2
        );
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

    // ─── unified queue behavior tests ───

    #[test]
    fn drain_issue_pr_concurrency_isolated_in_unified_queue() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.issue_concurrency = 1;
        repo.pr_concurrency = 1;

        // 1 issue + 1 PR in Pending — independent concurrency budgets
        repo.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", 1),
        );
        repo.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Pr, TaskKind::Review, "org/repo", 10),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();

        // Both should drain — issue concurrency doesn't block PR and vice versa
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn drain_extract_does_not_consume_pr_concurrency() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.pr_concurrency = 1;

        // 1 Extract already running — should NOT count against PR concurrency
        repo.queue.push(
            QueuePhase::Running,
            make_test_queue_item(QueueType::Pr, TaskKind::Extract, "org/repo", 99),
        );
        // 1 Review pending
        repo.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Pr, TaskKind::Review, "org/repo", 10),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();

        // Review should drain because Extract doesn't consume concurrency
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].work_id(), "pr:org/repo:10");
    }

    #[test]
    fn drain_task_kind_determines_task_type() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.issue_concurrency = 2;

        // Same queue_type (Issue), different task_kind
        repo.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", 1),
        );
        repo.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Issue, TaskKind::Implement, "org/repo", 2),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();

        // Both should drain (shared issue concurrency budget)
        assert_eq!(tasks.len(), 2);
        // Analyze task comes first (drain order)
        assert_eq!(tasks[0].work_id(), "issue:org/repo:1");
        assert_eq!(tasks[1].work_id(), "issue:org/repo:2");
    }

    // ═══════════════════════════════════════════════
    // Re-review 경로 검증: ImproveTask 완료 후 Pending(Review)로 push된 PR이
    // drain에서 ReviewTask로 생성되는지 확인한다.
    // ═══════════════════════════════════════════════

    #[test]
    fn drain_improved_routes_through_pending_then_reviewing() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.pr_concurrency = 2;

        // After ImproveTask completes, item is pushed to Pending with TaskKind::Review
        repo.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Pr, TaskKind::Review, "org/repo", 1),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();

        // ReviewTask should be created
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].work_id(), "pr:org/repo:1");

        // Item should be in Running
        let repo = &source.repos["org/repo"];
        assert_eq!(
            repo.queue
                .iter(QueuePhase::Pending)
                .filter(|i| i.is(QueueType::Pr, TaskKind::Review))
                .count(),
            0
        );
        assert_eq!(
            repo.queue
                .iter(QueuePhase::Running)
                .filter(|i| i.is(QueueType::Pr, TaskKind::Review))
                .count(),
            1
        );
    }

    #[test]
    fn drain_improved_respects_pr_concurrency() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.pr_concurrency = 1;

        // 2 PRs queued for re-review, concurrency = 1
        repo.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Pr, TaskKind::Review, "org/repo", 1),
        );
        repo.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Pr, TaskKind::Review, "org/repo", 2),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_queue_items();

        // concurrency=1 so only 1 should drain to Running
        assert_eq!(tasks.len(), 1);
        let repo = &source.repos["org/repo"];
        assert_eq!(
            repo.queue
                .iter(QueuePhase::Running)
                .filter(|i| i.is(QueueType::Pr, TaskKind::Review))
                .count(),
            1
        );
        // 1 remains in Pending
        assert_eq!(
            repo.queue
                .iter(QueuePhase::Pending)
                .filter(|i| i.is(QueueType::Pr, TaskKind::Review))
                .count(),
            1
        );
    }
}
