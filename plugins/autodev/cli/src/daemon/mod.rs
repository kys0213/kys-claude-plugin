pub mod agent;
pub mod agent_impl;
pub mod daily_reporter;
pub mod log;
pub mod pid;
pub mod status;
pub mod task;
pub mod task_manager;
pub mod task_manager_impl;
pub mod task_runner;
pub mod task_runner_impl;
pub mod task_source;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Result};
use tokio::task::JoinSet;
use tracing::info;

use crate::components::workspace::OwnedWorkspace;
use crate::config::{self, Env};
use crate::domain::git_repository_factory::GitRepositoryFactory;
use crate::domain::repository::ConsumerLogRepository;
use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::infrastructure::suggest_workflow::SuggestWorkflow;
use crate::queue::Database;
use crate::sources::github::GitHubTaskSource;

use self::agent_impl::ClaudeAgent;
use self::daily_reporter::DailyReporter;
use self::task::TaskResult;
use self::task_manager::TaskManager;
use self::task_runner::TaskRunner;
use self::task_runner_impl::DefaultTaskRunner;

// ─── In-Flight Concurrency Tracker ───

/// Spawned task 동시 실행 제한기.
/// per-repo 카운트 + 전역 상한으로 Claude 세션 수를 제한한다.
struct InFlightTracker {
    per_repo: HashMap<String, usize>,
    total: usize,
    max_total: usize,
}

impl InFlightTracker {
    fn new(max_total: u32) -> Self {
        Self {
            per_repo: HashMap::new(),
            total: 0,
            max_total: max_total as usize,
        }
    }

    fn can_spawn(&self) -> bool {
        self.total < self.max_total
    }

    fn track(&mut self, repo_name: &str) {
        *self.per_repo.entry(repo_name.to_string()).or_default() += 1;
        self.total += 1;
    }

    fn release(&mut self, repo_name: &str) {
        if let Some(count) = self.per_repo.get_mut(repo_name) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.per_repo.remove(repo_name);
            }
        }
        self.total = self.total.saturating_sub(1);
    }
}

// ─── Task Spawner ───

/// pending_tasks 버퍼에서 InFlightTracker 상한까지 Task를 꺼내 spawn한다.
fn try_spawn(
    pending: &mut Vec<Box<dyn task::Task>>,
    tracker: &mut InFlightTracker,
    join_set: &mut JoinSet<TaskResult>,
    runner: &Arc<dyn TaskRunner>,
) {
    while tracker.can_spawn() {
        let task = match pending.pop() {
            Some(t) => t,
            None => break,
        };
        tracker.track(task.repo_name());
        let r = Arc::clone(runner);
        join_set.spawn(async move { r.run(task).await });
    }
}

// ─── Daemon ───

/// 데몬 이벤트 루프를 관리하는 구조체.
///
/// trait 기반 의존성 주입으로 테스트 가능:
/// - `TaskManager`: Task 수집 + 분배
/// - `TaskRunner`: Task 생명주기 실행
/// - `DailyReporter`: 일간 보고서 생성
pub struct Daemon {
    manager: Box<dyn TaskManager>,
    runner: Arc<dyn TaskRunner>,
    reporter: Box<dyn DailyReporter>,
    tracker: InFlightTracker,
    log_db: Database,
    status_path: PathBuf,
    counters: status::StatusCounters,
    tick_interval_secs: u64,
}

impl Daemon {
    pub fn new(
        manager: Box<dyn TaskManager>,
        runner: Arc<dyn TaskRunner>,
        reporter: Box<dyn DailyReporter>,
        max_concurrent_tasks: u32,
        log_db: Database,
        status_path: PathBuf,
        tick_interval_secs: u64,
    ) -> Self {
        Self {
            manager,
            runner,
            reporter,
            tracker: InFlightTracker::new(max_concurrent_tasks),
            log_db,
            status_path,
            counters: status::StatusCounters::default(),
            tick_interval_secs,
        }
    }

    /// 메인 이벤트 루프 실행.
    ///
    /// task completion / tick / status heartbeat / shutdown 4개 arm으로 구성.
    /// SIGINT 수신 시 in-flight tasks를 대기한 뒤 종료한다.
    pub async fn run(&mut self) {
        let start_time = std::time::Instant::now();
        let mut join_set: JoinSet<TaskResult> = JoinSet::new();
        let mut pending_tasks: Vec<Box<dyn task::Task>> = Vec::new();

        let mut tick =
            tokio::time::interval(std::time::Duration::from_secs(self.tick_interval_secs));
        let mut status_tick = tokio::time::interval(std::time::Duration::from_secs(5));

        loop {
            tokio::select! {
                // ── Task completion ──
                Some(result) = join_set.join_next() => {
                    match result {
                        Ok(task_result) => {
                            self.tracker.release(&task_result.repo_name);
                            info!(
                                "task completed: {} - {} (in-flight: {})",
                                task_result.work_id, task_result.status, self.tracker.total
                            );
                            self.manager.apply(&task_result);
                            for log_entry in &task_result.logs {
                                let _ = self.log_db.log_insert(log_entry);
                            }
                        }
                        Err(e) => {
                            tracing::error!("spawned task panicked: {e}");
                            self.tracker.total = self.tracker.total.saturating_sub(1);
                        }
                    }

                    try_spawn(&mut pending_tasks, &mut self.tracker, &mut join_set, &self.runner);
                }

                // ── Tick: housekeeping + spawn + daily report ──
                _ = tick.tick() => {
                    self.manager.tick().await;
                    pending_tasks.extend(self.manager.drain_ready());

                    try_spawn(&mut pending_tasks, &mut self.tracker, &mut join_set, &self.runner);

                    self.reporter.maybe_run().await;
                }

                // ── Status heartbeat ──
                _ = status_tick.tick() => {
                    let ds = status::build_status(
                        self.manager.active_items(), &self.counters, start_time,
                    );
                    status::write_status(&self.status_path, &ds);
                }

                // ── Graceful shutdown ──
                _ = tokio::signal::ctrl_c() => {
                    info!("received SIGINT, shutting down...");
                    break;
                }
            }
        }

        // Wait for in-flight tasks to complete
        if !join_set.is_empty() {
            info!("waiting for {} in-flight tasks...", join_set.len());
            while let Some(result) = join_set.join_next().await {
                if let Ok(task_result) = result {
                    self.tracker.release(&task_result.repo_name);
                    self.manager.apply(&task_result);
                    for log_entry in &task_result.logs {
                        let _ = self.log_db.log_insert(log_entry);
                    }
                }
            }
        }

        status::remove_status(&self.status_path);
    }
}

// ─── Daemon Entry Point ───

/// 데몬을 포그라운드로 시작 (non-blocking event loop)
pub async fn start(
    home: &Path,
    env: Arc<dyn Env>,
    gh: Arc<dyn Gh>,
    git: Arc<dyn Git>,
    claude: Arc<dyn Claude>,
    sw: Arc<dyn SuggestWorkflow>,
) -> Result<()> {
    if pid::is_running(home) {
        bail!(
            "daemon is already running (pid: {})",
            pid::read_pid(home).unwrap_or(0)
        );
    }

    info!("starting autodev daemon...");

    pid::write_pid(home)?;

    let cfg = config::loader::load_merged(&*env, None);

    let db_path = home.join("autodev.db");
    // Source DB: owned by GitHubTaskSource (repo sync, cursor operations)
    let source_db = Database::open(&db_path)?;
    source_db.initialize()?;
    // Logging DB: separate connection for task result logging
    let log_db = Database::open(&db_path)?;
    // Report DB: separate connection for daily reporter (repo_find_enabled + knowledge logs)
    let report_db = Database::open(&db_path)?;

    println!("autodev daemon started (pid: {})", std::process::id());

    // ── TaskRunner: ClaudeAgent → DefaultTaskRunner ──
    let agent = Arc::new(ClaudeAgent::new(Arc::clone(&claude)));
    let runner: Arc<dyn TaskRunner> = Arc::new(DefaultTaskRunner::new(agent));

    // ── TaskSource: GitHubTaskSource ──
    let workspace = Arc::new(OwnedWorkspace::new(Arc::clone(&git), Arc::clone(&env)));
    let config_loader = Arc::new(config::RealConfigLoader::new(Box::new(EnvClone(
        Arc::clone(&env),
    ))));
    let mut source = GitHubTaskSource::new(
        workspace,
        Arc::clone(&gh),
        config_loader,
        Arc::clone(&env),
        Arc::clone(&git),
        Arc::clone(&sw),
        source_db,
    );

    // ── Startup Reconcile ──
    match GitRepositoryFactory::create_all(&log_db, &*env, &*gh).await {
        Ok(mut repo_map) => {
            let mut total_recovered = 0u64;
            for repo in repo_map.values_mut() {
                let n = repo.startup_reconcile(&*gh).await;
                if n > 0 {
                    total_recovered += n;
                }
            }
            if total_recovered > 0 {
                info!("startup reconcile: recovered {total_recovered} items");
            }
            source.set_repos(repo_map);
        }
        Err(e) => tracing::error!("startup reconcile failed: {e}"),
    }

    // ── TaskManager: DefaultTaskManager wrapping source ──
    let manager: Box<dyn TaskManager> =
        Box::new(task_manager_impl::DefaultTaskManager::new(vec![Box::new(
            source,
        )]));

    // ── DailyReporter ──
    let log_dir = config::resolve_log_dir(&cfg.daemon.log_dir, home);
    let reporter: Box<dyn DailyReporter> = Box::new(daily_reporter::DefaultDailyReporter::new(
        Arc::clone(&gh),
        Arc::clone(&claude),
        Arc::clone(&git),
        Arc::clone(&env),
        Arc::clone(&sw),
        report_db,
        log_dir.clone(),
        cfg.daemon.log_retention_days,
        cfg.daemon.daily_report_hour,
        cfg.sources.github.knowledge_extraction,
    ));

    // ── Startup log cleanup ──
    let n = log::cleanup_old_logs(&log_dir, cfg.daemon.log_retention_days);
    if n > 0 {
        info!("startup log cleanup: deleted {n} old log files");
    }

    info!(
        "event loop starting (max_concurrent_tasks={})",
        cfg.daemon.max_concurrent_tasks
    );

    // ── Daemon ──
    let status_path = home.join("daemon.status.json");
    let mut daemon = Daemon::new(
        manager,
        runner,
        reporter,
        cfg.daemon.max_concurrent_tasks,
        log_db,
        status_path,
        cfg.daemon.tick_interval_secs,
    );

    daemon.run().await;

    pid::remove_pid(home);
    Ok(())
}

/// Arc<dyn Env>를 Box<dyn Env>로 변환하기 위한 어댑터.
struct EnvClone(Arc<dyn Env>);

impl Env for EnvClone {
    fn var(&self, key: &str) -> Result<String, std::env::VarError> {
        self.0.var(key)
    }
}

/// 데몬 중지 (PID → SIGTERM)
pub fn stop(home: &Path) -> Result<()> {
    let pid = pid::read_pid(home).ok_or_else(|| anyhow::anyhow!("daemon is not running"))?;

    std::process::Command::new("kill")
        .arg(pid.to_string())
        .status()?;

    pid::remove_pid(home);
    println!("autodev daemon stopped (pid: {pid})");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════
    // InFlightTracker 테스트
    // ═══════════════════════════════════════════════

    #[test]
    fn tracker_respects_max_total() {
        let mut t = InFlightTracker::new(2);
        assert!(t.can_spawn());
        t.track("org/repo-a");
        assert!(t.can_spawn());
        t.track("org/repo-b");
        assert!(!t.can_spawn());
        t.release("org/repo-a");
        assert!(t.can_spawn());
    }

    #[test]
    fn tracker_per_repo_cleanup() {
        let mut t = InFlightTracker::new(10);
        t.track("org/repo");
        t.track("org/repo");
        assert_eq!(t.per_repo["org/repo"], 2);
        t.release("org/repo");
        assert_eq!(t.per_repo["org/repo"], 1);
        t.release("org/repo");
        assert!(!t.per_repo.contains_key("org/repo"));
    }
}
