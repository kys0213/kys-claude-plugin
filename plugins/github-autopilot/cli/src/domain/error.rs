use thiserror::Error;

use super::epic::EpicStatus;
use super::task::TaskStatus;
use super::task_id::TaskId;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("dependency cycle detected: {0:?}")]
    DepCycle(Vec<TaskId>),

    #[error("illegal status transition for task {0}: {1:?} -> {2:?}")]
    IllegalTransition(TaskId, TaskStatus, TaskStatus),

    /// Precondition failure: the operation needs the task to currently be in a
    /// specific status, but it isn't. Distinct from [`Self::IllegalTransition`]
    /// (which describes an attempted-target failure): here, the *current*
    /// status is the bug, not the target.
    ///
    /// Args: `(task_id, required_current_status, actual_current_status)`.
    #[error("task {0} requires status {1:?}, was {2:?}")]
    RequiresStatus(TaskId, TaskStatus, TaskStatus),

    #[error("epic '{0}' already exists with status {1:?}")]
    EpicAlreadyExists(String, EpicStatus),

    #[error("dep references unknown task: {0}")]
    UnknownDepTarget(TaskId),

    #[error("duplicate task id in plan: {0}")]
    DuplicateTaskId(TaskId),

    #[error("inconsistency: {0}")]
    Inconsistency(String),
}
