pub mod log;
pub mod pid;
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
use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::infrastructure::suggest_workflow::SuggestWorkflow;
use crate::pipeline;
use crate::queue::repository::{RepoRepository, ScanCursorRepository};
use crate::queue::task_queues::{issue_phase, merge_phase, pr_phase, TaskQueues};
use crate::queue::Database;
use crate::scanner;

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

/// 큐에서 Ready 아이템을 pop → working phase 전이 → spawned task로 실행.
/// InFlightTracker가 상한에 도달하면 즉시 반환한다.
#[allow(clippy::too_many_arguments)]
fn spawn_ready_tasks(
    queues: &mut TaskQueues,
    tracker: &mut InFlightTracker,
    join_set: &mut JoinSet<pipeline::TaskOutput>,
    env: &Arc<dyn Env>,
    gh: &Arc<dyn Gh>,
    git: &Arc<dyn Git>,
    claude: &Arc<dyn Claude>,
    sw: &Arc<dyn SuggestWorkflow>,
) {
    // Issue: Pending → Analyzing
    while tracker.can_spawn() {
        let item = match queues.issues.pop(issue_phase::PENDING) {
            Some(item) => item,
            None => break,
        };
        tracker.track(&item.repo_name);
        queues.issues.push(issue_phase::ANALYZING, item.clone());
        tracing::debug!("issue #{}: spawned analyze_one", item.github_number);

        let (e, g, gi, c) = (
            Arc::clone(env),
            Arc::clone(gh),
            Arc::clone(git),
            Arc::clone(claude),
        );
        join_set
            .spawn(async move { pipeline::issue::analyze_one(item, &*e, &*g, &*gi, &*c).await });
    }

    // Issue: Ready → Implementing
    while tracker.can_spawn() {
        let item = match queues.issues.pop(issue_phase::READY) {
            Some(item) => item,
            None => break,
        };
        tracker.track(&item.repo_name);
        queues.issues.push(issue_phase::IMPLEMENTING, item.clone());
        tracing::debug!("issue #{}: spawned implement_one", item.github_number);

        let (e, g, gi, c) = (
            Arc::clone(env),
            Arc::clone(gh),
            Arc::clone(git),
            Arc::clone(claude),
        );
        join_set
            .spawn(async move { pipeline::issue::implement_one(item, &*e, &*g, &*gi, &*c).await });
    }

    // PR: Pending → Reviewing
    while tracker.can_spawn() {
        let item = match queues.prs.pop(pr_phase::PENDING) {
            Some(item) => item,
            None => break,
        };
        tracker.track(&item.repo_name);
        queues.prs.push(pr_phase::REVIEWING, item.clone());
        tracing::debug!("PR #{}: spawned review_one", item.github_number);

        let (e, g, gi, c, s) = (
            Arc::clone(env),
            Arc::clone(gh),
            Arc::clone(git),
            Arc::clone(claude),
            Arc::clone(sw),
        );
        join_set
            .spawn(async move { pipeline::pr::review_one(item, &*e, &*g, &*gi, &*c, &*s).await });
    }

    // PR: ReviewDone → Improving
    while tracker.can_spawn() {
        let item = match queues.prs.pop(pr_phase::REVIEW_DONE) {
            Some(item) => item,
            None => break,
        };
        tracker.track(&item.repo_name);
        queues.prs.push(pr_phase::IMPROVING, item.clone());
        tracing::debug!("PR #{}: spawned improve_one", item.github_number);

        let (e, g, gi, c) = (
            Arc::clone(env),
            Arc::clone(gh),
            Arc::clone(git),
            Arc::clone(claude),
        );
        join_set.spawn(async move { pipeline::pr::improve_one(item, &*e, &*g, &*gi, &*c).await });
    }

    // PR: Improved → Reviewing (re-review)
    while tracker.can_spawn() {
        let item = match queues.prs.pop(pr_phase::IMPROVED) {
            Some(item) => item,
            None => break,
        };
        tracker.track(&item.repo_name);
        queues.prs.push(pr_phase::REVIEWING, item.clone());
        tracing::debug!("PR #{}: spawned re_review_one", item.github_number);

        let (e, g, gi, c, s) = (
            Arc::clone(env),
            Arc::clone(gh),
            Arc::clone(git),
            Arc::clone(claude),
            Arc::clone(sw),
        );
        join_set.spawn(
            async move { pipeline::pr::re_review_one(item, &*e, &*g, &*gi, &*c, &*s).await },
        );
    }

    // Merge: Pending → Merging
    while tracker.can_spawn() {
        let item = match queues.merges.pop(merge_phase::PENDING) {
            Some(item) => item,
            None => break,
        };
        tracker.track(&item.repo_name);
        queues.merges.push(merge_phase::MERGING, item.clone());
        tracing::debug!("merge PR #{}: spawned merge_one", item.pr_number);

        let (e, g, gi, c) = (
            Arc::clone(env),
            Arc::clone(gh),
            Arc::clone(git),
            Arc::clone(claude),
        );
        join_set.spawn(async move { pipeline::merge::merge_one(item, &*e, &*g, &*gi, &*c).await });
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

    let mut queues = TaskQueues::new();
    let mut join_set: JoinSet<pipeline::TaskOutput> = JoinSet::new();
    let mut tracker = InFlightTracker::new(cfg.daemon.max_concurrent_tasks);

    let daily_report_hour = cfg.daemon.daily_report_hour;
    let knowledge_extraction = cfg.consumer.knowledge_extraction;
    let mut last_daily_report_date = String::new();

    let start_time = std::time::Instant::now();
    let status_path = home.join("daemon.status.json");
    let counters = status::StatusCounters::default();

    let reconcile_window_hours = cfg.daemon.reconcile_window_hours;
    let tick_interval_secs = cfg.daemon.tick_interval_secs;

    let log_dir = config::resolve_log_dir(&cfg.daemon.log_dir, home);
    let log_retention_days = cfg.daemon.log_retention_days;

    // Startup cleanup: 보존 기간 초과 로그 삭제
    let n = log::cleanup_old_logs(&log_dir, log_retention_days);
    if n > 0 {
        info!("startup log cleanup: deleted {n} old log files");
    }

    // 0. Startup Reconcile (bounded recovery)
    match startup_reconcile(&db, &*env, &*gh, &mut queues, reconcile_window_hours).await {
        Ok(n) if n > 0 => info!("startup reconcile: recovered {n} items"),
        Err(e) => tracing::error!("startup reconcile failed: {e}"),
        _ => {}
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
                        pipeline::handle_task_output(&mut queues, &db, output);
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
                    &mut queues, &mut tracker, &mut join_set,
                    &env, &gh, &git, &claude, &sw,
                );
            }

            // ── Tick: housekeeping + spawn ──
            _ = tick.tick() => {
                // 1. Recovery: orphan autodev:wip 라벨 정리
                match db.repo_find_enabled() {
                    Ok(repos) => {
                        let resolved = recovery::resolve_repos(&repos, &*env);
                        match recovery::recover_orphan_wip(
                            &resolved, &*gh, &queues,
                        ).await {
                            Ok(n) if n > 0 => info!("recovered {n} orphan wip items"),
                            Err(e) => tracing::error!("recovery error: {e}"),
                            _ => {}
                        }
                        match recovery::recover_orphan_implementing(
                            &resolved, &*gh, &queues,
                        ).await {
                            Ok(n) if n > 0 => info!("recovered {n} orphan implementing items"),
                            Err(e) => tracing::error!("implementing recovery error: {e}"),
                            _ => {}
                        }
                    }
                    Err(e) => tracing::error!("recovery repo lookup failed: {e}"),
                }

                // 2. Scan
                if let Err(e) = scanner::scan_all(&db, &*env, &*gh, &mut queues).await {
                    tracing::error!("scan error: {e}");
                }

                // 3. Spawn ready tasks
                spawn_ready_tasks(
                    &mut queues, &mut tracker, &mut join_set,
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
                                if let Ok(repos) = db.repo_find_enabled() {
                                    if let Some(repo) = repos.first() {
                                        if let Ok(base) = workspace.ensure_cloned(&repo.url, &repo.name).await {
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

                                            let resolved_repo = recovery::resolve_repos(
                                                std::slice::from_ref(repo), &*env,
                                            );
                                            let repo_gh_host = resolved_repo.first()
                                                .and_then(|r| r.gh_host.as_deref());
                                            crate::knowledge::daily::post_daily_report(
                                                &*gh, &repo.name, &report, repo_gh_host,
                                            ).await;

                                            if !report.suggestions.is_empty() {
                                                crate::knowledge::daily::create_knowledge_prs(
                                                    &*gh, &workspace, &repo.name, &report,
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
                let ds = status::build_status(&queues, &counters, start_time);
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
                pipeline::handle_task_output(&mut queues, &db, output);
            }
        }
    }

    status::remove_status(&status_path);
    pid::remove_pid(home);
    Ok(())
}

/// cursor에서 reconcile_window_hours를 빼서 safe_since를 계산
///
/// cursor가 없으면 현재 시점 - window를 사용.
/// cursor 파싱 실패 시에도 현재 시점 - window를 사용.
fn compute_safe_since(cursor: Option<String>, window_hours: u32) -> Option<String> {
    let window = chrono::Duration::hours(window_hours as i64);
    let base = match cursor {
        Some(ref s) => chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        None => chrono::Utc::now(),
    };
    Some((base - window).to_rfc3339())
}

/// PR body에서 `Closes #N`, `Fixes #N`, `Resolves #N` 패턴을 파싱하여
/// source issue number를 추출한다. 대소문자 무시.
fn extract_source_issue_from_body(body: Option<&str>) -> Option<i64> {
    let body = body?;
    let lower = body.to_lowercase();
    for prefix in &["closes #", "fixes #", "resolves #"] {
        if let Some(pos) = lower.find(prefix) {
            let start = pos + prefix.len();
            let num_str: String = lower[start..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(n) = num_str.parse::<i64>() {
                if n > 0 {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// Bounded reconciliation: 재시작 시 메모리 큐를 GitHub 라벨 기반으로 복구
///
/// cursor - reconcile_window_hours 범위의 open 이슈/PR을 조회하여,
/// autodev 라벨이 없는 항목을 큐에 적재한다.
async fn startup_reconcile(
    db: &Database,
    env: &dyn Env,
    gh: &dyn Gh,
    queues: &mut TaskQueues,
    reconcile_window_hours: u32,
) -> Result<u64> {
    use crate::queue::task_queues::{
        issue_phase, labels, make_work_id, pr_phase, IssueItem, PrItem,
    };

    let repos = db.repo_find_enabled()?;
    let mut recovered = 0u64;

    let resolved = recovery::resolve_repos(&repos, env);

    for (i, repo) in repos.iter().enumerate() {
        let gh_host = resolved[i].gh_host();
        // issues 복구: cursor - reconcile_window_hours로 bounded window 적용
        let safe_since = compute_safe_since(
            db.cursor_get_last_seen(&repo.id, "issues")?,
            reconcile_window_hours,
        );
        let mut params: Vec<(&str, &str)> =
            vec![("state", "open"), ("sort", "updated"), ("per_page", "100")];
        if let Some(ref s) = safe_since {
            params.push(("since", s));
        }

        if let Ok(data) = gh
            .api_paginate(&repo.name, "issues", &params, gh_host)
            .await
        {
            let items: Vec<serde_json::Value> = serde_json::from_slice(&data).unwrap_or_default();
            for item in items {
                // PR 제외
                if item.get("pull_request").is_some() {
                    continue;
                }
                let number = match item["number"].as_i64() {
                    Some(n) if n > 0 => n,
                    _ => continue,
                };

                let item_labels: Vec<&str> = item["labels"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|l| l["name"].as_str()).collect())
                    .unwrap_or_default();

                let has_done = item_labels.contains(&labels::DONE);
                let has_skip = item_labels.contains(&labels::SKIP);
                let has_wip = item_labels.contains(&labels::WIP);
                let has_analyze = item_labels.contains(&labels::ANALYZE);
                // v2: 새 라벨 상태 확인
                let has_analyzed = item_labels.contains(&labels::ANALYZED);
                let has_approved = item_labels.contains(&labels::APPROVED_ANALYSIS);
                let has_implementing = item_labels.contains(&labels::IMPLEMENTING);

                if has_done || has_skip {
                    continue;
                }

                // 트리거 라벨 → scan()이 다음 주기에 처리
                if has_analyze {
                    continue;
                }

                // v2: analyzed → 사람 리뷰 대기 중, skip
                if has_analyzed {
                    continue;
                }

                // v2: implementing → PR pipeline이 처리 중, skip
                if has_implementing {
                    continue;
                }

                let work_id = make_work_id("issue", &repo.name, number);
                if queues.contains(&work_id) {
                    continue;
                }

                // v2: approved-analysis → Ready 큐에 적재
                if has_approved {
                    gh.label_remove(&repo.name, number, labels::APPROVED_ANALYSIS, gh_host)
                        .await;
                    gh.label_remove(&repo.name, number, labels::ANALYZED, gh_host)
                        .await;
                    gh.label_add(&repo.name, number, labels::IMPLEMENTING, gh_host)
                        .await;

                    let issue_item = IssueItem {
                        work_id,
                        repo_id: repo.id.clone(),
                        repo_name: repo.name.clone(),
                        repo_url: repo.url.clone(),
                        github_number: number,
                        title: item["title"].as_str().unwrap_or("").to_string(),
                        body: item["body"].as_str().map(|s| s.to_string()),
                        labels: item_labels.iter().map(|s| s.to_string()).collect(),
                        author: item["user"]["login"].as_str().unwrap_or("").to_string(),
                        analysis_report: None,
                    };

                    queues.issues.push(issue_phase::READY, issue_item);
                    recovered += 1;
                    continue;
                }

                // orphan wip → 분석 중 크래시. wip 유지 + Pending 적재하여 분석 재개
                if has_wip {
                    let issue_item = IssueItem {
                        work_id,
                        repo_id: repo.id.clone(),
                        repo_name: repo.name.clone(),
                        repo_url: repo.url.clone(),
                        github_number: number,
                        title: item["title"].as_str().unwrap_or("").to_string(),
                        body: item["body"].as_str().map(|s| s.to_string()),
                        labels: item_labels.iter().map(|s| s.to_string()).collect(),
                        author: item["user"]["login"].as_str().unwrap_or("").to_string(),
                        analysis_report: None,
                    };

                    queues.issues.push(issue_phase::PENDING, issue_item);
                    recovered += 1;
                    continue;
                }

                // Label-Positive: autodev 라벨 없음 → 무시
                // (사람이 autodev:analyze 라벨을 추가해야 scan() 대상이 됨)
            }
        }

        // pulls 복구: cursor - reconcile_window_hours로 bounded window 적용
        let safe_since_pulls = compute_safe_since(
            db.cursor_get_last_seen(&repo.id, "pulls")?,
            reconcile_window_hours,
        );
        let mut params: Vec<(&str, &str)> =
            vec![("state", "open"), ("sort", "updated"), ("per_page", "100")];
        if let Some(ref s) = safe_since_pulls {
            params.push(("since", s));
        }

        if let Ok(data) = gh.api_paginate(&repo.name, "pulls", &params, gh_host).await {
            let items: Vec<serde_json::Value> = serde_json::from_slice(&data).unwrap_or_default();
            for item in items {
                let number = match item["number"].as_i64() {
                    Some(n) if n > 0 => n,
                    _ => continue,
                };

                let item_labels: Vec<&str> = item["labels"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|l| l["name"].as_str()).collect())
                    .unwrap_or_default();

                let has_done = item_labels.contains(&labels::DONE);
                let has_skip = item_labels.contains(&labels::SKIP);
                let has_wip = item_labels.contains(&labels::WIP);

                if has_done || has_skip {
                    continue;
                }

                let work_id = make_work_id("pr", &repo.name, number);
                if queues.contains(&work_id) {
                    continue;
                }

                if has_wip {
                    gh.label_remove(&repo.name, number, labels::WIP, gh_host)
                        .await;
                }

                let pr_body = item["body"].as_str();
                let source_issue_number = extract_source_issue_from_body(pr_body);

                let pr_item = PrItem {
                    work_id,
                    repo_id: repo.id.clone(),
                    repo_name: repo.name.clone(),
                    repo_url: repo.url.clone(),
                    github_number: number,
                    title: item["title"].as_str().unwrap_or("").to_string(),
                    head_branch: item["head"]["ref"].as_str().unwrap_or("").to_string(),
                    base_branch: item["base"]["ref"].as_str().unwrap_or("").to_string(),
                    review_comment: None,
                    source_issue_number,
                    review_iteration: labels::parse_iteration(&item_labels),
                };

                gh.label_add(&repo.name, number, labels::WIP, gh_host).await;
                queues.prs.push(pr_phase::PENDING, pr_item);
                recovered += 1;
            }
        }
    }

    Ok(recovered)
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
    use crate::infrastructure::gh::mock::MockGh;
    use crate::queue::task_queues::{issue_phase, make_work_id, IssueItem, TaskQueues};

    struct MockEnv;

    impl Env for MockEnv {
        fn var(&self, key: &str) -> Result<String, std::env::VarError> {
            match key {
                "AUTODEV_HOME" => Ok("/tmp/autodev-test".to_string()),
                _ => Err(std::env::VarError::NotPresent),
            }
        }
    }

    fn open_memory_db() -> Database {
        let db = Database::open(std::path::Path::new(":memory:")).expect("open in-memory db");
        db.initialize().expect("initialize schema");
        db
    }

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
    // compute_safe_since 테스트
    // ═══════════════════════════════════════════════

    #[test]
    fn compute_safe_since_valid_cursor() {
        let cursor = Some("2026-02-22T10:00:00+00:00".to_string());
        let result = compute_safe_since(cursor, 24).unwrap();
        let dt = chrono::DateTime::parse_from_rfc3339(&result).unwrap();
        let expected = chrono::DateTime::parse_from_rfc3339("2026-02-21T10:00:00+00:00").unwrap();
        assert_eq!(dt, expected);
    }

    #[test]
    fn compute_safe_since_none_cursor_uses_now() {
        let before =
            chrono::Utc::now() - chrono::Duration::hours(24) - chrono::Duration::seconds(5);
        let result = compute_safe_since(None, 24).unwrap();
        let dt = chrono::DateTime::parse_from_rfc3339(&result)
            .unwrap()
            .with_timezone(&chrono::Utc);
        let after = chrono::Utc::now() - chrono::Duration::hours(24) + chrono::Duration::seconds(5);
        assert!(dt >= before && dt <= after);
    }

    /// M-1: 잘못된 cursor 파싱 실패 시 now()로 폴백 (로그 없이)
    #[test]
    fn compute_safe_since_malformed_cursor_falls_back_to_now() {
        let before = chrono::Utc::now() - chrono::Duration::hours(6) - chrono::Duration::seconds(5);
        let result = compute_safe_since(Some("not-a-timestamp".to_string()), 6).unwrap();
        let dt = chrono::DateTime::parse_from_rfc3339(&result)
            .unwrap()
            .with_timezone(&chrono::Utc);
        let after = chrono::Utc::now() - chrono::Duration::hours(6) + chrono::Duration::seconds(5);
        assert!(
            dt >= before && dt <= after,
            "malformed cursor should fall back to now - 6h, got {dt}"
        );
    }

    /// M-2: 미래 cursor가 클램핑되지 않음을 검증
    #[test]
    fn compute_safe_since_future_cursor_not_clamped() {
        let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        let result = compute_safe_since(Some(future), 24).unwrap();
        let dt = chrono::DateTime::parse_from_rfc3339(&result)
            .unwrap()
            .with_timezone(&chrono::Utc);
        // dt = (now + 1d) - 24h ≈ now
        let diff = (dt - chrono::Utc::now()).num_seconds().abs();
        assert!(
            diff < 10,
            "future cursor - 24h should be ~now, but diff was {diff}s"
        );
    }

    #[test]
    fn compute_safe_since_zero_window() {
        let cursor = Some("2026-02-22T12:00:00+00:00".to_string());
        let result = compute_safe_since(cursor.clone(), 0).unwrap();
        let dt = chrono::DateTime::parse_from_rfc3339(&result).unwrap();
        let expected = chrono::DateTime::parse_from_rfc3339("2026-02-22T12:00:00+00:00").unwrap();
        assert_eq!(dt, expected);
    }

    // ═══════════════════════════════════════════════
    // startup_reconcile 테스트
    // ═══════════════════════════════════════════════

    /// Label-Positive: 라벨 없는 이슈는 무시됨 (사람이 analyze 라벨 추가 필요)
    #[tokio::test]
    async fn startup_reconcile_skips_unlabeled_issues() {
        let db = open_memory_db();
        let _repo_id = db
            .repo_add("https://github.com/org/repo", "org/repo")
            .unwrap();

        let gh = MockGh::new();
        let issues = serde_json::json!([{
            "number": 10,
            "title": "Test issue",
            "body": "test body",
            "labels": [],
            "user": {"login": "alice"}
        }]);
        gh.set_paginate("org/repo", "issues", serde_json::to_vec(&issues).unwrap());
        gh.set_paginate("org/repo", "pulls", b"[]".to_vec());

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&db, &MockEnv, &gh, &mut queues, 24)
            .await
            .unwrap();

        assert_eq!(
            result, 0,
            "unlabeled issues should be ignored (Label-Positive)"
        );
        assert!(!queues.contains("issue:org/repo:10"));
    }

    #[tokio::test]
    async fn startup_reconcile_recovers_open_prs() {
        let db = open_memory_db();
        let _repo_id = db
            .repo_add("https://github.com/org/repo", "org/repo")
            .unwrap();

        let gh = MockGh::new();
        gh.set_paginate("org/repo", "issues", b"[]".to_vec());
        let pulls = serde_json::json!([{
            "number": 20,
            "title": "Test PR",
            "labels": [],
            "head": {"ref": "feat/test"},
            "base": {"ref": "main"},
            "user": {"login": "bob"}
        }]);
        gh.set_paginate("org/repo", "pulls", serde_json::to_vec(&pulls).unwrap());

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&db, &MockEnv, &gh, &mut queues, 24)
            .await
            .unwrap();

        assert_eq!(result, 1);
        assert!(queues.contains("pr:org/repo:20"));
    }

    #[tokio::test]
    async fn startup_reconcile_skips_done_and_skip_labels() {
        let db = open_memory_db();
        let _repo_id = db
            .repo_add("https://github.com/org/repo", "org/repo")
            .unwrap();

        let gh = MockGh::new();
        let issues = serde_json::json!([
            {"number": 1, "title": "Done", "labels": [{"name": "autodev:done"}], "user": {"login": "a"}},
            {"number": 2, "title": "Skip", "labels": [{"name": "autodev:skip"}], "user": {"login": "a"}},
            {"number": 3, "title": "Normal", "labels": [], "user": {"login": "a"}}
        ]);
        gh.set_paginate("org/repo", "issues", serde_json::to_vec(&issues).unwrap());
        gh.set_paginate("org/repo", "pulls", b"[]".to_vec());

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&db, &MockEnv, &gh, &mut queues, 24)
            .await
            .unwrap();

        // Label-Positive: done/skip → skip, unlabeled #3 → also ignored
        assert_eq!(result, 0, "all issues should be skipped");
        assert!(!queues.contains("issue:org/repo:1"));
        assert!(!queues.contains("issue:org/repo:2"));
        assert!(!queues.contains("issue:org/repo:3"));
    }

    /// Label-Positive: orphan wip → wip 유지 + Pending 적재 (분석 재개)
    #[tokio::test]
    async fn startup_reconcile_recovers_orphan_wip() {
        let db = open_memory_db();
        let _repo_id = db
            .repo_add("https://github.com/org/repo", "org/repo")
            .unwrap();

        let gh = MockGh::new();
        let issues = serde_json::json!([{
            "number": 42,
            "title": "Orphan WIP",
            "labels": [{"name": "autodev:wip"}],
            "user": {"login": "alice"}
        }]);
        gh.set_paginate("org/repo", "issues", serde_json::to_vec(&issues).unwrap());
        gh.set_paginate("org/repo", "pulls", b"[]".to_vec());

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&db, &MockEnv, &gh, &mut queues, 24)
            .await
            .unwrap();

        assert_eq!(result, 1);
        assert!(queues.contains("issue:org/repo:42"));

        // wip 라벨은 유지 (제거도 추가도 안 함)
        let removed = gh.removed_labels.lock().unwrap();
        assert!(!removed
            .iter()
            .any(|(r, n, l)| r == "org/repo" && *n == 42 && l == "autodev:wip"));

        let added = gh.added_labels.lock().unwrap();
        assert!(!added
            .iter()
            .any(|(r, n, l)| r == "org/repo" && *n == 42 && l == "autodev:wip"));
    }

    #[tokio::test]
    async fn startup_reconcile_empty_repos_returns_zero() {
        let db = open_memory_db();
        let gh = MockGh::new();
        let mut queues = TaskQueues::new();

        let result = startup_reconcile(&db, &MockEnv, &gh, &mut queues, 24)
            .await
            .unwrap();
        assert_eq!(result, 0);
    }

    #[tokio::test]
    async fn startup_reconcile_skips_prs_in_issue_endpoint() {
        let db = open_memory_db();
        let _repo_id = db
            .repo_add("https://github.com/org/repo", "org/repo")
            .unwrap();

        let gh = MockGh::new();
        let issues = serde_json::json!([{
            "number": 5,
            "title": "PR in issues endpoint",
            "labels": [],
            "pull_request": {"url": "https://api.github.com/repos/org/repo/pulls/5"},
            "user": {"login": "alice"}
        }]);
        gh.set_paginate("org/repo", "issues", serde_json::to_vec(&issues).unwrap());
        gh.set_paginate("org/repo", "pulls", b"[]".to_vec());

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&db, &MockEnv, &gh, &mut queues, 24)
            .await
            .unwrap();

        assert_eq!(result, 0, "PRs in issue endpoint should be skipped");
    }

    #[tokio::test]
    async fn startup_reconcile_skips_already_queued_items() {
        let db = open_memory_db();
        let _repo_id = db
            .repo_add("https://github.com/org/repo", "org/repo")
            .unwrap();

        let gh = MockGh::new();
        let issues = serde_json::json!([
            {"number": 10, "title": "Issue 10", "labels": [], "user": {"login": "a"}}
        ]);
        gh.set_paginate("org/repo", "issues", serde_json::to_vec(&issues).unwrap());
        gh.set_paginate("org/repo", "pulls", b"[]".to_vec());

        let mut queues = TaskQueues::new();
        queues.issues.push(
            issue_phase::PENDING,
            IssueItem {
                work_id: make_work_id("issue", "org/repo", 10),
                repo_id: "r1".to_string(),
                repo_name: "org/repo".to_string(),
                repo_url: "https://github.com/org/repo".to_string(),
                github_number: 10,
                title: "Already queued".to_string(),
                body: None,
                labels: vec![],
                author: "a".to_string(),
                analysis_report: None,
            },
        );

        let result = startup_reconcile(&db, &MockEnv, &gh, &mut queues, 24)
            .await
            .unwrap();
        assert_eq!(result, 0, "already queued items should be skipped");
    }

    /// C-6: reconcile_window_hours 파라미터가 정상 동작하는지 검증
    #[tokio::test]
    async fn startup_reconcile_uses_configurable_window() {
        let db = open_memory_db();
        let _repo_id = db
            .repo_add("https://github.com/org/repo", "org/repo")
            .unwrap();

        let gh = MockGh::new();
        // Label-Positive: autodev:wip 라벨이 있어야 reconcile 대상
        let issues = serde_json::json!([
            {"number": 1, "title": "Issue", "labels": [{"name": "autodev:wip"}], "user": {"login": "a"}}
        ]);
        gh.set_paginate("org/repo", "issues", serde_json::to_vec(&issues).unwrap());
        gh.set_paginate("org/repo", "pulls", b"[]".to_vec());

        let mut queues = TaskQueues::new();

        // window=48h should still work — function accepts the param
        let result = startup_reconcile(&db, &MockEnv, &gh, &mut queues, 48)
            .await
            .unwrap();
        assert_eq!(result, 1, "48h window should work the same as 24h");
    }

    // ═══════════════════════════════════════════════
    // v2: reconcile 라벨 필터 확장
    // ═══════════════════════════════════════════════

    #[tokio::test]
    async fn startup_reconcile_skips_analyzed_label() {
        let db = open_memory_db();
        let _repo_id = db
            .repo_add("https://github.com/org/repo", "org/repo")
            .unwrap();

        let gh = MockGh::new();
        let issues = serde_json::json!([
            {"number": 1, "title": "Analyzed", "labels": [{"name": "autodev:analyzed"}], "user": {"login": "a"}}
        ]);
        gh.set_paginate("org/repo", "issues", serde_json::to_vec(&issues).unwrap());
        gh.set_paginate("org/repo", "pulls", b"[]".to_vec());

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&db, &MockEnv, &gh, &mut queues, 24)
            .await
            .unwrap();

        assert_eq!(
            result, 0,
            "analyzed issues should be skipped (awaiting human review)"
        );
    }

    #[tokio::test]
    async fn startup_reconcile_skips_implementing_label() {
        let db = open_memory_db();
        let _repo_id = db
            .repo_add("https://github.com/org/repo", "org/repo")
            .unwrap();

        let gh = MockGh::new();
        let issues = serde_json::json!([
            {"number": 2, "title": "Implementing", "labels": [{"name": "autodev:implementing"}], "user": {"login": "a"}}
        ]);
        gh.set_paginate("org/repo", "issues", serde_json::to_vec(&issues).unwrap());
        gh.set_paginate("org/repo", "pulls", b"[]".to_vec());

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&db, &MockEnv, &gh, &mut queues, 24)
            .await
            .unwrap();

        assert_eq!(
            result, 0,
            "implementing issues should be skipped (PR pipeline handles)"
        );
    }

    #[tokio::test]
    async fn startup_reconcile_recovers_approved_analysis_to_ready() {
        let db = open_memory_db();
        let _repo_id = db
            .repo_add("https://github.com/org/repo", "org/repo")
            .unwrap();

        let gh = MockGh::new();
        let issues = serde_json::json!([
            {"number": 3, "title": "Approved", "labels": [{"name": "autodev:approved-analysis"}], "user": {"login": "a"}}
        ]);
        gh.set_paginate("org/repo", "issues", serde_json::to_vec(&issues).unwrap());
        gh.set_paginate("org/repo", "pulls", b"[]".to_vec());

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&db, &MockEnv, &gh, &mut queues, 24)
            .await
            .unwrap();

        assert_eq!(
            result, 1,
            "approved-analysis issues should be recovered to Ready"
        );
        assert!(queues.contains("issue:org/repo:3"));

        // implementing 라벨 추가, approved-analysis 제거
        let added = gh.added_labels.lock().unwrap();
        assert!(added
            .iter()
            .any(|(r, n, l)| r == "org/repo" && *n == 3 && l == "autodev:implementing"));

        let removed = gh.removed_labels.lock().unwrap();
        assert!(removed
            .iter()
            .any(|(r, n, l)| r == "org/repo" && *n == 3 && l == "autodev:approved-analysis"));
    }
}
