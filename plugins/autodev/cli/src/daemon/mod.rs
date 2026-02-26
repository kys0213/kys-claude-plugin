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
use crate::queue::models::ResolvedRepo;
use crate::queue::repository::RepoRepository;
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

    let tick_interval_secs = cfg.daemon.tick_interval_secs;

    let log_dir = config::resolve_log_dir(&cfg.daemon.log_dir, home);
    let log_retention_days = cfg.daemon.log_retention_days;

    // Startup cleanup: 보존 기간 초과 로그 삭제
    let n = log::cleanup_old_logs(&log_dir, log_retention_days);
    if n > 0 {
        info!("startup log cleanup: deleted {n} old log files");
    }

    // 0. Startup Reconcile: pre-fetched VO 기반 큐 복구
    match db.repo_find_enabled() {
        Ok(repos) => {
            let resolved = recovery::resolve_repos(&repos, &*env, &*gh).await;
            match startup_reconcile(&resolved, &*gh, &mut queues).await {
                Ok(n) if n > 0 => info!("startup reconcile: recovered {n} items"),
                Err(e) => tracing::error!("startup reconcile failed: {e}"),
                _ => {}
            }
        }
        Err(e) => tracing::error!("startup reconcile repo lookup failed: {e}"),
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
                        let resolved = recovery::resolve_repos(&repos, &*env, &*gh).await;
                        match recovery::recover_orphan_wip(&resolved, &*gh, &queues).await {
                            Ok(n) if n > 0 => info!("recovered {n} orphan wip items"),
                            Err(e) => tracing::error!("recovery error: {e}"),
                            _ => {}
                        }
                        match recovery::recover_orphan_implementing(&resolved, &*gh, &queues)
                            .await
                        {
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
                                                std::slice::from_ref(repo),
                                                &*env,
                                                &*gh,
                                            )
                                            .await;
                                            let repo_gh_host = resolved_repo
                                                .first()
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

/// Bounded reconciliation: 재시작 시 메모리 큐를 pre-fetched VO 기반으로 복구
///
/// ResolvedRepo에 내장된 open issues/pulls를 순회하여,
/// autodev 라벨 상태에 따라 큐에 적재한다.
async fn startup_reconcile(
    resolved: &[ResolvedRepo],
    gh: &dyn Gh,
    queues: &mut TaskQueues,
) -> Result<u64> {
    use crate::queue::task_queues::{
        issue_phase, labels, make_work_id, pr_phase, IssueItem, PrItem,
    };

    let mut recovered = 0u64;

    for repo in resolved {
        let gh_host = repo.gh_host();

        // ── Issues 복구 ──
        for issue in &repo.issues {
            if issue.is_terminal() {
                continue;
            }

            // 트리거 라벨 → scan()이 다음 주기에 처리
            if issue.is_analyze() {
                continue;
            }

            // v2: analyzed → 사람 리뷰 대기 중, skip
            if issue.is_analyzed() {
                continue;
            }

            // v2: implementing → PR pipeline이 처리 중, skip
            if issue.is_implementing() {
                continue;
            }

            let work_id = make_work_id("issue", &repo.name, issue.number);
            if queues.contains(&work_id) {
                continue;
            }

            // v2: approved-analysis → Ready 큐에 적재
            if issue.is_approved() {
                gh.label_remove(&repo.name, issue.number, labels::APPROVED_ANALYSIS, gh_host)
                    .await;
                gh.label_remove(&repo.name, issue.number, labels::ANALYZED, gh_host)
                    .await;
                gh.label_add(&repo.name, issue.number, labels::IMPLEMENTING, gh_host)
                    .await;

                let issue_item = IssueItem {
                    work_id,
                    repo_id: repo.id.clone(),
                    repo_name: repo.name.clone(),
                    repo_url: repo.url.clone(),
                    github_number: issue.number,
                    title: issue.title.clone(),
                    body: issue.body.clone(),
                    labels: issue.labels.clone(),
                    author: issue.author.clone(),
                    analysis_report: None,
                };

                queues.issues.push(issue_phase::READY, issue_item);
                recovered += 1;
                continue;
            }

            // orphan wip → 분석 중 크래시. wip 유지 + Pending 적재하여 분석 재개
            if issue.is_wip() {
                let issue_item = IssueItem {
                    work_id,
                    repo_id: repo.id.clone(),
                    repo_name: repo.name.clone(),
                    repo_url: repo.url.clone(),
                    github_number: issue.number,
                    title: issue.title.clone(),
                    body: issue.body.clone(),
                    labels: issue.labels.clone(),
                    author: issue.author.clone(),
                    analysis_report: None,
                };

                queues.issues.push(issue_phase::PENDING, issue_item);
                recovered += 1;
                continue;
            }

            // Label-Positive: autodev 라벨 없음 → 무시
            // (사람이 autodev:analyze 라벨을 추가해야 scan() 대상이 됨)
        }

        // ── PRs 복구 ──
        // Label-Positive: autodev:wip 라벨이 있는 PR만 복구 대상.
        // 라벨 없는 PR은 scanner가 다음 주기에 처리하므로 skip.
        for pull in repo.pulls.iter().filter(|p| p.is_wip()) {
            if pull.is_terminal() {
                continue;
            }

            let work_id = make_work_id("pr", &repo.name, pull.number);
            if queues.contains(&work_id) {
                continue;
            }

            // wip 유지 + Pending 적재 (issue wip 복구와 동일 패턴)
            let pr_item = PrItem {
                work_id,
                repo_id: repo.id.clone(),
                repo_name: repo.name.clone(),
                repo_url: repo.url.clone(),
                github_number: pull.number,
                title: pull.title.clone(),
                head_branch: pull.head_branch.clone(),
                base_branch: pull.base_branch.clone(),
                review_comment: None,
                source_issue_number: pull.source_issue_number(),
                review_iteration: pull.review_iteration(),
            };

            queues.prs.push(pr_phase::PENDING, pr_item);
            recovered += 1;
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
    use crate::queue::models::{RepoIssue, RepoPull};
    use crate::queue::task_queues::{issue_phase, make_work_id, IssueItem, TaskQueues};

    /// 테스트용 ResolvedRepo 빌더
    fn resolved_repo(issues: Vec<RepoIssue>, pulls: Vec<RepoPull>) -> ResolvedRepo {
        ResolvedRepo {
            id: "repo-id-1".to_string(),
            url: "https://github.com/org/repo".to_string(),
            name: "org/repo".to_string(),
            gh_host: None,
            issues,
            pulls,
        }
    }

    /// JSON에서 RepoIssue 생성 헬퍼
    fn issue_from_json(v: serde_json::Value) -> RepoIssue {
        RepoIssue::from_json(&v).expect("valid issue JSON")
    }

    /// JSON에서 RepoPull 생성 헬퍼
    fn pull_from_json(v: serde_json::Value) -> RepoPull {
        RepoPull::from_json(&v).expect("valid pull JSON")
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
    // startup_reconcile 테스트
    // ═══════════════════════════════════════════════

    /// Label-Positive: 라벨 없는 이슈는 무시됨 (사람이 analyze 라벨 추가 필요)
    #[tokio::test]
    async fn startup_reconcile_skips_unlabeled_issues() {
        let gh = MockGh::new();
        let resolved = vec![resolved_repo(
            vec![issue_from_json(serde_json::json!({
                "number": 10, "title": "Test issue", "body": "test body",
                "labels": [], "user": {"login": "alice"}
            }))],
            vec![],
        )];

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&resolved, &gh, &mut queues)
            .await
            .unwrap();

        assert_eq!(
            result, 0,
            "unlabeled issues should be ignored (Label-Positive)"
        );
        assert!(!queues.contains("issue:org/repo:10"));
    }

    /// Label-Positive: 라벨 없는 PR은 무시됨 (scanner가 다음 주기에 처리)
    #[tokio::test]
    async fn startup_reconcile_skips_unlabeled_prs() {
        let gh = MockGh::new();
        let resolved = vec![resolved_repo(
            vec![],
            vec![pull_from_json(serde_json::json!({
                "number": 20, "title": "Test PR", "labels": [],
                "head": {"ref": "feat/test"}, "base": {"ref": "main"},
                "user": {"login": "bob"}
            }))],
        )];

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&resolved, &gh, &mut queues)
            .await
            .unwrap();

        assert_eq!(
            result, 0,
            "unlabeled PRs should be ignored (Label-Positive)"
        );
        assert!(!queues.contains("pr:org/repo:20"));
    }

    /// Label-Positive: wip PR은 복구 대상 (크래시 복구)
    #[tokio::test]
    async fn startup_reconcile_recovers_wip_prs() {
        let gh = MockGh::new();
        let resolved = vec![resolved_repo(
            vec![],
            vec![pull_from_json(serde_json::json!({
                "number": 20, "title": "WIP PR",
                "labels": [{"name": "autodev:wip"}],
                "head": {"ref": "feat/test"}, "base": {"ref": "main"},
                "user": {"login": "bob"}
            }))],
        )];

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&resolved, &gh, &mut queues)
            .await
            .unwrap();

        assert_eq!(result, 1);
        assert!(queues.contains("pr:org/repo:20"));

        // wip 라벨 유지 (제거도 추가도 안 함)
        assert!(gh.removed_labels.lock().unwrap().is_empty());
        assert!(gh.added_labels.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn startup_reconcile_skips_done_and_skip_labels() {
        let gh = MockGh::new();
        let resolved = vec![resolved_repo(
            vec![
                issue_from_json(
                    serde_json::json!({"number": 1, "title": "Done", "labels": [{"name": "autodev:done"}], "user": {"login": "a"}}),
                ),
                issue_from_json(
                    serde_json::json!({"number": 2, "title": "Skip", "labels": [{"name": "autodev:skip"}], "user": {"login": "a"}}),
                ),
                issue_from_json(
                    serde_json::json!({"number": 3, "title": "Normal", "labels": [], "user": {"login": "a"}}),
                ),
            ],
            vec![],
        )];

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&resolved, &gh, &mut queues)
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
        let gh = MockGh::new();
        let resolved = vec![resolved_repo(
            vec![issue_from_json(serde_json::json!({
                "number": 42, "title": "Orphan WIP",
                "labels": [{"name": "autodev:wip"}], "user": {"login": "alice"}
            }))],
            vec![],
        )];

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&resolved, &gh, &mut queues)
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
        let gh = MockGh::new();
        let mut queues = TaskQueues::new();

        let result = startup_reconcile(&[], &gh, &mut queues).await.unwrap();
        assert_eq!(result, 0);
    }

    /// PRs in issues endpoint는 RepoIssue::from_json에서 필터링됨.
    /// ResolvedRepo.issues에 PR이 포함되지 않으므로 이 시나리오는 pre-fetch 단계에서 해결.
    #[tokio::test]
    async fn startup_reconcile_prs_filtered_at_prefetch() {
        // pull_request 필드가 있으면 RepoIssue::from_json은 None을 반환
        let json = serde_json::json!({
            "number": 5, "title": "PR in issues endpoint", "labels": [],
            "pull_request": {"url": "https://api.github.com/repos/org/repo/pulls/5"},
            "user": {"login": "alice"}
        });
        assert!(
            RepoIssue::from_json(&json).is_none(),
            "RepoIssue::from_json should return None for PRs"
        );
    }

    #[tokio::test]
    async fn startup_reconcile_skips_already_queued_items() {
        let gh = MockGh::new();
        // wip 라벨이 있어야 reconcile 대상이므로 wip 추가
        let resolved = vec![resolved_repo(
            vec![issue_from_json(serde_json::json!({
                "number": 10, "title": "Issue 10",
                "labels": [{"name": "autodev:wip"}], "user": {"login": "a"}
            }))],
            vec![],
        )];

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

        let result = startup_reconcile(&resolved, &gh, &mut queues)
            .await
            .unwrap();
        assert_eq!(result, 0, "already queued items should be skipped");
    }

    /// WIP 라벨이 있는 이슈가 reconcile 대상이 됨을 검증
    #[tokio::test]
    async fn startup_reconcile_recovers_wip_issue() {
        let gh = MockGh::new();
        let resolved = vec![resolved_repo(
            vec![issue_from_json(serde_json::json!({
                "number": 1, "title": "Issue",
                "labels": [{"name": "autodev:wip"}], "user": {"login": "a"}
            }))],
            vec![],
        )];

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&resolved, &gh, &mut queues)
            .await
            .unwrap();
        assert_eq!(result, 1, "wip issue should be recovered");
    }

    // ═══════════════════════════════════════════════
    // v2: reconcile 라벨 필터 확장
    // ═══════════════════════════════════════════════

    #[tokio::test]
    async fn startup_reconcile_skips_analyzed_label() {
        let gh = MockGh::new();
        let resolved = vec![resolved_repo(
            vec![issue_from_json(serde_json::json!({
                "number": 1, "title": "Analyzed",
                "labels": [{"name": "autodev:analyzed"}], "user": {"login": "a"}
            }))],
            vec![],
        )];

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&resolved, &gh, &mut queues)
            .await
            .unwrap();

        assert_eq!(
            result, 0,
            "analyzed issues should be skipped (awaiting human review)"
        );
    }

    #[tokio::test]
    async fn startup_reconcile_skips_implementing_label() {
        let gh = MockGh::new();
        let resolved = vec![resolved_repo(
            vec![issue_from_json(serde_json::json!({
                "number": 2, "title": "Implementing",
                "labels": [{"name": "autodev:implementing"}], "user": {"login": "a"}
            }))],
            vec![],
        )];

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&resolved, &gh, &mut queues)
            .await
            .unwrap();

        assert_eq!(
            result, 0,
            "implementing issues should be skipped (PR pipeline handles)"
        );
    }

    #[tokio::test]
    async fn startup_reconcile_recovers_approved_analysis_to_ready() {
        let gh = MockGh::new();
        let resolved = vec![resolved_repo(
            vec![issue_from_json(serde_json::json!({
                "number": 3, "title": "Approved",
                "labels": [{"name": "autodev:approved-analysis"}], "user": {"login": "a"}
            }))],
            vec![],
        )];

        let mut queues = TaskQueues::new();
        let result = startup_reconcile(&resolved, &gh, &mut queues)
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
