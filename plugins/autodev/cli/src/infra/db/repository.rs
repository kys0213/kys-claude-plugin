use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::core::models::*;
use crate::core::repository::*;

use super::Database;

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

        // Lookup repo_id explicitly first to avoid subquery issues with FK enforcement
        let repo_id: Option<String> = conn
            .query_row(
                "SELECT id FROM repositories WHERE name = ?1",
                rusqlite::params![name],
                |row| row.get(0),
            )
            .ok();

        let repo_id = match repo_id {
            Some(id) => id,
            None => anyhow::bail!("repository not found: {name}"),
        };

        let tx = conn.unchecked_transaction()?;
        // Child tables with FK to hitl_events/specs must be deleted first
        tx.execute(
            "DELETE FROM hitl_responses WHERE event_id IN (SELECT id FROM hitl_events WHERE repo_id = ?1)",
            rusqlite::params![repo_id],
        )?;
        tx.execute(
            "DELETE FROM spec_issues WHERE spec_id IN (SELECT id FROM specs WHERE repo_id = ?1)",
            rusqlite::params![repo_id],
        )?;
        tx.execute(
            "DELETE FROM hitl_events WHERE repo_id = ?1",
            rusqlite::params![repo_id],
        )?;
        tx.execute(
            "DELETE FROM specs WHERE repo_id = ?1",
            rusqlite::params![repo_id],
        )?;
        tx.execute(
            "DELETE FROM queue_items WHERE repo_id = ?1",
            rusqlite::params![repo_id],
        )?;
        tx.execute(
            "DELETE FROM claw_decisions WHERE repo_id = ?1",
            rusqlite::params![repo_id],
        )?;
        tx.execute(
            "DELETE FROM token_usage WHERE repo_id = ?1",
            rusqlite::params![repo_id],
        )?;
        tx.execute(
            "DELETE FROM scan_cursors WHERE repo_id = ?1",
            rusqlite::params![repo_id],
        )?;
        tx.execute(
            "DELETE FROM consumer_logs WHERE repo_id = ?1",
            rusqlite::params![repo_id],
        )?;
        tx.execute(
            "DELETE FROM feedback_patterns WHERE repo_id = ?1",
            rusqlite::params![repo_id],
        )?;
        tx.execute(
            "DELETE FROM history WHERE workspace_id = ?1",
            rusqlite::params![repo_id],
        )?;
        tx.execute(
            "DELETE FROM cron_jobs WHERE repo_id = ?1",
            rusqlite::params![repo_id],
        )?;
        tx.execute(
            "DELETE FROM repositories WHERE id = ?1",
            rusqlite::params![repo_id],
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
    fn log_insert(&self, log: &NewConsumerLog) -> Result<String> {
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
        Ok(id)
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

impl SpecRepository for Database {
    fn spec_add(&self, spec: &NewSpec) -> Result<String> {
        let conn = self.conn();
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        conn.execute(
            "INSERT INTO specs (id, repo_id, title, body, status, source_path, test_commands, acceptance_criteria, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, ?7, ?8, ?8)",
            rusqlite::params![
                id, spec.repo_id, spec.title, spec.body,
                spec.source_path, spec.test_commands, spec.acceptance_criteria, now
            ],
        )?;

        Ok(id)
    }

    fn spec_list(&self, repo: Option<&str>) -> Result<Vec<Spec>> {
        let conn = self.conn();

        let (query, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(name) = repo {
                (
                    "SELECT s.id, s.repo_id, s.title, s.body, s.status, s.source_path, \
                 s.test_commands, s.acceptance_criteria, s.priority, s.created_at, s.updated_at \
                 FROM specs s JOIN repositories r ON s.repo_id = r.id \
                 WHERE r.name = ?1 ORDER BY s.created_at DESC"
                        .to_string(),
                    vec![Box::new(name.to_string())],
                )
            } else {
                (
                    "SELECT id, repo_id, title, body, status, source_path, \
                 test_commands, acceptance_criteria, priority, created_at, updated_at \
                 FROM specs ORDER BY created_at DESC"
                        .to_string(),
                    vec![],
                )
            };

        let mut stmt = conn.prepare(&query)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), map_spec_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn spec_show(&self, id: &str) -> Result<Option<Spec>> {
        let conn = self.conn();
        let result = conn.query_row(
            "SELECT id, repo_id, title, body, status, source_path, \
             test_commands, acceptance_criteria, priority, created_at, updated_at \
             FROM specs WHERE id = ?1",
            rusqlite::params![id],
            map_spec_row,
        );
        optional_query_row(result)
    }

    fn spec_update(
        &self,
        id: &str,
        body: &str,
        test_commands: Option<&str>,
        acceptance_criteria: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn();
        let now = Utc::now().to_rfc3339();

        let affected = conn.execute(
            "UPDATE specs SET body = ?1, test_commands = ?2, acceptance_criteria = ?3, updated_at = ?4 \
             WHERE id = ?5",
            rusqlite::params![body, test_commands, acceptance_criteria, now, id],
        )?;

        if affected == 0 {
            anyhow::bail!("spec not found: {id}");
        }
        Ok(())
    }

    fn spec_set_status(&self, id: &str, status: SpecStatus) -> Result<()> {
        let conn = self.conn();
        let now = Utc::now().to_rfc3339();

        let affected = conn.execute(
            "UPDATE specs SET status = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![status.as_str(), now, id],
        )?;

        if affected == 0 {
            anyhow::bail!("spec not found: {id}");
        }
        Ok(())
    }

    fn spec_issues(&self, spec_id: &str) -> Result<Vec<SpecIssue>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT spec_id, issue_number, created_at FROM spec_issues \
             WHERE spec_id = ?1 ORDER BY issue_number",
        )?;
        let rows = stmt.query_map(rusqlite::params![spec_id], |row| {
            Ok(SpecIssue {
                spec_id: row.get(0)?,
                issue_number: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn spec_issues_all(&self) -> Result<std::collections::HashMap<String, Vec<SpecIssue>>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT spec_id, issue_number, created_at FROM spec_issues \
             ORDER BY spec_id, issue_number",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SpecIssue {
                spec_id: row.get(0)?,
                issue_number: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?;
        let mut map: std::collections::HashMap<String, Vec<SpecIssue>> =
            std::collections::HashMap::new();
        for row in rows {
            let si = row?;
            map.entry(si.spec_id.clone()).or_default().push(si);
        }
        Ok(map)
    }

    fn spec_issue_counts(&self) -> Result<std::collections::HashMap<String, usize>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare("SELECT spec_id, COUNT(*) FROM spec_issues GROUP BY spec_id")?;
        let rows = stmt.query_map([], |row| {
            let spec_id: String = row.get(0)?;
            let count: usize = row.get(1)?;
            Ok((spec_id, count))
        })?;
        let mut map = std::collections::HashMap::new();
        for row in rows {
            let (spec_id, count) = row?;
            map.insert(spec_id, count);
        }
        Ok(map)
    }

    fn spec_link_issue(&self, spec_id: &str, issue_number: i64) -> Result<()> {
        let conn = self.conn();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO spec_issues (spec_id, issue_number, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![spec_id, issue_number, now],
        )?;
        Ok(())
    }

    fn spec_unlink_issue(&self, spec_id: &str, issue_number: i64) -> Result<()> {
        let conn = self.conn();

        let affected = conn.execute(
            "DELETE FROM spec_issues WHERE spec_id = ?1 AND issue_number = ?2",
            rusqlite::params![spec_id, issue_number],
        )?;

        if affected == 0 {
            anyhow::bail!("issue link not found: spec={spec_id}, issue=#{issue_number}");
        }
        Ok(())
    }

    fn spec_list_by_status(&self, status: SpecStatus) -> Result<Vec<Spec>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, repo_id, title, body, status, source_path, \
             test_commands, acceptance_criteria, priority, created_at, updated_at \
             FROM specs WHERE status = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(rusqlite::params![status.as_str()], map_spec_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn spec_set_priority(&self, id: &str, priority: i32) -> Result<()> {
        let conn = self.conn();
        let now = Utc::now().to_rfc3339();

        let affected = conn.execute(
            "UPDATE specs SET priority = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![priority, now, id],
        )?;

        if affected == 0 {
            anyhow::bail!("spec not found: {id}");
        }
        Ok(())
    }
}

impl TokenUsageRepository for Database {
    fn usage_insert(&self, usage: &NewTokenUsage) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "INSERT INTO token_usage (log_id, repo_id, queue_type, queue_item_id, \
             input_tokens, output_tokens, cache_write_tokens, cache_read_tokens, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                usage.log_id,
                usage.repo_id,
                usage.queue_type,
                usage.queue_item_id,
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_write_tokens,
                usage.cache_read_tokens,
                now
            ],
        )?;
        Ok(())
    }

    fn usage_summary(&self, repo: Option<&str>, since: Option<&str>) -> Result<UsageSummary> {
        let conn = self.conn();

        if let Some(name) = repo {
            if !name
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'))
            {
                anyhow::bail!("invalid repo name: {name}");
            }
        }

        // Build WHERE clauses
        let mut conditions = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(name) = repo {
            conditions.push(format!("r.name = ?{idx}"));
            params.push(Box::new(name.to_string()));
            idx += 1;
        }
        if let Some(date) = since {
            conditions.push(format!("cl.started_at >= ?{idx}"));
            params.push(Box::new(date.to_string()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // Total summary from consumer_logs (sessions + duration)
        let totals_query = format!(
            "SELECT COUNT(*), COALESCE(SUM(cl.duration_ms), 0) \
             FROM consumer_logs cl JOIN repositories r ON cl.repo_id = r.id {where_clause}"
        );
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let (total_sessions, total_duration_ms): (i64, i64) =
            conn.query_row(&totals_query, params_refs.as_slice(), |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;

        // Token totals from token_usage table
        let token_totals_query = format!(
            "SELECT COALESCE(SUM(tu.input_tokens), 0), COALESCE(SUM(tu.output_tokens), 0), \
             COALESCE(SUM(tu.cache_write_tokens), 0), COALESCE(SUM(tu.cache_read_tokens), 0) \
             FROM token_usage tu JOIN repositories r ON tu.repo_id = r.id \
             JOIN consumer_logs cl ON tu.log_id = cl.id {where_clause}"
        );
        let (total_input, total_output, total_cache_write, total_cache_read): (i64, i64, i64, i64) =
            conn.query_row(&token_totals_query, params_refs.as_slice(), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?;

        // By queue_type
        let qt_query = format!(
            "SELECT cl.queue_type, COUNT(*), COALESCE(SUM(cl.duration_ms), 0), \
             COALESCE(SUM(tu.input_tokens), 0), COALESCE(SUM(tu.output_tokens), 0) \
             FROM consumer_logs cl \
             JOIN repositories r ON cl.repo_id = r.id \
             LEFT JOIN token_usage tu ON tu.log_id = cl.id \
             {where_clause} GROUP BY cl.queue_type ORDER BY cl.queue_type"
        );
        let mut stmt = conn.prepare(&qt_query)?;
        let qt_rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(UsageByQueueType {
                queue_type: row.get(0)?,
                sessions: row.get(1)?,
                duration_ms: row.get(2)?,
                input_tokens: row.get(3)?,
                output_tokens: row.get(4)?,
            })
        })?;
        let by_queue_type: Vec<UsageByQueueType> = qt_rows.collect::<Result<Vec<_>, _>>()?;

        // By repo
        let repo_query = format!(
            "SELECT r.name, COUNT(*), COALESCE(SUM(cl.duration_ms), 0), \
             COALESCE(SUM(tu.input_tokens), 0), COALESCE(SUM(tu.output_tokens), 0) \
             FROM consumer_logs cl \
             JOIN repositories r ON cl.repo_id = r.id \
             LEFT JOIN token_usage tu ON tu.log_id = cl.id \
             {where_clause} GROUP BY r.name ORDER BY r.name"
        );
        let mut stmt = conn.prepare(&repo_query)?;
        let repo_rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(UsageByRepo {
                repo_name: row.get(0)?,
                sessions: row.get(1)?,
                duration_ms: row.get(2)?,
                input_tokens: row.get(3)?,
                output_tokens: row.get(4)?,
            })
        })?;
        let by_repo: Vec<UsageByRepo> = repo_rows.collect::<Result<Vec<_>, _>>()?;

        Ok(UsageSummary {
            total_sessions,
            total_duration_ms,
            total_input_tokens: total_input,
            total_output_tokens: total_output,
            total_cache_write_tokens: total_cache_write,
            total_cache_read_tokens: total_cache_read,
            by_queue_type,
            by_repo,
        })
    }

    fn usage_by_issue(&self, repo: &str, issue: i64) -> Result<Vec<UsageByIssue>> {
        let conn = self.conn();
        let issue_str = issue.to_string();
        let mut stmt = conn.prepare(
            "SELECT cl.queue_item_id, cl.queue_type, COUNT(*), \
             COALESCE(SUM(cl.duration_ms), 0), \
             COALESCE(SUM(tu.input_tokens), 0), COALESCE(SUM(tu.output_tokens), 0) \
             FROM consumer_logs cl \
             JOIN repositories r ON cl.repo_id = r.id \
             LEFT JOIN token_usage tu ON tu.log_id = cl.id \
             WHERE r.name = ?1 AND cl.queue_item_id = ?2 \
             GROUP BY cl.queue_item_id, cl.queue_type \
             ORDER BY cl.queue_type",
        )?;
        let rows = stmt.query_map(rusqlite::params![repo, issue_str], |row| {
            Ok(UsageByIssue {
                queue_item_id: row.get(0)?,
                queue_type: row.get(1)?,
                sessions: row.get(2)?,
                duration_ms: row.get(3)?,
                input_tokens: row.get(4)?,
                output_tokens: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

impl HitlRepository for Database {
    fn hitl_create(&self, event: &NewHitlEvent) -> Result<String> {
        let conn = self.conn();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let options_json = serde_json::to_string(&event.options)?;

        conn.execute(
            "INSERT INTO hitl_events (id, repo_id, spec_id, work_id, severity, situation, context, options, status, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', ?9)",
            rusqlite::params![
                id,
                event.repo_id,
                event.spec_id,
                event.work_id,
                event.severity.to_string(),
                event.situation,
                event.context,
                options_json,
                now
            ],
        )?;

        Ok(id)
    }

    fn hitl_list(&self, repo: Option<&str>) -> Result<Vec<HitlEvent>> {
        let conn = self.conn();

        let (query, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(name) =
            repo
        {
            (
                "SELECT e.id, e.repo_id, e.spec_id, e.work_id, e.severity, e.situation, e.context, e.options, e.status, e.created_at \
                 FROM hitl_events e JOIN repositories r ON e.repo_id = r.id \
                 WHERE r.name = ?1 ORDER BY e.created_at DESC"
                    .to_string(),
                vec![Box::new(name.to_string())],
            )
        } else {
            (
                "SELECT id, repo_id, spec_id, work_id, severity, situation, context, options, status, created_at \
                 FROM hitl_events ORDER BY created_at DESC"
                    .to_string(),
                vec![],
            )
        };

        let mut stmt = conn.prepare(&query)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), map_hitl_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn hitl_show(&self, id: &str) -> Result<Option<HitlEvent>> {
        let conn = self.conn();
        let result = conn.query_row(
            "SELECT id, repo_id, spec_id, work_id, severity, situation, context, options, status, created_at \
             FROM hitl_events WHERE id = ?1",
            rusqlite::params![id],
            map_hitl_row,
        );
        optional_query_row(result)
    }

    fn hitl_respond(&self, response: &NewHitlResponse) -> Result<()> {
        let conn = self.conn();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        let tx = conn.unchecked_transaction()?;

        tx.execute(
            "INSERT INTO hitl_responses (id, event_id, choice, message, source, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                id,
                response.event_id,
                response.choice,
                response.message,
                response.source,
                now
            ],
        )?;

        tx.execute(
            "UPDATE hitl_events SET status = 'responded' WHERE id = ?1",
            rusqlite::params![response.event_id],
        )?;

        tx.commit()?;
        Ok(())
    }

    fn hitl_set_status(&self, id: &str, status: HitlStatus) -> Result<()> {
        let conn = self.conn();
        let affected = conn.execute(
            "UPDATE hitl_events SET status = ?1 WHERE id = ?2",
            rusqlite::params![status.to_string(), id],
        )?;
        if affected == 0 {
            anyhow::bail!("hitl event not found: {id}");
        }
        Ok(())
    }

    fn hitl_pending_count(&self, repo: Option<&str>) -> Result<i64> {
        let conn = self.conn();

        let (query, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(name) = repo {
                (
                    "SELECT COUNT(*) FROM hitl_events e JOIN repositories r ON e.repo_id = r.id \
                 WHERE r.name = ?1 AND e.status = 'pending'"
                        .to_string(),
                    vec![Box::new(name.to_string())],
                )
            } else {
                (
                    "SELECT COUNT(*) FROM hitl_events WHERE status = 'pending'".to_string(),
                    vec![],
                )
            };

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let count: i64 = conn.query_row(&query, params_refs.as_slice(), |row| row.get(0))?;
        Ok(count)
    }

    fn hitl_total_count(&self, repo: Option<&str>) -> Result<i64> {
        let conn = self.conn();
        let (query, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(name) = repo {
                (
                    "SELECT COUNT(*) FROM hitl_events e JOIN repositories r ON e.repo_id = r.id \
                     WHERE r.name = ?1"
                        .to_string(),
                    vec![Box::new(name.to_string())],
                )
            } else {
                ("SELECT COUNT(*) FROM hitl_events".to_string(), vec![])
            };
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let count: i64 = conn.query_row(&query, params_refs.as_slice(), |row| row.get(0))?;
        Ok(count)
    }

    fn hitl_responses(&self, event_id: &str) -> Result<Vec<HitlResponse>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, event_id, choice, message, source, created_at \
             FROM hitl_responses WHERE event_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(rusqlite::params![event_id], |row| {
            Ok(HitlResponse {
                id: row.get(0)?,
                event_id: row.get(1)?,
                choice: row.get(2)?,
                message: row.get(3)?,
                source: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn hitl_expired_list(&self, timeout_hours: i64) -> Result<Vec<HitlEvent>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, repo_id, spec_id, work_id, severity, situation, context, options, status, created_at \
             FROM hitl_events \
             WHERE status = 'pending' \
             AND datetime(created_at) < datetime('now', '-' || ?1 || ' hours') \
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![timeout_hours], map_hitl_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn hitl_count_by_spec(&self, spec_id: &str) -> Result<(i64, i64)> {
        let conn = self.conn();
        let (total, pending) = conn.query_row(
            "SELECT \
               COUNT(*), \
               SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END) \
             FROM hitl_events WHERE spec_id = ?1",
            rusqlite::params![spec_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1).unwrap_or(0))),
        )?;
        Ok((total, pending))
    }
}

impl QueueRepository for Database {
    fn queue_get_phase(&self, work_id: &str) -> Result<Option<QueuePhase>> {
        let result = self.conn().query_row(
            "SELECT phase FROM queue_items WHERE work_id = ?1",
            rusqlite::params![work_id],
            |row| {
                let phase_str: String = row.get(0)?;
                phase_str.parse().map_err(|e: String| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                    )
                })
            },
        );
        optional_query_row(result)
    }

    fn queue_advance(&self, work_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        // Atomic CAS-style transitions: UPDATE with WHERE on expected phase.
        // Try each valid forward transition; exactly one should match.
        let transitions: &[(QueuePhase, QueuePhase)] = &[
            (QueuePhase::Pending, QueuePhase::Ready),
            (QueuePhase::Ready, QueuePhase::Running),
            (QueuePhase::Running, QueuePhase::Completed),
        ];

        let conn = self.conn();
        for (from, to) in transitions {
            let affected = conn.execute(
                "UPDATE queue_items SET phase = ?1, updated_at = ?2 WHERE work_id = ?3 AND phase = ?4",
                rusqlite::params![to.as_str(), now, work_id, from.as_str()],
            )?;
            if affected > 0 {
                return Ok(());
            }
        }

        // None of the transitions matched — determine why.
        let current = self.queue_get_phase(work_id)?;
        match current {
            None => anyhow::bail!("queue item not found: {work_id}"),
            Some(QueuePhase::Hitl) => {
                anyhow::bail!("cannot advance hitl item: respond via 'hitl respond' first")
            }
            Some(QueuePhase::Completed) => {
                anyhow::bail!(
                    "cannot advance completed item: use 'queue done' or 'queue hitl' instead"
                )
            }
            Some(phase) => {
                if phase.is_terminal() {
                    anyhow::bail!("cannot advance terminal state: {phase}")
                }
                anyhow::bail!("unexpected phase: {phase}")
            }
        }
    }

    fn queue_skip(&self, work_id: &str, reason: Option<&str>) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        // Atomic: only skip if not already in a terminal state.
        let affected = self.conn().execute(
            "UPDATE queue_items SET phase = 'skipped', skip_reason = ?1, updated_at = ?2 \
             WHERE work_id = ?3 AND phase NOT IN ('done', 'skipped')",
            rusqlite::params![reason, now, work_id],
        )?;

        if affected == 0 {
            let current = self.queue_get_phase(work_id)?;
            match current {
                None => anyhow::bail!("queue item not found: {work_id}"),
                Some(phase) => {
                    anyhow::bail!("cannot skip terminal state: {phase}")
                }
            }
        }

        Ok(())
    }

    fn queue_list_items(&self, repo: Option<&str>) -> Result<Vec<QueueItemRow>> {
        let conn = self.conn();

        let (query, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(name) = repo {
                (
                    "SELECT q.work_id, q.repo_id, q.queue_type, q.phase, q.title, \
                     q.skip_reason, q.created_at, q.updated_at, \
                     q.task_kind, q.github_number, q.metadata_json, \
                     q.failure_count, q.escalation_level \
                     FROM queue_items q JOIN repositories r ON q.repo_id = r.id \
                     WHERE r.name = ?1 ORDER BY q.created_at DESC"
                        .to_string(),
                    vec![Box::new(name.to_string())],
                )
            } else {
                (
                    "SELECT work_id, repo_id, queue_type, phase, title, \
                     skip_reason, created_at, updated_at, \
                     task_kind, github_number, metadata_json, \
                     failure_count, escalation_level \
                     FROM queue_items ORDER BY created_at DESC"
                        .to_string(),
                    vec![],
                )
            };

        let mut stmt = conn.prepare(&query)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), map_queue_item_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn queue_upsert(&self, item: &QueueItemRow) -> Result<()> {
        let conn = self.conn();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO queue_items (work_id, repo_id, queue_type, phase, title, skip_reason, \
             created_at, updated_at, task_kind, github_number, metadata_json, \
             failure_count, escalation_level) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13) \
             ON CONFLICT(work_id) DO UPDATE SET \
             phase = excluded.phase, title = excluded.title, skip_reason = excluded.skip_reason, \
             updated_at = ?8, task_kind = excluded.task_kind, \
             github_number = excluded.github_number, metadata_json = excluded.metadata_json",
            rusqlite::params![
                item.work_id,
                item.repo_id,
                item.queue_type.as_str(),
                item.phase.as_str(),
                item.title,
                item.skip_reason,
                item.created_at,
                now,
                item.task_kind.as_str(),
                item.github_number,
                item.metadata_json,
                item.failure_count,
                item.escalation_level,
            ],
        )?;
        Ok(())
    }

    fn queue_remove(&self, work_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "UPDATE queue_items SET phase = 'done', updated_at = ?1 WHERE work_id = ?2",
            rusqlite::params![now, work_id],
        )?;
        Ok(())
    }

    fn queue_load_active(&self, repo_id: &str) -> Result<Vec<QueueItemRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT work_id, repo_id, queue_type, phase, title, \
             skip_reason, created_at, updated_at, \
             task_kind, github_number, metadata_json, \
             failure_count, escalation_level \
             FROM queue_items WHERE repo_id = ?1 AND phase NOT IN ('done', 'skipped') \
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![repo_id], map_queue_item_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn queue_transit(&self, work_id: &str, from: QueuePhase, to: QueuePhase) -> Result<bool> {
        let now = Utc::now().to_rfc3339();
        let affected = self.conn().execute(
            "UPDATE queue_items SET phase = ?1, updated_at = ?2 WHERE work_id = ?3 AND phase = ?4",
            rusqlite::params![to.as_str(), now, work_id, from.as_str()],
        )?;
        Ok(affected > 0)
    }

    fn queue_get_item(&self, work_id: &str) -> Result<Option<QueueItemRow>> {
        let conn = self.conn();
        let result = conn.query_row(
            "SELECT work_id, repo_id, queue_type, phase, title, \
             skip_reason, created_at, updated_at, \
             task_kind, github_number, metadata_json, \
             failure_count, escalation_level \
             FROM queue_items WHERE work_id = ?1",
            rusqlite::params![work_id],
            map_queue_item_row,
        );
        optional_query_row(result)
    }

    fn queue_increment_failure(&self, work_id: &str) -> Result<i32> {
        let conn = self.conn();
        let now = Utc::now().to_rfc3339();
        let count: i32 = conn.query_row(
            "UPDATE queue_items SET failure_count = failure_count + 1, updated_at = ?1 \
             WHERE work_id = ?2 RETURNING failure_count",
            rusqlite::params![now, work_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    fn queue_get_failure_count(&self, work_id: &str) -> Result<i32> {
        let conn = self.conn();
        let count: i32 = conn.query_row(
            "SELECT failure_count FROM queue_items WHERE work_id = ?1",
            rusqlite::params![work_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}

impl ClawDecisionRepository for Database {
    fn decision_add(&self, decision: &NewClawDecision) -> Result<String> {
        let conn = self.conn();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO claw_decisions (id, repo_id, spec_id, decision_type, target_work_id, reasoning, context_json, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                id,
                decision.repo_id,
                decision.spec_id,
                decision.decision_type.as_str(),
                decision.target_work_id,
                decision.reasoning,
                decision.context_json,
                now
            ],
        )?;

        Ok(id)
    }

    fn decision_list(&self, repo: Option<&str>, limit: usize) -> Result<Vec<ClawDecision>> {
        let conn = self.conn();

        let (query, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(name) = repo {
                (
                    "SELECT d.id, d.repo_id, d.spec_id, d.decision_type, d.target_work_id, \
                     d.reasoning, d.context_json, d.created_at \
                     FROM claw_decisions d JOIN repositories r ON d.repo_id = r.id \
                     WHERE r.name = ?1 ORDER BY d.created_at DESC LIMIT ?2"
                        .to_string(),
                    vec![Box::new(name.to_string()), Box::new(limit as i64)],
                )
            } else {
                (
                    "SELECT id, repo_id, spec_id, decision_type, target_work_id, \
                     reasoning, context_json, created_at \
                     FROM claw_decisions ORDER BY created_at DESC LIMIT ?1"
                        .to_string(),
                    vec![Box::new(limit as i64)],
                )
            };

        let mut stmt = conn.prepare(&query)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), map_decision_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn decision_show(&self, id: &str) -> Result<Option<ClawDecision>> {
        let conn = self.conn();
        let result = conn.query_row(
            "SELECT id, repo_id, spec_id, decision_type, target_work_id, \
             reasoning, context_json, created_at \
             FROM claw_decisions WHERE id = ?1",
            rusqlite::params![id],
            map_decision_row,
        );
        optional_query_row(result)
    }

    fn decision_list_by_spec(&self, spec_id: &str, limit: usize) -> Result<Vec<ClawDecision>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, repo_id, spec_id, decision_type, target_work_id, \
             reasoning, context_json, created_at \
             FROM claw_decisions WHERE spec_id = ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![spec_id, limit as i64], map_decision_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn decision_count(&self, repo: Option<&str>) -> Result<i64> {
        let conn = self.conn();

        let (query, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(name) = repo {
                (
                    "SELECT COUNT(*) FROM claw_decisions d JOIN repositories r ON d.repo_id = r.id \
                     WHERE r.name = ?1"
                        .to_string(),
                    vec![Box::new(name.to_string())],
                )
            } else {
                ("SELECT COUNT(*) FROM claw_decisions".to_string(), vec![])
            };

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let count: i64 = conn.query_row(&query, params_refs.as_slice(), |row| row.get(0))?;
        Ok(count)
    }
}

impl FeedbackPatternRepository for Database {
    fn feedback_upsert(&self, pattern: &NewFeedbackPattern) -> Result<String> {
        let conn = self.conn();
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        let tx = conn.unchecked_transaction()?;

        // Try insert first
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO feedback_patterns \
             (id, repo_id, pattern_type, suggestion, source, occurrence_count, confidence, status, sources_json, created_at, last_seen_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 1, 0.5, 'active', ?6, ?7, ?7)",
            rusqlite::params![
                id,
                pattern.repo_id,
                pattern.pattern_type,
                pattern.suggestion,
                pattern.source,
                format!(r#"{{"{}": 1}}"#, pattern.source),
                now
            ],
        )?;

        if inserted == 0 {
            // Row already exists — increment occurrence_count and update sources_json
            tx.execute(
                "UPDATE feedback_patterns \
                 SET occurrence_count = occurrence_count + 1, \
                     last_seen_at = ?1, \
                     source = ?2, \
                     sources_json = json_set(sources_json, '$.' || ?2, \
                         COALESCE(json_extract(sources_json, '$.' || ?2), 0) + 1) \
                 WHERE repo_id = ?3 AND pattern_type = ?4 AND suggestion = ?5",
                rusqlite::params![
                    now,
                    pattern.source,
                    pattern.repo_id,
                    pattern.pattern_type,
                    pattern.suggestion
                ],
            )?;

            // Return the existing id
            let existing_id: String = tx.query_row(
                "SELECT id FROM feedback_patterns \
                 WHERE repo_id = ?1 AND pattern_type = ?2 AND suggestion = ?3",
                rusqlite::params![pattern.repo_id, pattern.pattern_type, pattern.suggestion],
                |row| row.get(0),
            )?;
            tx.commit()?;
            return Ok(existing_id);
        }

        tx.commit()?;
        Ok(id)
    }

    fn feedback_list(&self, repo_id: &str) -> Result<Vec<FeedbackPattern>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, repo_id, pattern_type, suggestion, source, occurrence_count, \
             confidence, status, sources_json, created_at, last_seen_at \
             FROM feedback_patterns WHERE repo_id = ?1 \
             ORDER BY occurrence_count DESC, last_seen_at DESC",
        )?;
        let rows = stmt.query_map(rusqlite::params![repo_id], map_feedback_pattern_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn feedback_list_actionable(
        &self,
        repo_id: &str,
        min_count: i32,
    ) -> Result<Vec<FeedbackPattern>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, repo_id, pattern_type, suggestion, source, occurrence_count, \
             confidence, status, sources_json, created_at, last_seen_at \
             FROM feedback_patterns \
             WHERE repo_id = ?1 AND occurrence_count >= ?2 AND status = 'active' \
             ORDER BY occurrence_count DESC, last_seen_at DESC",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![repo_id, min_count],
            map_feedback_pattern_row,
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn feedback_set_status(&self, id: &str, status: FeedbackPatternStatus) -> Result<()> {
        let conn = self.conn();
        let rows_affected = conn.execute(
            "UPDATE feedback_patterns SET status = ?1 WHERE id = ?2",
            rusqlite::params![status.as_str(), id],
        )?;
        if rows_affected == 0 {
            anyhow::bail!("feedback pattern not found: {id}");
        }
        Ok(())
    }
}

impl TransitionEventRepository for Database {
    fn transition_insert(&self, event: &NewTransitionEvent) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "INSERT INTO transition_events (id, work_id, source_id, event_type, phase, detail, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                id,
                event.work_id,
                event.source_id,
                event.event_type.as_str(),
                event.phase,
                event.detail,
                now,
            ],
        )?;
        Ok(id)
    }

    fn transition_list_by_work_id(&self, work_id: &str) -> Result<Vec<TransitionEvent>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, work_id, source_id, event_type, phase, detail, created_at \
             FROM transition_events WHERE work_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![work_id], |row| {
            Ok(TransitionEvent {
                id: row.get(0)?,
                work_id: row.get(1)?,
                source_id: row.get(2)?,
                event_type: row
                    .get::<_, String>(3)?
                    .parse()
                    .unwrap_or(TransitionEventType::PhaseEnter),
                phase: row.get(4)?,
                detail: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn transition_list_recent(&self, limit: usize) -> Result<Vec<TransitionEvent>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, work_id, source_id, event_type, phase, detail, created_at \
             FROM transition_events ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
            Ok(TransitionEvent {
                id: row.get(0)?,
                work_id: row.get(1)?,
                source_id: row.get(2)?,
                event_type: row
                    .get::<_, String>(3)?
                    .parse()
                    .unwrap_or(TransitionEventType::PhaseEnter),
                phase: row.get(4)?,
                detail: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

impl CronRepository for Database {
    fn cron_add(&self, job: &NewCronJob) -> Result<String> {
        let conn = self.conn();
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        let (schedule_type, schedule_value) = match &job.schedule {
            CronSchedule::Interval { secs } => ("interval".to_string(), secs.to_string()),
            CronSchedule::Expression { cron } => ("expression".to_string(), cron.clone()),
        };

        conn.execute(
            "INSERT INTO cron_jobs (id, name, repo_id, schedule_type, schedule_value, script_path, status, builtin, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7, ?8)",
            rusqlite::params![
                id,
                job.name,
                job.repo_id,
                schedule_type,
                schedule_value,
                job.script_path,
                job.builtin as i32,
                now
            ],
        )?;

        Ok(id)
    }

    fn cron_list(&self, repo: Option<&str>) -> Result<Vec<CronJob>> {
        let conn = self.conn();

        let (query, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(r) = repo
        {
            (
                "SELECT id, name, repo_id, schedule_type, schedule_value, script_path, status, builtin, last_run_at, created_at \
                 FROM cron_jobs WHERE repo_id = (SELECT id FROM repositories WHERE name = ?1) \
                 ORDER BY name"
                    .to_string(),
                vec![Box::new(r.to_string())],
            )
        } else {
            (
                "SELECT id, name, repo_id, schedule_type, schedule_value, script_path, status, builtin, last_run_at, created_at \
                 FROM cron_jobs ORDER BY name"
                    .to_string(),
                vec![],
            )
        };

        let mut stmt = conn.prepare(&query)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), |row| Ok(map_cron_row(row)))?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(row??);
        }
        Ok(jobs)
    }

    fn cron_show(&self, name: &str, repo: Option<&str>) -> Result<Option<CronJob>> {
        let conn = self.conn();

        let (query, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(r) = repo {
            (
                "SELECT id, name, repo_id, schedule_type, schedule_value, script_path, status, builtin, last_run_at, created_at \
                 FROM cron_jobs WHERE name = ?1 AND repo_id = (SELECT id FROM repositories WHERE name = ?2)",
                vec![Box::new(name.to_string()), Box::new(r.to_string())],
            )
        } else {
            (
                "SELECT id, name, repo_id, schedule_type, schedule_value, script_path, status, builtin, last_run_at, created_at \
                 FROM cron_jobs WHERE name = ?1 AND repo_id IS NULL",
                vec![Box::new(name.to_string())],
            )
        };

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let result = conn.query_row(query, params_refs.as_slice(), |row| Ok(map_cron_row(row)));

        match optional_query_row(result)? {
            Some(job) => Ok(Some(job?)),
            None => Ok(None),
        }
    }

    fn cron_update_interval(
        &self,
        name: &str,
        repo: Option<&str>,
        interval_secs: u64,
    ) -> Result<()> {
        let conn = self.conn();
        let rows_affected = if let Some(r) = repo {
            conn.execute(
                "UPDATE cron_jobs SET schedule_type = 'interval', schedule_value = ?1 \
                 WHERE name = ?2 AND repo_id = (SELECT id FROM repositories WHERE name = ?3)",
                rusqlite::params![interval_secs.to_string(), name, r],
            )?
        } else {
            conn.execute(
                "UPDATE cron_jobs SET schedule_type = 'interval', schedule_value = ?1 \
                 WHERE name = ?2 AND repo_id IS NULL",
                rusqlite::params![interval_secs.to_string(), name],
            )?
        };
        if rows_affected == 0 {
            anyhow::bail!("cron job not found: {name}");
        }
        Ok(())
    }

    fn cron_update_schedule(&self, name: &str, repo: Option<&str>, cron_expr: &str) -> Result<()> {
        let conn = self.conn();
        let rows_affected = if let Some(r) = repo {
            conn.execute(
                "UPDATE cron_jobs SET schedule_type = 'expression', schedule_value = ?1 \
                 WHERE name = ?2 AND repo_id = (SELECT id FROM repositories WHERE name = ?3)",
                rusqlite::params![cron_expr, name, r],
            )?
        } else {
            conn.execute(
                "UPDATE cron_jobs SET schedule_type = 'expression', schedule_value = ?1 \
                 WHERE name = ?2 AND repo_id IS NULL",
                rusqlite::params![cron_expr, name],
            )?
        };
        if rows_affected == 0 {
            anyhow::bail!("cron job not found: {name}");
        }
        Ok(())
    }

    fn cron_set_status(&self, name: &str, repo: Option<&str>, status: CronStatus) -> Result<()> {
        let conn = self.conn();
        let status_str = status.to_string();
        let rows_affected = if let Some(r) = repo {
            conn.execute(
                "UPDATE cron_jobs SET status = ?1 \
                 WHERE name = ?2 AND repo_id = (SELECT id FROM repositories WHERE name = ?3)",
                rusqlite::params![status_str, name, r],
            )?
        } else {
            conn.execute(
                "UPDATE cron_jobs SET status = ?1 WHERE name = ?2 AND repo_id IS NULL",
                rusqlite::params![status_str, name],
            )?
        };
        if rows_affected == 0 {
            anyhow::bail!("cron job not found: {name}");
        }
        Ok(())
    }

    fn cron_remove(&self, name: &str, repo: Option<&str>) -> Result<()> {
        let conn = self.conn();

        // Atomic DELETE with builtin = 0 guard
        let rows_affected = if let Some(r) = repo {
            conn.execute(
                "DELETE FROM cron_jobs WHERE name = ?1 AND repo_id = (SELECT id FROM repositories WHERE name = ?2) AND builtin = 0",
                rusqlite::params![name, r],
            )?
        } else {
            conn.execute(
                "DELETE FROM cron_jobs WHERE name = ?1 AND repo_id IS NULL AND builtin = 0",
                rusqlite::params![name],
            )?
        };

        if rows_affected == 0 {
            // Diagnostic: determine if the job doesn't exist or is builtin
            let (diag_query, diag_params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) =
                if let Some(r) = repo {
                    (
                        "SELECT builtin FROM cron_jobs WHERE name = ?1 AND repo_id = (SELECT id FROM repositories WHERE name = ?2)",
                        vec![Box::new(name.to_string()), Box::new(r.to_string())],
                    )
                } else {
                    (
                        "SELECT builtin FROM cron_jobs WHERE name = ?1 AND repo_id IS NULL",
                        vec![Box::new(name.to_string())],
                    )
                };

            let diag_refs: Vec<&dyn rusqlite::types::ToSql> =
                diag_params.iter().map(|p| p.as_ref()).collect();
            match conn.query_row(diag_query, diag_refs.as_slice(), |row| row.get::<_, i32>(0)) {
                Ok(builtin) if builtin != 0 => {
                    anyhow::bail!("cannot remove built-in cron job: {name}");
                }
                _ => {
                    anyhow::bail!("cron job not found: {name}");
                }
            }
        }

        Ok(())
    }

    fn cron_update_last_run(&self, id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let rows_affected = self.conn().execute(
            "UPDATE cron_jobs SET last_run_at = ?1 WHERE id = ?2",
            rusqlite::params![now, id],
        )?;
        if rows_affected == 0 {
            anyhow::bail!("cron job not found: {id}");
        }
        Ok(())
    }

    fn cron_reset_last_run(&self, name: &str, repo: Option<&str>) -> Result<()> {
        let rows_affected = if let Some(repo_id) = repo {
            self.conn().execute(
                "UPDATE cron_jobs SET last_run_at = NULL WHERE name = ?1 AND repo_id = ?2",
                rusqlite::params![name, repo_id],
            )?
        } else {
            self.conn().execute(
                "UPDATE cron_jobs SET last_run_at = NULL WHERE name = ?1 AND repo_id IS NULL",
                rusqlite::params![name],
            )?
        };
        if rows_affected == 0 {
            anyhow::bail!("cron job not found: {name}");
        }
        Ok(())
    }

    fn cron_find_due(&self) -> Result<Vec<CronJob>> {
        let conn = self.conn();
        let now = Utc::now();

        let mut stmt = conn.prepare(
            "SELECT id, name, repo_id, schedule_type, schedule_value, script_path, status, builtin, last_run_at, created_at \
             FROM cron_jobs WHERE status = 'active'",
        )?;

        let rows = stmt.query_map([], |row| Ok(map_cron_row(row)))?;

        let mut due_jobs = Vec::new();
        for row in rows {
            let job = row??;
            match &job.schedule {
                CronSchedule::Interval { secs } => {
                    let is_due = if let Some(ref last) = job.last_run_at {
                        if let Ok(last_time) = chrono::DateTime::parse_from_rfc3339(last) {
                            let elapsed = now.signed_duration_since(last_time);
                            elapsed.num_seconds() >= *secs as i64
                        } else {
                            true
                        }
                    } else {
                        true
                    };
                    if is_due {
                        due_jobs.push(job);
                    }
                }
                CronSchedule::Expression { ref cron } => {
                    use cron::Schedule;
                    use std::str::FromStr;

                    let Ok(schedule) = Schedule::from_str(cron) else {
                        // Skip jobs with invalid cron expressions
                        continue;
                    };

                    let is_due = if let Some(ref last) = job.last_run_at {
                        if let Ok(last_time) = chrono::DateTime::parse_from_rfc3339(last) {
                            // Check if there's a scheduled time between last_run and now
                            schedule
                                .after(&last_time.with_timezone(&chrono::Utc))
                                .next()
                                .map(|next| next <= now)
                                .unwrap_or(false)
                        } else {
                            true // Can't parse last_run → treat as due
                        }
                    } else {
                        true // Never run → due
                    };

                    if is_due {
                        due_jobs.push(job);
                    }
                }
            }
        }

        Ok(due_jobs)
    }
}

// ─── Row-mapping helpers ───

/// Converts a `query_row` result into `Ok(Some(x))` / `Ok(None)` / `Err(e)`,
/// collapsing `QueryReturnedNoRows` into `None`.
fn optional_query_row<T>(result: std::result::Result<T, rusqlite::Error>) -> Result<Option<T>> {
    match result {
        Ok(val) => Ok(Some(val)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn map_hitl_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HitlEvent> {
    let severity_str: String = row.get(4)?;
    let status_str: String = row.get(8)?;
    Ok(HitlEvent {
        id: row.get(0)?,
        repo_id: row.get(1)?,
        spec_id: row.get(2)?,
        work_id: row.get(3)?,
        severity: severity_str.parse().map_err(|e: String| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        situation: row.get(5)?,
        context: row.get(6)?,
        options: row.get(7)?,
        status: status_str.parse().map_err(|e: String| {
            rusqlite::Error::FromSqlConversionFailure(
                8,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        created_at: row.get(9)?,
    })
}

fn map_decision_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClawDecision> {
    let decision_type_str: String = row.get(3)?;
    Ok(ClawDecision {
        id: row.get(0)?,
        repo_id: row.get(1)?,
        spec_id: row.get(2)?,
        decision_type: decision_type_str.parse().map_err(|e: String| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        target_work_id: row.get(4)?,
        reasoning: row.get(5)?,
        confidence: 1.0,
        context_json: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn map_spec_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Spec> {
    let status_str: String = row.get(4)?;
    Ok(Spec {
        id: row.get(0)?,
        repo_id: row.get(1)?,
        title: row.get(2)?,
        body: row.get(3)?,
        status: status_str.parse().map_err(|e: String| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        source_path: row.get(5)?,
        test_commands: row.get(6)?,
        acceptance_criteria: row.get(7)?,
        priority: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn map_queue_item_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueueItemRow> {
    let queue_type_str: String = row.get(2)?;
    let phase_str: String = row.get(3)?;
    let task_kind_str: String = row.get(8)?;
    Ok(QueueItemRow {
        work_id: row.get(0)?,
        repo_id: row.get(1)?,
        queue_type: queue_type_str.parse().map_err(|e: String| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        phase: phase_str.parse().map_err(|e: String| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        title: row.get(4)?,
        skip_reason: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        task_kind: task_kind_str.parse().map_err(|e: String| {
            rusqlite::Error::FromSqlConversionFailure(
                8,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        github_number: row.get(9)?,
        metadata_json: row.get(10)?,
        failure_count: row.get::<_, Option<i32>>(11)?.unwrap_or(0),
        escalation_level: row.get::<_, Option<i32>>(12)?.unwrap_or(0),
    })
}

fn map_feedback_pattern_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeedbackPattern> {
    let status_str: String = row.get(7)?;
    Ok(FeedbackPattern {
        id: row.get(0)?,
        repo_id: row.get(1)?,
        pattern_type: row.get(2)?,
        suggestion: row.get(3)?,
        source: row.get(4)?,
        occurrence_count: row.get(5)?,
        confidence: row.get(6)?,
        status: status_str.parse().map_err(|e: String| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        sources_json: row.get(8)?,
        created_at: row.get(9)?,
        last_seen_at: row.get(10)?,
    })
}

fn map_cron_row(row: &rusqlite::Row<'_>) -> Result<CronJob> {
    let schedule_type: String = row.get(3)?;
    let schedule_value: String = row.get(4)?;
    let status_str: String = row.get(6)?;
    let builtin_int: i32 = row.get(7)?;

    let schedule = match schedule_type.as_str() {
        "interval" => CronSchedule::Interval {
            secs: schedule_value.parse().map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("invalid interval: {e}"),
                    )),
                )
            })?,
        },
        "expression" => CronSchedule::Expression {
            cron: schedule_value,
        },
        other => {
            return Err(anyhow::anyhow!("unknown schedule type: {other}"));
        }
    };

    let status: CronStatus = status_str
        .parse()
        .map_err(|e: anyhow::Error| anyhow::anyhow!("invalid status: {e}"))?;

    Ok(CronJob {
        id: row.get(0)?,
        name: row.get(1)?,
        repo_id: row.get(2)?,
        schedule,
        script_path: row.get(5)?,
        status,
        builtin: builtin_int != 0,
        last_run_at: row.get(8)?,
        created_at: row.get(9)?,
    })
}

impl HistoryRepository for Database {
    fn history_insert(&self, entry: &NewHistoryEntry) -> Result<String> {
        let conn = self.conn();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO history \
             (id, source_id, workspace_id, task_kind, status, error_message, duration_ms, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                id,
                entry.source_id,
                entry.workspace_id,
                entry.task_kind,
                entry.status.as_str(),
                entry.error_message,
                entry.duration_ms,
                now,
            ],
        )?;
        Ok(id)
    }

    fn history_count_failures(&self, source_id: &str) -> Result<i64> {
        let conn = self.conn();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM history WHERE source_id = ?1 AND status = 'failed'",
            rusqlite::params![source_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    fn history_list_by_source(&self, source_id: &str, limit: usize) -> Result<Vec<HistoryEntry>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, source_id, workspace_id, task_kind, status, \
             error_message, duration_ms, created_at \
             FROM history WHERE source_id = ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![source_id, limit as i64], map_history_row)?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    fn history_list_by_workspace(
        &self,
        workspace_id: &str,
        limit: usize,
    ) -> Result<Vec<HistoryEntry>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, source_id, workspace_id, task_kind, status, \
             error_message, duration_ms, created_at \
             FROM history WHERE workspace_id = ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![workspace_id, limit as i64],
            map_history_row,
        )?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }
}

fn map_history_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
    let status_str: String = row.get(4)?;
    Ok(HistoryEntry {
        id: row.get(0)?,
        source_id: row.get(1)?,
        workspace_id: row.get(2)?,
        task_kind: row.get(3)?,
        status: status_str.parse().map_err(|e: String| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        error_message: row.get(5)?,
        duration_ms: row.get(6)?,
        created_at: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Database {
        let db = Database::open(std::path::Path::new(":memory:")).unwrap();
        db.initialize().unwrap();
        db
    }

    #[test]
    fn history_insert_and_list_by_source() {
        let db = setup_db();

        let entry = NewHistoryEntry {
            source_id: "issue:org/repo:42".into(),
            workspace_id: "ws-1".into(),
            task_kind: "issue".into(),
            status: HistoryStatus::Completed,
            error_message: None,
            duration_ms: Some(1500),
        };
        let id = db.history_insert(&entry).unwrap();
        assert!(!id.is_empty());

        let entries = db.history_list_by_source("issue:org/repo:42", 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source_id, "issue:org/repo:42");
        assert_eq!(entries[0].status, HistoryStatus::Completed);
        assert_eq!(entries[0].duration_ms, Some(1500));
        assert!(entries[0].error_message.is_none());
    }

    #[test]
    fn history_count_failures() {
        let db = setup_db();

        for status in &[
            HistoryStatus::Failed,
            HistoryStatus::Completed,
            HistoryStatus::Failed,
        ] {
            db.history_insert(&NewHistoryEntry {
                source_id: "issue:org/repo:10".into(),
                workspace_id: "ws-1".into(),
                task_kind: "issue".into(),
                status: status.clone(),
                error_message: if *status == HistoryStatus::Failed {
                    Some("error".into())
                } else {
                    None
                },
                duration_ms: None,
            })
            .unwrap();
        }

        let count = db.history_count_failures("issue:org/repo:10").unwrap();
        assert_eq!(count, 2);

        let count = db.history_count_failures("issue:org/repo:99").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn history_list_by_workspace() {
        let db = setup_db();

        db.history_insert(&NewHistoryEntry {
            source_id: "issue:org/repo:1".into(),
            workspace_id: "ws-a".into(),
            task_kind: "issue".into(),
            status: HistoryStatus::Completed,
            error_message: None,
            duration_ms: None,
        })
        .unwrap();
        db.history_insert(&NewHistoryEntry {
            source_id: "issue:org/repo:2".into(),
            workspace_id: "ws-b".into(),
            task_kind: "pr".into(),
            status: HistoryStatus::Failed,
            error_message: Some("timeout".into()),
            duration_ms: Some(30000),
        })
        .unwrap();

        let ws_a = db.history_list_by_workspace("ws-a", 10).unwrap();
        assert_eq!(ws_a.len(), 1);
        assert_eq!(ws_a[0].source_id, "issue:org/repo:1");

        let ws_b = db.history_list_by_workspace("ws-b", 10).unwrap();
        assert_eq!(ws_b.len(), 1);
        assert_eq!(ws_b[0].status, HistoryStatus::Failed);
        assert_eq!(ws_b[0].error_message.as_deref(), Some("timeout"));
    }

    #[test]
    fn history_persists_across_connections() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");

        {
            let db = Database::open(&db_path).unwrap();
            db.initialize().unwrap();
            db.history_insert(&NewHistoryEntry {
                source_id: "issue:org/repo:5".into(),
                workspace_id: "ws-1".into(),
                task_kind: "issue".into(),
                status: HistoryStatus::Failed,
                error_message: Some("crash".into()),
                duration_ms: Some(500),
            })
            .unwrap();
        }

        {
            let db = Database::open(&db_path).unwrap();
            db.initialize().unwrap();
            let count = db.history_count_failures("issue:org/repo:5").unwrap();
            assert_eq!(count, 1, "failure_count should persist across connections");

            let entries = db.history_list_by_source("issue:org/repo:5", 10).unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].error_message.as_deref(), Some("crash"));
        }
    }

    #[test]
    fn history_skipped_not_counted_as_failure() {
        let db = setup_db();

        db.history_insert(&NewHistoryEntry {
            source_id: "pr:org/repo:7".into(),
            workspace_id: "ws-1".into(),
            task_kind: "pr".into(),
            status: HistoryStatus::Skipped,
            error_message: Some("preflight: issue closed".into()),
            duration_ms: None,
        })
        .unwrap();

        let entries = db.history_list_by_source("pr:org/repo:7", 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, HistoryStatus::Skipped);

        let count = db.history_count_failures("pr:org/repo:7").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn history_list_respects_limit() {
        let db = setup_db();

        for i in 0..5 {
            db.history_insert(&NewHistoryEntry {
                source_id: "issue:org/repo:1".into(),
                workspace_id: "ws-1".into(),
                task_kind: "issue".into(),
                status: HistoryStatus::Completed,
                error_message: None,
                duration_ms: Some(i * 100),
            })
            .unwrap();
        }

        let entries = db.history_list_by_source("issue:org/repo:1", 3).unwrap();
        assert_eq!(entries.len(), 3);
    }
}
