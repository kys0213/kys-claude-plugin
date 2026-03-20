//! GitHubTaskSource — GitHub 이슈/PR 스캔 기반 Collector 구현체.
//!
//! daemon의 per-repo 큐, 스캔, 복구 로직을 캡슐화한다.
//! `poll()`: repo sync → recovery → scan → queue drain → Task 생성
//! `apply()`: TaskResult의 QueueOp를 per-repo 큐에 적용

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::core::collector::Collector;
use crate::core::config::models::ClawConfig;
use crate::core::config::{self, ConfigLoader, Env};
use crate::core::models::{QueuePhase, QueueType};
use crate::core::phase::TaskKind;
use crate::core::repository::{QueueRepository, RepoRepository, ScanCursorRepository};
use crate::core::task::{QueueOp, Task, TaskResult};
use crate::infra::gh::Gh;
use crate::infra::git::Git;
use crate::infra::suggest_workflow::SuggestWorkflow;
use crate::service::tasks::analyze::AnalyzeTask;
use crate::service::tasks::extract::ExtractTask;
use crate::service::tasks::helpers::git_ops::GitRepository;
use crate::service::tasks::helpers::git_ops_factory::GitRepositoryFactory;
use crate::service::tasks::helpers::workspace::WorkspaceOps;
use crate::service::tasks::implement::ImplementTask;
use crate::service::tasks::improve::ImproveTask;
use crate::service::tasks::review::ReviewTask;

/// GitHub 이슈/PR 스캔 기반 Collector.
///
/// per-repo 큐를 소유하고, 스캔 → Task 생성 → 큐 적용 생명주기를 관리한다.
pub struct GitHubTaskSource<DB: RepoRepository + ScanCursorRepository + QueueRepository> {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
    env: Arc<dyn Env>,
    git: Arc<dyn Git>,
    sw: Arc<dyn SuggestWorkflow>,
    db: DB,
    repos: HashMap<String, GitRepository>,
    /// claw.enabled feature flag: true이면 Ready→Running drain, false이면 Pending→Running
    claw_enabled: bool,
    /// 인메모리 recovery throttle 타임스탬프
    last_recovery: Option<std::time::Instant>,
    /// recovery 실행 최소 간격 (초)
    recovery_interval_secs: u64,
}

impl<DB: RepoRepository + ScanCursorRepository + QueueRepository + Send> GitHubTaskSource<DB> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        workspace: Arc<dyn WorkspaceOps>,
        gh: Arc<dyn Gh>,
        config: Arc<dyn ConfigLoader>,
        env: Arc<dyn Env>,
        git: Arc<dyn Git>,
        sw: Arc<dyn SuggestWorkflow>,
        db: DB,
        claw: ClawConfig,
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
            claw_enabled: claw.enabled,
            last_recovery: None,
            recovery_interval_secs: claw.recovery_interval_secs,
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
            let n = repo.recover_orphan_wip(&*self.gh, &self.db).await;
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

                        if let Err(e) = repo.scan_approved_issues(&*self.gh, &self.db).await {
                            tracing::error!("approved scan error for {repo_name}: {e}");
                        }
                    }
                    "pulls" => {
                        if let Err(e) = repo
                            .scan_pulls(
                                &*self.gh,
                                &self.db,
                                &repo_cfg.sources.github.ignore_authors,
                            )
                            .await
                        {
                            tracing::error!("PR scan error for {repo_name}: {e}");
                        }

                        // done + merged + NOT extracted → knowledge extraction
                        if repo_cfg.sources.github.knowledge_extraction {
                            if let Err(e) = repo.scan_done_merged(&*self.gh, &self.db).await {
                                tracing::error!("done_merged scan error for {repo_name}: {e}");
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Claw enabled일 때 DB에서 Ready로 전이된 아이템을 in-memory 큐에 반영한다.
    ///
    /// Claw(CLI)가 DB에서 Pending→Ready 전이 → daemon의 in-memory queue는 여전히 Pending.
    /// DB를 읽어 Ready 상태인 아이템을 in-memory에서도 Ready로 전이한다.
    fn sync_queue_phases(&mut self) {
        if !self.claw_enabled {
            return;
        }
        for repo in self.repos.values_mut() {
            if let Ok(rows) = self.db.queue_load_active(repo.id()) {
                for row in rows {
                    if row.phase == QueuePhase::Ready {
                        repo.queue
                            .transit(&row.work_id, QueuePhase::Pending, QueuePhase::Ready);
                    }
                }
            }
        }
    }

    /// Claw 비활성 시 Pending 아이템을 자동으로 Ready로 전이한다.
    ///
    /// Claw가 없으면 Pending→Ready 판단을 대신할 주체가 없으므로,
    /// collector가 직접 전이하여 기존 동작(즉시 실행)을 유지한다.
    fn auto_advance_pending(&mut self) {
        if self.claw_enabled {
            return;
        }
        for repo in self.repos.values_mut() {
            let pending_ids: Vec<String> = repo
                .queue
                .iter(QueuePhase::Pending)
                .map(|item| item.work_id.clone())
                .collect();
            for work_id in pending_ids {
                repo.queue
                    .transit(&work_id, QueuePhase::Pending, QueuePhase::Ready);
                if let Err(e) =
                    self.db
                        .queue_transit(&work_id, QueuePhase::Pending, QueuePhase::Ready)
                {
                    tracing::warn!("auto_advance queue_transit failed for {work_id}: {e}");
                }
            }
        }
    }

    /// 모든 repo의 큐에서 ready 아이템을 pop → working phase 전이 → Task 생성.
    ///
    /// per-repo `issue_concurrency` / `pr_concurrency` 제한을 적용하여,
    /// in-flight 태스크 수를 초과하지 않도록 bounded drain을 수행한다.
    /// drain + DB transit 공통 헬퍼.
    /// in-memory 큐에서 drain한 뒤, DB phase도 동기화한다.
    fn drain_and_sync<F>(
        db: &DB,
        queue: &mut crate::core::state_queue::StateQueue<crate::core::queue_item::QueueItem>,
        from_phase: QueuePhase,
        limit: usize,
        predicate: F,
    ) -> Vec<crate::core::queue_item::QueueItem>
    where
        F: Fn(&crate::core::queue_item::QueueItem) -> bool,
    {
        let drained = queue.drain_to_filtered(from_phase, QueuePhase::Running, limit, predicate);
        for item in &drained {
            if let Err(e) = db.queue_transit(&item.work_id, from_phase, QueuePhase::Running) {
                tracing::warn!("queue_transit failed for {}: {e}", item.work_id);
            }
        }
        drained
    }

    /// Ready 상태의 큐 아이템을 Running으로 전이하며 Task를 생성한다.
    ///
    /// 항상 Ready→Running만 수행한다. Pending→Ready 전이는:
    /// - claw_enabled=true: Claw가 `queue advance`로 수행
    /// - claw_enabled=false: `auto_advance_pending()`이 poll() 시 자동 수행
    ///
    /// poll()에서 분리되어 별도 호출 가능. Collector trait의 drain_tasks() 구현에서 위임받는다.
    pub fn drain_ready_tasks(&mut self) -> Vec<Box<dyn Task>> {
        let mut tasks: Vec<Box<dyn Task>> = Vec::new();
        let from_phase = QueuePhase::Ready;

        for repo in self.repos.values_mut() {
            // ─── Issue concurrency: Analyze + Implement running 합산 제한 ───
            let issue_running = repo
                .queue
                .iter(QueuePhase::Running)
                .filter(|i| i.is_type(QueueType::Issue))
                .count();
            let mut issue_slots = repo.issue_concurrency.saturating_sub(issue_running);

            // Issue: from_phase(Analyze) → Running
            let drained =
                Self::drain_and_sync(&self.db, &mut repo.queue, from_phase, issue_slots, |i| {
                    i.is(QueueType::Issue, TaskKind::Analyze)
                });
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

            // Issue: from_phase(Implement) → Running
            for item in
                Self::drain_and_sync(&self.db, &mut repo.queue, from_phase, issue_slots, |i| {
                    i.is(QueueType::Issue, TaskKind::Implement)
                })
            {
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

            // PR: from_phase(Review) → Running
            let drained =
                Self::drain_and_sync(&self.db, &mut repo.queue, from_phase, pr_slots, |i| {
                    i.is(QueueType::Pr, TaskKind::Review)
                });
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

            // PR: from_phase(Improve) → Running
            let drained =
                Self::drain_and_sync(&self.db, &mut repo.queue, from_phase, pr_slots, |i| {
                    i.is(QueueType::Pr, TaskKind::Improve)
                });
            pr_slots -= drained.len();
            for item in drained {
                tracing::debug!("PR #{}: creating ImproveTask", item.github_number);
                tasks.push(Box::new(ImproveTask::new(
                    Arc::clone(&self.workspace),
                    Arc::clone(&self.gh),
                    item,
                )));
            }

            // PR: from_phase(Extract) → fire-and-forget
            // Extract는 PR이 이미 done 상태이므로 큐에 남겨둘 필요 없음.
            // drain_to_filtered로 꺼낸 뒤 즉시 remove하여 Running에 잔류하지 않도록 한다.
            let extract_items =
                repo.queue
                    .drain_to_filtered(from_phase, QueuePhase::Running, pr_slots, |i| {
                        i.is(QueueType::Pr, TaskKind::Extract)
                    });
            for item in extract_items {
                tracing::debug!(
                    "PR #{}: creating ExtractTask (knowledge)",
                    item.github_number
                );
                repo.queue.remove(&item.work_id);
                if let Err(e) = self.db.queue_remove(&item.work_id) {
                    tracing::warn!("queue_remove failed for {}: {e}", item.work_id);
                }
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
                    if let Err(e) = self.db.queue_remove(&result.work_id) {
                        tracing::warn!("queue_remove failed for {}: {e}", result.work_id);
                    }
                }
                QueueOp::Push { phase, item } => {
                    repo.queue.push(*phase, *item.clone());
                    if let Err(e) = self.db.queue_upsert(&item.to_row(*phase)) {
                        tracing::error!("queue_upsert failed for {}: {e}", item.work_id);
                    }
                }
            }
        }
    }
}

#[async_trait(?Send)]
impl<DB: RepoRepository + ScanCursorRepository + QueueRepository + Send> Collector
    for GitHubTaskSource<DB>
{
    async fn poll(&mut self) -> Vec<Box<dyn Task>> {
        self.sync_repos().await;

        // Recovery throttle: 첫 tick에서는 항상 실행, 이후 interval 경과 시에만 실행
        let should_recover = self.last_recovery.map_or(true, |t| {
            t.elapsed().as_secs() >= self.recovery_interval_secs
        });
        if should_recover {
            self.run_recovery().await;
            self.last_recovery = Some(std::time::Instant::now());
        }

        self.run_scans().await;
        self.sync_queue_phases();
        self.auto_advance_pending();
        Vec::new()
    }

    fn drain_tasks(&mut self) -> Vec<Box<dyn Task>> {
        self.drain_ready_tasks()
    }

    fn apply(&mut self, result: &TaskResult) {
        self.apply_queue_ops(result);
    }

    fn active_items(&self) -> Vec<crate::service::daemon::status::StatusItem> {
        let mut items = Vec::new();
        for repo in self.repos.values() {
            for (phase, item) in repo.queue.iter_all() {
                items.push(crate::service::daemon::status::StatusItem {
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
    use crate::core::queue_item::{PrMetadata, QueueItem};
    use crate::infra::gh::mock::MockGh;
    use crate::infra::git::Git;
    use crate::infra::suggest_workflow::SuggestWorkflow;
    use crate::service::tasks::knowledge::models::{
        RepetitionEntry, SessionEntry, ToolFrequencyEntry,
    };
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

    /// Minimal DB mock for tests.
    /// `active_items`를 설정하면 `queue_load_active()`에서 반환한다.
    struct MockDb {
        repos: Vec<EnabledRepo>,
        active_items: Vec<crate::core::models::QueueItemRow>,
    }

    impl MockDb {
        fn empty() -> Self {
            Self {
                repos: vec![],
                active_items: vec![],
            }
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

    impl QueueRepository for MockDb {
        fn queue_get_phase(&self, _: &str) -> anyhow::Result<Option<QueuePhase>> {
            Ok(None)
        }
        fn queue_advance(&self, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn queue_skip(&self, _: &str, _: Option<&str>) -> anyhow::Result<()> {
            Ok(())
        }
        fn queue_list_items(
            &self,
            _: Option<&str>,
        ) -> anyhow::Result<Vec<crate::core::models::QueueItemRow>> {
            Ok(vec![])
        }
        fn queue_upsert(&self, _: &crate::core::models::QueueItemRow) -> anyhow::Result<()> {
            Ok(())
        }
        fn queue_remove(&self, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn queue_load_active(
            &self,
            _: &str,
        ) -> anyhow::Result<Vec<crate::core::models::QueueItemRow>> {
            Ok(self.active_items.clone())
        }
        fn queue_transit(&self, _: &str, _: QueuePhase, _: QueuePhase) -> anyhow::Result<bool> {
            Ok(true)
        }
        fn queue_get_item(
            &self,
            _: &str,
        ) -> anyhow::Result<Option<crate::core::models::QueueItemRow>> {
            Ok(None)
        }
        fn queue_increment_failure(&self, _: &str) -> anyhow::Result<i32> {
            Ok(1)
        }
        fn queue_get_failure_count(&self, _: &str) -> anyhow::Result<i32> {
            Ok(0)
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
            ClawConfig::default(),
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
            QueueType::Pr | QueueType::Knowledge | QueueType::Agent => QueueItem::new_pr(
                &repo,
                number,
                task_kind,
                format!("PR #{number}"),
                PrMetadata {
                    head_branch: "feat".into(),
                    base_branch: "main".into(),
                    review_comment: None,
                    source_issue_number: None,
                    review_iteration: 0,
                },
            ),
        }
    }

    // ─── drain_ready_tasks tests ───

    #[test]
    fn drain_creates_analyze_task_from_ready_issue() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.queue.push(
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", 1),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_ready_tasks();
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
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Issue, TaskKind::Implement, "org/repo", 2),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_ready_tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].work_id(), "issue:org/repo:2");
    }

    #[test]
    fn drain_creates_review_task_from_ready_pr() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.queue.push(
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Pr, TaskKind::Review, "org/repo", 10),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_ready_tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].work_id(), "pr:org/repo:10");
    }

    #[test]
    fn drain_creates_improve_task_from_ready_pr() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.queue.push(
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Pr, TaskKind::Improve, "org/repo", 10),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_ready_tasks();
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
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo1", 1),
        );
        repo1.queue.push(
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Pr, TaskKind::Review, "org/repo1", 10),
        );

        let mut repo2 = GitRepository::new(
            "r2".to_string(),
            "org/repo2".to_string(),
            "https://github.com/org/repo2".to_string(),
            None,
        );
        repo2.queue.push(
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Pr, TaskKind::Improve, "org/repo2", 20),
        );

        source.repos.insert("org/repo1".to_string(), repo1);
        source.repos.insert("org/repo2".to_string(), repo2);

        let tasks = source.drain_ready_tasks();
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

        // 5 ready issues, but concurrency = 2
        for i in 1..=5 {
            repo.queue.push(
                QueuePhase::Ready,
                make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", i),
            );
        }
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_ready_tasks();
        assert_eq!(tasks.len(), 2);

        // 3 items should remain in Ready
        assert_eq!(
            source.repos["org/repo"]
                .queue
                .iter(QueuePhase::Ready)
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
        // 3 ready
        for i in 2..=4 {
            repo.queue.push(
                QueuePhase::Ready,
                make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", i),
            );
        }
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_ready_tasks();
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
                .iter(QueuePhase::Ready)
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
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", 2),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_ready_tasks();
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

        // 3 ready PRs, but concurrency = 1
        for i in 1..=3 {
            repo.queue.push(
                QueuePhase::Ready,
                make_test_queue_item(QueueType::Pr, TaskKind::Review, "org/repo", i),
            );
        }
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_ready_tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(
            source.repos["org/repo"]
                .queue
                .iter(QueuePhase::Ready)
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

        let tasks = source.drain_ready_tasks();
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

        // 1 issue + 1 PR in Ready — independent concurrency budgets
        repo.queue.push(
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", 1),
        );
        repo.queue.push(
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Pr, TaskKind::Review, "org/repo", 10),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_ready_tasks();

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
        // 1 Review in Ready
        repo.queue.push(
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Pr, TaskKind::Review, "org/repo", 10),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_ready_tasks();

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
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", 1),
        );
        repo.queue.push(
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Issue, TaskKind::Implement, "org/repo", 2),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_ready_tasks();

        // Both should drain (shared issue concurrency budget)
        assert_eq!(tasks.len(), 2);
        // Analyze task comes first (drain order)
        assert_eq!(tasks[0].work_id(), "issue:org/repo:1");
        assert_eq!(tasks[1].work_id(), "issue:org/repo:2");
    }

    // ═══════════════════════════════════════════════
    // Re-review 경로 검증: ImproveTask 완료 후 Ready(Review)로 전이된 PR이
    // drain에서 ReviewTask로 생성되는지 확인한다.
    // ═══════════════════════════════════════════════

    #[test]
    fn drain_improved_routes_through_ready_then_running() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.pr_concurrency = 2;

        // After ImproveTask completes and auto_advance, item is in Ready with TaskKind::Review
        repo.queue.push(
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Pr, TaskKind::Review, "org/repo", 1),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_ready_tasks();

        // ReviewTask should be created
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].work_id(), "pr:org/repo:1");

        // Item should be in Running
        let repo = &source.repos["org/repo"];
        assert_eq!(
            repo.queue
                .iter(QueuePhase::Ready)
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

        // 2 PRs queued for re-review in Ready, concurrency = 1
        repo.queue.push(
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Pr, TaskKind::Review, "org/repo", 1),
        );
        repo.queue.push(
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Pr, TaskKind::Review, "org/repo", 2),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_ready_tasks();

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
        // 1 remains in Ready
        assert_eq!(
            repo.queue
                .iter(QueuePhase::Ready)
                .filter(|i| i.is(QueueType::Pr, TaskKind::Review))
                .count(),
            1
        );
    }

    // ═══════════════════════════════════════════════
    // claw_enabled: drain phase gating tests
    // ═══════════════════════════════════════════════

    fn make_claw_source(gh: Arc<MockGh>) -> GitHubTaskSource<MockDb> {
        GitHubTaskSource::new(
            Arc::new(MockWorkspace),
            gh,
            Arc::new(MockConfigLoader),
            Arc::new(MockEnv),
            Arc::new(MockGit),
            Arc::new(MockSuggestWorkflow),
            MockDb::empty(),
            ClawConfig {
                enabled: true,
                ..ClawConfig::default()
            },
        )
    }

    #[test]
    fn drain_from_ready_when_claw_enabled() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_claw_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        // Item in Ready phase — should be drained when claw_enabled
        repo.queue.push(
            QueuePhase::Ready,
            make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", 1),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_ready_tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].work_id(), "issue:org/repo:1");
    }

    #[test]
    fn pending_items_not_drained_when_claw_enabled() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_claw_source(gh);

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        // Item in Pending phase — should NOT be drained when claw_enabled
        repo.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", 1),
        );
        source.repos.insert("org/repo".to_string(), repo);

        let tasks = source.drain_ready_tasks();
        assert!(tasks.is_empty());
        // Item should remain in Pending
        assert_eq!(source.repos["org/repo"].queue.len(QueuePhase::Pending), 1);
    }

    #[test]
    fn auto_advance_promotes_pending_to_ready_when_claw_disabled() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_source(gh); // claw_enabled = false

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

        // Before auto_advance: item is Pending
        assert_eq!(
            source.repos["org/repo"].queue.phase_of("issue:org/repo:1"),
            Some(QueuePhase::Pending)
        );

        // auto_advance should promote Pending → Ready
        source.auto_advance_pending();

        assert_eq!(
            source.repos["org/repo"].queue.phase_of("issue:org/repo:1"),
            Some(QueuePhase::Ready)
        );

        // Now drain should pick it up
        let tasks = source.drain_ready_tasks();
        assert_eq!(tasks.len(), 1);
    }

    #[test]
    fn auto_advance_noop_when_claw_enabled() {
        let gh = Arc::new(MockGh::new());
        let mut source = make_claw_source(gh); // claw_enabled = true

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

        // auto_advance should NOT promote when claw is enabled
        source.auto_advance_pending();

        assert_eq!(
            source.repos["org/repo"].queue.phase_of("issue:org/repo:1"),
            Some(QueuePhase::Pending)
        );
    }

    #[test]
    fn sync_queue_phases_promotes_ready() {
        let now = chrono::Utc::now().to_rfc3339();
        let active_row = crate::core::models::QueueItemRow {
            work_id: "issue:org/repo:1".to_string(),
            repo_id: "r1".to_string(),
            queue_type: QueueType::Issue,
            phase: QueuePhase::Ready,
            title: Some("Test".to_string()),
            skip_reason: None,
            created_at: now.clone(),
            updated_at: now,
            task_kind: TaskKind::Analyze,
            github_number: 1,
            metadata_json: None,
            failure_count: 0,
            escalation_level: 0,
        };

        let db = MockDb {
            repos: vec![],
            active_items: vec![active_row],
        };

        let mut source = GitHubTaskSource::new(
            Arc::new(MockWorkspace),
            Arc::new(MockGh::new()),
            Arc::new(MockConfigLoader),
            Arc::new(MockEnv),
            Arc::new(MockGit),
            Arc::new(MockSuggestWorkflow),
            db,
            ClawConfig {
                enabled: true,
                ..ClawConfig::default()
            },
        );

        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        // Item starts in Pending in memory
        repo.queue.push(
            QueuePhase::Pending,
            make_test_queue_item(QueueType::Issue, TaskKind::Analyze, "org/repo", 1),
        );
        source.repos.insert("org/repo".to_string(), repo);

        // Before sync: item is Pending
        assert_eq!(
            source.repos["org/repo"].queue.phase_of("issue:org/repo:1"),
            Some(QueuePhase::Pending)
        );

        source.sync_queue_phases();

        // After sync: item should be Ready (promoted from DB state)
        assert_eq!(
            source.repos["org/repo"].queue.phase_of("issue:org/repo:1"),
            Some(QueuePhase::Ready)
        );
    }
}
