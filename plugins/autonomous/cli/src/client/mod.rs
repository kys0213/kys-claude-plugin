use anyhow::Result;

use crate::config;
use crate::queue::repository::*;
use crate::queue::Database;

/// 상태 요약
pub fn status(db: &Database) -> Result<String> {
    let mut output = String::new();

    // 데몬 상태
    let home = config::autodev_home();
    let running = crate::daemon::pid::is_running(&home);
    output.push_str(&format!(
        "autodev daemon: {}\n\n",
        if running { "● running" } else { "○ stopped" }
    ));

    // 레포 목록
    let rows = db.repo_status_summary()?;

    output.push_str("Repositories:\n");
    if rows.is_empty() {
        output.push_str("  (no repositories registered)\n");
    } else {
        for row in &rows {
            let icon = if row.enabled { "●" } else { "○" };
            output.push_str(&format!(
                "  {icon} {}  issues:{} prs:{} merges:{}\n",
                row.name, row.issue_pending, row.pr_pending, row.merge_pending
            ));
        }
    }

    Ok(output)
}

/// 레포 등록
pub fn repo_add(db: &Database, url: &str) -> Result<()> {
    // URL에서 이름 추출 (예: https://github.com/org/repo -> org/repo)
    let name = url
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit('/')
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("/");

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
pub fn repo_config(name: &str) -> Result<()> {
    // 글로벌 설정
    let global_path = config::loader::global_config_path();
    println!("Global config: {}", global_path.display());

    if global_path.exists() {
        println!("  (exists)");
    } else {
        println!("  (not found — using defaults)");
    }

    // 워크스페이스에서 레포별 설정 탐색
    let ws = config::workspaces_path().join(name);
    let repo_config_path = ws.join(".develop-workflow.yaml");
    println!("\nRepo config: {}", repo_config_path.display());

    if repo_config_path.exists() {
        println!("  (exists)");
    } else {
        println!("  (not found — using global/defaults)");
    }

    // 최종 머지 결과 표시
    let merged = if ws.exists() {
        config::loader::load_merged(Some(&ws))
    } else {
        config::loader::load_merged(None)
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

/// 큐 목록
pub fn queue_list(db: &Database, repo: &str) -> Result<String> {
    let mut output = String::new();

    // Issue queue
    output.push_str("Issue Queue:\n");
    let issues = db.issue_list(repo, 20)?;
    for item in &issues {
        output.push_str(&format!("  #{} [{}] {}\n", item.github_number, item.status, item.title));
    }

    // PR queue
    output.push_str("\nPR Queue:\n");
    let prs = db.pr_list(repo, 20)?;
    for item in &prs {
        output.push_str(&format!("  #{} [{}] {}\n", item.github_number, item.status, item.title));
    }

    // Merge queue
    output.push_str("\nMerge Queue:\n");
    let merges = db.merge_list(repo, 20)?;
    for item in &merges {
        output.push_str(&format!("  PR #{} [{}] {}\n", item.github_number, item.status, item.title));
    }

    Ok(output)
}

/// 큐 항목 재시도
pub fn queue_retry(db: &Database, id: &str) -> Result<()> {
    let found = db.queue_retry(id)?;
    if found {
        println!("retrying: {id}");
    } else {
        println!("not found or not in failed status: {id}");
    }
    Ok(())
}

/// 큐 비우기
pub fn queue_clear(db: &Database, repo: &str) -> Result<()> {
    db.queue_clear(repo)?;
    println!("cleared completed/failed items for {repo}");
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
        let dur = entry.duration_ms.map(|d| format!(" ({d}ms)")).unwrap_or_default();
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
