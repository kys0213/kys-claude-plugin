//! Operator-facing diagnostics for the task store.
//!
//! These commands read or override the local SQLite cache directly without
//! touching git or GitHub — useful for debugging stuck epics, but should be
//! used sparingly because manual force_status bypasses the normal lifecycle.

use anyhow::{Context, Result};
use clap::ValueEnum;

use crate::domain::{Task, TaskId, TaskStatus};
use crate::ports::clock::{Clock, StdClock};
use crate::ports::task_store::TaskStore;

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum TaskStatusArg {
    Pending,
    Ready,
    Wip,
    Blocked,
    Done,
    Escalated,
}

impl From<TaskStatusArg> for TaskStatus {
    fn from(s: TaskStatusArg) -> TaskStatus {
        match s {
            TaskStatusArg::Pending => TaskStatus::Pending,
            TaskStatusArg::Ready => TaskStatus::Ready,
            TaskStatusArg::Wip => TaskStatus::Wip,
            TaskStatusArg::Blocked => TaskStatus::Blocked,
            TaskStatusArg::Done => TaskStatus::Done,
            TaskStatusArg::Escalated => TaskStatus::Escalated,
        }
    }
}

pub struct TaskService<'a> {
    store: &'a dyn TaskStore,
    clock: &'a dyn Clock,
}

impl<'a> TaskService<'a> {
    pub fn new(store: &'a dyn TaskStore, clock: &'a dyn Clock) -> Self {
        Self { store, clock }
    }

    pub fn list(
        &self,
        epic: &str,
        status: Option<TaskStatusArg>,
        json: bool,
        out: &mut dyn std::io::Write,
    ) -> Result<i32> {
        let tasks = self
            .store
            .list_tasks_by_epic(epic, status.map(TaskStatus::from))
            .with_context(|| format!("listing tasks for epic '{epic}'"))?;
        if json {
            serde_json::to_writer(&mut *out, &tasks)?;
            writeln!(out)?;
        } else if tasks.is_empty() {
            writeln!(out, "(no tasks)")?;
        } else {
            writeln!(out, "ID            STATUS     ATTEMPTS  TITLE")?;
            for t in &tasks {
                writeln!(
                    out,
                    "{:<12}  {:<9}  {:>8}  {}",
                    t.id.as_str(),
                    t.status.as_str(),
                    t.attempts,
                    t.title
                )?;
            }
        }
        Ok(0)
    }

    pub fn show(&self, task_id: &str, json: bool, out: &mut dyn std::io::Write) -> Result<i32> {
        let id = TaskId::from_raw(task_id);
        let task = self
            .store
            .get_task(&id)
            .with_context(|| format!("fetching task '{task_id}'"))?;
        match task {
            Some(t) => {
                if json {
                    serde_json::to_writer(&mut *out, &t)?;
                    writeln!(out)?;
                } else {
                    print_task_human(&t, out)?;
                }
                Ok(0)
            }
            None => {
                writeln!(out, "task '{task_id}' not found")?;
                Ok(1)
            }
        }
    }

    pub fn force_status(
        &self,
        task_id: &str,
        to: TaskStatusArg,
        reason: Option<&str>,
        out: &mut dyn std::io::Write,
    ) -> Result<i32> {
        let id = TaskId::from_raw(task_id);
        let now = self.clock.now();
        self.store
            .force_status(&id, TaskStatus::from(to), reason.unwrap_or(""), now)
            .with_context(|| format!("forcing status of task '{task_id}'"))?;
        if let Some(r) = reason {
            writeln!(out, "task '{task_id}' status forced to {:?} ({r})", to)?;
        } else {
            writeln!(out, "task '{task_id}' status forced to {:?}", to)?;
        }
        Ok(0)
    }
}

pub fn task_service<'a>(store: &'a dyn TaskStore, clock: &'a dyn Clock) -> TaskService<'a> {
    TaskService::new(store, clock)
}

pub fn default_clock() -> StdClock {
    StdClock
}

fn print_task_human(t: &Task, out: &mut dyn std::io::Write) -> Result<()> {
    writeln!(out, "id:              {}", t.id.as_str())?;
    writeln!(out, "epic:            {}", t.epic_name)?;
    writeln!(out, "status:          {}", t.status.as_str())?;
    writeln!(out, "source:          {}", t.source.as_str())?;
    writeln!(out, "attempts:        {}", t.attempts)?;
    writeln!(out, "title:           {}", t.title)?;
    if let Some(b) = &t.branch {
        writeln!(out, "branch:          {b}")?;
    }
    if let Some(pr) = t.pr_number {
        writeln!(out, "pr_number:       {pr}")?;
    }
    if let Some(issue) = t.escalated_issue {
        writeln!(out, "escalated_issue: {issue}")?;
    }
    if let Some(fp) = &t.fingerprint {
        writeln!(out, "fingerprint:     {fp}")?;
    }
    if let Some(body) = &t.body {
        writeln!(out, "---")?;
        writeln!(out, "{body}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::clock::FixedClock;
    use crate::ports::task_store::{EpicPlan, NewTask};
    use crate::store::InMemoryTaskStore;
    use chrono::TimeZone;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn fixture() -> (Arc<dyn TaskStore>, FixedClock) {
        let store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
        let clock = FixedClock::new(chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
        let now = clock.now();
        store
            .insert_epic_with_tasks(
                EpicPlan {
                    epic: crate::domain::Epic {
                        name: "e".to_string(),
                        spec_path: PathBuf::from("spec/x.md"),
                        branch: "epic/e".to_string(),
                        status: crate::domain::EpicStatus::Active,
                        created_at: now,
                        completed_at: None,
                    },
                    tasks: vec![NewTask {
                        id: TaskId::from_raw("a1b2c3d4e5f6"),
                        source: crate::domain::TaskSource::Decompose,
                        fingerprint: None,
                        title: "first".to_string(),
                        body: None,
                    }],
                    deps: vec![],
                },
                now,
            )
            .unwrap();
        (store, clock)
    }

    #[test]
    fn list_renders_human_table() {
        let (store, clock) = fixture();
        let svc = TaskService::new(store.as_ref(), &clock);
        let mut buf: Vec<u8> = Vec::new();
        let code = svc.list("e", None, false, &mut buf).unwrap();
        assert_eq!(code, 0);
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("a1b2c3d4e5f6"));
        assert!(s.contains("ready"));
        assert!(s.contains("first"));
    }

    #[test]
    fn list_renders_json() {
        let (store, clock) = fixture();
        let svc = TaskService::new(store.as_ref(), &clock);
        let mut buf: Vec<u8> = Vec::new();
        svc.list("e", None, true, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert!(parsed.is_array());
    }

    #[test]
    fn show_returns_1_when_missing() {
        let (store, clock) = fixture();
        let svc = TaskService::new(store.as_ref(), &clock);
        let mut buf: Vec<u8> = Vec::new();
        let code = svc.show("does-not-exist", false, &mut buf).unwrap();
        assert_eq!(code, 1);
    }

    #[test]
    fn force_status_overrides_lifecycle() {
        let (store, clock) = fixture();
        let svc = TaskService::new(store.as_ref(), &clock);
        let mut buf: Vec<u8> = Vec::new();
        svc.force_status(
            "a1b2c3d4e5f6",
            TaskStatusArg::Done,
            Some("manual"),
            &mut buf,
        )
        .unwrap();
        let task = store
            .get_task(&TaskId::from_raw("a1b2c3d4e5f6"))
            .unwrap()
            .unwrap();
        assert_eq!(task.status, TaskStatus::Done);
    }
}
