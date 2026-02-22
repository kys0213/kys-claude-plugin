use anyhow::Result;

use crate::config;
use crate::config::Env;
use crate::queue::repository::*;
use crate::queue::Database;

/// 상태 요약
pub fn status(db: &Database, env: &dyn Env) -> Result<String> {
    let mut output = String::new();

    // 데몬 상태
    let home = config::autodev_home(env);
    let running = crate::daemon::pid::is_running(&home);
    output.push_str(&format!(
        "autodev daemon: {}\n\n",
        if running {
            "● running"
        } else {
            "○ stopped"
        }
    ));

    // 레포 목록
    let rows = db.repo_status_summary()?;

    output.push_str("Repositories:\n");
    if rows.is_empty() {
        output.push_str("  (no repositories registered)\n");
    } else {
        for row in &rows {
            let icon = if row.enabled { "●" } else { "○" };
            output.push_str(&format!("  {icon} {}\n", row.name));
        }
    }

    Ok(output)
}

/// URL에서 org/repo 이름 추출
fn extract_repo_name(url: &str) -> Result<String> {
    let trimmed = url.trim_end_matches('/').trim_end_matches(".git");
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() < 2 {
        anyhow::bail!("invalid repository URL: {url} (expected https://github.com/org/repo)");
    }
    let org = parts[parts.len() - 2];
    let repo = parts[parts.len() - 1];
    if org.is_empty() || repo.is_empty() {
        anyhow::bail!("invalid repository URL: {url} (org or repo name is empty)");
    }
    Ok(format!("{org}/{repo}"))
}

/// 레포 등록
pub fn repo_add(db: &Database, url: &str) -> Result<()> {
    // URL에서 이름 추출 (예: https://github.com/org/repo -> org/repo)
    let name = extract_repo_name(url)?;

    db.repo_add(url, &name)?;

    println!("registered: {name} ({url})");
    println!("config: edit ~/.develop-workflow.yaml (global) or <repo>/.develop-workflow.yaml (per-repo)");
    Ok(())
}

/// 레포 목록
pub fn repo_list(db: &Database) -> Result<String> {
    let repos = db.repo_list()?;
    let mut output = String::new();

    for r in &repos {
        let icon = if r.enabled { "●" } else { "○" };
        output.push_str(&format!("{icon} {}\n  {}\n\n", r.name, r.url));
    }

    if output.is_empty() {
        output.push_str("No repositories registered. Use 'autodev repo add <url>' to add one.\n");
    }

    Ok(output)
}

/// 레포 설정 표시 (YAML 기반)
pub fn repo_config(env: &dyn Env, name: &str) -> Result<()> {
    // 글로벌 설정
    let global_path = config::loader::global_config_path(env);
    println!("Global config: {}", global_path.display());

    if global_path.exists() {
        println!("  (exists)");
    } else {
        println!("  (not found — using defaults)");
    }

    // 워크스페이스에서 레포별 설정 탐색
    let ws = config::workspaces_path(env).join(name);
    let repo_config_path = ws.join(".develop-workflow.yaml");
    println!("\nRepo config: {}", repo_config_path.display());

    if repo_config_path.exists() {
        println!("  (exists)");
    } else {
        println!("  (not found — using global/defaults)");
    }

    // 최종 머지 결과 표시
    let merged = if ws.exists() {
        config::loader::load_merged(env, Some(&ws))
    } else {
        config::loader::load_merged(env, None)
    };

    let yaml = serde_yaml::to_string(&merged)?;
    println!("\nEffective config for {name}:\n---\n{yaml}");

    Ok(())
}

/// 레포 제거
pub fn repo_remove(db: &Database, name: &str) -> Result<()> {
    db.repo_remove(name)?;
    println!("removed: {name}");
    Ok(())
}

/// 로그 조회
pub fn logs(db: &Database, repo: Option<&str>, limit: usize) -> Result<String> {
    let entries = db.log_recent(repo, limit)?;
    let mut output = String::new();

    for entry in &entries {
        let status = match entry.exit_code {
            Some(0) => "✓",
            Some(_) => "✗",
            None => "…",
        };
        let dur = entry
            .duration_ms
            .map(|d| format!(" ({d}ms)"))
            .unwrap_or_default();
        output.push_str(&format!(
            "  {} [{}] {} {}{}\n",
            entry.started_at, entry.queue_type, status, entry.command, dur
        ));
    }

    if output.is_empty() {
        output.push_str("No logs found.\n");
    }

    Ok(output)
}
