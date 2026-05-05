use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::task_id::TaskId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub task_id: Option<TaskId>,
    pub epic_name: Option<String>,
    pub kind: EventKind,
    pub payload: serde_json::Value,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    EpicStarted,
    EpicCompleted,
    EpicAbandoned,
    TaskInserted,
    TaskClaimed,
    TaskStarted,
    TaskCompleted,
    TaskFailed,
    TaskEscalated,
    TaskBlocked,
    TaskUnblocked,
    Reconciled,
    ClaimLost,
    MigratedFromIssue,
    EscalationResolved,
    WatchDuplicate,
    TaskForceStatus,
    /// Bulk recovery of a Wip task whose claim went stale (worker crashed,
    /// worktree destroyed, ctrl-C). Same effect as `release_claim` (Wip →
    /// Ready, attempts decremented), but emitted explicitly so the audit
    /// trail can distinguish "operator released" from "system reaped".
    TaskReleasedStale,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::EpicStarted => "epic_started",
            EventKind::EpicCompleted => "epic_completed",
            EventKind::EpicAbandoned => "epic_abandoned",
            EventKind::TaskInserted => "task_inserted",
            EventKind::TaskClaimed => "task_claimed",
            EventKind::TaskStarted => "task_started",
            EventKind::TaskCompleted => "task_completed",
            EventKind::TaskFailed => "task_failed",
            EventKind::TaskEscalated => "task_escalated",
            EventKind::TaskBlocked => "task_blocked",
            EventKind::TaskUnblocked => "task_unblocked",
            EventKind::Reconciled => "reconciled",
            EventKind::ClaimLost => "claim_lost",
            EventKind::MigratedFromIssue => "migrated_from_issue",
            EventKind::EscalationResolved => "escalation_resolved",
            EventKind::WatchDuplicate => "watch_duplicate",
            EventKind::TaskForceStatus => "task_force_status",
            EventKind::TaskReleasedStale => "task_released_stale",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "epic_started" => EventKind::EpicStarted,
            "epic_completed" => EventKind::EpicCompleted,
            "epic_abandoned" => EventKind::EpicAbandoned,
            "task_inserted" => EventKind::TaskInserted,
            "task_claimed" => EventKind::TaskClaimed,
            "task_started" => EventKind::TaskStarted,
            "task_completed" => EventKind::TaskCompleted,
            "task_failed" => EventKind::TaskFailed,
            "task_escalated" => EventKind::TaskEscalated,
            "task_blocked" => EventKind::TaskBlocked,
            "task_unblocked" => EventKind::TaskUnblocked,
            "reconciled" => EventKind::Reconciled,
            "claim_lost" => EventKind::ClaimLost,
            "migrated_from_issue" => EventKind::MigratedFromIssue,
            "escalation_resolved" => EventKind::EscalationResolved,
            "watch_duplicate" => EventKind::WatchDuplicate,
            "task_force_status" => EventKind::TaskForceStatus,
            "task_released_stale" => EventKind::TaskReleasedStale,
            _ => return None,
        })
    }
}
