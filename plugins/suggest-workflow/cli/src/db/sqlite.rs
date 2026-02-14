use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

use super::perspectives;
use super::repository::*;
use super::schema;

pub struct SqliteStore {
    conn: Connection,
    perspectives: Vec<PerspectiveInfo>,
}

impl SqliteStore {
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create DB directory: {}", parent.display()))?;
        }

        let conn = Connection::open(db_path)
            .with_context(|| format!("failed to open DB: {}", db_path.display()))?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        let perspectives = perspectives::register_perspectives();

        Ok(Self { conn, perspectives })
    }
}

impl IndexRepository for SqliteStore {
    fn initialize(&self) -> Result<()> {
        self.conn
            .execute_batch(schema::DDL)
            .context("failed to initialize schema")?;

        // Insert schema_version if not present
        self.conn.execute(
            "INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', ?1)",
            params![schema::SCHEMA_VERSION.to_string()],
        )?;

        Ok(())
    }

    fn check_session(&self, file_path: &Path, size: u64, mtime: i64) -> Result<SessionStatus> {
        let path_str = file_path.to_string_lossy();
        match self.conn.query_row(
            "SELECT file_size, file_mtime FROM sessions WHERE file_path = ?1",
            params![path_str.as_ref()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        ) {
            Ok((saved_size, saved_mtime)) => {
                if size as i64 == saved_size && mtime == saved_mtime {
                    Ok(SessionStatus::Unchanged)
                } else {
                    Ok(SessionStatus::Changed)
                }
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(SessionStatus::New),
            Err(e) => Err(e.into()),
        }
    }

    fn upsert_session(&self, session: &SessionData) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        // Delete existing data (CASCADE deletes prompts, tool_uses, file_edits)
        tx.execute("DELETE FROM sessions WHERE id = ?1", params![&session.id])?;

        let now = chrono::Utc::now().to_rfc3339();

        // Insert session
        tx.execute(
            "INSERT INTO sessions (id, file_path, file_size, file_mtime, first_ts, last_ts, prompt_count, tool_use_count, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                &session.id,
                &session.file_path,
                session.file_size as i64,
                session.file_mtime,
                session.first_ts,
                session.last_ts,
                session.prompt_count as i64,
                session.tool_use_count as i64,
                &now,
            ],
        )?;

        // Insert prompts
        {
            let mut stmt = tx.prepare(
                "INSERT INTO prompts (session_id, text, timestamp, char_count) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for p in &session.prompts {
                stmt.execute(params![&session.id, &p.text, p.timestamp, p.char_count as i64])?;
            }
        }

        // Insert tool_uses
        {
            let mut stmt = tx.prepare(
                "INSERT INTO tool_uses (session_id, seq_order, tool_name, classified_name, timestamp, input_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for t in &session.tool_uses {
                stmt.execute(params![
                    &session.id,
                    t.seq_order as i64,
                    &t.tool_name,
                    &t.classified_name,
                    t.timestamp,
                    &t.input_json,
                ])?;
            }
        }

        // Insert file_edits (link to tool_uses by matching seq_order)
        {
            let mut stmt = tx.prepare(
                "INSERT INTO file_edits (session_id, tool_use_id, file_path, timestamp)
                 VALUES (?1, (SELECT id FROM tool_uses WHERE session_id = ?1 AND seq_order = ?2), ?3, ?4)",
            )?;
            for f in &session.file_edits {
                stmt.execute(params![
                    &session.id,
                    f.tool_use_seq as i64,
                    &f.file_path,
                    f.timestamp,
                ])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    fn remove_stale_sessions(&self, existing_paths: &[&Path]) -> Result<u64> {
        // Get all session file_paths from DB
        let mut stmt = self
            .conn
            .prepare("SELECT id, file_path FROM sessions")?;

        let db_sessions: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        let existing_set: std::collections::HashSet<String> = existing_paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        let mut deleted = 0u64;
        for (id, path) in &db_sessions {
            if !existing_set.contains(path) {
                self.conn
                    .execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
                deleted += 1;
            }
        }

        Ok(deleted)
    }

    fn rebuild_derived_tables(&self) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        // Clear derived tables
        tx.execute_batch(
            "DELETE FROM tool_transitions;
             DELETE FROM weekly_buckets;
             DELETE FROM file_hotspots;
             DELETE FROM session_links;",
        )?;

        // tool_transitions: count consecutive tool pairs across all sessions
        tx.execute_batch(
            "INSERT INTO tool_transitions (from_tool, to_tool, count, probability)
             SELECT from_tool, to_tool, pair_count,
                    CAST(pair_count AS REAL) / from_total
             FROM (
                 SELECT t1.classified_name AS from_tool,
                        t2.classified_name AS to_tool,
                        COUNT(*) AS pair_count
                 FROM tool_uses t1
                 JOIN tool_uses t2
                   ON t1.session_id = t2.session_id
                  AND t2.seq_order = t1.seq_order + 1
                 GROUP BY t1.classified_name, t2.classified_name
             ) pairs
             JOIN (
                 SELECT t1.classified_name AS tool,
                        COUNT(*) AS from_total
                 FROM tool_uses t1
                 JOIN tool_uses t2
                   ON t1.session_id = t2.session_id
                  AND t2.seq_order = t1.seq_order + 1
                 GROUP BY t1.classified_name
             ) totals ON pairs.from_tool = totals.tool;",
        )?;

        // file_hotspots: aggregate file_edits
        tx.execute_batch(
            "INSERT INTO file_hotspots (file_path, edit_count, session_count)
             SELECT file_path,
                    COUNT(*) AS edit_count,
                    COUNT(DISTINCT session_id) AS session_count
             FROM file_edits
             GROUP BY file_path;",
        )?;

        // weekly_buckets: aggregate tool_uses by ISO week
        tx.execute_batch(
            "INSERT INTO weekly_buckets (week_start, tool_name, count, session_count)
             SELECT strftime('%Y-%m-%d', datetime(timestamp / 1000, 'unixepoch'), 'weekday 0', '-6 days') AS week_start,
                    classified_name AS tool_name,
                    COUNT(*) AS count,
                    COUNT(DISTINCT session_id) AS session_count
             FROM tool_uses
             WHERE timestamp IS NOT NULL
             GROUP BY week_start, classified_name;",
        )?;

        // session_links: sessions sharing edited files
        tx.execute_batch(
            "INSERT INTO session_links (session_a, session_b, shared_files, overlap_ratio, time_gap_minutes)
             SELECT a.session_id, b.session_id,
                    COUNT(DISTINCT a.file_path) AS shared_files,
                    CAST(COUNT(DISTINCT a.file_path) AS REAL) /
                        MAX(
                            (SELECT COUNT(DISTINCT file_path) FROM file_edits WHERE session_id = a.session_id),
                            (SELECT COUNT(DISTINCT file_path) FROM file_edits WHERE session_id = b.session_id)
                        ) AS overlap_ratio,
                    ABS(COALESCE(sa.first_ts, 0) - COALESCE(sb.first_ts, 0)) / 60000 AS time_gap_minutes
             FROM file_edits a
             JOIN file_edits b ON a.file_path = b.file_path AND a.session_id < b.session_id
             JOIN sessions sa ON sa.id = a.session_id
             JOIN sessions sb ON sb.id = b.session_id
             GROUP BY a.session_id, b.session_id
             HAVING shared_files >= 1;",
        )?;

        tx.commit()?;
        Ok(())
    }

    fn update_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    fn schema_version(&self) -> Result<Option<u32>> {
        match self.conn.query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        ) {
            Ok(v) => Ok(v.parse().ok()),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

// --- QueryRepository implementation ---

impl QueryRepository for SqliteStore {
    fn list_perspectives(&self) -> Result<Vec<PerspectiveInfo>> {
        Ok(self.perspectives.clone())
    }

    fn query(&self, perspective: &str, params: &QueryParams) -> Result<serde_json::Value> {
        let info = self
            .perspectives
            .iter()
            .find(|p| p.name == perspective)
            .ok_or_else(|| anyhow::anyhow!("unknown perspective: {}", perspective))?;

        // Validate required parameters
        for param_def in &info.params {
            if param_def.required && !params.contains_key(&param_def.name) {
                anyhow::bail!(
                    "missing required param: --param {}=<value>",
                    param_def.name
                );
            }
        }

        // Replace :name with ?N and build bind values
        let (bound_sql, bind_values) = bind_named_params(&info.sql, &info.params, params)?;

        // Execute and collect results as JSON
        let mut stmt = self.conn.prepare(&bound_sql)?;
        stmt_to_json(&mut stmt, &bind_values)
    }

    fn execute_sql(&self, sql: &str) -> Result<serde_json::Value> {
        // Validate: only SELECT allowed
        let trimmed = sql.trim().to_uppercase();
        if !trimmed.starts_with("SELECT") {
            anyhow::bail!("only SELECT statements are allowed in custom SQL");
        }

        let mut stmt = self.conn.prepare(sql)?;
        stmt_to_json(&mut stmt, &[])
    }
}

/// Replace `:name` placeholders with `?N` positional params and build bind values array.
fn bind_named_params(
    sql: &str,
    defs: &[ParamDef],
    params: &QueryParams,
) -> Result<(String, Vec<rusqlite::types::Value>)> {
    let mut bound_sql = sql.to_string();
    let mut values = Vec::new();

    for (i, def) in defs.iter().enumerate() {
        let raw_value = params
            .get(&def.name)
            .cloned()
            .or_else(|| def.default.clone())
            .ok_or_else(|| anyhow::anyhow!("missing param: {}", def.name))?;

        let placeholder = format!(":{}", def.name);
        bound_sql = bound_sql.replace(&placeholder, &format!("?{}", i + 1));
        values.push(coerce_value(&raw_value, &def.param_type)?);
    }

    Ok((bound_sql, values))
}

/// Coerce a string value to the appropriate rusqlite Value based on ParamType.
fn coerce_value(raw: &str, param_type: &ParamType) -> Result<rusqlite::types::Value> {
    match param_type {
        ParamType::Integer => {
            let n: i64 = raw
                .parse()
                .with_context(|| format!("expected integer, got '{}'", raw))?;
            Ok(rusqlite::types::Value::Integer(n))
        }
        ParamType::Float => {
            let f: f64 = raw
                .parse()
                .with_context(|| format!("expected float, got '{}'", raw))?;
            Ok(rusqlite::types::Value::Real(f))
        }
        ParamType::Text => Ok(rusqlite::types::Value::Text(raw.to_string())),
        ParamType::Date => {
            // Validate YYYY-MM-DD format
            chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                .with_context(|| format!("expected date YYYY-MM-DD, got '{}'", raw))?;
            Ok(rusqlite::types::Value::Text(raw.to_string()))
        }
    }
}

/// Execute a prepared statement with bind values and return results as JSON array.
fn stmt_to_json(
    stmt: &mut rusqlite::Statement,
    bind_values: &[rusqlite::types::Value],
) -> Result<serde_json::Value> {
    let column_names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|s| s.to_string())
        .collect();

    let params: Vec<&dyn rusqlite::types::ToSql> = bind_values
        .iter()
        .map(|v| v as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = stmt.query_map(params.as_slice(), |row| {
        let mut map = serde_json::Map::new();
        for (i, name) in column_names.iter().enumerate() {
            let val: rusqlite::types::Value = row.get(i)?;
            let json_val = match val {
                rusqlite::types::Value::Null => serde_json::Value::Null,
                rusqlite::types::Value::Integer(n) => serde_json::Value::Number(n.into()),
                rusqlite::types::Value::Real(f) => serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null),
                rusqlite::types::Value::Text(s) => serde_json::Value::String(s),
                rusqlite::types::Value::Blob(_) => serde_json::Value::Null,
            };
            map.insert(name.clone(), json_val);
        }
        Ok(serde_json::Value::Object(map))
    })?;

    let result: Vec<serde_json::Value> = rows.collect::<Result<Vec<_>, _>>()?;
    Ok(serde_json::Value::Array(result))
}
