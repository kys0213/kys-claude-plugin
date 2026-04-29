//! SQLite adapter for the TaskStore port.
//!
//! Implementation is built incrementally; methods that are not yet wired up
//! return [`TaskStoreError::Backend`] so the lib still compiles while the
//! conformance suite drives in remaining behaviour.

use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use rusqlite::Connection;

use crate::domain::{Epic, EpicStatus, Event, Task, TaskFailureOutcome, TaskId, TaskStatus};
use crate::ports::task_store::{
    EpicPlan, EpicRepo, EventFilter, EventLog, NewWatchTask, ReconciliationPlan, Result, TaskRepo,
    TaskStoreError, UnblockReport, UpsertOutcome,
};

const SCHEMA_VERSION: u32 = 1;
const V1_SQL: &str = include_str!("migrations/V1__initial.sql");

pub struct SqliteTaskStore {
    #[allow(dead_code)] // populated incrementally as methods land
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

impl EpicRepo for SqliteTaskStore {
    fn upsert_epic(&self, _epic: &Epic) -> Result<()> {
        Err(unimpl("upsert_epic"))
    }

    fn get_epic(&self, name: &str) -> Result<Option<Epic>> {
        let _ = name;
        Err(unimpl("get_epic"))
    }

    fn list_epics(&self, _status: Option<EpicStatus>) -> Result<Vec<Epic>> {
        Err(unimpl("list_epics"))
    }

    fn set_epic_status(&self, _name: &str, _status: EpicStatus, _at: DateTime<Utc>) -> Result<()> {
        Err(unimpl("set_epic_status"))
    }
}

impl TaskRepo for SqliteTaskStore {
    fn insert_epic_with_tasks(&self, _plan: EpicPlan, _now: DateTime<Utc>) -> Result<()> {
        Err(unimpl("insert_epic_with_tasks"))
    }

    fn get_task(&self, _id: &TaskId) -> Result<Option<Task>> {
        Err(unimpl("get_task"))
    }

    fn list_tasks_by_epic(&self, _epic: &str, _status: Option<TaskStatus>) -> Result<Vec<Task>> {
        Err(unimpl("list_tasks_by_epic"))
    }

    fn find_by_fingerprint(&self, _epic: &str, _fingerprint: &str) -> Result<Option<Task>> {
        Err(unimpl("find_by_fingerprint"))
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

    fn escalate_task(&self, _id: &TaskId, _issue_number: u64, _now: DateTime<Utc>) -> Result<()> {
        Err(unimpl("escalate_task"))
    }

    fn revert_to_ready(&self, _id: &TaskId, _now: DateTime<Utc>) -> Result<()> {
        Err(unimpl("revert_to_ready"))
    }

    fn force_status(
        &self,
        _id: &TaskId,
        _new_status: TaskStatus,
        _now: DateTime<Utc>,
    ) -> Result<()> {
        Err(unimpl("force_status"))
    }

    fn apply_reconciliation(&self, _plan: ReconciliationPlan, _now: DateTime<Utc>) -> Result<()> {
        Err(unimpl("apply_reconciliation"))
    }

    fn list_deps(&self, _task_id: &TaskId) -> Result<Vec<TaskId>> {
        Err(unimpl("list_deps"))
    }
}

impl EventLog for SqliteTaskStore {
    fn append_event(&self, _event: &Event) -> Result<()> {
        Err(unimpl("append_event"))
    }

    fn list_events(&self, _filter: EventFilter) -> Result<Vec<Event>> {
        Err(unimpl("list_events"))
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
}
