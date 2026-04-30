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

    #[error("epic '{0}' already exists with status {1:?}")]
    EpicAlreadyExists(String, EpicStatus),

    #[error("dep references unknown task: {0}")]
    UnknownDepTarget(TaskId),

    #[error("duplicate task id in plan: {0}")]
    DuplicateTaskId(TaskId),
}
