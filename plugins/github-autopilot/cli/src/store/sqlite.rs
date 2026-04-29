//! SQLite adapter for the TaskStore port.
//!
//! Implementation is built incrementally; transactional write methods that
//! span multiple steps may still return [`TaskStoreError::Backend`] until
//! they are wired up to the conformance suite.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::domain::{
    DomainError, Epic, EpicStatus, Event, EventKind, Task, TaskFailureOutcome, TaskId, TaskSource,
    TaskStatus,
};
use crate::ports::task_store::{
    EpicPlan, EpicRepo, EventFilter, EventLog, NewWatchTask, ReconciliationPlan, Result, TaskRepo,
    TaskStoreError, UnblockReport, UpsertOutcome,
};

const SCHEMA_VERSION: u32 = 1;
const V1_SQL: &str = include_str!("migrations/V1__initial.sql");

pub struct SqliteTaskStore {
    conn: Mutex<Connection>,
}

impl SqliteTaskStore {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(backend)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(backend)?;
        Self::init(conn)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(backend)?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(backend)?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(backend)?;
        conn.busy_timeout(std::time::Duration::from_millis(5_000))
            .map_err(backend)?;
        Self::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn migrate(conn: &mut Connection) -> Result<()> {
        let has_meta: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='meta'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if !has_meta {
            conn.execute_batch(V1_SQL).map_err(backend)?;
            return Ok(());
        }

        let found: u32 = conn
            .query_row(
                "SELECT value FROM meta WHERE key='schema_version'",
                [],
                |row| {
                    row.get::<_, String>(0)
                        .map(|s| s.parse::<u32>().unwrap_or(0))
                },
            )
            .map_err(backend)?;

        if found > SCHEMA_VERSION {
            return Err(TaskStoreError::SchemaMismatch {
                found,
                expected: SCHEMA_VERSION,
            });
        }
        Ok(())
    }
}

fn backend(e: rusqlite::Error) -> TaskStoreError {
    TaskStoreError::Backend(e.to_string())
}

fn unimpl(name: &str) -> TaskStoreError {
    TaskStoreError::Backend(format!("sqlite::{name} not yet implemented"))
}

fn epic_from_row(row: &Row<'_>) -> rusqlite::Result<Epic> {
    let status_str: String = row.get("status")?;
    let status = EpicStatus::parse(&status_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("invalid epic status: {status_str}").into(),
        )
    })?;
    Ok(Epic {
        name: row.get("name")?,
        spec_path: PathBuf::from(row.get::<_, String>("spec_path")?),
        branch: row.get("branch")?,
        status,
        created_at: row.get("created_at")?,
        completed_at: row.get("completed_at")?,
    })
}

fn task_from_row(row: &Row<'_>) -> rusqlite::Result<Task> {
    let status_str: String = row.get("status")?;
    let status = TaskStatus::parse(&status_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("invalid task status: {status_str}").into(),
        )
    })?;
    let source_str: String = row.get("source")?;
    let source = TaskSource::parse(&source_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("invalid task source: {source_str}").into(),
        )
    })?;
    Ok(Task {
        id: TaskId::from_raw(row.get::<_, String>("id")?),
        epic_name: row.get("epic_name")?,
        source,
        fingerprint: row.get("fingerprint")?,
        title: row.get("title")?,
        body: row.get("body")?,
        status,
        attempts: row.get::<_, i64>("attempts")? as u32,
        branch: row.get("branch")?,
        pr_number: row.get::<_, Option<i64>>("pr_number")?.map(|n| n as u64),
        escalated_issue: row
            .get::<_, Option<i64>>("escalated_issue")?
            .map(|n| n as u64),
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn event_from_row(row: &Row<'_>) -> rusqlite::Result<Event> {
    let kind_str: String = row.get("kind")?;
    let kind = EventKind::parse(&kind_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("invalid event kind: {kind_str}").into(),
        )
    })?;
    let payload_str: String = row.get("payload")?;
    let payload: serde_json::Value =
        serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Object(Default::default()));
    let task_id: Option<String> = row.get("task_id")?;
    Ok(Event {
        task_id: task_id.map(TaskId::from_raw),
        epic_name: row.get("epic_name")?,
        kind,
        payload,
        at: row.get("at")?,
    })
}

impl SqliteTaskStore {
    fn append_event_with(
        conn: &Connection,
        kind: EventKind,
        epic: Option<&str>,
        task: Option<&TaskId>,
        payload: &serde_json::Value,
        at: DateTime<Utc>,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT INTO events(epic_name, task_id, kind, payload, at) VALUES (?, ?, ?, ?, ?)",
            params![
                epic,
                task.map(|t| t.as_str().to_string()),
                kind.as_str(),
                payload.to_string(),
                at,
            ],
        )?;
        Ok(())
    }
}

impl EpicRepo for SqliteTaskStore {
    fn upsert_epic(&self, epic: &Epic) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "INSERT INTO epics(name, spec_path, branch, status, created_at, completed_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(name) DO UPDATE SET
               spec_path=excluded.spec_path,
               branch=excluded.branch,
               status=excluded.status,
               completed_at=excluded.completed_at",
            params![
                epic.name,
                epic.spec_path.to_string_lossy(),
                epic.branch,
                epic.status.as_str(),
                epic.created_at,
                epic.completed_at,
            ],
        )
        .map_err(backend)?;
        Ok(())
    }

    fn get_epic(&self, name: &str) -> Result<Option<Epic>> {
        let conn = self.conn.lock().expect("poisoned");
        conn.query_row(
            "SELECT name, spec_path, branch, status, created_at, completed_at FROM epics WHERE name=?",
            params![name],
            epic_from_row,
        )
        .optional()
        .map_err(backend)
    }

    fn list_epics(&self, status: Option<EpicStatus>) -> Result<Vec<Epic>> {
        let conn = self.conn.lock().expect("poisoned");
        let (sql, status_str) = match status {
            Some(s) => (
                "SELECT name, spec_path, branch, status, created_at, completed_at
                   FROM epics WHERE status=? ORDER BY name",
                Some(s.as_str().to_string()),
            ),
            None => (
                "SELECT name, spec_path, branch, status, created_at, completed_at
                   FROM epics ORDER BY name",
                None,
            ),
        };
        let mut stmt = conn.prepare(sql).map_err(backend)?;
        let rows = if let Some(s) = status_str {
            stmt.query_map(params![s], epic_from_row)
                .map_err(backend)?
                .collect::<std::result::Result<Vec<_>, _>>()
        } else {
            stmt.query_map([], epic_from_row)
                .map_err(backend)?
                .collect::<std::result::Result<Vec<_>, _>>()
        };
        rows.map_err(backend)
    }

    fn set_epic_status(&self, name: &str, status: EpicStatus, at: DateTime<Utc>) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned");
        let completed_at = if matches!(status, EpicStatus::Completed | EpicStatus::Abandoned) {
            Some(at)
        } else {
            None
        };
        let updated = conn
            .execute(
                "UPDATE epics SET status=?, completed_at=COALESCE(?, completed_at) WHERE name=?",
                params![status.as_str(), completed_at, name],
            )
            .map_err(backend)?;
        if updated == 0 {
            return Err(TaskStoreError::NotFound(format!("epic '{name}'")));
        }
        let kind = match status {
            EpicStatus::Active => EventKind::EpicStarted,
            EpicStatus::Completed => EventKind::EpicCompleted,
            EpicStatus::Abandoned => EventKind::EpicAbandoned,
        };
        SqliteTaskStore::append_event_with(
            &conn,
            kind,
            Some(name),
            None,
            &serde_json::json!({}),
            at,
        )
        .map_err(backend)?;
        Ok(())
    }
}

impl TaskRepo for SqliteTaskStore {
    fn insert_epic_with_tasks(&self, _plan: EpicPlan, _now: DateTime<Utc>) -> Result<()> {
        Err(unimpl("insert_epic_with_tasks"))
    }

    fn get_task(&self, id: &TaskId) -> Result<Option<Task>> {
        let conn = self.conn.lock().expect("poisoned");
        conn.query_row(
            "SELECT id, epic_name, source, fingerprint, title, body, status, attempts,
                    branch, pr_number, escalated_issue, created_at, updated_at
               FROM tasks WHERE id=?",
            params![id.as_str()],
            task_from_row,
        )
        .optional()
        .map_err(backend)
    }

    fn list_tasks_by_epic(&self, epic: &str, status: Option<TaskStatus>) -> Result<Vec<Task>> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = if status.is_some() {
            conn.prepare(
                "SELECT id, epic_name, source, fingerprint, title, body, status, attempts,
                        branch, pr_number, escalated_issue, created_at, updated_at
                   FROM tasks WHERE epic_name=? AND status=? ORDER BY created_at, id",
            )
        } else {
            conn.prepare(
                "SELECT id, epic_name, source, fingerprint, title, body, status, attempts,
                        branch, pr_number, escalated_issue, created_at, updated_at
                   FROM tasks WHERE epic_name=? ORDER BY created_at, id",
            )
        }
        .map_err(backend)?;
        let rows = if let Some(s) = status {
            stmt.query_map(params![epic, s.as_str()], task_from_row)
                .map_err(backend)?
                .collect::<std::result::Result<Vec<_>, _>>()
        } else {
            stmt.query_map(params![epic], task_from_row)
                .map_err(backend)?
                .collect::<std::result::Result<Vec<_>, _>>()
        };
        rows.map_err(backend)
    }

    fn find_by_fingerprint(&self, epic: &str, fingerprint: &str) -> Result<Option<Task>> {
        let conn = self.conn.lock().expect("poisoned");
        conn.query_row(
            "SELECT id, epic_name, source, fingerprint, title, body, status, attempts,
                    branch, pr_number, escalated_issue, created_at, updated_at
               FROM tasks WHERE epic_name=? AND fingerprint=? LIMIT 1",
            params![epic, fingerprint],
            task_from_row,
        )
        .optional()
        .map_err(backend)
    }

    fn upsert_watch_task(&self, _task: NewWatchTask, _now: DateTime<Utc>) -> Result<UpsertOutcome> {
        Err(unimpl("upsert_watch_task"))
    }

    fn claim_next_task(&self, _epic: &str, _now: DateTime<Utc>) -> Result<Option<Task>> {
        Err(unimpl("claim_next_task"))
    }

    fn complete_task_and_unblock(
        &self,
        _id: &TaskId,
        _pr_number: u64,
        _now: DateTime<Utc>,
    ) -> Result<UnblockReport> {
        Err(unimpl("complete_task_and_unblock"))
    }

    fn mark_task_failed(
        &self,
        _id: &TaskId,
        _max_attempts: u32,
        _now: DateTime<Utc>,
    ) -> Result<TaskFailureOutcome> {
        Err(unimpl("mark_task_failed"))
    }

    fn escalate_task(&self, id: &TaskId, issue_number: u64, now: DateTime<Utc>) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned");
        let updated = conn
            .execute(
                "UPDATE tasks SET escalated_issue=?, updated_at=? WHERE id=?",
                params![issue_number as i64, now, id.as_str()],
            )
            .map_err(backend)?;
        if updated == 0 {
            return Err(TaskStoreError::NotFound(format!("task '{id}'")));
        }
        let epic_name: String = conn
            .query_row(
                "SELECT epic_name FROM tasks WHERE id=?",
                params![id.as_str()],
                |row| row.get(0),
            )
            .map_err(backend)?;
        SqliteTaskStore::append_event_with(
            &conn,
            EventKind::TaskEscalated,
            Some(&epic_name),
            Some(id),
            &serde_json::json!({"issue": issue_number}),
            now,
        )
        .map_err(backend)?;
        Ok(())
    }

    fn revert_to_ready(&self, id: &TaskId, now: DateTime<Utc>) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned");
        let cur: Option<String> = conn
            .query_row(
                "SELECT status FROM tasks WHERE id=?",
                params![id.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)?;
        let cur = cur.ok_or_else(|| TaskStoreError::NotFound(format!("task '{id}'")))?;
        let cur_status = TaskStatus::parse(&cur)
            .ok_or_else(|| TaskStoreError::Backend(format!("invalid stored task status: {cur}")))?;
        if matches!(cur_status, TaskStatus::Done | TaskStatus::Escalated) {
            return Err(
                DomainError::IllegalTransition(id.clone(), cur_status, TaskStatus::Ready).into(),
            );
        }
        conn.execute(
            "UPDATE tasks SET status='ready', updated_at=? WHERE id=?",
            params![now, id.as_str()],
        )
        .map_err(backend)?;
        Ok(())
    }

    fn force_status(&self, id: &TaskId, new_status: TaskStatus, now: DateTime<Utc>) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned");
        let updated = conn
            .execute(
                "UPDATE tasks SET status=?, updated_at=? WHERE id=?",
                params![new_status.as_str(), now, id.as_str()],
            )
            .map_err(backend)?;
        if updated == 0 {
            return Err(TaskStoreError::NotFound(format!("task '{id}'")));
        }
        Ok(())
    }

    fn apply_reconciliation(&self, _plan: ReconciliationPlan, _now: DateTime<Utc>) -> Result<()> {
        Err(unimpl("apply_reconciliation"))
    }

    fn list_deps(&self, task_id: &TaskId) -> Result<Vec<TaskId>> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn
            .prepare("SELECT depends_on FROM task_deps WHERE task_id=?")
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![task_id.as_str()], |row| {
                row.get::<_, String>(0).map(TaskId::from_raw)
            })
            .map_err(backend)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(backend)?;
        Ok(rows)
    }
}

impl EventLog for SqliteTaskStore {
    fn append_event(&self, event: &Event) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned");
        SqliteTaskStore::append_event_with(
            &conn,
            event.kind,
            event.epic_name.as_deref(),
            event.task_id.as_ref(),
            &event.payload,
            event.at,
        )
        .map_err(backend)?;
        Ok(())
    }

    fn list_events(&self, filter: EventFilter) -> Result<Vec<Event>> {
        let conn = self.conn.lock().expect("poisoned");
        let mut sql =
            String::from("SELECT epic_name, task_id, kind, payload, at FROM events WHERE 1=1");
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(epic) = &filter.epic {
            sql.push_str(" AND epic_name=?");
            binds.push(Box::new(epic.clone()));
        }
        if let Some(task) = &filter.task {
            sql.push_str(" AND task_id=?");
            binds.push(Box::new(task.as_str().to_string()));
        }
        if !filter.kinds.is_empty() {
            sql.push_str(" AND kind IN (");
            for (i, k) in filter.kinds.iter().enumerate() {
                if i > 0 {
                    sql.push(',');
                }
                sql.push('?');
                binds.push(Box::new(k.as_str().to_string()));
            }
            sql.push(')');
        }
        if let Some(since) = filter.since {
            sql.push_str(" AND at >= ?");
            binds.push(Box::new(since));
        }
        sql.push_str(" ORDER BY id ASC");
        if let Some(limit) = filter.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }

        let mut stmt = conn.prepare(&sql).map_err(backend)?;
        let bind_refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
        let rows = stmt
            .query_map(bind_refs.as_slice(), event_from_row)
            .map_err(backend)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(backend)?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_initializes_schema_v1() {
        let store = SqliteTaskStore::open_in_memory().expect("open");
        let conn = store.conn.lock().unwrap();
        let v: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key='schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(v, "1");
    }

    #[test]
    fn rejects_unknown_higher_version() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(V1_SQL).unwrap();
        conn.execute("UPDATE meta SET value='999' WHERE key='schema_version'", [])
            .unwrap();
        let err = match SqliteTaskStore::init(conn) {
            Err(e) => e,
            Ok(_) => panic!("expected SchemaMismatch error"),
        };
        match err {
            TaskStoreError::SchemaMismatch { found, expected } => {
                assert_eq!(found, 999);
                assert_eq!(expected, 1);
            }
            other => panic!("expected SchemaMismatch, got {other:?}"),
        }
    }

    #[test]
    fn upsert_and_get_epic_round_trip() {
        let store = SqliteTaskStore::open_in_memory().unwrap();
        let epic = Epic {
            name: "e".to_string(),
            spec_path: PathBuf::from("spec/x.md"),
            branch: "epic/e".to_string(),
            status: EpicStatus::Active,
            created_at: chrono::Utc::now(),
            completed_at: None,
        };
        store.upsert_epic(&epic).unwrap();
        let got = store.get_epic("e").unwrap().unwrap();
        assert_eq!(got.branch, "epic/e");
        assert_eq!(got.status, EpicStatus::Active);
    }
}
