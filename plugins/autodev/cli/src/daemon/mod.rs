pub mod pid;
pub mod recovery;

use std::path::Path;

use anyhow::{bail, Result};
use chrono::Timelike;
use tracing::info;

use crate::components::notifier::Notifier;
use crate::components::workspace::Workspace;
use crate::config::{self, Env};
use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::pipeline;
use crate::queue::repository::{RepoRepository, ScanCursorRepository};
use crate::queue::task_queues::TaskQueues;
use crate::queue::Database;
use crate::scanner;

/// 데몬을 포그라운드로 시작
pub async fn start(
    home: &Path,
    env: &dyn Env,
    gh: &dyn Gh,
    git: &dyn Git,
    claude: &dyn Claude,
) -> Result<()> {
    if pid::is_running(home) {
        bail!(
            "daemon is already running (pid: {})",
            pid::read_pid(home).unwrap_or(0)
        );
    }

    info!("starting autodev daemon...");

    pid::write_pid(home)?;

    let cfg = config::loader::load_merged(env, None);

    let db_path = home.join("autodev.db");
    let db = Database::open(&db_path)?;
    db.initialize()?;

    println!("autodev daemon started (pid: {})", std::process::id());

    let mut queues = TaskQueues::new();
    let workspace = Workspace::new(git, env);
    let notifier = Notifier::new(gh);

    let gh_host = cfg.consumer.gh_host.clone();
    let daily_report_hour = cfg.consumer.daily_report_hour;
    let knowledge_extraction = cfg.consumer.knowledge_extraction;
    let mut last_daily_report_date = String::new();

    let reconcile_window_hours = 24u32;

    // 0. Startup Reconcile (bounded recovery)
    match startup_reconcile(&db, gh, &mut queues, gh_host.as_deref(), reconcile_window_hours).await {
        Ok(n) if n > 0 => info!("startup reconcile: recovered {n} items"),
        Err(e) => tracing::error!("startup reconcile failed: {e}"),
        _ => {}
    }

    // 메인 루프: recovery → scanner → pipeline
    tokio::select! {
        _ = async {
            loop {
                // 1. Recovery: orphan autodev:wip 라벨 정리
                match db.repo_find_enabled() {
                    Ok(repos) => {
                        match recovery::recover_orphan_wip(&repos, gh, &queues, gh_host.as_deref()).await {
                            Ok(n) if n > 0 => info!("recovered {n} orphan wip items"),
                            Err(e) => tracing::error!("recovery error: {e}"),
                            _ => {}
                        }
                    }
                    Err(e) => tracing::error!("recovery repo lookup failed: {e}"),
                }

                // 2. Scan
                if let Err(e) = scanner::scan_all(&db, env, gh, &mut queues).await {
                    tracing::error!("scan error: {e}");
                }

                // 3. Pipeline
                if let Err(e) = pipeline::process_all(&db, env, &workspace, &notifier, gh, claude, &mut queues).await {
                    tracing::error!("pipeline error: {e}");
                }

                // 4. Daily Report (scheduled at daily_report_hour)
                if knowledge_extraction {
                    let now = chrono::Local::now();
                    let today = now.format("%Y-%m-%d").to_string();
                    if now.hour() >= daily_report_hour && last_daily_report_date != today {
                        let yesterday = (now - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
                        let log_path = home.join(format!("daemon.{yesterday}.log"));

                        if log_path.exists() {
                            let stats = crate::knowledge::daily::parse_daemon_log(&log_path);
                            if stats.task_count > 0 {
                                let patterns = crate::knowledge::daily::detect_patterns(&stats);
                                let mut report = crate::knowledge::daily::build_daily_report(&yesterday, &stats, patterns);

                                // 첫 번째 활성 레포의 worktree에서 Claude 실행
                                if let Ok(repos) = db.repo_find_enabled() {
                                    if let Some(repo) = repos.first() {
                                        if let Ok(base) = workspace.ensure_cloned(&repo.url, &repo.name).await {
                                            if let Some(ks) = crate::knowledge::daily::generate_daily_suggestions(
                                                claude, &report, &base,
                                            ).await {
                                                report.suggestions = ks.suggestions;
                                            }

                                            crate::knowledge::daily::post_daily_report(
                                                gh, &repo.name, &report, gh_host.as_deref(),
                                            ).await;

                                            // Knowledge PR 생성 (suggestions → PR + autodev:skip)
                                            if !report.suggestions.is_empty() {
                                                crate::knowledge::daily::create_knowledge_prs(
                                                    gh, git, &repo.name, &report, &base,
                                                    gh_host.as_deref(),
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

                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
        } => {},
        _ = tokio::signal::ctrl_c() => {
            info!("received SIGINT, shutting down...");
        }
    }

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

/// Bounded reconciliation: 재시작 시 메모리 큐를 GitHub 라벨 기반으로 복구
///
/// cursor - reconcile_window_hours 범위의 open 이슈/PR을 조회하여,
/// autodev 라벨이 없는 항목을 큐에 적재한다.
async fn startup_reconcile(
    db: &Database,
    gh: &dyn Gh,
    queues: &mut TaskQueues,
    gh_host: Option<&str>,
    reconcile_window_hours: u32,
) -> Result<u64> {
    use crate::queue::task_queues::{labels, issue_phase, pr_phase, make_work_id, IssueItem, PrItem};

    let repos = db.repo_find_enabled()?;
    let mut recovered = 0u64;

    for repo in &repos {
        // issues 복구: cursor - reconcile_window_hours로 bounded window 적용
        let safe_since = compute_safe_since(
            db.cursor_get_last_seen(&repo.id, "issues")?,
            reconcile_window_hours,
        );
        let mut params: Vec<(&str, &str)> = vec![
            ("state", "open"),
            ("sort", "updated"),
            ("per_page", "100"),
        ];
        if let Some(ref s) = safe_since {
            params.push(("since", s));
        }

        if let Ok(data) = gh.api_paginate(&repo.name, "issues", &params, gh_host).await {
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

                let has_done = item_labels.iter().any(|l| *l == labels::DONE);
                let has_skip = item_labels.iter().any(|l| *l == labels::SKIP);
                let has_wip = item_labels.iter().any(|l| *l == labels::WIP);

                if has_done || has_skip {
                    continue;
                }

                let work_id = make_work_id("issue", &repo.name, number);
                if queues.contains(&work_id) {
                    continue;
                }

                // orphan wip → 라벨 제거 후 큐 적재
                if has_wip {
                    gh.label_remove(&repo.name, number, labels::WIP, gh_host).await;
                }

                // 큐에 적재 + wip 라벨 추가
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

                gh.label_add(&repo.name, number, labels::WIP, gh_host).await;
                queues.issues.push(issue_phase::PENDING, issue_item);
                recovered += 1;
            }
        }

        // pulls 복구: cursor - reconcile_window_hours로 bounded window 적용
        let safe_since_pulls = compute_safe_since(
            db.cursor_get_last_seen(&repo.id, "pulls")?,
            reconcile_window_hours,
        );
        let mut params: Vec<(&str, &str)> = vec![
            ("state", "open"),
            ("sort", "updated"),
            ("per_page", "100"),
        ];
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

                let has_done = item_labels.iter().any(|l| *l == labels::DONE);
                let has_skip = item_labels.iter().any(|l| *l == labels::SKIP);
                let has_wip = item_labels.iter().any(|l| *l == labels::WIP);

                if has_done || has_skip {
                    continue;
                }

                let work_id = make_work_id("pr", &repo.name, number);
                if queues.contains(&work_id) {
                    continue;
                }

                if has_wip {
                    gh.label_remove(&repo.name, number, labels::WIP, gh_host).await;
                }

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
pub fn stop(home: &Path) -> Result<()> {
    let pid = pid::read_pid(home).ok_or_else(|| anyhow::anyhow!("daemon is not running"))?;

    std::process::Command::new("kill")
        .arg(pid.to_string())
        .status()?;

    pid::remove_pid(home);
    println!("autodev daemon stopped (pid: {pid})");
    Ok(())
}
