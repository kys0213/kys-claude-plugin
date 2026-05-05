use std::path::Path;

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

    /// Returns the unique active epic for `spec_path`, if any. The
    /// invariant is that at most one active epic owns a given spec; if two
    /// or more are observed, returns [`DomainError::Inconsistency`] so the
    /// caller can surface it rather than silently picking one.
    fn find_active_by_spec_path(&self, spec_path: &Path) -> Result<Option<Epic>>;
}

pub trait TaskRepo: Send + Sync {
    fn insert_epic_with_tasks(&self, plan: EpicPlan, now: DateTime<Utc>) -> Result<()>;

    fn get_task(&self, id: &TaskId) -> Result<Option<Task>>;

    fn list_tasks_by_epic(&self, epic: &str, status: Option<TaskStatus>) -> Result<Vec<Task>>;

    fn find_by_fingerprint(&self, epic: &str, fingerprint: &str) -> Result<Option<Task>>;

    /// Looks up the task that owns a merged PR. Used by `MergeLoop` after a
    /// PR merges to drive the corresponding task to `Done`.
    fn find_task_by_pr(&self, pr_number: u64) -> Result<Option<Task>>;

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

    /// Releases an unused claim (UC-11 push-reject / claim_lost). Decrements
    /// `attempts` so the cancelled try does not count toward `max_attempts`.
    /// Use [`TaskRepo::mark_task_failed`] instead when an attempt was made
    /// and failed (CI failure, implementer error, ...): that path preserves
    /// `attempts` and feeds escalation.
    fn release_claim(&self, id: &TaskId, now: DateTime<Utc>) -> Result<()>;

    /// Bulk-recovers Wip tasks whose `updated_at` is older than `before`,
    /// reverting each to Ready with `attempts` decremented (same effect as
    /// `release_claim`). Used by the cron supervisor to reap orphaned claims
    /// after worker crashes / ctrl-C / worktree destruction so the task can
    /// reach another worker. Each recovered task gets a `TaskReleasedStale`
    /// event for audit. Returns the ids of recovered tasks (empty when no
    /// stale Wip exists — idempotent).
    fn release_stale(&self, before: DateTime<Utc>, now: DateTime<Utc>) -> Result<Vec<TaskId>>;

    /// Operator override (CLI `task force-status`). Bypasses the normal
    /// transition graph but records `reason` in the emitted event for audit.
    /// Does NOT cascade dependents (no auto-unblock / auto-block); the
    /// caller must drive any further reconciliation explicitly.
    fn force_status(
        &self,
        id: &TaskId,
        target: TaskStatus,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<()>;

    fn apply_reconciliation(&self, plan: ReconciliationPlan, now: DateTime<Utc>) -> Result<()>;

    fn list_deps(&self, task_id: &TaskId) -> Result<Vec<TaskId>>;
}

pub trait EventLog: Send + Sync {
    fn append_event(&self, event: &Event) -> Result<()>;
    fn list_events(&self, filter: EventFilter) -> Result<Vec<Event>>;
}

/// Fingerprint-scoped suppression for HITL escalation (UC-7 unmatched
/// watch, UC-9c rejected close). Prevents repeat-issue spam for the same
/// finding within a configurable window.
pub trait SuppressionRepo: Send + Sync {
    fn suppress(
        &self,
        fingerprint: &str,
        reason: &str,
        suppress_until: DateTime<Utc>,
    ) -> Result<()>;

    fn is_suppressed(&self, fingerprint: &str, reason: &str, now: DateTime<Utc>) -> Result<bool>;

    fn clear(&self, fingerprint: &str, reason: &str) -> Result<()>;
}

pub trait TaskStore: EpicRepo + TaskRepo + EventLog + SuppressionRepo + Send + Sync {}
impl<T: EpicRepo + TaskRepo + EventLog + SuppressionRepo + Send + Sync + ?Sized> TaskStore for T {}
