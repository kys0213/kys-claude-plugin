pub mod log;
pub mod pid;
#[allow(dead_code)]
pub mod recovery;
pub mod status;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Result};
use chrono::Timelike;
use tokio::task::JoinSet;
use tracing::info;

use crate::components::workspace::Workspace;
use crate::config::{self, Env};
use crate::domain::git_repository::GitRepository;
use crate::domain::git_repository_factory::GitRepositoryFactory;
use crate::domain::repository::{ConsumerLogRepository, RepoRepository, ScanCursorRepository};
use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::infrastructure::suggest_workflow::SuggestWorkflow;
use crate::pipeline;
use crate::pipeline::QueueOp;
use crate::queue::task_queues::{issue_phase, merge_phase, pr_phase};
use crate::queue::Database;

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

// ─── Queue Op Applier ───

/// TaskOutput의 큐 조작을 해당 repo의 per-repo 큐에 적용한다.
fn apply_queue_ops(repos: &mut HashMap<String, GitRepository>, output: &pipeline::TaskOutput) {
    let repo = match repos.get_mut(&output.repo_name) {
        Some(r) => r,
        None => {
            tracing::warn!(
                "task output for unknown repo {}: {}",
                output.repo_name,
                output.work_id
            );
            return;
        }
    };

    for op in &output.queue_ops {
        match op {
            QueueOp::Remove => {
                repo.issue_queue.remove(&output.work_id);
                repo.pr_queue.remove(&output.work_id);
                repo.merge_queue.remove(&output.work_id);
            }
            QueueOp::PushIssue { phase, item } => {
                repo.issue_queue.push(phase, item.clone());
            }
            QueueOp::PushPr { phase, item } => {
                repo.pr_queue.push(phase, item.clone());
            }
            QueueOp::PushMerge { phase, item } => {
                repo.merge_queue.push(phase, item.clone());
            }
        }
    }
}

// ─── Task Spawner ───

/// 모든 repo의 큐에서 Ready 아이템을 pop → working phase 전이 → spawned task로 실행.
/// InFlightTracker가 상한에 도달하면 즉시 반환한다.
#[allow(clippy::too_many_arguments)]
fn spawn_ready_tasks(
    repos: &mut HashMap<String, GitRepository>,
    tracker: &mut InFlightTracker,
    join_set: &mut JoinSet<pipeline::TaskOutput>,
    env: &Arc<dyn Env>,
    gh: &Arc<dyn Gh>,
    git: &Arc<dyn Git>,
    claude: &Arc<dyn Claude>,
    sw: &Arc<dyn SuggestWorkflow>,
) {
    for repo in repos.values_mut() {
        // Issue: Pending → Analyzing
        while tracker.can_spawn() {
            let item = match repo.issue_queue.pop(issue_phase::PENDING) {
                Some(item) => item,
                None => break,
            };
            tracker.track(&item.repo_name);
            repo.issue_queue.push(issue_phase::ANALYZING, item.clone());
            tracing::debug!("issue #{}: spawned analyze_one", item.github_number);

            let (e, g, gi, c) = (
                Arc::clone(env),
                Arc::clone(gh),
                Arc::clone(git),
                Arc::clone(claude),
            );
            join_set.spawn(
                async move { pipeline::issue::analyze_one(item, &*e, &*g, &*gi, &*c).await },
            );
        }

        // Issue: Ready → Implementing
        while tracker.can_spawn() {
            let item = match repo.issue_queue.pop(issue_phase::READY) {
                Some(item) => item,
                None => break,
            };
            tracker.track(&item.repo_name);
            repo.issue_queue
                .push(issue_phase::IMPLEMENTING, item.clone());
            tracing::debug!("issue #{}: spawned implement_one", item.github_number);

            let (e, g, gi, c) = (
                Arc::clone(env),
                Arc::clone(gh),
                Arc::clone(git),
                Arc::clone(claude),
            );
            join_set.spawn(async move {
                pipeline::issue::implement_one(item, &*e, &*g, &*gi, &*c).await
            });
        }

        // PR: Pending → Reviewing
        while tracker.can_spawn() {
            let item = match repo.pr_queue.pop(pr_phase::PENDING) {
                Some(item) => item,
                None => break,
            };
            tracker.track(&item.repo_name);
            repo.pr_queue.push(pr_phase::REVIEWING, item.clone());
            tracing::debug!("PR #{}: spawned review_one", item.github_number);

            let (e, g, gi, c, s) = (
                Arc::clone(env),
                Arc::clone(gh),
                Arc::clone(git),
                Arc::clone(claude),
                Arc::clone(sw),
            );
            join_set.spawn(async move {
                pipeline::pr::review_one(item, &*e, &*g, &*gi, &*c, &*s).await
            });
        }

        // PR: ReviewDone → Improving
        while tracker.can_spawn() {
            let item = match repo.pr_queue.pop(pr_phase::REVIEW_DONE) {
                Some(item) => item,
                None => break,
            };
            tracker.track(&item.repo_name);
            repo.pr_queue.push(pr_phase::IMPROVING, item.clone());
            tracing::debug!("PR #{}: spawned improve_one", item.github_number);

            let (e, g, gi, c) = (
                Arc::clone(env),
                Arc::clone(gh),
                Arc::clone(git),
                Arc::clone(claude),
            );
            join_set
                .spawn(async move { pipeline::pr::improve_one(item, &*e, &*g, &*gi, &*c).await });
        }

        // PR: Improved → Reviewing (re-review)
        while tracker.can_spawn() {
            let item = match repo.pr_queue.pop(pr_phase::IMPROVED) {
                Some(item) => item,
                None => break,
            };
            tracker.track(&item.repo_name);
            repo.pr_queue.push(pr_phase::REVIEWING, item.clone());
            tracing::debug!("PR #{}: spawned re_review_one", item.github_number);

            let (e, g, gi, c, s) = (
                Arc::clone(env),
                Arc::clone(gh),
                Arc::clone(git),
                Arc::clone(claude),
                Arc::clone(sw),
            );
            join_set.spawn(async move {
                pipeline::pr::re_review_one(item, &*e, &*g, &*gi, &*c, &*s).await
            });
        }

        // Merge: Pending → Merging
        while tracker.can_spawn() {
            let item = match repo.merge_queue.pop(merge_phase::PENDING) {
                Some(item) => item,
                None => break,
            };
            tracker.track(&item.repo_name);
            repo.merge_queue.push(merge_phase::MERGING, item.clone());
            tracing::debug!("merge PR #{}: spawned merge_one", item.pr_number);

            let (e, g, gi, c) = (
                Arc::clone(env),
                Arc::clone(gh),
                Arc::clone(git),
                Arc::clone(claude),
            );
            join_set
                .spawn(async move { pipeline::merge::merge_one(item, &*e, &*g, &*gi, &*c).await });
        }
    }
}

// ─── Daemon Entry Point ───

/// 데몬을 포그라운드로 시작 (non-blocking event loop)
#[allow(clippy::too_many_arguments)]
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
    let db = Database::open(&db_path)?;
    db.initialize()?;

    println!("autodev daemon started (pid: {})", std::process::id());

    let mut repos: HashMap<String, GitRepository> = HashMap::new();
    let mut join_set: JoinSet<pipeline::TaskOutput> = JoinSet::new();
    let mut tracker = InFlightTracker::new(cfg.daemon.max_concurrent_tasks);

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

    // 0. Startup Reconcile: GitRepositoryFactory로 레포 생성 + per-repo 큐 복구
    match GitRepositoryFactory::create_all(&db, &*env, &*gh).await {
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
            repos = repo_map;
        }
        Err(e) => tracing::error!("startup reconcile failed: {e}"),
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
                    Ok(output) => {
                        tracker.release(&output.repo_name);
                        info!(
                            "task completed: {} (in-flight: {})",
                            output.work_id, tracker.total
                        );
                        // Apply queue ops to the per-repo queues
                        apply_queue_ops(&mut repos, &output);
                        // DB logging
                        for log_entry in &output.logs {
                            let _ = db.log_insert(log_entry);
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
                spawn_ready_tasks(
                    &mut repos, &mut tracker, &mut join_set,
                    &env, &gh, &git, &claude, &sw,
                );
            }

            // ── Tick: housekeeping + spawn ──
            _ = tick.tick() => {
                // 0. Repo map sync: DB에서 최신 enabled repos 반영
                if let Ok(enabled) = db.repo_find_enabled() {
                    // 새 레포 추가
                    for er in &enabled {
                        if !repos.contains_key(&er.name) {
                            let git_repo = GitRepositoryFactory::create(er, &*env, &*gh).await;
                            repos.insert(er.name.clone(), git_repo);
                            info!("added new repo: {}", er.name);
                        }
                    }
                    // 비활성 레포 제거 (in-flight task가 없는 경우만)
                    let enabled_names: std::collections::HashSet<&str> =
                        enabled.iter().map(|r| r.name.as_str()).collect();
                    let to_remove: Vec<String> = repos
                        .keys()
                        .filter(|k| !enabled_names.contains(k.as_str()))
                        .cloned()
                        .collect();
                    for name in to_remove {
                        if !tracker.per_repo.contains_key(&name) {
                            repos.remove(&name);
                            info!("removed disabled repo: {name}");
                        }
                    }
                }

                // 1. Recovery: per-repo refresh + orphan 라벨 정리
                for repo in repos.values_mut() {
                    repo.refresh(&*gh).await;
                    let n = repo.recover_orphan_wip(&*gh).await;
                    if n > 0 {
                        info!("recovered {n} orphan wip items in {}", repo.name());
                    }
                    let n = repo.recover_orphan_implementing(&*gh).await;
                    if n > 0 {
                        info!("recovered {n} orphan implementing items in {}", repo.name());
                    }
                }

                // 2. Scan: per-repo config 기반 스캔
                for repo in repos.values_mut() {
                    let ws_path = config::workspaces_path(&*env)
                        .join(config::sanitize_repo_name(repo.name()));
                    let repo_cfg = config::loader::load_merged(
                        &*env,
                        if ws_path.exists() {
                            Some(ws_path.as_path())
                        } else {
                            None
                        },
                    );

                    let should_scan = db
                        .cursor_should_scan(repo.id(), repo_cfg.consumer.scan_interval_secs as i64)
                        .unwrap_or(false);
                    if !should_scan {
                        continue;
                    }

                    tracing::info!("scanning {}...", repo.name());

                    for target in &repo_cfg.consumer.scan_targets {
                        match target.as_str() {
                            "issues" => {
                                if let Err(e) = repo
                                    .scan_issues(
                                        &*gh,
                                        &db,
                                        &repo_cfg.consumer.ignore_authors,
                                        &repo_cfg.consumer.filter_labels,
                                    )
                                    .await
                                {
                                    tracing::error!("issue scan error for {}: {e}", repo.name());
                                }

                                if let Err(e) = repo.scan_approved_issues(&*gh).await {
                                    tracing::error!(
                                        "approved scan error for {}: {e}",
                                        repo.name()
                                    );
                                }
                            }
                            "pulls" => {
                                if let Err(e) = repo
                                    .scan_pulls(
                                        &*gh,
                                        &db,
                                        &repo_cfg.consumer.ignore_authors,
                                    )
                                    .await
                                {
                                    tracing::error!("PR scan error for {}: {e}", repo.name());
                                }
                            }
                            "merges" => {
                                if repo_cfg.consumer.auto_merge {
                                    if let Err(e) = repo.scan_merges(&*gh).await {
                                        tracing::error!(
                                            "merge scan error for {}: {e}",
                                            repo.name()
                                        );
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }

                // 3. Spawn ready tasks
                spawn_ready_tasks(
                    &mut repos, &mut tracker, &mut join_set,
                    &env, &gh, &git, &claude, &sw,
                );

                // 4. Daily Report (scheduled at daily_report_hour)
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

                                let workspace = Workspace::new(&*git, &*env);
                                if let Ok(enabled) = db.repo_find_enabled() {
                                    if let Some(er) = enabled.first() {
                                        if let Ok(base) = workspace.ensure_cloned(&er.url, &er.name).await {
                                            crate::knowledge::daily::enrich_with_cross_analysis(
                                                &mut report, &*sw,
                                            ).await;

                                            let per_task = crate::knowledge::daily::aggregate_daily_suggestions(&db, &yesterday);

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
                                            let repo_gh_host = repos
                                                .get(&er.name)
                                                .and_then(|r| r.gh_host());
                                            crate::knowledge::daily::post_daily_report(
                                                &*gh, &er.name, &report, repo_gh_host,
                                            ).await;

                                            if !report.suggestions.is_empty() {
                                                crate::knowledge::daily::create_knowledge_prs(
                                                    &*gh, &workspace, &er.name, &report,
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
                let ds = status::build_status_from_repos(&repos, &counters, start_time);
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
            if let Ok(output) = result {
                tracker.release(&output.repo_name);
                apply_queue_ops(&mut repos, &output);
                for log_entry in &output.logs {
                    let _ = db.log_insert(log_entry);
                }
            }
        }
    }

    status::remove_status(&status_path);
    pid::remove_pid(home);
    Ok(())
}

/// 데몬 중지 (PID → SIGTERM)
#[allow(dead_code)]
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

    // ═══════════════════════════════════════════════
    // apply_queue_ops 테스트
    // ═══════════════════════════════════════════════

    #[test]
    fn apply_queue_ops_remove_clears_item() {
        use crate::queue::task_queues::{make_work_id, IssueItem};

        let mut repos = HashMap::new();
        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.issue_queue.push(
            issue_phase::ANALYZING,
            IssueItem {
                work_id: make_work_id("issue", "org/repo", 1),
                repo_id: "r1".to_string(),
                repo_name: "org/repo".to_string(),
                repo_url: "https://github.com/org/repo".to_string(),
                github_number: 1,
                title: "Test".to_string(),
                body: None,
                labels: vec![],
                author: "user".to_string(),
                analysis_report: None,
            },
        );
        repos.insert("org/repo".to_string(), repo);

        let output = pipeline::TaskOutput {
            work_id: "issue:org/repo:1".to_string(),
            repo_name: "org/repo".to_string(),
            queue_ops: vec![QueueOp::Remove],
            logs: vec![],
        };

        apply_queue_ops(&mut repos, &output);
        assert!(!repos["org/repo"].contains("issue:org/repo:1"));
    }

    #[test]
    fn apply_queue_ops_remove_then_push_pr() {
        use crate::queue::task_queues::{make_work_id, IssueItem, PrItem};

        let mut repos = HashMap::new();
        let mut repo = GitRepository::new(
            "r1".to_string(),
            "org/repo".to_string(),
            "https://github.com/org/repo".to_string(),
            None,
        );
        repo.issue_queue.push(
            issue_phase::IMPLEMENTING,
            IssueItem {
                work_id: make_work_id("issue", "org/repo", 1),
                repo_id: "r1".to_string(),
                repo_name: "org/repo".to_string(),
                repo_url: "https://github.com/org/repo".to_string(),
                github_number: 1,
                title: "Test".to_string(),
                body: None,
                labels: vec![],
                author: "user".to_string(),
                analysis_report: None,
            },
        );
        repos.insert("org/repo".to_string(), repo);

        let output = pipeline::TaskOutput {
            work_id: "issue:org/repo:1".to_string(),
            repo_name: "org/repo".to_string(),
            queue_ops: vec![
                QueueOp::Remove,
                QueueOp::PushPr {
                    phase: pr_phase::PENDING,
                    item: PrItem {
                        work_id: make_work_id("pr", "org/repo", 10),
                        repo_id: "r1".to_string(),
                        repo_name: "org/repo".to_string(),
                        repo_url: "https://github.com/org/repo".to_string(),
                        github_number: 10,
                        title: "PR #10".to_string(),
                        head_branch: "feat".to_string(),
                        base_branch: "main".to_string(),
                        review_comment: None,
                        source_issue_number: None,
                        review_iteration: 0,
                    },
                },
            ],
            logs: vec![],
        };

        apply_queue_ops(&mut repos, &output);
        assert!(!repos["org/repo"].contains("issue:org/repo:1"));
        assert!(repos["org/repo"].contains("pr:org/repo:10"));
    }

    #[test]
    fn apply_queue_ops_unknown_repo_is_noop() {
        let mut repos = HashMap::new();

        let output = pipeline::TaskOutput {
            work_id: "issue:unknown/repo:1".to_string(),
            repo_name: "unknown/repo".to_string(),
            queue_ops: vec![QueueOp::Remove],
            logs: vec![],
        };

        // Should not panic
        apply_queue_ops(&mut repos, &output);
    }
}
