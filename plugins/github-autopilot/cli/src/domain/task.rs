use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::task_id::TaskId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub epic_name: String,
    pub source: TaskSource,
    pub fingerprint: Option<String>,
    pub title: String,
    pub body: Option<String>,
    pub status: TaskStatus,
    pub attempts: u32,
    pub branch: Option<String>,
    pub pr_number: Option<u64>,
    pub escalated_issue: Option<u64>,
    /// 64-bit weighted simhash signature derived from the task's title/body
    /// (or skill-supplied text). Used by ledger-based stagnation detection
    /// (see `plans/ledger-stagnation-redesign.md` §3.3). Old rows that
    /// pre-date the V3 schema migration leave this `None`; new tasks fill
    /// it via `task add` / `task add-batch`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub simhash: Option<u64>,
    /// Top-N source paths affected by this task (line-count desc), used as
    /// the path-set side of the hybrid stagnation similarity check
    /// (Jaccard ≥ J). `None` for tasks created before V3 or when the
    /// caller didn't supply `--paths`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected_paths: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Ready,
    Wip,
    Blocked,
    Done,
    Escalated,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Ready => "ready",
            TaskStatus::Wip => "wip",
            TaskStatus::Blocked => "blocked",
            TaskStatus::Done => "done",
            TaskStatus::Escalated => "escalated",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(TaskStatus::Pending),
            "ready" => Some(TaskStatus::Ready),
            "wip" => Some(TaskStatus::Wip),
            "blocked" => Some(TaskStatus::Blocked),
            "done" => Some(TaskStatus::Done),
            "escalated" => Some(TaskStatus::Escalated),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, TaskStatus::Done | TaskStatus::Escalated)
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskSource {
    Decompose,
    GapWatch,
    QaBoost,
    CiWatch,
    Human,
}

impl TaskSource {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskSource::Decompose => "decompose",
            TaskSource::GapWatch => "gap-watch",
            TaskSource::QaBoost => "qa-boost",
            TaskSource::CiWatch => "ci-watch",
            TaskSource::Human => "human",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "decompose" => Some(TaskSource::Decompose),
            "gap-watch" => Some(TaskSource::GapWatch),
            "qa-boost" => Some(TaskSource::QaBoost),
            "ci-watch" => Some(TaskSource::CiWatch),
            "human" => Some(TaskSource::Human),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskFailureOutcome {
    Retried { attempts: u32 },
    Escalated { attempts: u32 },
}
