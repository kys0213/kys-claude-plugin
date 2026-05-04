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
    DomainError, Epic, EpicStatus, Event, EventKind, Task, TaskFailureOutcome, TaskGraph, TaskId,
    TaskSource, TaskStatus,
};
use crate::ports::task_store::{
    EpicPlan, EpicRepo, EventFilter, EventLog, NewWatchTask, ReconciliationPlan, Result,
    SuppressionRepo, TaskRepo, TaskStoreError, UnblockReport, UpsertOutcome,
};

const SCHEMA_VERSION: u32 = 2;
const V1_SQL: &str = include_str!("migrations/V1__initial.sql");
const V2_SQL: &str = include_str!("migrations/V2__lookup_indexes.sql");

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
            conn.execute_batch(V2_SQL).map_err(backend)?;
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

        if found < 2 {
            conn.execute_batch(V2_SQL).map_err(backend)?;
        }
        Ok(())
    }
}

fn backend(e: rusqlite::Error) -> TaskStoreError {
    TaskStoreError::Backend(e.to_string())
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

    fn find_active_by_spec_path(&self, spec_path: &Path) -> Result<Option<Epic>> {
        let conn = self.conn.lock().expect("poisoned");
        let path_str = spec_path.to_string_lossy().to_string();
        let mut stmt = conn
            .prepare(
                "SELECT * FROM epics WHERE status='active' AND spec_path=? ORDER BY name LIMIT 2",
            )
            .map_err(backend)?;
        let mut rows = stmt
            .query_map(params![path_str], epic_from_row)
            .map_err(backend)?;
        let first = match rows.next() {
            None => return Ok(None),
            Some(r) => r.map_err(backend)?,
        };
        if let Some(second) = rows.next() {
            let second = second.map_err(backend)?;
            return Err(DomainError::Inconsistency(format!(
                "active epics share spec_path {path_str:?}: {} vs {}",
                first.name, second.name
            ))
            .into());
        }
        Ok(Some(first))
    }
}

impl TaskRepo for SqliteTaskStore {
    fn insert_epic_with_tasks(&self, plan: EpicPlan, now: DateTime<Utc>) -> Result<()> {
        let mut seen = std::collections::BTreeSet::new();
        for t in &plan.tasks {
            if !seen.insert(t.id.clone()) {
                return Err(DomainError::DuplicateTaskId(t.id.clone()).into());
            }
        }
        for (a, b) in &plan.deps {
            if !seen.contains(a) {
                return Err(DomainError::UnknownDepTarget(a.clone()).into());
            }
            if !seen.contains(b) {
                return Err(DomainError::UnknownDepTarget(b.clone()).into());
            }
        }
        let graph = TaskGraph::build(plan.deps.iter().cloned());
        if let Some(cycle) = graph.detect_cycle() {
            return Err(DomainError::DepCycle(cycle).into());
        }

        let mut conn = self.conn.lock().expect("poisoned");
        let tx = conn.transaction().map_err(backend)?;

        let exists: Option<String> = tx
            .query_row(
                "SELECT status FROM epics WHERE name=?",
                params![plan.epic.name],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)?;
        if let Some(status_str) = exists {
            let status = EpicStatus::parse(&status_str).unwrap_or(EpicStatus::Active);
            return Err(DomainError::EpicAlreadyExists(plan.epic.name.clone(), status).into());
        }

        tx.execute(
            "INSERT INTO epics(name, spec_path, branch, status, created_at, completed_at)
             VALUES (?, ?, ?, 'active', ?, NULL)",
            params![
                plan.epic.name,
                plan.epic.spec_path.to_string_lossy(),
                plan.epic.branch,
                plan.epic.created_at,
            ],
        )
        .map_err(backend)?;

        for nt in &plan.tasks {
            tx.execute(
                "INSERT INTO tasks(id, epic_name, source, fingerprint, title, body,
                                   status, attempts, branch, pr_number, escalated_issue,
                                   created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, 'pending', 0, NULL, NULL, NULL, ?, ?)",
                params![
                    nt.id.as_str(),
                    plan.epic.name,
                    nt.source.as_str(),
                    nt.fingerprint,
                    nt.title,
                    nt.body,
                    now,
                    now,
                ],
            )
            .map_err(backend)?;
        }

        for (a, b) in &plan.deps {
            tx.execute(
                "INSERT INTO task_deps(task_id, depends_on) VALUES (?, ?)",
                params![a.as_str(), b.as_str()],
            )
            .map_err(backend)?;
        }

        tx.execute(
            "UPDATE tasks SET status='ready', updated_at=?
              WHERE epic_name=? AND status='pending'
                AND id NOT IN (SELECT task_id FROM task_deps)",
            params![now, plan.epic.name],
        )
        .map_err(backend)?;

        SqliteTaskStore::append_event_with(
            &tx,
            EventKind::EpicStarted,
            Some(&plan.epic.name),
            None,
            &serde_json::json!({}),
            now,
        )
        .map_err(backend)?;
        for nt in &plan.tasks {
            SqliteTaskStore::append_event_with(
                &tx,
                EventKind::TaskInserted,
                Some(&plan.epic.name),
                Some(&nt.id),
                &serde_json::json!({"source": nt.source.as_str()}),
                now,
            )
            .map_err(backend)?;
        }

        tx.commit().map_err(backend)?;
        Ok(())
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

    fn upsert_watch_task(&self, task: NewWatchTask, now: DateTime<Utc>) -> Result<UpsertOutcome> {
        let mut conn = self.conn.lock().expect("poisoned");
        let tx = conn.transaction().map_err(backend)?;

        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM tasks WHERE epic_name=? AND fingerprint=? LIMIT 1",
                params![task.epic_name, task.fingerprint],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)?;

        if let Some(existing_id) = existing {
            SqliteTaskStore::append_event_with(
                &tx,
                EventKind::WatchDuplicate,
                Some(&task.epic_name),
                Some(&TaskId::from_raw(existing_id.clone())),
                &serde_json::json!({"fingerprint": task.fingerprint}),
                now,
            )
            .map_err(backend)?;
            tx.commit().map_err(backend)?;
            return Ok(UpsertOutcome::DuplicateFingerprint(TaskId::from_raw(
                existing_id,
            )));
        }

        tx.execute(
            "INSERT INTO tasks(id, epic_name, source, fingerprint, title, body,
                               status, attempts, branch, pr_number, escalated_issue,
                               created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 'ready', 0, NULL, NULL, NULL, ?, ?)",
            params![
                task.id.as_str(),
                task.epic_name,
                task.source.as_str(),
                task.fingerprint,
                task.title,
                task.body,
                now,
                now,
            ],
        )
        .map_err(backend)?;

        SqliteTaskStore::append_event_with(
            &tx,
            EventKind::TaskInserted,
            Some(&task.epic_name),
            Some(&task.id),
            &serde_json::json!({"source": task.source.as_str(), "fingerprint": task.fingerprint}),
            now,
        )
        .map_err(backend)?;

        tx.commit().map_err(backend)?;
        Ok(UpsertOutcome::Inserted(task.id))
    }

    fn claim_next_task(&self, epic: &str, now: DateTime<Utc>) -> Result<Option<Task>> {
        let mut conn = self.conn.lock().expect("poisoned");
        let tx = conn.transaction().map_err(backend)?;

        let candidate_id: Option<String> = tx
            .query_row(
                "SELECT t.id FROM tasks t
                  WHERE t.epic_name = ? AND t.status = 'ready'
                    AND NOT EXISTS (
                      SELECT 1 FROM task_deps d
                      JOIN tasks dep ON dep.id = d.depends_on
                      WHERE d.task_id = t.id AND dep.status != 'done'
                    )
                  ORDER BY t.created_at, t.id
                  LIMIT 1",
                params![epic],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)?;

        let id = match candidate_id {
            Some(id) => id,
            None => return Ok(None),
        };

        let updated = tx
            .execute(
                "UPDATE tasks SET status='wip', attempts = attempts + 1, updated_at=?
                  WHERE id=? AND status='ready'",
                params![now, id],
            )
            .map_err(backend)?;
        if updated != 1 {
            return Ok(None);
        }

        let task = tx
            .query_row(
                "SELECT id, epic_name, source, fingerprint, title, body, status, attempts,
                        branch, pr_number, escalated_issue, created_at, updated_at
                   FROM tasks WHERE id=?",
                params![id],
                task_from_row,
            )
            .map_err(backend)?;

        SqliteTaskStore::append_event_with(
            &tx,
            EventKind::TaskClaimed,
            Some(epic),
            Some(&task.id),
            &serde_json::json!({"attempts": task.attempts}),
            now,
        )
        .map_err(backend)?;

        tx.commit().map_err(backend)?;
        Ok(Some(task))
    }

    fn complete_task_and_unblock(
        &self,
        id: &TaskId,
        pr_number: u64,
        now: DateTime<Utc>,
    ) -> Result<UnblockReport> {
        let mut conn = self.conn.lock().expect("poisoned");
        let tx = conn.transaction().map_err(backend)?;

        let cur_status: Option<String> = tx
            .query_row(
                "SELECT status FROM tasks WHERE id=?",
                params![id.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)?;
        let cur = cur_status.ok_or_else(|| TaskStoreError::NotFound(format!("task '{id}'")))?;
        if cur != "wip" {
            let cur_status = TaskStatus::parse(&cur).unwrap_or(TaskStatus::Pending);
            return Err(
                DomainError::IllegalTransition(id.clone(), cur_status, TaskStatus::Done).into(),
            );
        }

        let updated = tx
            .execute(
                "UPDATE tasks SET status='done', pr_number=?, updated_at=?
                  WHERE id=? AND status='wip'",
                params![pr_number as i64, now, id.as_str()],
            )
            .map_err(backend)?;
        if updated != 1 {
            return Err(DomainError::IllegalTransition(
                id.clone(),
                TaskStatus::Wip,
                TaskStatus::Done,
            )
            .into());
        }

        let epic_name: String = tx
            .query_row(
                "SELECT epic_name FROM tasks WHERE id=?",
                params![id.as_str()],
                |row| row.get(0),
            )
            .map_err(backend)?;

        SqliteTaskStore::append_event_with(
            &tx,
            EventKind::TaskCompleted,
            Some(&epic_name),
            Some(id),
            &serde_json::json!({"pr_number": pr_number}),
            now,
        )
        .map_err(backend)?;

        let newly_ready_ids: Vec<String> = {
            let mut stmt = tx
                .prepare(
                    "SELECT d.task_id FROM task_deps d
                     JOIN tasks t ON t.id = d.task_id
                      WHERE d.depends_on = ?
                        AND t.status IN ('pending','blocked')
                        AND NOT EXISTS (
                          SELECT 1 FROM task_deps d2
                          JOIN tasks dep ON dep.id = d2.depends_on
                          WHERE d2.task_id = d.task_id AND dep.status != 'done'
                        )",
                )
                .map_err(backend)?;
            let iter = stmt
                .query_map(params![id.as_str()], |row| row.get::<_, String>(0))
                .map_err(backend)?;
            let mut out: Vec<String> = Vec::new();
            for row in iter {
                out.push(row.map_err(backend)?);
            }
            out
        };

        for nid in &newly_ready_ids {
            tx.execute(
                "UPDATE tasks SET status='ready', updated_at=?
                  WHERE id=? AND status IN ('pending','blocked')",
                params![now, nid],
            )
            .map_err(backend)?;
            SqliteTaskStore::append_event_with(
                &tx,
                EventKind::TaskUnblocked,
                Some(&epic_name),
                Some(&TaskId::from_raw(nid.clone())),
                &serde_json::json!({}),
                now,
            )
            .map_err(backend)?;
        }

        tx.commit().map_err(backend)?;
        Ok(UnblockReport {
            completed: id.clone(),
            newly_ready: newly_ready_ids.into_iter().map(TaskId::from_raw).collect(),
        })
    }

    fn mark_task_failed(
        &self,
        id: &TaskId,
        max_attempts: u32,
        now: DateTime<Utc>,
    ) -> Result<TaskFailureOutcome> {
        let mut conn = self.conn.lock().expect("poisoned");
        let tx = conn.transaction().map_err(backend)?;

        let row: Option<(String, i64, String)> = tx
            .query_row(
                "SELECT status, attempts, epic_name FROM tasks WHERE id=?",
                params![id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(backend)?;
        let (cur, attempts, epic_name) =
            row.ok_or_else(|| TaskStoreError::NotFound(format!("task '{id}'")))?;
        if cur != "wip" {
            let cur_status = TaskStatus::parse(&cur).unwrap_or(TaskStatus::Pending);
            return Err(
                DomainError::IllegalTransition(id.clone(), cur_status, TaskStatus::Ready).into(),
            );
        }
        let attempts = attempts as u32;

        if attempts >= max_attempts {
            tx.execute(
                "UPDATE tasks SET status='escalated', updated_at=? WHERE id=?",
                params![now, id.as_str()],
            )
            .map_err(backend)?;
            SqliteTaskStore::append_event_with(
                &tx,
                EventKind::TaskFailed,
                Some(&epic_name),
                Some(id),
                &serde_json::json!({"final": true, "attempts": attempts}),
                now,
            )
            .map_err(backend)?;
            SqliteTaskStore::append_event_with(
                &tx,
                EventKind::TaskEscalated,
                Some(&epic_name),
                Some(id),
                &serde_json::json!({"attempts": attempts}),
                now,
            )
            .map_err(backend)?;

            let dependents: Vec<String> = {
                let mut stmt = tx
                    .prepare("SELECT task_id FROM task_deps WHERE depends_on=?")
                    .map_err(backend)?;
                let iter = stmt
                    .query_map(params![id.as_str()], |row| row.get::<_, String>(0))
                    .map_err(backend)?;
                let mut out: Vec<String> = Vec::new();
                for row in iter {
                    out.push(row.map_err(backend)?);
                }
                out
            };
            for dep_id in dependents {
                let updated = tx
                    .execute(
                        "UPDATE tasks SET status='blocked', updated_at=?
                          WHERE id=? AND status IN ('pending','ready')",
                        params![now, dep_id],
                    )
                    .map_err(backend)?;
                if updated > 0 {
                    SqliteTaskStore::append_event_with(
                        &tx,
                        EventKind::TaskBlocked,
                        Some(&epic_name),
                        Some(&TaskId::from_raw(dep_id.clone())),
                        &serde_json::json!({"reason":"parent_escalated","parent": id.as_str()}),
                        now,
                    )
                    .map_err(backend)?;
                }
            }

            tx.commit().map_err(backend)?;
            Ok(TaskFailureOutcome::Escalated { attempts })
        } else {
            tx.execute(
                "UPDATE tasks SET status='ready', updated_at=? WHERE id=?",
                params![now, id.as_str()],
            )
            .map_err(backend)?;
            SqliteTaskStore::append_event_with(
                &tx,
                EventKind::TaskFailed,
                Some(&epic_name),
                Some(id),
                &serde_json::json!({"final": false, "attempts": attempts}),
                now,
            )
            .map_err(backend)?;
            tx.commit().map_err(backend)?;
            Ok(TaskFailureOutcome::Retried { attempts })
        }
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

    fn release_claim(&self, id: &TaskId, now: DateTime<Utc>) -> Result<()> {
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
        if cur_status != TaskStatus::Wip {
            return Err(
                DomainError::IllegalTransition(id.clone(), cur_status, TaskStatus::Ready).into(),
            );
        }
        conn.execute(
            "UPDATE tasks
                SET status='ready',
                    attempts = MAX(attempts - 1, 0),
                    updated_at=?
              WHERE id=? AND status='wip'",
            params![now, id.as_str()],
        )
        .map_err(backend)?;
        Ok(())
    }

    fn force_status(
        &self,
        id: &TaskId,
        target: TaskStatus,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned");
        let prev: Option<(String, String)> = conn
            .query_row(
                "SELECT epic_name, status FROM tasks WHERE id=?",
                params![id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(backend)?;
        let (epic_name, prev_status_str) =
            prev.ok_or_else(|| TaskStoreError::NotFound(format!("task '{id}'")))?;
        let prev = TaskStatus::parse(&prev_status_str).ok_or_else(|| {
            TaskStoreError::Backend(format!("invalid stored task status: {prev_status_str}"))
        })?;
        conn.execute(
            "UPDATE tasks SET status=?, updated_at=? WHERE id=?",
            params![target.as_str(), now, id.as_str()],
        )
        .map_err(backend)?;
        SqliteTaskStore::append_event_with(
            &conn,
            EventKind::TaskForceStatus,
            Some(&epic_name),
            Some(id),
            &serde_json::json!({
                "from": prev.as_str(),
                "to": target.as_str(),
                "reason": reason,
            }),
            now,
        )
        .map_err(backend)?;
        Ok(())
    }

    fn find_task_by_pr(&self, pr_number: u64) -> Result<Option<Task>> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn
            .prepare("SELECT * FROM tasks WHERE pr_number=? LIMIT 1")
            .map_err(backend)?;
        let task = stmt
            .query_row(params![pr_number as i64], task_from_row)
            .optional()
            .map_err(backend)?;
        Ok(task)
    }

    fn apply_reconciliation(&self, plan: ReconciliationPlan, now: DateTime<Utc>) -> Result<()> {
        let graph = TaskGraph::build(plan.deps.iter().cloned());
        if let Some(cycle) = graph.detect_cycle() {
            return Err(DomainError::DepCycle(cycle).into());
        }

        let mut conn = self.conn.lock().expect("poisoned");
        let tx = conn.transaction().map_err(backend)?;

        // Upsert epic as active, preserving created_at if existing.
        let existing_created: Option<DateTime<Utc>> = tx
            .query_row(
                "SELECT created_at FROM epics WHERE name=?",
                params![plan.epic.name],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)?;
        let created_at = existing_created.unwrap_or(plan.epic.created_at);
        tx.execute(
            "INSERT INTO epics(name, spec_path, branch, status, created_at, completed_at)
             VALUES (?, ?, ?, 'active', ?, NULL)
             ON CONFLICT(name) DO UPDATE SET
               spec_path=excluded.spec_path,
               branch=excluded.branch,
               status='active',
               completed_at=NULL",
            params![
                plan.epic.name,
                plan.epic.spec_path.to_string_lossy(),
                plan.epic.branch,
                created_at,
            ],
        )
        .map_err(backend)?;

        // Upsert tasks: insert if missing, else update title/body (preserve attempts/status).
        for nt in &plan.tasks {
            let exists: bool = tx
                .query_row(
                    "SELECT 1 FROM tasks WHERE id=?",
                    params![nt.id.as_str()],
                    |_| Ok(true),
                )
                .optional()
                .map_err(backend)?
                .unwrap_or(false);
            if exists {
                tx.execute(
                    "UPDATE tasks SET title=?, body=?, source=?, updated_at=?,
                                       fingerprint = COALESCE(fingerprint, ?)
                      WHERE id=?",
                    params![
                        nt.title,
                        nt.body,
                        nt.source.as_str(),
                        now,
                        nt.fingerprint,
                        nt.id.as_str()
                    ],
                )
                .map_err(backend)?;
            } else {
                tx.execute(
                    "INSERT INTO tasks(id, epic_name, source, fingerprint, title, body,
                                       status, attempts, branch, pr_number, escalated_issue,
                                       created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, 'pending', 0, NULL, NULL, NULL, ?, ?)",
                    params![
                        nt.id.as_str(),
                        plan.epic.name,
                        nt.source.as_str(),
                        nt.fingerprint,
                        nt.title,
                        nt.body,
                        now,
                        now,
                    ],
                )
                .map_err(backend)?;
            }
        }

        // Replace deps for plan tasks.
        let plan_ids: Vec<String> = plan
            .tasks
            .iter()
            .map(|t| t.id.as_str().to_string())
            .collect();
        if !plan_ids.is_empty() {
            let placeholders = std::iter::repeat_n("?", plan_ids.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!("DELETE FROM task_deps WHERE task_id IN ({placeholders})");
            let bind: Vec<&dyn rusqlite::ToSql> =
                plan_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
            tx.execute(&sql, bind.as_slice()).map_err(backend)?;
        }
        for (a, b) in &plan.deps {
            tx.execute(
                "INSERT OR IGNORE INTO task_deps(task_id, depends_on) VALUES (?, ?)",
                params![a.as_str(), b.as_str()],
            )
            .map_err(backend)?;
        }

        // Apply remote_state to set status and pr_number.
        let in_remote: std::collections::BTreeSet<String> = plan
            .remote_state
            .iter()
            .map(|r| r.task_id.as_str().to_string())
            .collect();
        for r in &plan.remote_state {
            let desired = match (&r.pr, r.branch_exists) {
                (Some(pr), _) if pr.merged => "done",
                (Some(_), _) => "wip",
                (None, true) => "wip",
                (None, false) => {
                    let deps_satisfied: i64 = tx
                        .query_row(
                            "SELECT CASE WHEN NOT EXISTS (
                              SELECT 1 FROM task_deps d
                              JOIN tasks dep ON dep.id = d.depends_on
                              WHERE d.task_id = ? AND dep.status != 'done'
                            ) THEN 1 ELSE 0 END",
                            params![r.task_id.as_str()],
                            |row| row.get(0),
                        )
                        .map_err(backend)?;
                    if deps_satisfied == 1 {
                        "ready"
                    } else {
                        "pending"
                    }
                }
            };
            let pr_num = r.pr.as_ref().map(|p| p.number as i64);
            tx.execute(
                "UPDATE tasks SET status=?, pr_number=COALESCE(?, pr_number), updated_at=?
                  WHERE id=?",
                params![desired, pr_num, now, r.task_id.as_str()],
            )
            .map_err(backend)?;
        }

        // For tasks in plan but not in remote_state: re-classify Pending only.
        for nt in &plan.tasks {
            if in_remote.contains(nt.id.as_str()) {
                continue;
            }
            let cur: Option<String> = tx
                .query_row(
                    "SELECT status FROM tasks WHERE id=?",
                    params![nt.id.as_str()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(backend)?;
            if cur.as_deref() != Some("pending") {
                continue;
            }
            let deps_satisfied: i64 = tx
                .query_row(
                    "SELECT CASE WHEN NOT EXISTS (
                      SELECT 1 FROM task_deps d
                      JOIN tasks dep ON dep.id = d.depends_on
                      WHERE d.task_id = ? AND dep.status != 'done'
                    ) THEN 1 ELSE 0 END",
                    params![nt.id.as_str()],
                    |row| row.get(0),
                )
                .map_err(backend)?;
            let desired = if deps_satisfied == 1 {
                "ready"
            } else {
                "pending"
            };
            tx.execute(
                "UPDATE tasks SET status=?, updated_at=? WHERE id=? AND status='pending'",
                params![desired, now, nt.id.as_str()],
            )
            .map_err(backend)?;
        }

        for branch in &plan.orphan_branches {
            SqliteTaskStore::append_event_with(
                &tx,
                EventKind::Reconciled,
                Some(&plan.epic.name),
                None,
                &serde_json::json!({"orphan_branch": branch}),
                now,
            )
            .map_err(backend)?;
        }
        SqliteTaskStore::append_event_with(
            &tx,
            EventKind::Reconciled,
            Some(&plan.epic.name),
            None,
            &serde_json::json!({"tasks": plan.tasks.len()}),
            now,
        )
        .map_err(backend)?;

        tx.commit().map_err(backend)?;
        Ok(())
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

impl SuppressionRepo for SqliteTaskStore {
    fn suppress(
        &self,
        fingerprint: &str,
        reason: &str,
        suppress_until: DateTime<Utc>,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "INSERT INTO escalation_suppression(fingerprint, reason, suppress_until)
             VALUES (?, ?, ?)
             ON CONFLICT(fingerprint, reason) DO UPDATE SET suppress_until=excluded.suppress_until",
            params![fingerprint, reason, suppress_until],
        )
        .map_err(backend)?;
        Ok(())
    }

    fn is_suppressed(&self, fingerprint: &str, reason: &str, now: DateTime<Utc>) -> Result<bool> {
        let conn = self.conn.lock().expect("poisoned");
        let until: Option<DateTime<Utc>> = conn
            .query_row(
                "SELECT suppress_until FROM escalation_suppression
                 WHERE fingerprint=? AND reason=?",
                params![fingerprint, reason],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)?;
        Ok(until.is_some_and(|u| now < u))
    }

    fn clear(&self, fingerprint: &str, reason: &str) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "DELETE FROM escalation_suppression WHERE fingerprint=? AND reason=?",
            params![fingerprint, reason],
        )
        .map_err(backend)?;
        Ok(())
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
    fn open_in_memory_initializes_to_current_schema() {
        let store = SqliteTaskStore::open_in_memory().expect("open");
        let conn = store.conn.lock().unwrap();
        let v: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key='schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION.to_string());
    }

    #[test]
    fn v2_lookup_indexes_are_present() {
        let store = SqliteTaskStore::open_in_memory().expect("open");
        let conn = store.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master
                  WHERE type='index' AND name IN ('idx_tasks_pr_number','idx_epics_active_spec')",
            )
            .unwrap();
        let names: std::collections::HashSet<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(names.contains("idx_tasks_pr_number"));
        assert!(names.contains("idx_epics_active_spec"));
    }

    #[test]
    fn migrates_v1_db_to_current() {
        // Simulate an older DB at V1: only V1 SQL applied. open() should
        // upgrade by running V2 forward migrations.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(V1_SQL).unwrap();
        SqliteTaskStore::migrate(&mut conn).unwrap();
        let v: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key='schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION.to_string());
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
                assert_eq!(expected, SCHEMA_VERSION);
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
