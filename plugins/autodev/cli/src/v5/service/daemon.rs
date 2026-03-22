use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::Result;

use crate::v5::core::action::Action;
use crate::v5::core::context::HistoryEntry;
use crate::v5::core::datasource::DataSource;
use crate::v5::core::escalation::EscalationAction;
use crate::v5::core::phase::V5QueuePhase;
use crate::v5::core::queue_item::V5QueueItem;
use crate::v5::core::runtime::RuntimeRegistry;
use crate::v5::core::state_machine;
use crate::v5::core::workspace::{StateConfig, WorkspaceConfig};
use crate::v5::service::concurrency::ConcurrencyTracker;
use crate::v5::service::executor::{ActionEnv, ActionExecutor};
use crate::v5::service::worktree::WorktreeManager;

/// V5 Daemon — 상태 머신 + yaml prompt/script 실행기.
///
/// 핵심 루프:
///   1. Collect: DataSource.collect() → Pending
///   2. Transition: Pending → Ready → Running (concurrency 제한)
///   3. Execute: yaml 정의 handler 실행
///   4. Complete: handler 성공 → Completed
///   5. on_done/on_fail: script 실행
///   6. Classify: evaluate → Done or HITL
pub struct V5Daemon {
    config: WorkspaceConfig,
    sources: Vec<Box<dyn DataSource>>,
    executor: ActionExecutor,
    worktree_mgr: Box<dyn WorktreeManager>,
    tracker: ConcurrencyTracker,
    queue: VecDeque<V5QueueItem>,
    history: Vec<HistoryEntry>,
}

/// Daemon tick의 결과.
#[derive(Debug)]
pub struct TickResult {
    pub collected: usize,
    pub advanced: usize,
    pub executed: usize,
    pub completed: usize,
    pub failed: usize,
}

/// 단일 아이템 실행 결과.
#[derive(Debug)]
pub enum ItemOutcome {
    Completed(V5QueueItem),
    Failed {
        item: V5QueueItem,
        error: String,
        escalation: EscalationAction,
    },
    Skipped(V5QueueItem),
}

impl V5Daemon {
    pub fn new(
        config: WorkspaceConfig,
        sources: Vec<Box<dyn DataSource>>,
        registry: Arc<RuntimeRegistry>,
        worktree_mgr: Box<dyn WorktreeManager>,
        max_concurrent: u32,
    ) -> Self {
        Self {
            config,
            sources,
            executor: ActionExecutor::new(registry),
            worktree_mgr,
            tracker: ConcurrencyTracker::new(max_concurrent),
            queue: VecDeque::new(),
            history: Vec::new(),
        }
    }

    /// 1단계: DataSource에서 새 아이템을 수집하여 Pending 큐에 추가.
    pub async fn collect(&mut self) -> Result<usize> {
        let mut total = 0;
        for source in &mut self.sources {
            let items = source.collect(&self.config).await?;
            total += items.len();
            for item in items {
                // 중복 체크
                if !self.queue.iter().any(|q| q.work_id == item.work_id) {
                    self.queue.push_back(item);
                }
            }
        }
        Ok(total)
    }

    /// 2단계: Pending → Ready → Running 자동 전이.
    /// concurrency 제한에 따라 Running으로 전이할 수 있는 만큼만 전이.
    pub fn advance(&mut self) -> usize {
        let mut advanced = 0;

        // Pending → Ready (전부)
        for item in self.queue.iter_mut() {
            if item.phase == V5QueuePhase::Pending
                && state_machine::transit(V5QueuePhase::Pending, V5QueuePhase::Ready).is_ok()
            {
                item.phase = V5QueuePhase::Ready;
                advanced += 1;
            }
        }

        // Ready → Running (concurrency 제한)
        let ws_id = &self.config.name;
        let ws_concurrency = self
            .config
            .sources
            .values()
            .next()
            .map(|s| s.concurrency)
            .unwrap_or(1);

        for item in self.queue.iter_mut() {
            if item.phase == V5QueuePhase::Ready
                && self.tracker.can_spawn_in_workspace(ws_id, ws_concurrency)
            {
                item.phase = V5QueuePhase::Running;
                self.tracker.track(ws_id);
                advanced += 1;
            }
        }

        advanced
    }

    /// 3단계: Running 아이템의 handler를 실행한다.
    pub async fn execute_running(&mut self) -> Vec<ItemOutcome> {
        let mut outcomes = Vec::new();

        // Running 아이템을 분리
        let running_indices: Vec<usize> = self
            .queue
            .iter()
            .enumerate()
            .filter(|(_, item)| item.phase == V5QueuePhase::Running)
            .map(|(i, _)| i)
            .collect();

        for &idx in running_indices.iter().rev() {
            let mut item = self.queue.remove(idx).unwrap();
            let outcome = self.execute_item(&mut item).await;
            match &outcome {
                ItemOutcome::Completed(_) | ItemOutcome::Skipped(_) => {}
                ItemOutcome::Failed { .. } => {}
            }
            outcomes.push(outcome);
        }

        outcomes
    }

    /// 단일 아이템의 handler 체인을 실행하고 결과를 반환한다.
    async fn execute_item(&mut self, item: &mut V5QueueItem) -> ItemOutcome {
        let state_name = &item.state;
        let state_config = self.find_state_config(state_name);

        let state_config = match state_config {
            Some(cfg) => cfg.clone(),
            None => {
                item.phase = V5QueuePhase::Skipped;
                return ItemOutcome::Skipped(item.clone());
            }
        };

        // Worktree 생성/재사용
        let worktree = match self
            .worktree_mgr
            .create_or_reuse(&self.config.name, &item.source_id)
            .await
        {
            Ok(path) => path,
            Err(e) => {
                item.phase = V5QueuePhase::Failed;
                return ItemOutcome::Failed {
                    item: item.clone(),
                    error: format!("worktree creation failed: {e}"),
                    escalation: EscalationAction::Retry,
                };
            }
        };

        let env = ActionEnv::new(&item.work_id, &worktree);

        // on_enter scripts
        let on_enter: Vec<Action> = state_config.on_enter.iter().map(Action::from).collect();
        if let Err(e) = self.executor.execute_all(&on_enter, &env).await {
            tracing::warn!("on_enter failed for {}: {e}", item.work_id);
        }

        // handler 체인 실행
        let handlers: Vec<Action> = state_config.handlers.iter().map(Action::from).collect();
        let result = self.executor.execute_all(&handlers, &env).await;

        match result {
            Ok(Some(r)) if !r.success() => {
                // handler 실패 → escalation
                let failure_count = self.count_failures(&item.source_id, &item.state);
                let escalation = self.resolve_escalation(&item.state, failure_count + 1);

                self.record_history(item, "failed", Some(&r.stderr));

                // on_fail 실행 (retry가 아닌 경우만)
                if escalation.should_run_on_fail() {
                    let on_fail: Vec<Action> =
                        state_config.on_fail.iter().map(Action::from).collect();
                    let _ = self.executor.execute_all(&on_fail, &env).await;
                }

                self.handle_escalation(item, escalation);
                self.tracker.release(&self.config.name.clone());

                ItemOutcome::Failed {
                    item: item.clone(),
                    error: r.stderr.clone(),
                    escalation,
                }
            }
            Ok(_) => {
                // 모든 handler 성공 → Completed
                item.phase = V5QueuePhase::Completed;
                self.record_history(item, "completed", None);
                self.tracker.release(&self.config.name.clone());
                self.queue.push_back(item.clone());
                ItemOutcome::Completed(item.clone())
            }
            Err(e) => {
                item.phase = V5QueuePhase::Failed;
                self.record_history(item, "failed", Some(&e.to_string()));
                self.tracker.release(&self.config.name.clone());

                ItemOutcome::Failed {
                    item: item.clone(),
                    error: e.to_string(),
                    escalation: EscalationAction::Retry,
                }
            }
        }
    }

    /// on_done script 실행. 성공 시 Done, 실패 시 Failed.
    pub async fn execute_on_done(&mut self, item: &mut V5QueueItem) -> Result<bool> {
        let state_config = self.find_state_config(&item.state).cloned();
        let state_config = match state_config {
            Some(cfg) => cfg,
            None => {
                item.phase = V5QueuePhase::Done;
                return Ok(true);
            }
        };

        if state_config.on_done.is_empty() {
            item.phase = V5QueuePhase::Done;
            self.record_history(item, "done", None);
            // worktree cleanup
            let worktree = self
                .worktree_mgr
                .create_or_reuse(&self.config.name, &item.source_id)
                .await?;
            let _ = self.worktree_mgr.cleanup(&worktree).await;
            return Ok(true);
        }

        let worktree = self
            .worktree_mgr
            .create_or_reuse(&self.config.name, &item.source_id)
            .await?;
        let env = ActionEnv::new(&item.work_id, &worktree);
        let on_done: Vec<Action> = state_config.on_done.iter().map(Action::from).collect();
        let result = self.executor.execute_all(&on_done, &env).await?;

        match result {
            Some(r) if !r.success() => {
                item.phase = V5QueuePhase::Failed;
                self.record_history(item, "failed", Some("on_done script failed"));
                Ok(false)
            }
            _ => {
                item.phase = V5QueuePhase::Done;
                self.record_history(item, "done", None);
                let _ = self.worktree_mgr.cleanup(&worktree).await;
                Ok(true)
            }
        }
    }

    /// retry-script: Failed 아이템의 on_done을 재실행.
    pub async fn retry_script(&mut self, work_id: &str) -> Result<bool> {
        let item_idx = self
            .queue
            .iter()
            .position(|i| i.work_id == work_id && i.phase == V5QueuePhase::Failed);

        if let Some(idx) = item_idx {
            let mut item = self.queue.remove(idx).unwrap();
            let success = self.execute_on_done(&mut item).await?;
            if !success {
                self.queue.push_back(item);
            }
            Ok(success)
        } else {
            anyhow::bail!("item not found or not in Failed phase: {work_id}");
        }
    }

    // --- 내부 헬퍼 ---

    fn find_state_config(&self, state: &str) -> Option<&StateConfig> {
        for source in self.config.sources.values() {
            if let Some(cfg) = source.states.get(state) {
                return Some(cfg);
            }
        }
        None
    }

    fn count_failures(&self, _source_id: &str, state: &str) -> u32 {
        self.history
            .iter()
            .filter(|h| {
                h.state == state && h.status == crate::v5::core::context::HistoryStatus::Failed
            })
            .count() as u32
    }

    fn resolve_escalation(&self, _state: &str, failure_count: u32) -> EscalationAction {
        let policy = self
            .config
            .sources
            .values()
            .next()
            .map(|s| &s.escalation)
            .cloned()
            .unwrap_or_default();
        policy.resolve(failure_count)
    }

    fn handle_escalation(&mut self, item: &mut V5QueueItem, action: EscalationAction) {
        match action {
            EscalationAction::Retry | EscalationAction::RetryWithComment => {
                // 새 아이템으로 재시도 (같은 source_id, 같은 state)
                let mut retry_item = item.clone();
                retry_item.phase = V5QueuePhase::Pending;
                retry_item.updated_at = chrono::Utc::now().to_rfc3339();
                self.queue.push_back(retry_item);
            }
            EscalationAction::Skip => {
                item.phase = V5QueuePhase::Skipped;
            }
            EscalationAction::Hitl | EscalationAction::Replan => {
                item.phase = V5QueuePhase::Hitl;
                self.queue.push_back(item.clone());
            }
        }
    }

    fn record_history(&mut self, item: &V5QueueItem, status: &str, error: Option<&str>) {
        let attempt = self
            .history
            .iter()
            .filter(|h| h.state == item.state)
            .count() as u32
            + 1;

        self.history.push(HistoryEntry {
            state: item.state.clone(),
            status: status
                .parse()
                .unwrap_or(crate::v5::core::context::HistoryStatus::Failed),
            attempt,
            summary: None,
            error: error.map(|s| s.to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
        });
    }

    // --- 조회 ---

    pub fn queue_items(&self) -> &VecDeque<V5QueueItem> {
        &self.queue
    }

    pub fn items_in_phase(&self, phase: V5QueuePhase) -> Vec<&V5QueueItem> {
        self.queue.iter().filter(|i| i.phase == phase).collect()
    }

    pub fn history(&self) -> &[HistoryEntry] {
        &self.history
    }

    /// 큐에 아이템을 직접 추가한다 (테스트용).
    pub fn push_item(&mut self, item: V5QueueItem) {
        self.queue.push_back(item);
    }

    /// DataSource를 교체한다 (테스트용).
    pub fn replace_sources(&mut self, sources: Vec<Box<dyn DataSource>>) {
        self.sources = sources;
    }

    /// tokio::select! 기반 async event loop.
    ///
    /// v4 Daemon.run()과 동일한 패턴:
    ///   Arm 1: tick → collect + advance + execute
    ///   Arm 2: SIGINT → graceful shutdown
    pub async fn run(&mut self, tick_interval_secs: u64) {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(tick_interval_secs));
        tracing::info!("v5 daemon started (tick={}s)", tick_interval_secs);

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if let Err(e) = self.tick().await {
                        tracing::error!("tick error: {e}");
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("received SIGINT, shutting down v5 daemon...");
                    break;
                }
            }
        }

        // Graceful shutdown: Completed 아이템의 on_done 처리
        let completed: Vec<String> = self
            .items_in_phase(V5QueuePhase::Completed)
            .iter()
            .map(|i| i.work_id.clone())
            .collect();

        for work_id in completed {
            if let Some(idx) = self.queue.iter().position(|i| i.work_id == work_id) {
                let mut item = self.queue.remove(idx).unwrap();
                let _ = self.execute_on_done(&mut item).await;
            }
        }

        tracing::info!("v5 daemon stopped");
    }

    /// 단일 tick: collect → advance → execute → on_done.
    pub async fn tick(&mut self) -> Result<()> {
        // 1. Collect
        let collected = self.collect().await?;
        if collected > 0 {
            tracing::info!("collected {collected} items");
        }

        // 2. Advance
        let advanced = self.advance();
        if advanced > 0 {
            tracing::debug!("advanced {advanced} items");
        }

        // 3. Execute Running items
        let outcomes = self.execute_running().await;
        for outcome in &outcomes {
            match outcome {
                ItemOutcome::Completed(item) => {
                    tracing::info!("completed: {}", item.work_id);
                }
                ItemOutcome::Failed {
                    item,
                    error,
                    escalation,
                } => {
                    tracing::warn!(
                        "failed: {} (escalation={:?}, error={})",
                        item.work_id,
                        escalation,
                        error
                    );
                }
                ItemOutcome::Skipped(item) => {
                    tracing::info!("skipped: {}", item.work_id);
                }
            }
        }

        // 4. Process Completed → Done (on_done)
        let completed: Vec<String> = self
            .items_in_phase(V5QueuePhase::Completed)
            .iter()
            .map(|i| i.work_id.clone())
            .collect();

        for work_id in completed {
            if let Some(idx) = self.queue.iter().position(|i| i.work_id == work_id) {
                let mut item = self.queue.remove(idx).unwrap();
                match self.execute_on_done(&mut item).await {
                    Ok(true) => tracing::info!("done: {}", item.work_id),
                    Ok(false) => {
                        tracing::warn!("on_done failed: {}", item.work_id);
                    }
                    Err(e) => {
                        tracing::error!("on_done error for {}: {e}", item.work_id);
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v5::core::queue_item::testing::test_item;
    use crate::v5::infra::runtimes::mock::MockRuntime;
    use crate::v5::infra::sources::mock::MockDataSource;
    use crate::v5::service::worktree::MockWorktreeManager;
    use tempfile::TempDir;

    fn test_workspace_config() -> WorkspaceConfig {
        let yaml = r#"
name: test-ws
sources:
  github:
    url: https://github.com/org/repo
    concurrency: 2
    states:
      analyze:
        trigger:
          label: "autodev:analyze"
        handlers:
          - prompt: "analyze this issue"
        on_done:
          - script: "echo done"
      implement:
        trigger:
          label: "autodev:implement"
        handlers:
          - prompt: "implement this"
          - script: "echo test"
        on_done:
          - script: "echo created PR"
        on_fail:
          - script: "echo failed"
    escalation:
      1: retry
      2: retry_with_comment
      3: hitl
"#;
        serde_yml::from_str(yaml).unwrap()
    }

    fn setup_daemon(tmp: &TempDir, source: MockDataSource, exit_codes: Vec<i32>) -> V5Daemon {
        let config = test_workspace_config();
        let mut registry = RuntimeRegistry::new("mock".to_string());
        registry.register(Arc::new(MockRuntime::new("mock", exit_codes)));
        let worktree_mgr = MockWorktreeManager::new(tmp.path());

        V5Daemon::new(
            config,
            vec![Box::new(source)],
            Arc::new(registry),
            Box::new(worktree_mgr),
            4,
        )
    }

    #[tokio::test]
    async fn collect_adds_to_queue() {
        let tmp = TempDir::new().unwrap();
        let mut source = MockDataSource::new("github");
        source.add_item(test_item("github:org/repo#1", "analyze"));
        source.add_item(test_item("github:org/repo#2", "implement"));

        let mut daemon = setup_daemon(&tmp, source, vec![]);
        let collected = daemon.collect().await.unwrap();
        assert_eq!(collected, 2);
        assert_eq!(daemon.queue_items().len(), 2);
    }

    #[tokio::test]
    async fn collect_deduplicates() {
        let tmp = TempDir::new().unwrap();
        let source = MockDataSource::new("github");
        let mut daemon = setup_daemon(&tmp, source, vec![]);

        // 수동으로 큐에 추가
        daemon
            .queue
            .push_back(test_item("github:org/repo#1", "analyze"));

        // 같은 work_id 수집 시도
        let mut source2 = MockDataSource::new("github");
        source2.add_item(test_item("github:org/repo#1", "analyze"));
        daemon.sources = vec![Box::new(source2)];

        let collected = daemon.collect().await.unwrap();
        assert_eq!(collected, 1); // source가 1개 반환
        assert_eq!(daemon.queue_items().len(), 1); // 하지만 큐에는 중복 없음
    }

    #[tokio::test]
    async fn advance_pending_to_ready_to_running() {
        let tmp = TempDir::new().unwrap();
        let mut source = MockDataSource::new("github");
        source.add_item(test_item("github:org/repo#1", "analyze"));

        let mut daemon = setup_daemon(&tmp, source, vec![]);
        daemon.collect().await.unwrap();

        let advanced = daemon.advance();
        assert!(advanced >= 1);

        // Running 상태 아이템이 있어야 함
        let running = daemon.items_in_phase(V5QueuePhase::Running);
        assert_eq!(running.len(), 1);
    }

    #[tokio::test]
    async fn advance_respects_concurrency() {
        let tmp = TempDir::new().unwrap();
        let mut source = MockDataSource::new("github");
        // concurrency=2인 워크스페이스에 3개 아이템
        source.add_item(test_item("github:org/repo#1", "analyze"));
        source.add_item(test_item("github:org/repo#2", "analyze"));
        source.add_item(test_item("github:org/repo#3", "analyze"));

        let mut daemon = setup_daemon(&tmp, source, vec![]);
        daemon.collect().await.unwrap();
        daemon.advance();

        let running = daemon.items_in_phase(V5QueuePhase::Running);
        assert_eq!(running.len(), 2); // concurrency=2이므로 2개만
    }

    #[tokio::test]
    async fn execute_handler_success() {
        let tmp = TempDir::new().unwrap();
        let mut source = MockDataSource::new("github");
        source.add_item(test_item("github:org/repo#1", "analyze"));

        // handler 성공 (exit_code=0)
        let mut daemon = setup_daemon(&tmp, source, vec![0]);
        daemon.collect().await.unwrap();
        daemon.advance();

        let outcomes = daemon.execute_running().await;
        assert_eq!(outcomes.len(), 1);
        match &outcomes[0] {
            ItemOutcome::Completed(item) => {
                assert_eq!(item.phase, V5QueuePhase::Completed);
            }
            other => panic!("expected Completed, got {other:?}"),
        }

        // Completed 아이템이 큐에 있어야 함
        let completed = daemon.items_in_phase(V5QueuePhase::Completed);
        assert_eq!(completed.len(), 1);
    }

    #[tokio::test]
    async fn execute_handler_failure_triggers_escalation() {
        let tmp = TempDir::new().unwrap();
        let mut source = MockDataSource::new("github");
        source.add_item(test_item("github:org/repo#1", "analyze"));

        // handler 실패 (exit_code=1)
        let mut daemon = setup_daemon(&tmp, source, vec![1]);
        daemon.collect().await.unwrap();
        daemon.advance();

        let outcomes = daemon.execute_running().await;
        assert_eq!(outcomes.len(), 1);
        match &outcomes[0] {
            ItemOutcome::Failed { escalation, .. } => {
                // failure_count=1 → Retry
                assert_eq!(*escalation, EscalationAction::Retry);
            }
            other => panic!("expected Failed, got {other:?}"),
        }

        // retry로 인해 Pending 아이템이 큐에 추가됨
        let pending = daemon.items_in_phase(V5QueuePhase::Pending);
        assert_eq!(pending.len(), 1);
    }

    #[tokio::test]
    async fn on_done_success_transitions_to_done() {
        let tmp = TempDir::new().unwrap();
        let source = MockDataSource::new("github");
        let mut daemon = setup_daemon(&tmp, source, vec![]);

        let mut item = test_item("github:org/repo#1", "analyze");
        item.phase = V5QueuePhase::Completed;

        let success = daemon.execute_on_done(&mut item).await.unwrap();
        assert!(success);
        assert_eq!(item.phase, V5QueuePhase::Done);
    }

    #[tokio::test]
    async fn full_pipeline_collect_advance_execute() {
        let tmp = TempDir::new().unwrap();
        let mut source = MockDataSource::new("github");
        source.add_item(test_item("github:org/repo#1", "analyze"));

        // handler 성공
        let mut daemon = setup_daemon(&tmp, source, vec![0]);

        // 1. Collect
        daemon.collect().await.unwrap();
        assert_eq!(daemon.items_in_phase(V5QueuePhase::Pending).len(), 1);

        // 2. Advance
        daemon.advance();
        assert_eq!(daemon.items_in_phase(V5QueuePhase::Running).len(), 1);

        // 3. Execute
        let outcomes = daemon.execute_running().await;
        assert_eq!(outcomes.len(), 1);
        assert!(matches!(outcomes[0], ItemOutcome::Completed(_)));

        // 4. Completed → Done (on_done)
        let completed: Vec<V5QueueItem> = daemon
            .items_in_phase(V5QueuePhase::Completed)
            .into_iter()
            .cloned()
            .collect();
        for mut item in completed {
            let idx = daemon
                .queue
                .iter()
                .position(|i| i.work_id == item.work_id)
                .unwrap();
            daemon.queue.remove(idx);
            daemon.execute_on_done(&mut item).await.unwrap();
        }

        // History에 기록됨
        assert!(daemon.history().len() >= 2); // completed + done
    }

    #[tokio::test]
    async fn state_not_found_skips() {
        let tmp = TempDir::new().unwrap();
        let mut source = MockDataSource::new("github");
        source.add_item(test_item("github:org/repo#1", "nonexistent_state"));

        let mut daemon = setup_daemon(&tmp, source, vec![]);
        daemon.collect().await.unwrap();
        daemon.advance();

        let outcomes = daemon.execute_running().await;
        assert_eq!(outcomes.len(), 1);
        assert!(matches!(outcomes[0], ItemOutcome::Skipped(_)));
    }
}
