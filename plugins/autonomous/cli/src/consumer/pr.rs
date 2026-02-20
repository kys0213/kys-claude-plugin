use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::queue::Database;
use crate::session;
use crate::workspace;

/// pending PR 처리
pub async fn process_pending(db: &Database) -> Result<()> {
    let conn = db.conn();

    let items: Vec<(String, String, String, i64, String, String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT pq.id, pq.repo_id, r.name, pq.github_number, pq.title, pq.head_branch, pq.base_branch, r.url \
             FROM pr_queue pq JOIN repositories r ON pq.repo_id = r.id \
             WHERE pq.status = 'pending' \
             ORDER BY pq.created_at ASC LIMIT 5",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    for (item_id, repo_id, repo_name, pr_num, _title, head_branch, _base_branch, repo_url) in items {
        let worker_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE pr_queue SET status = 'reviewing', worker_id = ?2, updated_at = ?3 WHERE id = ?1",
            rusqlite::params![item_id, worker_id, now],
        )?;

        // 워크스페이스 준비
        let task_id = format!("pr-{pr_num}");
        if let Err(e) = workspace::ensure_cloned(&repo_url, &repo_name).await {
            mark_failed(conn, &item_id, &format!("clone failed: {e}"))?;
            continue;
        }

        let wt_path = match workspace::create_worktree(&repo_name, &task_id, Some(&head_branch)).await {
            Ok(p) => p,
            Err(e) => {
                mark_failed(conn, &item_id, &format!("worktree failed: {e}"))?;
                continue;
            }
        };

        // 1단계: Multi-LLM 리뷰
        let log_id = Uuid::new_v4().to_string();
        let started = Utc::now().to_rfc3339();

        let result = session::run_claude(&wt_path, "/multi-review", Some("json")).await;

        match result {
            Ok(res) => {
                let finished = Utc::now().to_rfc3339();
                let duration = chrono::Utc::now()
                    .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
                    .num_milliseconds();

                conn.execute(
                    "INSERT INTO consumer_logs (id, repo_id, queue_type, queue_item_id, worker_id, command, stdout, stderr, exit_code, started_at, finished_at, duration_ms) \
                     VALUES (?1, ?2, 'pr', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    rusqlite::params![
                        log_id, repo_id, item_id, worker_id,
                        format!("claude -p \"/multi-review\" (PR #{pr_num})"),
                        res.stdout, res.stderr, res.exit_code,
                        started, finished, duration
                    ],
                )?;

                if res.exit_code == 0 {
                    let review = session::output::parse_output(&res.stdout);
                    let now = Utc::now().to_rfc3339();
                    conn.execute(
                        "UPDATE pr_queue SET status = 'review_done', review_comment = ?2, updated_at = ?3 WHERE id = ?1",
                        rusqlite::params![item_id, review, now],
                    )?;
                    tracing::info!("PR #{pr_num} review complete");
                } else {
                    mark_failed(conn, &item_id, &format!("review exited with {}", res.exit_code))?;
                }
            }
            Err(e) => {
                mark_failed(conn, &item_id, &format!("review error: {e}"))?;
            }
        }
    }

    Ok(())
}

fn mark_failed(conn: &rusqlite::Connection, item_id: &str, error: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE pr_queue SET status = 'failed', error_message = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![item_id, error, now],
    )?;
    tracing::error!("PR {item_id} failed: {error}");
    Ok(())
}
