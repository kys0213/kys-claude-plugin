use anyhow::Result;

use crate::core::config;
use crate::core::config::Env;
use crate::core::models::*;
use crate::core::repository::*;
use crate::infra::db::Database;

/// List all cron jobs, optionally as JSON
pub fn cron_list(db: &Database, json: bool) -> Result<String> {
    let jobs = db.cron_list(None)?;

    if json {
        return Ok(serde_json::to_string_pretty(&jobs)?);
    }

    let mut output = String::new();

    let global_jobs: Vec<&CronJob> = jobs.iter().filter(|j| j.repo_id.is_none()).collect();
    let repo_jobs: Vec<&CronJob> = jobs.iter().filter(|j| j.repo_id.is_some()).collect();

    output.push_str("Global jobs:\n");
    if global_jobs.is_empty() {
        output.push_str("  (none)\n");
    } else {
        for job in &global_jobs {
            output.push_str(&format_job_line(job));
        }
    }

    output.push('\n');
    output.push_str("Per-repo jobs:\n");
    if repo_jobs.is_empty() {
        output.push_str("  (none)\n");
    } else {
        for job in &repo_jobs {
            output.push_str(&format_job_line(job));
        }
    }

    Ok(output)
}

fn format_job_line(job: &CronJob) -> String {
    let status_icon = match job.status {
        CronStatus::Active => "●",
        CronStatus::Paused => "○",
    };
    let schedule_str = match &job.schedule {
        CronSchedule::Interval { secs } => format!("every {secs}s"),
        CronSchedule::Expression { cron } => format!("cron \"{cron}\""),
    };
    let builtin_tag = if job.builtin { " [builtin]" } else { "" };
    let last_run = job.last_run_at.as_deref().unwrap_or("never");
    format!(
        "  {status_icon} {name}  {schedule}  last={last_run}{builtin}\n",
        name = job.name,
        schedule = schedule_str,
        last_run = last_run,
        builtin = builtin_tag,
    )
}

/// Add a new cron job
pub fn cron_add(
    db: &Database,
    name: &str,
    script_path: &str,
    repo: Option<&str>,
    interval: Option<u64>,
    schedule: Option<&str>,
) -> Result<()> {
    let cron_schedule = match (interval, schedule) {
        (Some(secs), None) => CronSchedule::Interval { secs },
        (None, Some(expr)) => CronSchedule::Expression {
            cron: expr.to_string(),
        },
        (Some(_), Some(_)) => {
            anyhow::bail!("specify either --interval or --schedule, not both");
        }
        (None, None) => {
            anyhow::bail!("specify either --interval <secs> or --schedule \"<cron>\"");
        }
    };

    let repo_id = if let Some(r) = repo {
        Some(resolve_repo_id(db, r)?)
    } else {
        None
    };

    let job = NewCronJob {
        name: name.to_string(),
        repo_id,
        schedule: cron_schedule,
        script_path: script_path.to_string(),
        builtin: false,
    };

    let id = db.cron_add(&job)?;
    println!("added cron job: {name} (id={id})");
    Ok(())
}

/// Update cron job interval
pub fn cron_update(db: &Database, name: &str, repo: Option<&str>, interval: u64) -> Result<()> {
    db.cron_update_interval(name, repo, interval)?;
    println!("updated cron job: {name} (interval={interval}s)");
    Ok(())
}

/// Pause a cron job
pub fn cron_pause(db: &Database, name: &str, repo: Option<&str>) -> Result<()> {
    db.cron_set_status(name, repo, CronStatus::Paused)?;
    println!("paused cron job: {name}");
    Ok(())
}

/// Resume a cron job
pub fn cron_resume(db: &Database, name: &str, repo: Option<&str>) -> Result<()> {
    db.cron_set_status(name, repo, CronStatus::Active)?;
    println!("resumed cron job: {name}");
    Ok(())
}

/// Remove a cron job
pub fn cron_remove(db: &Database, name: &str, repo: Option<&str>) -> Result<()> {
    db.cron_remove(name, repo)?;
    println!("removed cron job: {name}");
    Ok(())
}

/// Trigger (execute immediately) a cron job
pub fn cron_trigger(db: &Database, env: &dyn Env, name: &str, repo: Option<&str>) -> Result<()> {
    let job = db
        .cron_show(name, repo)?
        .ok_or_else(|| anyhow::anyhow!("cron job not found: {name}"))?;

    let home = config::autodev_home(env);
    let db_path = home.join("autodev.db");

    let mut cmd = std::process::Command::new(&job.script_path);

    // Always-present env vars
    cmd.env("AUTODEV_HOME", home.to_string_lossy().as_ref());
    cmd.env("AUTODEV_DB", db_path.to_string_lossy().as_ref());
    cmd.env(
        "AUTODEV_CLAW_WORKSPACE",
        config::workspaces_path(env).to_string_lossy().as_ref(),
    );

    // Per-repo env vars
    if let Some(repo_name) = repo {
        if let Some(repo_info) = find_repo_info(db, repo_name)? {
            cmd.env("AUTODEV_REPO_NAME", &repo_info.name);
            // Derive repo root from workspace path
            let ws = config::workspaces_path(env).join(config::sanitize_repo_name(&repo_info.name));
            cmd.env("AUTODEV_REPO_ROOT", ws.to_string_lossy().as_ref());
        }
    }

    println!("triggering cron job: {name} ({})", job.script_path);
    let output = cmd.output()?;

    if !output.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }

    // Update last_run_at
    db.cron_update_last_run(&job.id)?;

    if output.status.success() {
        println!("cron job {name} completed successfully");
    } else {
        let code = output.status.code().unwrap_or(-1);
        anyhow::bail!("cron job {name} failed with exit code {code}");
    }

    Ok(())
}

// ─── Helpers ───

fn resolve_repo_id(db: &Database, repo_name: &str) -> Result<String> {
    let repos = db.repo_list()?;
    repos
        .iter()
        .find(|r| r.name == repo_name)
        .map(|_| {
            // Get the actual ID by querying enabled repos
            let enabled = db.repo_find_enabled().unwrap_or_default();
            enabled
                .iter()
                .find(|e| e.name == repo_name)
                .map(|e| e.id.clone())
                .unwrap_or_default()
        })
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow::anyhow!("repository not found: {repo_name}"))
}

fn find_repo_info(db: &Database, repo_name: &str) -> Result<Option<EnabledRepo>> {
    let repos = db.repo_find_enabled()?;
    Ok(repos.into_iter().find(|r| r.name == repo_name))
}
