pub mod agent;
pub mod agent_impl;
pub mod collectors;
pub mod cron;
pub mod daily_reporter;
pub mod escalation;
pub mod log;
pub mod notifiers;
pub mod pid;
pub mod reply_scanner;
pub mod status;
pub mod task_manager;
pub mod task_manager_impl;
pub mod task_runner;
pub mod task_runner_impl;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Result};
use tokio::task::JoinSet;
use tracing::info;

use crate::core::config::{self, Env};
use crate::core::repository::{ConsumerLogRepository, TokenUsageRepository};
use crate::infra::claude::Claude;
use crate::infra::db::Database;
use crate::infra::gh::Gh;
use crate::infra::git::Git;
use crate::infra::suggest_workflow::SuggestWorkflow;
use crate::service::daemon::collectors::github::GitHubTaskSource;
use crate::service::tasks::helpers::git_ops_factory::GitRepositoryFactory;
use crate::service::tasks::helpers::workspace::OwnedWorkspace;

use self::agent_impl::ClaudeAgent;
use self::cron::engine::CronEngine;
use self::daily_reporter::DailyReporter;
use self::task_manager::TaskManager;
use self::task_runner::TaskRunner;
use self::task_runner_impl::DefaultTaskRunner;
use crate::core::notifier::NotificationEvent;
use crate::core::task::{TaskResult, TaskStatus};

// ─── In-Flight Concurrency Tracker ───

/// v5 2-level concurrency 제한기.
///
/// 두 단계의 동시 실행 제한을 적용한다:
/// - **Workspace level**: 워크스페이스(레포)당 동시 실행 상한 (`workspace_limits`)
/// - **Global level**: 전체 시스템 동시 실행 상한 (`max_global`)
///
/// Ready → Running 전이 시 두 제한을 모두 확인한다:
/// ```text
/// ws_slots = workspace.concurrency - per_workspace_running
/// global_slots = max_global - total_running - active_evaluate_count
/// spawnable = ws_slots > 0 && global_slots > 0
/// ```
struct InFlightTracker {
    per_repo: HashMap<String, usize>,
    total: usize,
    max_global: usize,
    /// 워크스페이스(레포)별 동시 실행 상한. 0이면 제한 없음.
    workspace_limits: HashMap<String, usize>,
    /// evaluate cron이 소비하는 active slot 수.
    active_evaluate_count: usize,
}

impl InFlightTracker {
    fn new(max_global: u32) -> Self {
        Self {
            per_repo: HashMap::new(),
            total: 0,
            max_global: max_global as usize,
            workspace_limits: HashMap::new(),
            active_evaluate_count: 0,
        }
    }

    /// 워크스페이스별 concurrency 상한을 설정한다.
    /// 0이면 해당 워크스페이스에 workspace-level 제한 없음.
    fn set_workspace_limit(&mut self, repo_name: &str, limit: usize) {
        if limit > 0 {
            self.workspace_limits.insert(repo_name.to_string(), limit);
        } else {
            self.workspace_limits.remove(repo_name);
        }
    }

    /// evaluate cron active slot 수를 갱신한다.
    fn set_active_evaluate_count(&mut self, count: usize) {
        self.active_evaluate_count = count;
    }

    /// 글로벌 레벨에서 spawn 가능한지 확인한다.
    fn has_global_slot(&self) -> bool {
        self.total + self.active_evaluate_count < self.max_global
    }

    /// 특정 워크스페이스에서 spawn 가능한지 확인한다.
    fn has_workspace_slot(&self, repo_name: &str) -> bool {
        match self.workspace_limits.get(repo_name) {
            Some(&limit) => {
                let running = self.per_repo.get(repo_name).copied().unwrap_or(0);
                running < limit
            }
            None => true, // 제한 없음
        }
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

/// pending_tasks 버퍼에서 2-level concurrency 상한까지 Task를 꺼내 spawn한다.
///
/// workspace + global 두 레벨을 모두 확인하여 slot이 있는 task만 spawn한다.
/// workspace slot이 부족한 task는 건너뛰되 버퍼에 잔류시킨다.
fn try_spawn(
    pending: &mut Vec<Box<dyn crate::core::task::Task>>,
    tracker: &mut InFlightTracker,
    join_set: &mut JoinSet<TaskResult>,
    runner: &Arc<dyn TaskRunner>,
) {
    let mut deferred: Vec<Box<dyn crate::core::task::Task>> = Vec::new();

    while let Some(task) = pending.pop() {
        if !tracker.has_global_slot() {
            // 글로벌 상한 도달 — 남은 task를 모두 되돌린다
            deferred.push(task);
            break;
        }
        if !tracker.has_workspace_slot(task.repo_name()) {
            // 이 workspace는 slot 부족 — 건너뛰고 다른 workspace task 시도
            deferred.push(task);
            continue;
        }
        tracker.track(task.repo_name());
        let r = Arc::clone(runner);
        join_set.spawn(async move { r.run(task).await });
    }

    // 글로벌 상한 도달로 pop하지 못한 나머지 + deferred를 되돌린다
    deferred.append(pending);
    *pending = deferred;
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
    tick_interval_secs: u64,
    cron_engine: Option<CronEngine>,
    notifier: Option<notifiers::dispatcher::NotificationDispatcher>,
    shutdown_drain_timeout_secs: u64,
    escalation_config: config::models::EscalationConfig,
}

impl Daemon {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        manager: Box<dyn TaskManager>,
        runner: Arc<dyn TaskRunner>,
        reporter: Box<dyn DailyReporter>,
        max_concurrent_tasks: u32,
        log_db: Database,
        status_path: PathBuf,
        tick_interval_secs: u64,
        shutdown_drain_timeout_secs: u64,
        escalation_config: config::models::EscalationConfig,
    ) -> Self {
        Self {
            manager,
            runner,
            reporter,
            tracker: InFlightTracker::new(max_concurrent_tasks),
            log_db,
            status_path,
            tick_interval_secs,
            cron_engine: None,
            notifier: None,
            shutdown_drain_timeout_secs,
            escalation_config,
        }
    }

    pub fn with_cron_engine(mut self, engine: CronEngine) -> Self {
        self.cron_engine = Some(engine);
        self
    }

    pub fn with_notifier(
        mut self,
        notifier: notifiers::dispatcher::NotificationDispatcher,
    ) -> Self {
        self.notifier = Some(notifier);
        self
    }

    /// 완료된 태스크의 post-processing을 수행한다.
    ///
    /// escalation, manager.apply, 로그/토큰 기록, 알림 발송,
    /// cron force-trigger, spec auto-completion 등 모든 후처리를 포함한다.
    /// 메인 이벤트 루프와 graceful shutdown 양쪽에서 호출된다.
    async fn handle_task_result(&mut self, task_result: &TaskResult) {
        // Escalation: 실패 시 failure_count 증가 → 레벨별 대응
        let mut escalation_hitl = None;
        let escalation_retry = if let TaskStatus::Failed(ref msg) = task_result.status {
            match crate::cli::resolve_repo_id(&self.log_db, &task_result.repo_name) {
                Ok(repo_id) => {
                    match escalation::escalate_with_config(
                        &self.log_db,
                        &task_result.work_id,
                        &repo_id,
                        msg,
                        &self.escalation_config,
                    ) {
                        escalation::EscalationOutcome::Retry => true,
                        escalation::EscalationOutcome::Remove => false,
                        escalation::EscalationOutcome::RemoveWithHitl(event, hitl_id) => {
                            escalation_hitl = Some((event, hitl_id));
                            false
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("skipping escalation for {}: {e}", task_result.work_id);
                    false
                }
            }
        } else {
            false
        };

        // Retry일 때는 apply(Remove) 건너뛴다 — pending으로 이미 복구됨.
        if !escalation_retry {
            self.manager.apply(task_result);
        }

        for log_entry in &task_result.logs {
            if let Ok(log_id) = self.log_db.log_insert(log_entry) {
                let usage = parse_token_usage(&log_id, log_entry);
                if usage.input_tokens > 0 || usage.output_tokens > 0 {
                    if let Err(e) = self.log_db.usage_insert(&usage) {
                        tracing::warn!("failed to record token usage: {e}");
                    }
                }
            }
        }

        // Notify on task failure (escalation으로 retry되더라도 기록)
        if let TaskStatus::Failed(ref msg) = task_result.status {
            let notif = NotificationEvent::from_task_failed(
                &task_result.work_id,
                &task_result.repo_name,
                msg,
            );
            dispatch_notification(&self.notifier, &notif).await;
        }

        // Notify on escalation-generated HITL event
        if let Some((ref hitl_event, ref hitl_id)) = escalation_hitl {
            let notif = NotificationEvent::from_hitl_created(hitl_event, Some(hitl_id.clone()));
            dispatch_notification(&self.notifier, &notif).await;
        }

        // Force-trigger claw-evaluate on any task completion/failure
        if let Some(ref cron) = self.cron_engine {
            cron.force_trigger(crate::cli::cron::CLAW_EVALUATE_JOB);
        }

        // Auto-check spec completion on successful task completion
        if let TaskStatus::Completed = task_result.status {
            let env = crate::core::config::RealEnv;
            let completable = crate::cli::spec::check_completable_specs(&self.log_db, &env);
            for (spec_id, hitl_event, hitl_id) in &completable {
                info!("spec auto-completion triggered for {spec_id}");
                let notif = NotificationEvent::from_hitl_created(hitl_event, Some(hitl_id.clone()));
                dispatch_notification(&self.notifier, &notif).await;
            }
        }
    }

    /// 메인 이벤트 루프 실행.
    ///
    /// task completion / tick / status heartbeat / shutdown 4개 arm으로 구성.
    /// SIGINT 수신 시 in-flight tasks를 대기한 뒤 종료한다.
    pub async fn run(&mut self) {
        let start_time = std::time::Instant::now();
        let mut join_set: JoinSet<TaskResult> = JoinSet::new();
        let mut pending_tasks: Vec<Box<dyn crate::core::task::Task>> = Vec::new();

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
                            self.handle_task_result(&task_result).await;
                        }
                        Err(e) => {
                            tracing::error!("spawned task panicked: {e}");
                            self.tracker.total = self.tracker.total.saturating_sub(1);
                        }
                    }

                    try_spawn(&mut pending_tasks, &mut self.tracker, &mut join_set, &self.runner);
                }

                // ── Tick: housekeeping + spawn + daily report + cron ──
                _ = tick.tick() => {
                    self.manager.tick().await;
                    pending_tasks.extend(self.manager.drain_ready());

                    // v5 2-level concurrency: sync workspace limits from config
                    for (repo_name, limit) in self.manager.workspace_limits() {
                        self.tracker.set_workspace_limit(&repo_name, limit);
                    }

                    // v5: track evaluate cron active slots
                    let evaluate_count = self.cron_engine.as_ref()
                        .map(|c| if c.is_running(crate::cli::cron::CLAW_EVALUATE_JOB) { 1 } else { 0 })
                        .unwrap_or(0);
                    self.tracker.set_active_evaluate_count(evaluate_count);

                    try_spawn(&mut pending_tasks, &mut self.tracker, &mut join_set, &self.runner);

                    self.reporter.maybe_run().await;

                    // Execute due cron jobs
                    if let Some(ref mut cron) = self.cron_engine {
                        let results = cron.tick().await;
                        for r in &results {
                            info!("cron '{}' completed: exit_code={}", r.job_name, r.exit_code);

                            // Record claw-evaluate decisions
                            if r.job_name == crate::cli::cron::CLAW_EVALUATE_JOB {
                                cron.record_claw_evaluate_decision(r);
                            }
                        }
                    }
                }

                // ── Status heartbeat ──
                _ = status_tick.tick() => {
                    let ds = status::build_status(
                        self.manager.active_items(), start_time,
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

        // Wait for in-flight tasks to complete (with timeout + second SIGINT support)
        if !join_set.is_empty() {
            let remaining = join_set.len();
            let drain_timeout = std::time::Duration::from_secs(self.shutdown_drain_timeout_secs);
            info!(
                "waiting for {} in-flight tasks (drain timeout={}s)...",
                remaining, self.shutdown_drain_timeout_secs
            );

            let drain_result = tokio::time::timeout(drain_timeout, async {
                loop {
                    tokio::select! {
                        result = join_set.join_next() => {
                            match result {
                                Some(Ok(task_result)) => {
                                    self.tracker.release(&task_result.repo_name);
                                    info!(
                                        "shutdown drain: task completed: {} - {}",
                                        task_result.work_id, task_result.status
                                    );
                                    self.handle_task_result(&task_result).await;
                                }
                                Some(Err(e)) => {
                                    tracing::error!("shutdown drain: spawned task panicked: {e}");
                                    self.tracker.total = self.tracker.total.saturating_sub(1);
                                }
                                None => break, // all tasks completed
                            }
                        }
                        // 두 번째 SIGINT: 즉시 종료
                        _ = tokio::signal::ctrl_c() => {
                            tracing::warn!(
                                "received second SIGINT, force-aborting {} remaining tasks",
                                join_set.len()
                            );
                            join_set.abort_all();
                            break;
                        }
                    }
                }
            })
            .await;

            if drain_result.is_err() {
                tracing::warn!(
                    "shutdown drain timed out after {}s, aborting {} remaining tasks",
                    self.shutdown_drain_timeout_secs,
                    join_set.len()
                );
                join_set.abort_all();
            }
        }

        status::remove_status(&self.status_path);
    }
}

// ─── Daemon Entry Point ───

/// 현재 프로세스를 백그라운드 데몬으로 전환한다 (포그라운드/백그라운드 모두 지원).
///
/// Unix fork() + setsid() 패턴을 사용:
/// - 부모 프로세스: 자식 PID 출력 후 즉시 종료
/// - 자식 프로세스: 새 세션 생성 후 이벤트 루프 실행
///
/// stdout/stderr는 log_dir/daemon.out 파일로 리다이렉트된다.
#[cfg(unix)]
pub fn daemonize(log_dir: &Path) -> Result<()> {
    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;

    let pid = unsafe { libc::fork() };
    if pid < 0 {
        bail!("fork failed: {}", std::io::Error::last_os_error());
    }
    if pid > 0 {
        // 부모: 자식 PID 출력 후 종료
        println!("autodev daemon started in background (pid: {pid})");
        std::process::exit(0);
    }

    // 자식: 새 세션 생성
    if unsafe { libc::setsid() } < 0 {
        bail!("setsid failed: {}", std::io::Error::last_os_error());
    }

    // stdout/stderr → 로그 파일
    let log_file = log_dir.join("daemon.out");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)?;

    unsafe {
        if libc::dup2(file.as_raw_fd(), libc::STDOUT_FILENO) < 0 {
            bail!("dup2 stdout failed: {}", std::io::Error::last_os_error());
        }
        if libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO) < 0 {
            bail!("dup2 stderr failed: {}", std::io::Error::last_os_error());
        }
    }

    // stdin → /dev/null (close 대신)
    let devnull = std::fs::File::open("/dev/null")
        .map_err(|e| anyhow::anyhow!("open /dev/null failed: {e}"))?;
    unsafe {
        if libc::dup2(devnull.as_raw_fd(), libc::STDIN_FILENO) < 0 {
            bail!("dup2 stdin failed: {}", std::io::Error::last_os_error());
        }
    }

    std::mem::forget(file);
    std::mem::forget(devnull);

    Ok(())
}

/// Dispatcher가 있으면 알림을 발송하고, 에러를 로깅한다.
async fn dispatch_notification(
    notifier: &Option<notifiers::dispatcher::NotificationDispatcher>,
    event: &NotificationEvent,
) {
    if let Some(ref dispatcher) = notifier {
        let errors = dispatcher.dispatch(event).await;
        for (ch, err) in &errors {
            tracing::warn!("notification error ({ch}): {err}");
        }
    }
}

/// Claude CLI stderr에서 토큰 사용량을 파싱한다.
///
/// Claude Code의 stderr에는 다양한 형식의 토큰 정보가 출력될 수 있다.
/// 예: `"input_tokens": 1234`, `"output_tokens": 567`
/// JSON 응답이 포함된 경우도 파싱한다.
/// Claude CLI stderr에서 토큰 사용량을 파싱한다.
fn parse_token_usage(
    log_id: &str,
    log: &crate::core::models::NewConsumerLog,
) -> crate::core::models::NewTokenUsage {
    let mut input_tokens: i64 = 0;
    let mut output_tokens: i64 = 0;
    let mut cache_write_tokens: i64 = 0;
    let mut cache_read_tokens: i64 = 0;

    // Parse JSON objects from stderr (Claude emits usage events as JSON lines)
    for line in log.stderr.lines() {
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(line.trim()) {
            if let Some(v) = obj.get("input_tokens").and_then(|v| v.as_i64()) {
                input_tokens += v;
            }
            if let Some(v) = obj.get("output_tokens").and_then(|v| v.as_i64()) {
                output_tokens += v;
            }
            if let Some(v) = obj
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_i64())
            {
                cache_write_tokens += v;
            }
            if let Some(v) = obj.get("cache_read_input_tokens").and_then(|v| v.as_i64()) {
                cache_read_tokens += v;
            }
        }
    }

    crate::core::models::NewTokenUsage {
        log_id: log_id.to_string(),
        repo_id: log.repo_id.clone(),
        queue_type: log.queue_type.clone(),
        queue_item_id: log.queue_item_id.clone(),
        input_tokens,
        output_tokens,
        cache_write_tokens,
        cache_read_tokens,
    }
}

/// 데몬을 포그라운드 또는 백그라운드로 시작 (non-blocking event loop)
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
    // Source DB: owned by GitHubTaskSource / Collector (repo sync, cursor operations)
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

    // ── Collector: GitHubTaskSource ──
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
        cfg.claw.clone(),
    );

    // ── Startup Reconcile ──
    // Separate DB connection for startup (source_db is already moved into source)
    let startup_db = Database::open(&db_path)?;
    startup_db.initialize()?;
    match GitRepositoryFactory::create_all(&log_db, &*env, &*gh).await {
        Ok(mut repo_map) => {
            // DB-first 복구: DB에서 활성 아이템을 로드
            for repo in repo_map.values_mut() {
                repo.load_from_db(&startup_db);
            }

            // 라벨 기반 fallback 복구
            let mut total_recovered = 0u64;
            for repo in repo_map.values_mut() {
                let n = repo.startup_reconcile(&*gh, &startup_db).await;
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
        daily_reporter::DailyReporterConfig {
            log_dir: log_dir.clone(),
            log_retention_days: cfg.daemon.log_retention_days,
            daily_report_hour: cfg.daemon.daily_report_hour,
            knowledge_extraction: cfg.sources.github.knowledge_extraction,
        },
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

    // ── CronEngine + global cron seed ──
    let cron_db = Database::open(&db_path)?;
    match crate::cli::cron::seed_global_crons(&cron_db, home) {
        Ok(n) if n > 0 => info!("seeded {n} global built-in cron jobs"),
        Ok(_) => {}
        Err(e) => tracing::warn!("failed to seed global cron jobs: {e}"),
    }
    let cron_engine = CronEngine::new(cron_db, home.to_path_buf());

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
        cfg.daemon.shutdown_drain_timeout_secs,
        cfg.escalation.clone(),
    )
    .with_cron_engine(cron_engine);

    if let Some(notifier) = notifiers::dispatcher::NotificationDispatcher::from_config_with_gh(
        &cfg.daemon,
        Some(Arc::clone(&gh)),
        cfg.sources.github.gh_host.clone(),
    ) {
        daemon = daemon.with_notifier(notifier);
        info!("notification dispatcher enabled");
    }

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

/// v5 daemon 시작.
///
/// v4와 동일한 PID 파일을 공유하여 동시 실행을 방지한다.
/// v5 daemon은 workspace.yaml 기반 상태 머신 루프를 실행한다.
pub async fn start_v5(
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

    info!("starting autodev v5 daemon...");

    pid::write_pid(home)?;

    let cfg = config::loader::load_merged(&*env, None);

    let db_path = home.join("autodev.db");
    let log_db = Database::open(&db_path)?;
    log_db.initialize()?;

    println!("autodev v5 daemon started (pid: {})", std::process::id());

    let log_dir = config::resolve_log_dir(&cfg.daemon.log_dir, home);

    // ── Startup log cleanup ──
    let n = log::cleanup_old_logs(&log_dir, cfg.daemon.log_retention_days);
    if n > 0 {
        info!("startup log cleanup: deleted {n} old log files");
    }

    info!(
        "v5 event loop starting (max_concurrent={})",
        cfg.daemon.max_concurrent_tasks
    );

    // ── CronEngine + global cron seed ──
    let cron_db = Database::open(&db_path)?;
    match crate::cli::cron::seed_global_crons(&cron_db, home) {
        Ok(n) if n > 0 => info!("seeded {n} global built-in cron jobs"),
        Ok(_) => {}
        Err(e) => tracing::warn!("failed to seed global cron jobs: {e}"),
    }
    let mut cron_engine = CronEngine::new(cron_db, home.to_path_buf());

    // ── v5 main loop ──
    let tick_secs = cfg.daemon.tick_interval_secs;
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(tick_secs));

    loop {
        tokio::select! {
            _ = tick.tick() => {
                // Execute due cron jobs
                let results = cron_engine.tick().await;
                for r in &results {
                    info!("cron '{}' completed: exit_code={}", r.job_name, r.exit_code);
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("received SIGINT, shutting down v5 daemon...");
                break;
            }
        }
    }

    pid::remove_pid(home);

    // Suppress unused variable warnings — these dependencies will be wired
    // as the v5 daemon loop is fleshed out.
    let _ = (gh, git, claude, sw, log_db);

    Ok(())
}

/// 데몬 중지 (PID → SIGTERM + poll for exit)
pub fn stop(home: &Path) -> Result<()> {
    pid::stop(home)
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
        assert!(t.has_global_slot());
        t.track("org/repo-a");
        assert!(t.has_global_slot());
        t.track("org/repo-b");
        assert!(!t.has_global_slot());
        t.release("org/repo-a");
        assert!(t.has_global_slot());
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

    // ═══════════════════════════════════════════════
    // v5 2-level concurrency 테스트
    // ═══════════════════════════════════════════════

    #[test]
    fn workspace_limit_blocks_spawn_for_that_workspace() {
        let mut t = InFlightTracker::new(10);
        t.set_workspace_limit("org/repo-a", 1);

        assert!(t.has_workspace_slot("org/repo-a"));
        t.track("org/repo-a");
        // workspace limit reached
        assert!(!t.has_workspace_slot("org/repo-a"));
        // other workspace unaffected
        assert!(t.has_workspace_slot("org/repo-b"));
        // global still has room
        assert!(t.has_global_slot());
    }

    #[test]
    fn workspace_limit_zero_means_unlimited() {
        let mut t = InFlightTracker::new(10);
        t.set_workspace_limit("org/repo", 0);

        // limit=0 removes the entry → no workspace-level cap
        assert!(t.has_workspace_slot("org/repo"));
        t.track("org/repo");
        t.track("org/repo");
        t.track("org/repo");
        assert!(t.has_workspace_slot("org/repo"));
    }

    #[test]
    fn workspace_limit_release_reopens_slot() {
        let mut t = InFlightTracker::new(10);
        t.set_workspace_limit("org/repo", 1);

        t.track("org/repo");
        assert!(!t.has_workspace_slot("org/repo"));
        t.release("org/repo");
        assert!(t.has_workspace_slot("org/repo"));
    }

    #[test]
    fn evaluate_count_reduces_global_slots() {
        let mut t = InFlightTracker::new(3);
        t.track("org/repo-a");
        // 1 running + 0 evaluate = 1 total, global max=3 → has slot
        assert!(t.has_global_slot());

        t.set_active_evaluate_count(2);
        // 1 running + 2 evaluate = 3 total, global max=3 → no slot
        assert!(!t.has_global_slot());

        t.set_active_evaluate_count(0);
        // 1 running + 0 evaluate = 1 total → has slot again
        assert!(t.has_global_slot());
    }

    #[test]
    fn two_level_both_must_pass() {
        let mut t = InFlightTracker::new(2);
        t.set_workspace_limit("org/repo-a", 2);
        t.set_workspace_limit("org/repo-b", 1);

        // Fill global to 1/2
        t.track("org/repo-b");
        // repo-b workspace full, but global has room
        assert!(!t.has_workspace_slot("org/repo-b"));
        assert!(t.has_global_slot());

        // repo-a workspace has room, global has room
        assert!(t.has_workspace_slot("org/repo-a"));
        assert!(t.has_global_slot());

        t.track("org/repo-a");
        // global full (2/2)
        assert!(!t.has_global_slot());
        // repo-a workspace still has room (1/2) but global blocks
        assert!(t.has_workspace_slot("org/repo-a"));
    }

    #[tokio::test]
    async fn try_spawn_skips_workspace_full_takes_others() {
        use crate::core::task::{AgentRequest, AgentResponse, QueueOp, SkipReason, TaskStatus};
        use crate::infra::claude::SessionOptions;
        use std::path::PathBuf;

        struct DummyTask {
            id: String,
            repo: String,
        }

        #[async_trait::async_trait]
        impl crate::core::task::Task for DummyTask {
            fn work_id(&self) -> &str {
                &self.id
            }
            fn repo_name(&self) -> &str {
                &self.repo
            }
            async fn before_invoke(&mut self) -> Result<AgentRequest, SkipReason> {
                Ok(AgentRequest {
                    working_dir: PathBuf::from("/tmp"),
                    prompt: "test".to_string(),
                    session_opts: SessionOptions::default(),
                })
            }
            async fn after_invoke(&mut self, _: AgentResponse) -> TaskResult {
                TaskResult {
                    work_id: self.id.clone(),
                    repo_name: self.repo.clone(),
                    queue_ops: vec![QueueOp::Remove],
                    logs: vec![],
                    status: TaskStatus::Completed,
                }
            }
        }

        struct NoopRunner;

        #[async_trait::async_trait]
        impl TaskRunner for NoopRunner {
            async fn run(&self, _task: Box<dyn crate::core::task::Task>) -> TaskResult {
                TaskResult {
                    work_id: String::new(),
                    repo_name: String::new(),
                    queue_ops: vec![],
                    logs: vec![],
                    status: TaskStatus::Completed,
                }
            }
        }

        let runner: Arc<dyn TaskRunner> = Arc::new(NoopRunner);

        let mut tracker = InFlightTracker::new(10);
        tracker.set_workspace_limit("org/repo-a", 1);
        // repo-a already full
        tracker.track("org/repo-a");

        let mut pending: Vec<Box<dyn crate::core::task::Task>> = vec![
            Box::new(DummyTask {
                id: "t1".into(),
                repo: "org/repo-a".into(),
            }),
            Box::new(DummyTask {
                id: "t2".into(),
                repo: "org/repo-b".into(),
            }),
        ];

        let mut join_set: JoinSet<TaskResult> = JoinSet::new();
        try_spawn(&mut pending, &mut tracker, &mut join_set, &runner);

        // repo-b task should be spawned, repo-a deferred
        assert_eq!(join_set.len(), 1);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].repo_name(), "org/repo-a");
    }
}
