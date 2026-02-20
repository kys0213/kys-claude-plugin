use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::queue::Database;
use crate::session;
use crate::workspace;

/// pending 이슈 처리
pub async fn process_pending(db: &Database) -> Result<()> {
    let conn = db.conn();

    // concurrency 설정에 따라 처리할 이슈 수 결정
    let items: Vec<(String, String, String, i64, String, Option<String>, String)> = {
        let mut stmt = conn.prepare(
            "SELECT iq.id, iq.repo_id, r.name, iq.github_number, iq.title, iq.body, r.url \
             FROM issue_queue iq JOIN repositories r ON iq.repo_id = r.id \
             JOIN repo_configs c ON r.id = c.repo_id \
             WHERE iq.status = 'pending' \
             ORDER BY iq.created_at ASC \
             LIMIT (SELECT c.issue_concurrency FROM repo_configs c JOIN repositories r2 ON r2.id = c.repo_id LIMIT 1)",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    for (item_id, _repo_id, repo_name, issue_num, title, body, repo_url) in items {
        let worker_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        // status → analyzing
        conn.execute(
            "UPDATE issue_queue SET status = 'analyzing', worker_id = ?2, updated_at = ?3 WHERE id = ?1",
            rusqlite::params![item_id, worker_id, now],
        )?;

        // 워크스페이스 준비
        let task_id = format!("issue-{issue_num}");
        match workspace::ensure_cloned(&repo_url, &repo_name).await {
            Ok(_) => {}
            Err(e) => {
                mark_failed(conn, &item_id, &format!("clone failed: {e}"))?;
                continue;
            }
        }

        let wt_path = match workspace::create_worktree(&repo_name, &task_id, None).await {
            Ok(p) => p,
            Err(e) => {
                mark_failed(conn, &item_id, &format!("worktree failed: {e}"))?;
                continue;
            }
        };

        // 1단계: Multi-LLM 분석
        let body_text = body.as_deref().unwrap_or("");
        let prompt = format!(
            "Analyze issue #{issue_num}: {title}\n\n{body_text}\n\n\
             Provide a structured analysis report with: summary, affected files, \
             implementation direction, checkpoints, and risks."
        );

        let log_id = Uuid::new_v4().to_string();
        let started = Utc::now().to_rfc3339();

        let result = session::run_claude(&wt_path, &prompt, Some("json")).await;

        match result {
            Ok(res) => {
                let finished = Utc::now().to_rfc3339();
                let duration = chrono::Utc::now()
                    .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
                    .num_milliseconds();

                // 로그 기록
                conn.execute(
                    "INSERT INTO consumer_logs (id, repo_id, queue_type, queue_item_id, worker_id, command, stdout, stderr, exit_code, started_at, finished_at, duration_ms) \
                     VALUES (?1, ?2, 'issue', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    rusqlite::params![
                        log_id, _repo_id, item_id, worker_id,
                        format!("claude -p \"Analyze issue #{issue_num}...\""),
                        res.stdout, res.stderr, res.exit_code,
                        started, finished, duration
                    ],
                )?;

                if res.exit_code == 0 {
                    let report = session::output::parse_output(&res.stdout);
                    let now = Utc::now().to_rfc3339();
                    conn.execute(
                        "UPDATE issue_queue SET status = 'ready', analysis_report = ?2, updated_at = ?3 WHERE id = ?1",
                        rusqlite::params![item_id, report, now],
                    )?;
                    tracing::info!("issue #{issue_num} analysis complete");

                    // 2단계: 구현
                    process_ready_issue(conn, db, &item_id, &repo_name, &_repo_id, &worker_id, issue_num, &report, &wt_path).await?;
                } else {
                    mark_failed(conn, &item_id, &format!("claude exited with {}", res.exit_code))?;
                }
            }
            Err(e) => {
                mark_failed(conn, &item_id, &format!("session error: {e}"))?;
            }
        }
    }

    Ok(())
}

async fn process_ready_issue(
    conn: &rusqlite::Connection,
    _db: &Database,
    item_id: &str,
    repo_name: &str,
    repo_id: &str,
    worker_id: &str,
    issue_num: i64,
    report: &str,
    wt_path: &std::path::Path,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE issue_queue SET status = 'processing', updated_at = ?2 WHERE id = ?1",
        rusqlite::params![item_id, now],
    )?;

    let prompt = format!(
        "/develop implement based on analysis:\n\n{report}\n\nThis is for issue #{issue_num} in {repo_name}."
    );

    let log_id = Uuid::new_v4().to_string();
    let started = Utc::now().to_rfc3339();

    let result = session::run_claude(wt_path, &prompt, None).await;

    match result {
        Ok(res) => {
            let finished = Utc::now().to_rfc3339();
            let duration = chrono::Utc::now()
                .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
                .num_milliseconds();

            conn.execute(
                "INSERT INTO consumer_logs (id, repo_id, queue_type, queue_item_id, worker_id, command, stdout, stderr, exit_code, started_at, finished_at, duration_ms) \
                 VALUES (?1, ?2, 'issue', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                rusqlite::params![
                    log_id, repo_id, item_id, worker_id,
                    format!("claude -p \"/develop implement issue #{issue_num}\""),
                    res.stdout, res.stderr, res.exit_code,
                    started, finished, duration
                ],
            )?;

            if res.exit_code == 0 {
                let now = Utc::now().to_rfc3339();
                conn.execute(
                    "UPDATE issue_queue SET status = 'done', updated_at = ?2 WHERE id = ?1",
                    rusqlite::params![item_id, now],
                )?;
                tracing::info!("issue #{issue_num} implementation complete");
            } else {
                mark_failed(conn, item_id, &format!("implementation exited with {}", res.exit_code))?;
            }
        }
        Err(e) => {
            mark_failed(conn, item_id, &format!("implementation error: {e}"))?;
        }
    }

    Ok(())
}

fn mark_failed(conn: &rusqlite::Connection, item_id: &str, error: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE issue_queue SET status = 'failed', error_message = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![item_id, error, now],
    )?;
    tracing::error!("issue {item_id} failed: {error}");
    Ok(())
}
