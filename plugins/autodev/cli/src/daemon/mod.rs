pub mod agent;
pub mod agent_impl;
pub mod log;
pub mod pid;
#[allow(dead_code)]
pub mod recovery;
pub mod status;
pub mod task;
pub mod task_context;
pub mod task_manager;
pub mod task_manager_impl;
pub mod task_runner;
pub mod task_runner_impl;
pub mod task_source;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Result};
use chrono::Timelike;
use tokio::task::JoinSet;
use tracing::info;

use crate::components::workspace::{OwnedWorkspace, Workspace};
use crate::config::{self, Env};
use crate::domain::git_repository::GitRepository;
use crate::domain::git_repository_factory::GitRepositoryFactory;
use crate::domain::repository::{ConsumerLogRepository, RepoRepository};
use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::infrastructure::suggest_workflow::SuggestWorkflow;
use crate::queue::Database;
use crate::sources::github::GitHubTaskSource;

use self::agent_impl::ClaudeAgent;
use self::task::TaskResult;
use self::task_runner::TaskRunner;
use self::task_runner_impl::DefaultTaskRunner;
use self::task_source::TaskSource;

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
    // Logging DB: separate connection for DB logging + daily reports
    let log_db = Database::open(&db_path)?;

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
        workspace.clone(),
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

    let mut tracker = InFlightTracker::new(cfg.daemon.max_concurrent_tasks);
    let mut join_set: JoinSet<TaskResult> = JoinSet::new();
    let mut pending_tasks: Vec<Box<dyn task::Task>> = Vec::new();

    let daily_report_hour = cfg.daemon.daily_report_hour;
    let knowledge_extraction = cfg.consumer.knowledge_extraction;
    let mut last_daily_report_date = String::new();

    let start_time = std::time::Instant::now();
    let status_path = home.join("daemon.status.json");
    let counters = status::StatusCounters::default();

    let tick_interval_secs = cfg.daemon.tick_interval_secs;

    let log_dir = config::resolve_log_dir(&cfg.daemon.log_dir, home);
    let log_retention_days = cfg.daemon.log_retention_days;

    // Startup cleanup: 보존 기간 초과 로그 삭제
    let n = log::cleanup_old_logs(&log_dir, log_retention_days);
    if n > 0 {
        info!("startup log cleanup: deleted {n} old log files");
    }

    info!(
        "event loop starting (max_concurrent_tasks={})",
        cfg.daemon.max_concurrent_tasks
    );

    // 메인 이벤트 루프: task completion / tick / status / shutdown
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(tick_interval_secs));
    let mut status_tick = tokio::time::interval(std::time::Duration::from_secs(5));

    loop {
        tokio::select! {
            // ── Task completion ──
            Some(result) = join_set.join_next() => {
                match result {
                    Ok(task_result) => {
                        tracker.release(&task_result.repo_name);
                        info!(
                            "task completed: {} - {} (in-flight: {})",
                            task_result.work_id, task_result.status, tracker.total
                        );
                        // Apply queue ops to the per-repo queues
                        source.apply(&task_result);
                        // DB logging
                        for log_entry in &task_result.logs {
                            let _ = log_db.log_insert(log_entry);
                        }
                    }
                    Err(e) => {
                        // Task panicked — item stays in working phase.
                        // Startup recovery will clean up on next restart.
                        tracing::error!("spawned task panicked: {e}");
                        tracker.total = tracker.total.saturating_sub(1);
                    }
                }

                // Task 완료 후 즉시 새 task spawn 시도 (tick 대기 불필요)
                try_spawn(&mut pending_tasks, &mut tracker, &mut join_set, &runner);
            }

            // ── Tick: housekeeping + spawn ──
            _ = tick.tick() => {
                // poll: repo sync → recovery → scan → drain queues → Task 생성
                let new_tasks = source.poll().await;
                pending_tasks.extend(new_tasks);

                // Spawn ready tasks
                try_spawn(&mut pending_tasks, &mut tracker, &mut join_set, &runner);

                // Daily Report (scheduled at daily_report_hour)
                if knowledge_extraction {
                    let now = chrono::Local::now();
                    let today = now.format("%Y-%m-%d").to_string();
                    if now.hour() >= daily_report_hour && last_daily_report_date != today {
                        let yesterday = (now - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
                        let log_path = log_dir.join(format!("daemon.{yesterday}.log"));

                        log::cleanup_old_logs(&log_dir, log_retention_days);

                        if log_path.exists() {
                            let stats = crate::knowledge::daily::parse_daemon_log(&log_path);
                            if stats.task_count > 0 {
                                let patterns = crate::knowledge::daily::detect_patterns(&stats);
                                let mut report = crate::knowledge::daily::build_daily_report(&yesterday, &stats, patterns);

                                let ws = Workspace::new(&*git, &*env);
                                if let Ok(enabled) = RepoRepository::repo_find_enabled(&log_db) {
                                    if let Some(er) = enabled.first() {
                                        if let Ok(base) = ws.ensure_cloned(&er.url, &er.name).await {
                                            crate::knowledge::daily::enrich_with_cross_analysis(
                                                &mut report, &*sw,
                                            ).await;

                                            let per_task = crate::knowledge::daily::aggregate_daily_suggestions(&log_db, &yesterday);

                                            if let Some(ks) = crate::knowledge::daily::generate_daily_suggestions(
                                                &*claude, &report, &base,
                                            ).await {
                                                report.suggestions = ks.suggestions;
                                            }

                                            report.suggestions.extend(per_task);

                                            if !report.suggestions.is_empty() {
                                                let cross_patterns = crate::knowledge::daily::detect_cross_task_patterns(&report.suggestions);
                                                report.patterns.extend(cross_patterns);
                                            }

                                            // Use repo's gh_host directly
                                            let repo_gh_host = source.repos()
                                                .get(&er.name)
                                                .and_then(|r: &GitRepository| r.gh_host());
                                            crate::knowledge::daily::post_daily_report(
                                                &*gh, &er.name, &report, repo_gh_host,
                                            ).await;

                                            if !report.suggestions.is_empty() {
                                                crate::knowledge::daily::create_knowledge_prs(
                                                    &*gh, &ws, &er.name, &report,
                                                    repo_gh_host,
                                                ).await;
                                            }
                                        }
                                    }
                                }

                                info!("daily report generated for {yesterday}");
                            }
                        }

                        last_daily_report_date = today;
                    }
                }
            }

            // ── Status heartbeat ──
            _ = status_tick.tick() => {
                let ds = status::build_status_from_repos(source.repos(), &counters, start_time);
                status::write_status(&status_path, &ds);
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
                tracker.release(&task_result.repo_name);
                source.apply(&task_result);
                for log_entry in &task_result.logs {
                    let _ = log_db.log_insert(log_entry);
                }
            }
        }
    }

    status::remove_status(&status_path);
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
