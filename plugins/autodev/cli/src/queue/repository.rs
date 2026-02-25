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
    fn repo_status_summary(&self) -> Result<Vec<RepoStatusRow>>;
}

pub trait ScanCursorRepository {
    fn cursor_get_last_seen(&self, repo_id: &str, target: &str) -> Result<Option<String>>;
    fn cursor_upsert(&self, repo_id: &str, target: &str, last_seen: &str) -> Result<()>;
    fn cursor_should_scan(&self, repo_id: &str, interval_secs: i64) -> Result<bool>;
}

pub trait ConsumerLogRepository {
    fn log_insert(&self, log: &NewConsumerLog) -> Result<()>;
    fn log_recent(&self, repo_name: Option<&str>, limit: usize) -> Result<Vec<LogEntry>>;
    /// 특정 날짜의 knowledge extraction stdout를 모두 반환
    fn log_knowledge_stdout_by_date(&self, date: &str) -> Result<Vec<String>>;
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
        let tx = conn.unchecked_transaction()?;
        let repo_id_query = "(SELECT id FROM repositories WHERE name = ?1)";

        tx.execute(
            &format!("DELETE FROM scan_cursors WHERE repo_id = {repo_id_query}"),
            rusqlite::params![name],
        )?;
        tx.execute(
            &format!("DELETE FROM consumer_logs WHERE repo_id = {repo_id_query}"),
            rusqlite::params![name],
        )?;
        tx.execute(
            "DELETE FROM repositories WHERE name = ?1",
            rusqlite::params![name],
        )?;
        tx.commit()?;
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

    fn repo_status_summary(&self) -> Result<Vec<RepoStatusRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT name, enabled FROM repositories ORDER BY name")?;
        let rows = stmt.query_map([], |row| {
            Ok(RepoStatusRow {
                name: row.get(0)?,
                enabled: row.get(1)?,
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

    fn log_knowledge_stdout_by_date(&self, date: &str) -> Result<Vec<String>> {
        let conn = self.conn();
        let like_pattern = format!("{date}%");
        let mut stmt = conn.prepare(
            "SELECT stdout FROM consumer_logs \
             WHERE queue_type = 'knowledge' AND started_at LIKE ?1 \
             ORDER BY started_at",
        )?;
        let rows = stmt.query_map(rusqlite::params![like_pattern], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
