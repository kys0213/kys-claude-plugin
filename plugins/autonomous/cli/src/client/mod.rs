use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::config::models::RepoConfig;
use crate::queue::Database;

/// 상태 요약
pub fn status(db: &Database) -> Result<String> {
    let conn = db.conn();
    let mut output = String::new();

    // 데몬 상태
    let home = crate::config::autonomous_home();
    let running = crate::daemon::pid::is_running(&home);
    output.push_str(&format!(
        "autonomous daemon: {}\n\n",
        if running { "● running" } else { "○ stopped" }
    ));

    // 레포 목록
    let mut stmt = conn.prepare(
        "SELECT r.name, r.enabled, \
         (SELECT COUNT(*) FROM issue_queue WHERE repo_id = r.id AND status NOT IN ('done','failed')) as issue_pending, \
         (SELECT COUNT(*) FROM pr_queue WHERE repo_id = r.id AND status NOT IN ('done','failed')) as pr_pending, \
         (SELECT COUNT(*) FROM merge_queue WHERE repo_id = r.id AND status NOT IN ('done','failed')) as merge_pending \
         FROM repositories r ORDER BY r.name",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, bool>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;

    output.push_str("Repositories:\n");
    let mut has_repos = false;
    for row in rows {
        let (name, enabled, issues, prs, merges) = row?;
        has_repos = true;
        let icon = if enabled { "●" } else { "○" };
        output.push_str(&format!(
            "  {icon} {name}  issues:{issues} prs:{prs} merges:{merges}\n"
        ));
    }
    if !has_repos {
        output.push_str("  (no repositories registered)\n");
    }

    Ok(output)
}

/// 레포 등록
pub fn repo_add(db: &Database, url: &str, config_json: Option<&str>) -> Result<()> {
    let conn = db.conn();
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();

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

    let config: RepoConfig = if let Some(json) = config_json {
        serde_json::from_str(json)?
    } else {
        RepoConfig::default()
    };

    conn.execute(
        "INSERT INTO repositories (id, url, name, enabled, created_at, updated_at) VALUES (?1, ?2, ?3, 1, ?4, ?4)",
        rusqlite::params![id, url, name, now],
    )?;

    conn.execute(
        "INSERT INTO repo_configs (repo_id, scan_interval_secs, scan_targets, issue_concurrency, pr_concurrency, merge_concurrency, model, issue_workflow, pr_workflow, filter_labels, ignore_authors, workspace_strategy) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            id,
            config.scan_interval_secs,
            serde_json::to_string(&config.scan_targets)?,
            config.issue_concurrency,
            config.pr_concurrency,
            config.merge_concurrency,
            config.model,
            config.issue_workflow,
            config.pr_workflow,
            config.filter_labels.map(|l| serde_json::to_string(&l).unwrap_or_default()),
            serde_json::to_string(&config.ignore_authors)?,
            config.workspace_strategy,
        ],
    )?;

    println!("registered: {name} ({url})");
    Ok(())
}

/// 레포 목록
pub fn repo_list(db: &Database) -> Result<String> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT r.name, r.url, r.enabled, c.scan_interval_secs, c.issue_concurrency, c.pr_concurrency, c.merge_concurrency \
         FROM repositories r JOIN repo_configs c ON r.id = c.repo_id ORDER BY r.name",
    )?;

    let mut output = String::new();
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, bool>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
        ))
    })?;

    for row in rows {
        let (name, url, enabled, interval, ic, pc, mc) = row?;
        let icon = if enabled { "●" } else { "○" };
        output.push_str(&format!(
            "{icon} {name}\n  {url}\n  scan: {interval}s | issue×{ic} pr×{pc} merge×{mc}\n\n"
        ));
    }

    if output.is_empty() {
        output.push_str("No repositories registered. Use 'autonomous repo add <url>' to add one.\n");
    }

    Ok(output)
}

/// 레포 설정 변경
pub fn repo_config(db: &Database, name: &str, update_json: Option<&str>) -> Result<()> {
    let conn = db.conn();

    if let Some(json) = update_json {
        let config: RepoConfig = serde_json::from_str(json)?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE repo_configs SET \
             scan_interval_secs = ?2, issue_concurrency = ?3, pr_concurrency = ?4, merge_concurrency = ?5, \
             model = ?6, issue_workflow = ?7, pr_workflow = ?8, workspace_strategy = ?9 \
             WHERE repo_id = (SELECT id FROM repositories WHERE name = ?1)",
            rusqlite::params![
                name,
                config.scan_interval_secs,
                config.issue_concurrency,
                config.pr_concurrency,
                config.merge_concurrency,
                config.model,
                config.issue_workflow,
                config.pr_workflow,
                config.workspace_strategy,
            ],
        )?;

        conn.execute(
            "UPDATE repositories SET updated_at = ?2 WHERE name = ?1",
            rusqlite::params![name, now],
        )?;

        println!("updated config for {name}");
    } else {
        // 현재 설정 출력
        let mut stmt = conn.prepare(
            "SELECT c.* FROM repo_configs c JOIN repositories r ON r.id = c.repo_id WHERE r.name = ?1",
        )?;

        let config = stmt.query_row(rusqlite::params![name], |row| {
            Ok(format!(
                "scan_interval: {}s\nissue_concurrency: {}\npr_concurrency: {}\nmerge_concurrency: {}\nmodel: {}\nissue_workflow: {}\npr_workflow: {}\nworkspace_strategy: {}",
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(11)?,
            ))
        })?;

        println!("{name}:\n{config}");
    }

    Ok(())
}

/// 레포 제거
pub fn repo_remove(db: &Database, name: &str) -> Result<()> {
    let conn = db.conn();
    conn.execute(
        "DELETE FROM repo_configs WHERE repo_id = (SELECT id FROM repositories WHERE name = ?1)",
        rusqlite::params![name],
    )?;
    conn.execute(
        "DELETE FROM repositories WHERE name = ?1",
        rusqlite::params![name],
    )?;
    println!("removed: {name}");
    Ok(())
}

/// 큐 목록
pub fn queue_list(db: &Database, repo: &str) -> Result<String> {
    let conn = db.conn();
    let mut output = String::new();

    // Issue queue
    output.push_str("Issue Queue:\n");
    let mut stmt = conn.prepare(
        "SELECT iq.github_number, iq.title, iq.status FROM issue_queue iq \
         JOIN repositories r ON iq.repo_id = r.id WHERE r.name = ?1 \
         ORDER BY iq.created_at DESC LIMIT 20",
    )?;
    let rows = stmt.query_map(rusqlite::params![repo], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (num, title, status) = row?;
        output.push_str(&format!("  #{num} [{status}] {title}\n"));
    }

    // PR queue
    output.push_str("\nPR Queue:\n");
    let mut stmt = conn.prepare(
        "SELECT pq.github_number, pq.title, pq.status FROM pr_queue pq \
         JOIN repositories r ON pq.repo_id = r.id WHERE r.name = ?1 \
         ORDER BY pq.created_at DESC LIMIT 20",
    )?;
    let rows = stmt.query_map(rusqlite::params![repo], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (num, title, status) = row?;
        output.push_str(&format!("  #{num} [{status}] {title}\n"));
    }

    // Merge queue
    output.push_str("\nMerge Queue:\n");
    let mut stmt = conn.prepare(
        "SELECT mq.pr_number, mq.title, mq.status FROM merge_queue mq \
         JOIN repositories r ON mq.repo_id = r.id WHERE r.name = ?1 \
         ORDER BY mq.created_at DESC LIMIT 20",
    )?;
    let rows = stmt.query_map(rusqlite::params![repo], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (num, title, status) = row?;
        output.push_str(&format!("  PR #{num} [{status}] {title}\n"));
    }

    Ok(output)
}

/// 큐 항목 재시도
pub fn queue_retry(db: &Database, id: &str) -> Result<()> {
    let conn = db.conn();
    let now = Utc::now().to_rfc3339();

    // 각 큐 테이블에서 시도
    for table in &["issue_queue", "pr_queue", "merge_queue"] {
        let affected = conn.execute(
            &format!("UPDATE {table} SET status = 'pending', error_message = NULL, worker_id = NULL, updated_at = ?2 WHERE id = ?1 AND status = 'failed'"),
            rusqlite::params![id, now],
        )?;
        if affected > 0 {
            println!("retrying: {id} in {table}");
            return Ok(());
        }
    }

    println!("not found or not in failed status: {id}");
    Ok(())
}

/// 큐 비우기
pub fn queue_clear(db: &Database, repo: &str) -> Result<()> {
    let conn = db.conn();
    for table in &["issue_queue", "pr_queue", "merge_queue"] {
        conn.execute(
            &format!(
                "DELETE FROM {table} WHERE repo_id = (SELECT id FROM repositories WHERE name = ?1) AND status IN ('done', 'failed')"
            ),
            rusqlite::params![repo],
        )?;
    }
    println!("cleared completed/failed items for {repo}");
    Ok(())
}

/// 로그 조회
pub fn logs(db: &Database, repo: Option<&str>, limit: usize) -> Result<String> {
    let conn = db.conn();
    let mut output = String::new();

    let query = if let Some(repo_name) = repo {
        format!(
            "SELECT cl.started_at, cl.queue_type, cl.command, cl.exit_code, cl.duration_ms \
             FROM consumer_logs cl JOIN repositories r ON cl.repo_id = r.id \
             WHERE r.name = '{repo_name}' ORDER BY cl.started_at DESC LIMIT {limit}"
        )
    } else {
        format!(
            "SELECT cl.started_at, cl.queue_type, cl.command, cl.exit_code, cl.duration_ms \
             FROM consumer_logs cl ORDER BY cl.started_at DESC LIMIT {limit}"
        )
    };

    let mut stmt = conn.prepare(&query)?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<i32>>(3)?,
            row.get::<_, Option<i64>>(4)?,
        ))
    })?;

    for row in rows {
        let (time, qtype, cmd, exit_code, duration) = row?;
        let status = match exit_code {
            Some(0) => "✓",
            Some(_) => "✗",
            None => "…",
        };
        let dur = duration.map(|d| format!(" ({d}ms)")).unwrap_or_default();
        output.push_str(&format!("  {time} [{qtype}] {status} {cmd}{dur}\n"));
    }

    if output.is_empty() {
        output.push_str("No logs found.\n");
    }

    Ok(output)
}
