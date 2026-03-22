use anyhow::Result;

use crate::core::config;
use crate::core::config::Env;
use crate::core::repository::WorkspaceRepository;
use crate::infra::db::Database;
use crate::infra::gh::Gh;
use crate::service::tasks::helpers::git_ops_factory::resolve_gh_host;
use crate::service::tasks::knowledge::daily;

/// Generate and post a daily report for the given date.
///
/// This is a standalone CLI entry point that reuses the daemon's daily report
/// logic without requiring the full daemon infrastructure (Claude, Git, SuggestWorkflow).
/// It parses the daemon log, builds a report with detected patterns, and posts
/// a GitHub issue for each enabled repository.
pub async fn daily(
    db: &Database,
    env: &dyn Env,
    gh: &dyn Gh,
    home: &std::path::Path,
    date: &str,
) -> Result<String> {
    // Validate date format to prevent path traversal (e.g. "../../etc/passwd")
    if chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
        anyhow::bail!("invalid date format: {date} (expected YYYY-MM-DD)");
    }

    let cfg = config::loader::load_merged(env, None);
    let log_dir = config::resolve_log_dir(&cfg.daemon.log_dir, home);
    let log_path = log_dir.join(format!("daemon.{date}.log"));

    if !log_path.exists() {
        anyhow::bail!("log file not found: {}", log_path.display());
    }

    let stats = daily::parse_daemon_log(&log_path);
    if stats.task_count == 0 {
        return Ok(format!("skip: no tasks found in {date} log"));
    }

    let patterns = daily::detect_patterns(&stats);
    let report = daily::build_daily_report(date, &stats, patterns);

    let enabled = db.workspace_find_enabled()?;
    if enabled.is_empty() {
        anyhow::bail!("no enabled repositories found");
    }

    let mut posted = 0u32;
    for repo in &enabled {
        let gh_host = resolve_gh_host(env, &repo.name);
        daily::post_daily_report(gh, &repo.name, &report, gh_host.as_deref()).await;
        posted += 1;
    }

    Ok(format!(
        "daily report for {date}: {} tasks ({} issues, {} PRs, {} failed) — posted to {posted} repo(s)",
        stats.task_count, stats.issues_done, stats.prs_done, stats.failed
    ))
}
