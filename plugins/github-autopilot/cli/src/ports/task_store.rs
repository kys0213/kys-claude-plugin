use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::domain::{
    DomainError, Epic, EpicStatus, Event, EventKind, Task, TaskFailureOutcome, TaskId, TaskSource,
    TaskStatus,
};

pub type Result<T> = std::result::Result<T, TaskStoreError>;

#[derive(Debug, Error)]
pub enum TaskStoreError {
    #[error("storage busy")]
    Busy,

    #[error("storage backend: {0}")]
    Backend(String),

    #[error("schema mismatch: at v{found}, expected v{expected}")]
    SchemaMismatch { found: u32, expected: u32 },

    #[error("not found: {0}")]
    NotFound(String),

    #[error(transparent)]
    Domain(#[from] DomainError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTask {
    pub id: TaskId,
    pub source: TaskSource,
    pub fingerprint: Option<String>,
    pub title: String,
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewWatchTask {
    pub id: TaskId,
    pub epic_name: String,
    pub source: TaskSource,
    pub fingerprint: String,
    pub title: String,
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpsertOutcome {
    Inserted(TaskId),
    DuplicateFingerprint(TaskId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnblockReport {
    pub completed: TaskId,
    pub newly_ready: Vec<TaskId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpicPlan {
    pub epic: Epic,
    pub tasks: Vec<NewTask>,
    pub deps: Vec<(TaskId, TaskId)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationPlan {
    pub epic: Epic,
    pub tasks: Vec<NewTask>,
    pub deps: Vec<(TaskId, TaskId)>,
    pub remote_state: Vec<RemoteTaskState>,
    pub orphan_branches: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTaskState {
    pub task_id: TaskId,
    pub branch_exists: bool,
    pub pr: Option<RemotePrState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemotePrState {
    pub number: u64,
    pub merged: bool,
    pub closed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventFilter {
    pub epic: Option<String>,
    pub task: Option<TaskId>,
    pub kinds: Vec<EventKind>,
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<u32>,
}

pub trait EpicRepo: Send + Sync {
    fn upsert_epic(&self, epic: &Epic) -> Result<()>;
    fn get_epic(&self, name: &str) -> Result<Option<Epic>>;
    fn list_epics(&self, status: Option<EpicStatus>) -> Result<Vec<Epic>>;
    fn set_epic_status(&self, name: &str, status: EpicStatus, at: DateTime<Utc>) -> Result<()>;
}

pub trait TaskRepo: Send + Sync {
    fn insert_epic_with_tasks(&self, plan: EpicPlan, now: DateTime<Utc>) -> Result<()>;

    fn get_task(&self, id: &TaskId) -> Result<Option<Task>>;

    fn list_tasks_by_epic(&self, epic: &str, status: Option<TaskStatus>) -> Result<Vec<Task>>;

    fn find_by_fingerprint(&self, epic: &str, fingerprint: &str) -> Result<Option<Task>>;

    fn upsert_watch_task(&self, task: NewWatchTask, now: DateTime<Utc>) -> Result<UpsertOutcome>;

    fn claim_next_task(&self, epic: &str, now: DateTime<Utc>) -> Result<Option<Task>>;

    fn complete_task_and_unblock(
        &self,
        id: &TaskId,
        pr_number: u64,
        now: DateTime<Utc>,
    ) -> Result<UnblockReport>;

    fn mark_task_failed(
        &self,
        id: &TaskId,
        max_attempts: u32,
        now: DateTime<Utc>,
    ) -> Result<TaskFailureOutcome>;

    fn escalate_task(&self, id: &TaskId, issue_number: u64, now: DateTime<Utc>) -> Result<()>;

    fn revert_to_ready(&self, id: &TaskId, now: DateTime<Utc>) -> Result<()>;

    fn force_status(&self, id: &TaskId, new_status: TaskStatus, now: DateTime<Utc>) -> Result<()>;

    fn apply_reconciliation(&self, plan: ReconciliationPlan, now: DateTime<Utc>) -> Result<()>;

    fn list_deps(&self, task_id: &TaskId) -> Result<Vec<TaskId>>;
}

pub trait EventLog: Send + Sync {
    fn append_event(&self, event: &Event) -> Result<()>;
    fn list_events(&self, filter: EventFilter) -> Result<Vec<Event>>;
}

pub trait TaskStore: EpicRepo + TaskRepo + EventLog + Send + Sync {}
impl<T: EpicRepo + TaskRepo + EventLog + Send + Sync + ?Sized> TaskStore for T {}
