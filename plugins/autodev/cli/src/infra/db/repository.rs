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
                 s.test_commands, s.acceptance_criteria, s.created_at, s.updated_at \
                 FROM specs s JOIN repositories r ON s.repo_id = r.id \
                 WHERE r.name = ?1 ORDER BY s.created_at DESC"
                        .to_string(),
                    vec![Box::new(name.to_string())],
                )
            } else {
                (
                    "SELECT id, repo_id, title, body, status, source_path, \
                 test_commands, acceptance_criteria, created_at, updated_at \
                 FROM specs ORDER BY created_at DESC"
                        .to_string(),
                    vec![],
                )
            };

        let mut stmt = conn.prepare(&query)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            let status_str: String = row.get(4)?;
            Ok(Spec {
                id: row.get(0)?,
                repo_id: row.get(1)?,
                title: row.get(2)?,
                body: row.get(3)?,
                status: status_str.parse().unwrap_or(SpecStatus::Active),
                source_path: row.get(5)?,
                test_commands: row.get(6)?,
                acceptance_criteria: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn spec_show(&self, id: &str) -> Result<Option<Spec>> {
        let conn = self.conn();
        let result = conn.query_row(
            "SELECT id, repo_id, title, body, status, source_path, \
             test_commands, acceptance_criteria, created_at, updated_at \
             FROM specs WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                let status_str: String = row.get(4)?;
                Ok(Spec {
                    id: row.get(0)?,
                    repo_id: row.get(1)?,
                    title: row.get(2)?,
                    body: row.get(3)?,
                    status: status_str.parse().unwrap_or(SpecStatus::Active),
                    source_path: row.get(5)?,
                    test_commands: row.get(6)?,
                    acceptance_criteria: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            },
        );
        match result {
            Ok(spec) => Ok(Some(spec)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
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
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            let severity_str: String = row.get(4)?;
            let status_str: String = row.get(8)?;
            Ok(HitlEvent {
                id: row.get(0)?,
                repo_id: row.get(1)?,
                spec_id: row.get(2)?,
                work_id: row.get(3)?,
                severity: HitlSeverity::from_str_lowercase(&severity_str)
                    .unwrap_or(HitlSeverity::Medium),
                situation: row.get(5)?,
                context: row.get(6)?,
                options: row.get(7)?,
                status: HitlStatus::from_str_lowercase(&status_str).unwrap_or(HitlStatus::Pending),
                created_at: row.get(9)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn hitl_show(&self, id: &str) -> Result<Option<HitlEvent>> {
        let conn = self.conn();
        let result = conn.query_row(
            "SELECT id, repo_id, spec_id, work_id, severity, situation, context, options, status, created_at \
             FROM hitl_events WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                let severity_str: String = row.get(4)?;
                let status_str: String = row.get(8)?;
                Ok(HitlEvent {
                    id: row.get(0)?,
                    repo_id: row.get(1)?,
                    spec_id: row.get(2)?,
                    work_id: row.get(3)?,
                    severity: HitlSeverity::from_str_lowercase(&severity_str)
                        .unwrap_or(HitlSeverity::Medium),
                    situation: row.get(5)?,
                    context: row.get(6)?,
                    options: row.get(7)?,
                    status: HitlStatus::from_str_lowercase(&status_str)
                        .unwrap_or(HitlStatus::Pending),
                    created_at: row.get(9)?,
                })
            },
        );

        match result {
            Ok(event) => Ok(Some(event)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
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
        conn.execute(
            "UPDATE hitl_events SET status = ?1 WHERE id = ?2",
            rusqlite::params![status.to_string(), id],
        )?;
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
}
