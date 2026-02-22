use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use super::models::*;
use super::Database;

// ─── Repository traits ───

pub trait RepoRepository {
    fn repo_add(&self, url: &str, name: &str) -> Result<String>;
    fn repo_remove(&self, name: &str) -> Result<()>;
    fn repo_list(&self) -> Result<Vec<RepoInfo>>;
    fn repo_find_enabled(&self) -> Result<Vec<EnabledRepo>>;
    fn repo_count(&self) -> Result<i64>;
    fn repo_status_summary(&self) -> Result<Vec<RepoStatusRow>>;
}

pub trait IssueQueueRepository {
    fn issue_insert(&self, item: &NewIssueItem) -> Result<String>;
    fn issue_exists(&self, repo_id: &str, github_number: i64) -> Result<bool>;
    fn issue_find_pending(&self, limit: u32) -> Result<Vec<PendingIssue>>;
    fn issue_find_ready(&self, limit: u32) -> Result<Vec<ReadyIssue>>;
    fn issue_update_status(&self, id: &str, status: &str, fields: &StatusFields) -> Result<()>;
    fn issue_mark_failed(&self, id: &str, error: &str) -> Result<()>;
    fn issue_count_active(&self) -> Result<i64>;
    fn issue_list(&self, repo_name: &str, limit: u32) -> Result<Vec<QueueListItem>>;
}

pub trait PrQueueRepository {
    fn pr_insert(&self, item: &NewPrItem) -> Result<String>;
    fn pr_exists(&self, repo_id: &str, github_number: i64) -> Result<bool>;
    fn pr_find_pending(&self, limit: u32) -> Result<Vec<PendingPr>>;
    fn pr_update_status(&self, id: &str, status: &str, fields: &StatusFields) -> Result<()>;
    fn pr_mark_failed(&self, id: &str, error: &str) -> Result<()>;
    fn pr_count_active(&self) -> Result<i64>;
    fn pr_list(&self, repo_name: &str, limit: u32) -> Result<Vec<QueueListItem>>;
}

pub trait MergeQueueRepository {
    fn merge_insert(&self, item: &NewMergeItem) -> Result<String>;
    fn merge_exists(&self, repo_id: &str, pr_number: i64) -> Result<bool>;
    fn merge_find_pending(&self, limit: u32) -> Result<Vec<PendingMerge>>;
    fn merge_update_status(&self, id: &str, status: &str, fields: &StatusFields) -> Result<()>;
    fn merge_mark_failed(&self, id: &str, error: &str) -> Result<()>;
    fn merge_count_active(&self) -> Result<i64>;
    fn merge_list(&self, repo_name: &str, limit: u32) -> Result<Vec<QueueListItem>>;
}

pub trait ScanCursorRepository {
    fn cursor_get_last_seen(&self, repo_id: &str, target: &str) -> Result<Option<String>>;
    fn cursor_upsert(&self, repo_id: &str, target: &str, last_seen: &str) -> Result<()>;
    fn cursor_should_scan(&self, repo_id: &str, interval_secs: i64) -> Result<bool>;
}

pub trait ConsumerLogRepository {
    fn log_insert(&self, log: &NewConsumerLog) -> Result<()>;
    fn log_recent(&self, repo_name: Option<&str>, limit: usize) -> Result<Vec<LogEntry>>;
}

pub trait QueueAdmin {
    fn queue_retry(&self, id: &str) -> Result<bool>;
    fn queue_clear(&self, repo_name: &str) -> Result<()>;
    fn queue_reset_stuck(&self, threshold_secs: i64) -> Result<u64>;
    fn queue_auto_retry_failed(&self, max_retries: i64) -> Result<u64>;
}

// ─── SQLite implementations ───

impl RepoRepository for Database {
    fn repo_add(&self, url: &str, name: &str) -> Result<String> {
        let conn = self.conn();
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        conn.execute(
            "INSERT INTO repositories (id, url, name, enabled, created_at, updated_at) VALUES (?1, ?2, ?3, 1, ?4, ?4)",
            rusqlite::params![id, url, name, now],
        )?;

        Ok(id)
    }

    fn repo_remove(&self, name: &str) -> Result<()> {
        let conn = self.conn();
        let repo_id_query = "(SELECT id FROM repositories WHERE name = ?1)";

        // 관련 큐 아이템 삭제
        conn.execute(
            &format!("DELETE FROM issue_queue WHERE repo_id = {repo_id_query}"),
            rusqlite::params![name],
        )?;
        conn.execute(
            &format!("DELETE FROM pr_queue WHERE repo_id = {repo_id_query}"),
            rusqlite::params![name],
        )?;
        conn.execute(
            &format!("DELETE FROM merge_queue WHERE repo_id = {repo_id_query}"),
            rusqlite::params![name],
        )?;

        // 스캔 커서 삭제
        conn.execute(
            &format!("DELETE FROM scan_cursors WHERE repo_id = {repo_id_query}"),
            rusqlite::params![name],
        )?;

        // Consumer 로그 삭제
        conn.execute(
            &format!("DELETE FROM consumer_logs WHERE repo_id = {repo_id_query}"),
            rusqlite::params![name],
        )?;

        // 레포 삭제
        conn.execute(
            "DELETE FROM repositories WHERE name = ?1",
            rusqlite::params![name],
        )?;

        Ok(())
    }

    fn repo_list(&self) -> Result<Vec<RepoInfo>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT name, url, enabled FROM repositories ORDER BY name")?;

        let rows = stmt.query_map([], |row| {
            Ok(RepoInfo {
                name: row.get(0)?,
                url: row.get(1)?,
                enabled: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn repo_find_enabled(&self) -> Result<Vec<EnabledRepo>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT id, url, name FROM repositories WHERE enabled = 1")?;

        let rows = stmt.query_map([], |row| {
            Ok(EnabledRepo {
                id: row.get(0)?,
                url: row.get(1)?,
                name: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn repo_count(&self) -> Result<i64> {
        let count = self
            .conn()
            .query_row("SELECT COUNT(*) FROM repositories", [], |row| row.get(0))?;
        Ok(count)
    }

    fn repo_status_summary(&self) -> Result<Vec<RepoStatusRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT r.name, r.enabled, \
             (SELECT COUNT(*) FROM issue_queue WHERE repo_id = r.id AND status NOT IN ('done','failed')), \
             (SELECT COUNT(*) FROM pr_queue WHERE repo_id = r.id AND status NOT IN ('done','failed')), \
             (SELECT COUNT(*) FROM merge_queue WHERE repo_id = r.id AND status NOT IN ('done','failed')) \
             FROM repositories r ORDER BY r.name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(RepoStatusRow {
                name: row.get(0)?,
                enabled: row.get(1)?,
                issue_pending: row.get(2)?,
                pr_pending: row.get(3)?,
                merge_pending: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

impl IssueQueueRepository for Database {
    fn issue_insert(&self, item: &NewIssueItem) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "INSERT INTO issue_queue (id, repo_id, github_number, title, body, labels, author, status, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8, ?8) \
             ON CONFLICT(repo_id, github_number) DO UPDATE SET \
             title = excluded.title, body = excluded.body, labels = excluded.labels, \
             author = excluded.author, status = 'pending', error_message = NULL, \
             worker_id = NULL, retry_count = 0, updated_at = excluded.updated_at \
             WHERE status = 'done'",
            rusqlite::params![id, item.repo_id, item.github_number, item.title, item.body, item.labels, item.author, now],
        )?;
        Ok(id)
    }

    fn issue_exists(&self, repo_id: &str, github_number: i64) -> Result<bool> {
        let exists: bool = self.conn().query_row(
            "SELECT COUNT(*) > 0 FROM issue_queue WHERE repo_id = ?1 AND github_number = ?2 \
             AND status != 'done'",
            rusqlite::params![repo_id, github_number],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    fn issue_find_pending(&self, limit: u32) -> Result<Vec<PendingIssue>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT iq.id, iq.repo_id, r.name, iq.github_number, iq.title, iq.body, r.url \
             FROM issue_queue iq JOIN repositories r ON iq.repo_id = r.id \
             WHERE iq.status = 'pending' \
             ORDER BY iq.created_at ASC LIMIT ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![limit], |row| {
            Ok(PendingIssue {
                id: row.get(0)?,
                repo_id: row.get(1)?,
                repo_name: row.get(2)?,
                github_number: row.get(3)?,
                title: row.get(4)?,
                body: row.get(5)?,
                repo_url: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn issue_find_ready(&self, limit: u32) -> Result<Vec<ReadyIssue>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT iq.id, iq.repo_id, r.name, iq.github_number, iq.title, iq.analysis_report, r.url \
             FROM issue_queue iq JOIN repositories r ON iq.repo_id = r.id \
             WHERE iq.status = 'ready' \
             ORDER BY iq.created_at ASC LIMIT ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![limit], |row| {
            Ok(ReadyIssue {
                id: row.get(0)?,
                repo_id: row.get(1)?,
                repo_name: row.get(2)?,
                github_number: row.get(3)?,
                title: row.get(4)?,
                analysis_report: row.get(5)?,
                repo_url: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn issue_update_status(&self, id: &str, status: &str, fields: &StatusFields) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "UPDATE issue_queue SET status = ?2, \
             worker_id = COALESCE(?3, worker_id), \
             analysis_report = COALESCE(?4, analysis_report), \
             error_message = COALESCE(?5, error_message), \
             updated_at = ?6 \
             WHERE id = ?1",
            rusqlite::params![
                id,
                status,
                fields.worker_id.as_ref(),
                fields.analysis_report.as_ref(),
                fields.error_message.as_ref(),
                now
            ],
        )?;
        Ok(())
    }

    fn issue_mark_failed(&self, id: &str, error: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "UPDATE issue_queue SET status = 'failed', error_message = ?2, \
             retry_count = retry_count + 1, updated_at = ?3 WHERE id = ?1",
            rusqlite::params![id, error, now],
        )?;
        tracing::error!("issue {id} failed: {error}");
        Ok(())
    }

    fn issue_count_active(&self) -> Result<i64> {
        let count = self.conn().query_row(
            "SELECT COUNT(*) FROM issue_queue WHERE status NOT IN ('done', 'failed')",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    fn issue_list(&self, repo_name: &str, limit: u32) -> Result<Vec<QueueListItem>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT iq.github_number, iq.title, iq.status FROM issue_queue iq \
             JOIN repositories r ON iq.repo_id = r.id WHERE r.name = ?1 \
             ORDER BY iq.created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![repo_name, limit], |row| {
            Ok(QueueListItem {
                github_number: row.get(0)?,
                title: row.get(1)?,
                status: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

impl PrQueueRepository for Database {
    fn pr_insert(&self, item: &NewPrItem) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "INSERT INTO pr_queue (id, repo_id, github_number, title, body, author, head_branch, base_branch, status, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', ?9, ?9) \
             ON CONFLICT(repo_id, github_number) DO UPDATE SET \
             title = excluded.title, body = excluded.body, author = excluded.author, \
             head_branch = excluded.head_branch, base_branch = excluded.base_branch, \
             status = 'pending', error_message = NULL, worker_id = NULL, \
             retry_count = 0, updated_at = excluded.updated_at \
             WHERE status = 'done'",
            rusqlite::params![id, item.repo_id, item.github_number, item.title, item.body, item.author, item.head_branch, item.base_branch, now],
        )?;
        Ok(id)
    }

    fn pr_exists(&self, repo_id: &str, github_number: i64) -> Result<bool> {
        let exists: bool = self.conn().query_row(
            "SELECT COUNT(*) > 0 FROM pr_queue WHERE repo_id = ?1 AND github_number = ?2 \
             AND status != 'done'",
            rusqlite::params![repo_id, github_number],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    fn pr_find_pending(&self, limit: u32) -> Result<Vec<PendingPr>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT pq.id, pq.repo_id, r.name, pq.github_number, pq.title, pq.head_branch, pq.base_branch, r.url \
             FROM pr_queue pq JOIN repositories r ON pq.repo_id = r.id \
             WHERE pq.status = 'pending' \
             ORDER BY pq.created_at ASC LIMIT ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![limit], |row| {
            Ok(PendingPr {
                id: row.get(0)?,
                repo_id: row.get(1)?,
                repo_name: row.get(2)?,
                github_number: row.get(3)?,
                title: row.get(4)?,
                head_branch: row.get(5)?,
                base_branch: row.get(6)?,
                repo_url: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn pr_update_status(&self, id: &str, status: &str, fields: &StatusFields) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "UPDATE pr_queue SET status = ?2, \
             worker_id = COALESCE(?3, worker_id), \
             review_comment = COALESCE(?4, review_comment), \
             error_message = COALESCE(?5, error_message), \
             updated_at = ?6 \
             WHERE id = ?1",
            rusqlite::params![
                id,
                status,
                fields.worker_id.as_ref(),
                fields.review_comment.as_ref(),
                fields.error_message.as_ref(),
                now
            ],
        )?;
        Ok(())
    }

    fn pr_mark_failed(&self, id: &str, error: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "UPDATE pr_queue SET status = 'failed', error_message = ?2, \
             retry_count = retry_count + 1, updated_at = ?3 WHERE id = ?1",
            rusqlite::params![id, error, now],
        )?;
        tracing::error!("PR {id} failed: {error}");
        Ok(())
    }

    fn pr_count_active(&self) -> Result<i64> {
        let count = self.conn().query_row(
            "SELECT COUNT(*) FROM pr_queue WHERE status NOT IN ('done', 'failed')",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    fn pr_list(&self, repo_name: &str, limit: u32) -> Result<Vec<QueueListItem>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT pq.github_number, pq.title, pq.status FROM pr_queue pq \
             JOIN repositories r ON pq.repo_id = r.id WHERE r.name = ?1 \
             ORDER BY pq.created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![repo_name, limit], |row| {
            Ok(QueueListItem {
                github_number: row.get(0)?,
                title: row.get(1)?,
                status: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

impl MergeQueueRepository for Database {
    fn merge_insert(&self, item: &NewMergeItem) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "INSERT INTO merge_queue (id, repo_id, pr_number, title, head_branch, base_branch, status, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, ?7) \
             ON CONFLICT(repo_id, pr_number) DO UPDATE SET \
             title = excluded.title, head_branch = excluded.head_branch, \
             base_branch = excluded.base_branch, status = 'pending', \
             error_message = NULL, worker_id = NULL, retry_count = 0, \
             updated_at = excluded.updated_at \
             WHERE status = 'done'",
            rusqlite::params![id, item.repo_id, item.pr_number, item.title, item.head_branch, item.base_branch, now],
        )?;
        Ok(id)
    }

    fn merge_exists(&self, repo_id: &str, pr_number: i64) -> Result<bool> {
        let exists: bool = self.conn().query_row(
            "SELECT COUNT(*) > 0 FROM merge_queue WHERE repo_id = ?1 AND pr_number = ?2 \
             AND status != 'done'",
            rusqlite::params![repo_id, pr_number],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    fn merge_find_pending(&self, limit: u32) -> Result<Vec<PendingMerge>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT mq.id, mq.repo_id, r.name, mq.pr_number, mq.head_branch, mq.base_branch, r.url \
             FROM merge_queue mq JOIN repositories r ON mq.repo_id = r.id \
             WHERE mq.status = 'pending' \
             ORDER BY mq.created_at ASC LIMIT ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![limit], |row| {
            Ok(PendingMerge {
                id: row.get(0)?,
                repo_id: row.get(1)?,
                repo_name: row.get(2)?,
                pr_number: row.get(3)?,
                head_branch: row.get(4)?,
                base_branch: row.get(5)?,
                repo_url: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn merge_update_status(&self, id: &str, status: &str, fields: &StatusFields) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "UPDATE merge_queue SET status = ?2, \
             worker_id = COALESCE(?3, worker_id), \
             error_message = COALESCE(?4, error_message), \
             updated_at = ?5 \
             WHERE id = ?1",
            rusqlite::params![
                id,
                status,
                fields.worker_id.as_ref(),
                fields.error_message.as_ref(),
                now
            ],
        )?;
        Ok(())
    }

    fn merge_mark_failed(&self, id: &str, error: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "UPDATE merge_queue SET status = 'failed', error_message = ?2, \
             retry_count = retry_count + 1, updated_at = ?3 WHERE id = ?1",
            rusqlite::params![id, error, now],
        )?;
        tracing::error!("merge {id} failed: {error}");
        Ok(())
    }

    fn merge_count_active(&self) -> Result<i64> {
        let count = self.conn().query_row(
            "SELECT COUNT(*) FROM merge_queue WHERE status NOT IN ('done', 'failed')",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    fn merge_list(&self, repo_name: &str, limit: u32) -> Result<Vec<QueueListItem>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT mq.pr_number, mq.title, mq.status FROM merge_queue mq \
             JOIN repositories r ON mq.repo_id = r.id WHERE r.name = ?1 \
             ORDER BY mq.created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![repo_name, limit], |row| {
            Ok(QueueListItem {
                github_number: row.get(0)?,
                title: row.get(1)?,
                status: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

impl ScanCursorRepository for Database {
    fn cursor_get_last_seen(&self, repo_id: &str, target: &str) -> Result<Option<String>> {
        let result = self.conn().query_row(
            "SELECT last_seen FROM scan_cursors WHERE repo_id = ?1 AND target = ?2",
            rusqlite::params![repo_id, target],
            |row| row.get(0),
        );
        Ok(result.ok())
    }

    fn cursor_upsert(&self, repo_id: &str, target: &str, last_seen: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "INSERT OR REPLACE INTO scan_cursors (repo_id, target, last_seen, last_scan) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![repo_id, target, last_seen, now],
        )?;
        Ok(())
    }

    fn cursor_should_scan(&self, repo_id: &str, interval_secs: i64) -> Result<bool> {
        let last_scan: Option<String> = self
            .conn()
            .query_row(
                "SELECT MAX(last_scan) FROM scan_cursors WHERE repo_id = ?1",
                rusqlite::params![repo_id],
                |row| row.get(0),
            )
            .ok()
            .flatten();

        if let Some(last) = last_scan {
            if let Ok(last_time) = chrono::DateTime::parse_from_rfc3339(&last) {
                let elapsed = Utc::now().signed_duration_since(last_time);
                return Ok(elapsed.num_seconds() >= interval_secs);
            }
        }
        Ok(true)
    }
}

impl ConsumerLogRepository for Database {
    fn log_insert(&self, log: &NewConsumerLog) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        self.conn().execute(
            "INSERT INTO consumer_logs (id, repo_id, queue_type, queue_item_id, worker_id, command, stdout, stderr, exit_code, started_at, finished_at, duration_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                id, log.repo_id, log.queue_type, log.queue_item_id, log.worker_id,
                log.command, log.stdout, log.stderr, log.exit_code,
                log.started_at, log.finished_at, log.duration_ms
            ],
        )?;
        Ok(())
    }

    fn log_recent(&self, repo_name: Option<&str>, limit: usize) -> Result<Vec<LogEntry>> {
        let conn = self.conn();

        let (query, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(name) = repo_name {
                (
                    "SELECT cl.started_at, cl.queue_type, cl.command, cl.exit_code, cl.duration_ms \
                     FROM consumer_logs cl JOIN repositories r ON cl.repo_id = r.id \
                     WHERE r.name = ?1 ORDER BY cl.started_at DESC LIMIT ?2"
                        .to_string(),
                    vec![Box::new(name.to_string()), Box::new(limit as i64)],
                )
            } else {
                (
                    "SELECT cl.started_at, cl.queue_type, cl.command, cl.exit_code, cl.duration_ms \
                     FROM consumer_logs cl ORDER BY cl.started_at DESC LIMIT ?1"
                        .to_string(),
                    vec![Box::new(limit as i64)],
                )
            };

        let mut stmt = conn.prepare(&query)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(LogEntry {
                started_at: row.get(0)?,
                queue_type: row.get(1)?,
                command: row.get(2)?,
                exit_code: row.get(3)?,
                duration_ms: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

impl QueueAdmin for Database {
    fn queue_retry(&self, id: &str) -> Result<bool> {
        let now = Utc::now().to_rfc3339();
        for table in &["issue_queue", "pr_queue", "merge_queue"] {
            let affected = self.conn().execute(
                &format!("UPDATE {table} SET status = 'pending', error_message = NULL, worker_id = NULL, updated_at = ?2 WHERE id = ?1 AND status = 'failed'"),
                rusqlite::params![id, now],
            )?;
            if affected > 0 {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn queue_clear(&self, repo_name: &str) -> Result<()> {
        let conn = self.conn();
        for table in &["issue_queue", "pr_queue", "merge_queue"] {
            conn.execute(
                &format!(
                    "DELETE FROM {table} WHERE repo_id = (SELECT id FROM repositories WHERE name = ?1) AND status IN ('done', 'failed')"
                ),
                rusqlite::params![repo_name],
            )?;
        }
        Ok(())
    }

    fn queue_reset_stuck(&self, threshold_secs: i64) -> Result<u64> {
        let now = Utc::now();
        let cutoff = (now - chrono::Duration::seconds(threshold_secs)).to_rfc3339();
        let now_str = now.to_rfc3339();
        let conn = self.conn();
        let mut total = 0u64;

        let stuck_states = [
            (
                "issue_queue",
                &["analyzing", "processing", "ready"] as &[&str],
            ),
            ("pr_queue", &["reviewing"]),
            ("merge_queue", &["merging", "conflict"]),
        ];

        for (table, states) in &stuck_states {
            let placeholders: Vec<String> = states.iter().map(|s| format!("'{s}'")).collect();
            let in_clause = placeholders.join(",");
            let affected = conn.execute(
                &format!(
                    "UPDATE {table} SET status = 'pending', worker_id = NULL, error_message = NULL, updated_at = ?1 \
                     WHERE status IN ({in_clause}) AND updated_at <= ?2"
                ),
                rusqlite::params![now_str, cutoff],
            )?;
            total += affected as u64;
        }

        Ok(total)
    }

    fn queue_auto_retry_failed(&self, max_retries: i64) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn();
        let mut total = 0u64;

        for table in &["issue_queue", "pr_queue", "merge_queue"] {
            let affected = conn.execute(
                &format!(
                    "UPDATE {table} SET status = 'pending', error_message = NULL, worker_id = NULL, \
                     retry_count = retry_count + 1, updated_at = ?1 \
                     WHERE status = 'failed' AND retry_count < ?2"
                ),
                rusqlite::params![now, max_retries],
            )?;
            total += affected as u64;
        }

        Ok(total)
    }
}
