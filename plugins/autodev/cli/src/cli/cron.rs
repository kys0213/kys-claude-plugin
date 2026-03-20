use anyhow::Result;

use crate::cli::resolve_repo_id;
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

    // Global env vars (aligned with daemon cron runner)
    cmd.env("AUTODEV_HOME", home.to_string_lossy().as_ref());
    cmd.env("AUTODEV_DB", db_path.to_string_lossy().as_ref());
    cmd.env(
        "AUTODEV_CLAW_WORKSPACE",
        home.join("claw-workspace").to_string_lossy().as_ref(),
    );
    cmd.env("AUTODEV_JOB_NAME", &job.name);
    cmd.env("AUTODEV_JOB_ID", &job.id);

    // Per-repo env vars (aligned with daemon cron runner)
    if let Some(repo_name) = repo {
        if let Some(repo_info) = find_repo_info(db, repo_name)? {
            cmd.env("AUTODEV_REPO_NAME", &repo_info.name);
            cmd.env("AUTODEV_REPO_URL", &repo_info.url);
            cmd.env("AUTODEV_REPO_ID", &repo_info.id);
            let workspace =
                config::workspaces_path(env).join(config::sanitize_repo_name(&repo_info.name));
            cmd.env("AUTODEV_WORKSPACE", workspace.to_string_lossy().as_ref());
            cmd.env(
                "AUTODEV_REPO_ROOT",
                workspace.join("main").to_string_lossy().as_ref(),
            );
            let default_branch = detect_default_branch(&workspace);
            cmd.env("AUTODEV_REPO_DEFAULT_BRANCH", &default_branch);
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

// ─── Built-in Cron Seeding ───

/// Built-in cron 스크립트 정의
struct BuiltinCronDef {
    name: &'static str,
    script_filename: &'static str,
    script_content: &'static str,
    schedule: CronSchedule,
}

/// Cron job name for claw-evaluate (used by daemon force-trigger).
pub const CLAW_EVALUATE_JOB: &str = "claw-evaluate";

/// 템플릿에서 포함한 스크립트 내용
const CLAW_EVALUATE_SH: &str = include_str!("../../../templates/crons/claw-evaluate.sh");
const GAP_DETECTION_SH: &str = include_str!("../../../templates/crons/gap-detection.sh");
const KNOWLEDGE_EXTRACT_SH: &str = include_str!("../../../templates/crons/knowledge-extract.sh");
const HITL_TIMEOUT_SH: &str = include_str!("../../../templates/crons/hitl-timeout.sh");
const DAILY_REPORT_SH: &str = include_str!("../../../templates/crons/daily-report.sh");
const LOG_CLEANUP_SH: &str = include_str!("../../../templates/crons/log-cleanup.sh");

fn per_repo_cron_defs(claw_cfg: &config::models::ClawConfig) -> Vec<BuiltinCronDef> {
    vec![
        BuiltinCronDef {
            name: CLAW_EVALUATE_JOB,
            script_filename: "claw-evaluate.sh",
            script_content: CLAW_EVALUATE_SH,
            schedule: CronSchedule::Interval {
                secs: claw_cfg.schedule_interval_secs,
            },
        },
        BuiltinCronDef {
            name: "gap-detection",
            script_filename: "gap-detection.sh",
            script_content: GAP_DETECTION_SH,
            schedule: CronSchedule::Interval {
                secs: claw_cfg.gap_detection_interval_secs,
            },
        },
        BuiltinCronDef {
            name: "knowledge-extract",
            script_filename: "knowledge-extract.sh",
            script_content: KNOWLEDGE_EXTRACT_SH,
            schedule: CronSchedule::Interval { secs: 3600 },
        },
    ]
}

fn global_cron_defs() -> Vec<BuiltinCronDef> {
    vec![
        BuiltinCronDef {
            name: "hitl-timeout",
            script_filename: "hitl-timeout.sh",
            script_content: HITL_TIMEOUT_SH,
            schedule: CronSchedule::Interval { secs: 300 },
        },
        BuiltinCronDef {
            name: "daily-report",
            script_filename: "daily-report.sh",
            script_content: DAILY_REPORT_SH,
            schedule: CronSchedule::Expression {
                cron: "0 6 * * *".to_string(),
            },
        },
        BuiltinCronDef {
            name: "log-cleanup",
            script_filename: "log-cleanup.sh",
            script_content: LOG_CLEANUP_SH,
            schedule: CronSchedule::Expression {
                cron: "0 0 * * *".to_string(),
            },
        },
    ]
}

/// 스크립트를 ~/.autodev/crons/ 에 기록하고 실행 권한을 부여한다.
fn ensure_script(home: &std::path::Path, def: &BuiltinCronDef) -> Result<String> {
    let crons_dir = config::crons_path(home);
    std::fs::create_dir_all(&crons_dir)?;

    let script_path = crons_dir.join(def.script_filename);
    std::fs::write(&script_path, def.script_content)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))?;
    }

    Ok(script_path.to_string_lossy().to_string())
}

/// 멱등하게 builtin cron job 1개를 DB에 등록한다.
/// 이미 존재하면 건너뛴다.
fn seed_one(
    db: &Database,
    home: &std::path::Path,
    def: &BuiltinCronDef,
    repo_id: Option<&str>,
) -> Result<bool> {
    // UNIQUE(name, repo_id) 제약을 활용: 삽입 시 중복이면 에러 → 무시
    let script_path = ensure_script(home, def)?;

    let job = NewCronJob {
        name: def.name.to_string(),
        repo_id: repo_id.map(|s| s.to_string()),
        schedule: def.schedule.clone(),
        script_path,
        builtin: true,
    };

    match db.cron_add(&job) {
        Ok(_) => Ok(true),
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("UNIQUE constraint failed") {
                Ok(false) // 이미 존재 — 정상
            } else {
                Err(e)
            }
        }
    }
}

/// repo_add() 후 호출: per-repo builtin cron 3개를 seed한다.
pub fn seed_per_repo_crons(
    db: &Database,
    home: &std::path::Path,
    repo_id: &str,
    claw_cfg: &config::models::ClawConfig,
) -> Result<u32> {
    let defs = per_repo_cron_defs(claw_cfg);
    let mut seeded = 0u32;
    for def in &defs {
        if seed_one(db, home, def, Some(repo_id))? {
            seeded += 1;
        }
    }
    Ok(seeded)
}

/// daemon::start() 시 호출: global builtin cron 3개를 seed한다.
pub fn seed_global_crons(db: &Database, home: &std::path::Path) -> Result<u32> {
    let defs = global_cron_defs();
    let mut seeded = 0u32;
    for def in &defs {
        if seed_one(db, home, def, None)? {
            seeded += 1;
        }
    }
    Ok(seeded)
}

// ─── Helpers ───

fn find_repo_info(db: &Database, repo_name: &str) -> Result<Option<EnabledRepo>> {
    let repos = db.repo_find_enabled()?;
    Ok(repos.into_iter().find(|r| r.name == repo_name))
}

/// Detect the default branch for a repo workspace via `git symbolic-ref`.
/// Falls back to "main" with a warning if detection fails.
pub fn detect_default_branch(workspace: &std::path::Path) -> String {
    let repo_dir = workspace.join("main");
    let output = std::process::Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD", "--short"])
        .current_dir(&repo_dir)
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let full_ref = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let branch = full_ref
                .strip_prefix("origin/")
                .unwrap_or(&full_ref)
                .to_string();
            if branch.is_empty() {
                tracing::warn!(
                    "git symbolic-ref returned empty for {}, falling back to 'main'",
                    repo_dir.display()
                );
                "main".to_string()
            } else {
                branch
            }
        }
        Ok(_) => {
            tracing::warn!(
                "could not detect default branch for {}, falling back to 'main'",
                repo_dir.display()
            );
            "main".to_string()
        }
        Err(e) => {
            tracing::warn!(
                "failed to run git symbolic-ref for {}: {e}, falling back to 'main'",
                repo_dir.display()
            );
            "main".to_string()
        }
    }
}
