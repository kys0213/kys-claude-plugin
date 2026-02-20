use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::queue::Database;
use crate::session;
use crate::workspace;

/// pending 머지 처리
pub async fn process_pending(db: &Database) -> Result<()> {
    let conn = db.conn();

    let items: Vec<(String, String, String, i64, String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT mq.id, mq.repo_id, r.name, mq.pr_number, mq.head_branch, mq.base_branch, r.url \
             FROM merge_queue mq JOIN repositories r ON mq.repo_id = r.id \
             WHERE mq.status = 'pending' \
             ORDER BY mq.created_at ASC LIMIT 1",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                row.get(4)?, row.get(5)?, row.get(6)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    for (item_id, repo_id, repo_name, pr_num, _head_branch, _base_branch, repo_url) in items {
        let worker_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE merge_queue SET status = 'merging', worker_id = ?2, updated_at = ?3 WHERE id = ?1",
            rusqlite::params![item_id, worker_id, now],
        )?;

        // 워크스페이스 준비
        let task_id = format!("merge-pr-{pr_num}");
        if let Err(e) = workspace::ensure_cloned(&repo_url, &repo_name).await {
            mark_failed(conn, &item_id, &format!("clone failed: {e}"))?;
            continue;
        }

        let wt_path = match workspace::create_worktree(&repo_name, &task_id, None).await {
            Ok(p) => p,
            Err(e) => {
                mark_failed(conn, &item_id, &format!("worktree failed: {e}"))?;
                continue;
            }
        };

        // 머지 실행
        let prompt = format!("/merge-pr {pr_num}");
        let log_id = Uuid::new_v4().to_string();
        let started = Utc::now().to_rfc3339();

        let result = session::run_claude(&wt_path, &prompt, None).await;

        match result {
            Ok(res) => {
                let finished = Utc::now().to_rfc3339();
                let duration = chrono::Utc::now()
                    .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&started).unwrap())
                    .num_milliseconds();

                conn.execute(
                    "INSERT INTO consumer_logs (id, repo_id, queue_type, queue_item_id, worker_id, command, stdout, stderr, exit_code, started_at, finished_at, duration_ms) \
                     VALUES (?1, ?2, 'merge', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    rusqlite::params![
                        log_id, repo_id, item_id, worker_id,
                        format!("claude -p \"/merge-pr {pr_num}\""),
                        res.stdout, res.stderr, res.exit_code,
                        started, finished, duration
                    ],
                )?;

                if res.exit_code == 0 {
                    let now = Utc::now().to_rfc3339();
                    conn.execute(
                        "UPDATE merge_queue SET status = 'done', updated_at = ?2 WHERE id = ?1",
                        rusqlite::params![item_id, now],
                    )?;
                    tracing::info!("PR #{pr_num} merged successfully");

                    // worktree 정리
                    let _ = workspace::remove_worktree(&repo_name, &task_id).await;
                } else if res.stdout.contains("conflict") || res.stderr.contains("conflict") {
                    // 충돌 발생 - 해결 시도
                    let now = Utc::now().to_rfc3339();
                    conn.execute(
                        "UPDATE merge_queue SET status = 'conflict', updated_at = ?2 WHERE id = ?1",
                        rusqlite::params![item_id, now],
                    )?;

                    let resolve_result = session::run_claude(
                        &wt_path,
                        &format!("Resolve merge conflicts for PR #{pr_num}"),
                        None,
                    )
                    .await;

                    match resolve_result {
                        Ok(rr) if rr.exit_code == 0 => {
                            let now = Utc::now().to_rfc3339();
                            conn.execute(
                                "UPDATE merge_queue SET status = 'done', updated_at = ?2 WHERE id = ?1",
                                rusqlite::params![item_id, now],
                            )?;
                            tracing::info!("PR #{pr_num} conflicts resolved and merged");
                        }
                        _ => {
                            mark_failed(conn, &item_id, "conflict resolution failed")?;
                        }
                    }
                } else {
                    mark_failed(conn, &item_id, &format!("merge exited with {}", res.exit_code))?;
                }
            }
            Err(e) => {
                mark_failed(conn, &item_id, &format!("merge error: {e}"))?;
            }
        }
    }

    Ok(())
}

fn mark_failed(conn: &rusqlite::Connection, item_id: &str, error: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE merge_queue SET status = 'failed', error_message = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![item_id, error, now],
    )?;
    tracing::error!("merge {item_id} failed: {error}");
    Ok(())
}
