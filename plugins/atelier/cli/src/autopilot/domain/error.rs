use thiserror::Error;

use super::epic::EpicStatus;
use super::task::TaskStatus;
use super::task_id::TaskId;

/// Marker error for CLI user-input validation failures. Surfaced through
/// `anyhow::Error` and downcasted by `main::exit_code_for` to map onto
/// exit code 1 (user error, distinct from clap's exit 2 for argparse and
/// codes greater than 2 for system / unexpected failures). Use this
/// whenever a CLI handler rejects an argument before any store mutation —
/// the error string itself is the user-facing message, so write it
/// actionably.
#[derive(Debug, Error)]
#[error("{0}")]
pub struct UserInputError(pub String);

impl UserInputError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("dependency cycle detected: {0:?}")]
    DepCycle(Vec<TaskId>),

    /// A status change was attempted that is not allowed by the lifecycle
    /// state machine (e.g. `pending -> done` skipping `ready`/`wip`). The
    /// message names the offending transition so an operator can pick a
    /// valid intermediate hop.
    #[error(
        "illegal status transition for task {0}: {1} -> {2} is not allowed; \
         use `task set-status {0} --to <status> --reason ...` to override"
    )]
    IllegalTransition(TaskId, TaskStatus, TaskStatus),

    /// Precondition failure: the operation needs the task to currently be in a
    /// specific status, but it isn't. Distinct from [`Self::IllegalTransition`]
    /// (which describes an attempted-target failure): here, the *current*
    /// status is the bug, not the target.
    ///
    /// Args: `(task_id, required_current_status, actual_current_status)`.
    #[error(
        "task {0} requires status '{1}' but was '{2}'; \
         advance it first (e.g. `task claim --epic <name>` for ready->wip) \
         or use `task set-status {0} --to {1} --reason ...` to override"
    )]
    RequiresStatus(TaskId, TaskStatus, TaskStatus),

    #[error("epic '{0}' already exists with status {1:?}")]
    EpicAlreadyExists(String, EpicStatus),

    #[error("dep references unknown task: {0}")]
    UnknownDepTarget(TaskId),

    #[error(
        "task id '{0}' already exists in store; \
         use `task show {0}` to inspect or `task set-status {0} --to <status>` to override"
    )]
    DuplicateTaskId(TaskId),

    #[error("inconsistency: {0}")]
    Inconsistency(String),
}
