use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::task_id::TaskId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub task_id: Option<TaskId>,
    pub epic_name: Option<String>,
    pub kind: EventKind,
    pub payload: EventPayload,
    pub at: DateTime<Utc>,
}

impl Event {
    /// Build an `Event` whose [`EventKind`] matches the inner [`EventPayload`]
    /// variant. `debug_assert!` enforces the invariant in test/dev builds; in
    /// release builds the caller-supplied `kind` is trusted (we deliberately
    /// avoid panicking the daemon on a programming bug, since the same
    /// information is recoverable from `payload.kind()`).
    pub fn new(
        kind: EventKind,
        epic_name: Option<String>,
        task_id: Option<TaskId>,
        payload: EventPayload,
        at: DateTime<Utc>,
    ) -> Self {
        debug_assert_eq!(
            kind,
            payload.kind(),
            "Event::new: kind {kind:?} does not match payload variant {:?}",
            payload.kind()
        );
        Self {
            task_id,
            epic_name,
            kind,
            payload,
            at,
        }
    }
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

/// Typed counterpart of [`EventKind`]. Each variant carries the exact fields
/// emitted by the corresponding store operation, so consumers no longer have
/// to memorize untyped JSON keys.
///
/// Serializes with an internally tagged representation
/// (`{ "kind": "<snake_case>", ...fields }`). Variant ↔ [`EventKind`] mapping
/// is enforced by [`EventPayload::kind`] and asserted in [`Event::new`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventPayload {
    EpicStarted,
    EpicCompleted,
    EpicAbandoned,
    /// Emitted both for `insert_epic_with_tasks` (only `source` populated) and
    /// `upsert_watch_task` (both `source` and `fingerprint` populated).
    TaskInserted {
        source: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fingerprint: Option<String>,
    },
    TaskClaimed {
        attempts: u32,
    },
    /// Reserved — no emit site yet. Kept so [`EventKind`] ↔ [`EventPayload`]
    /// stays 1:1.
    TaskStarted,
    TaskCompleted {
        pr_number: u64,
    },
    TaskFailed {
        #[serde(rename = "final")]
        is_final: bool,
        attempts: u32,
    },
    /// Emitted from two paths: `mark_task_failed` (max-attempts → escalated)
    /// carries `attempts`; `escalate_task` (record GitHub issue number)
    /// carries `issue`. Both fields are optional so either path can be
    /// represented losslessly.
    TaskEscalated {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attempts: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        issue: Option<u64>,
    },
    TaskBlocked {
        reason: String,
        parent: String,
    },
    TaskUnblocked,
    /// Reconciliation events come in two shapes — per-orphan-branch and a
    /// summary line at the end. Modelled as separate sub-variants via an
    /// internal `event` discriminator so each shape is unambiguous.
    Reconciled(ReconciledPayload),
    /// Reserved — no emit site yet.
    ClaimLost,
    /// Reserved — no emit site yet.
    MigratedFromIssue,
    /// Reserved — no emit site yet.
    EscalationResolved,
    WatchDuplicate {
        fingerprint: String,
    },
    TaskForceStatus {
        from: String,
        to: String,
        reason: String,
    },
    TaskReleasedStale {
        prev_attempts: u32,
    },
}

/// Sub-variants of [`EventPayload::Reconciled`]. Reconciliation emits one
/// event per orphan branch plus a final summary; both shapes are recorded
/// distinctly so tooling can filter / count without inspecting payload keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ReconciledPayload {
    OrphanBranch { orphan_branch: String },
    Summary { tasks: u64 },
}

impl EventPayload {
    /// Returns the [`EventKind`] this payload corresponds to. Pairs with
    /// [`Event::new`] to guarantee `Event.kind == Event.payload.kind()`.
    pub fn kind(&self) -> EventKind {
        match self {
            EventPayload::EpicStarted => EventKind::EpicStarted,
            EventPayload::EpicCompleted => EventKind::EpicCompleted,
            EventPayload::EpicAbandoned => EventKind::EpicAbandoned,
            EventPayload::TaskInserted { .. } => EventKind::TaskInserted,
            EventPayload::TaskClaimed { .. } => EventKind::TaskClaimed,
            EventPayload::TaskStarted => EventKind::TaskStarted,
            EventPayload::TaskCompleted { .. } => EventKind::TaskCompleted,
            EventPayload::TaskFailed { .. } => EventKind::TaskFailed,
            EventPayload::TaskEscalated { .. } => EventKind::TaskEscalated,
            EventPayload::TaskBlocked { .. } => EventKind::TaskBlocked,
            EventPayload::TaskUnblocked => EventKind::TaskUnblocked,
            EventPayload::Reconciled(_) => EventKind::Reconciled,
            EventPayload::ClaimLost => EventKind::ClaimLost,
            EventPayload::MigratedFromIssue => EventKind::MigratedFromIssue,
            EventPayload::EscalationResolved => EventKind::EscalationResolved,
            EventPayload::WatchDuplicate { .. } => EventKind::WatchDuplicate,
            EventPayload::TaskForceStatus { .. } => EventKind::TaskForceStatus,
            EventPayload::TaskReleasedStale { .. } => EventKind::TaskReleasedStale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn roundtrip(payload: EventPayload) -> EventPayload {
        let s = serde_json::to_string(&payload).expect("serialize");
        serde_json::from_str(&s).expect("deserialize")
    }

    #[test]
    fn payload_kind_matches_for_every_variant() {
        let cases: Vec<(EventPayload, EventKind)> = vec![
            (EventPayload::EpicStarted, EventKind::EpicStarted),
            (EventPayload::EpicCompleted, EventKind::EpicCompleted),
            (EventPayload::EpicAbandoned, EventKind::EpicAbandoned),
            (
                EventPayload::TaskInserted {
                    source: "spec".into(),
                    fingerprint: None,
                },
                EventKind::TaskInserted,
            ),
            (
                EventPayload::TaskClaimed { attempts: 1 },
                EventKind::TaskClaimed,
            ),
            (EventPayload::TaskStarted, EventKind::TaskStarted),
            (
                EventPayload::TaskCompleted { pr_number: 42 },
                EventKind::TaskCompleted,
            ),
            (
                EventPayload::TaskFailed {
                    is_final: false,
                    attempts: 1,
                },
                EventKind::TaskFailed,
            ),
            (
                EventPayload::TaskEscalated {
                    attempts: Some(3),
                    issue: None,
                },
                EventKind::TaskEscalated,
            ),
            (
                EventPayload::TaskBlocked {
                    reason: "parent_escalated".into(),
                    parent: "abc".into(),
                },
                EventKind::TaskBlocked,
            ),
            (EventPayload::TaskUnblocked, EventKind::TaskUnblocked),
            (
                EventPayload::Reconciled(ReconciledPayload::Summary { tasks: 3 }),
                EventKind::Reconciled,
            ),
            (EventPayload::ClaimLost, EventKind::ClaimLost),
            (
                EventPayload::MigratedFromIssue,
                EventKind::MigratedFromIssue,
            ),
            (
                EventPayload::EscalationResolved,
                EventKind::EscalationResolved,
            ),
            (
                EventPayload::WatchDuplicate {
                    fingerprint: "fp".into(),
                },
                EventKind::WatchDuplicate,
            ),
            (
                EventPayload::TaskForceStatus {
                    from: "wip".into(),
                    to: "ready".into(),
                    reason: "manual".into(),
                },
                EventKind::TaskForceStatus,
            ),
            (
                EventPayload::TaskReleasedStale { prev_attempts: 2 },
                EventKind::TaskReleasedStale,
            ),
        ];
        for (payload, expected_kind) in cases {
            assert_eq!(
                payload.kind(),
                expected_kind,
                "kind() mismatch for {payload:?}"
            );
            // Round-trip preserves the variant.
            let restored = roundtrip(payload.clone());
            assert_eq!(restored, payload, "round-trip mismatch for {payload:?}");
        }
    }

    #[test]
    fn task_inserted_serializes_with_kind_tag() {
        let p = EventPayload::TaskInserted {
            source: "spec".into(),
            fingerprint: Some("fp1".into()),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["kind"], "task_inserted");
        assert_eq!(v["source"], "spec");
        assert_eq!(v["fingerprint"], "fp1");
    }

    #[test]
    fn task_inserted_omits_fingerprint_when_none() {
        let p = EventPayload::TaskInserted {
            source: "spec".into(),
            fingerprint: None,
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["kind"], "task_inserted");
        assert!(
            v.get("fingerprint").is_none(),
            "fingerprint key must be omitted when None, got {v}"
        );
    }

    #[test]
    fn reconciled_orphan_branch_distinct_from_summary() {
        let orphan = EventPayload::Reconciled(ReconciledPayload::OrphanBranch {
            orphan_branch: "feat/x".into(),
        });
        let summary = EventPayload::Reconciled(ReconciledPayload::Summary { tasks: 5 });
        let v_orphan = serde_json::to_value(&orphan).unwrap();
        let v_summary = serde_json::to_value(&summary).unwrap();
        assert_eq!(v_orphan["kind"], "reconciled");
        assert_eq!(v_orphan["event"], "orphan_branch");
        assert_eq!(v_orphan["orphan_branch"], "feat/x");
        assert_eq!(v_summary["kind"], "reconciled");
        assert_eq!(v_summary["event"], "summary");
        assert_eq!(v_summary["tasks"], 5);
    }

    #[test]
    fn event_new_propagates_fields() {
        let payload = EventPayload::TaskClaimed { attempts: 1 };
        let event = Event::new(
            EventKind::TaskClaimed,
            Some("e1".into()),
            Some(TaskId::from_raw("aaaaaaaaaaaa")),
            payload.clone(),
            at(),
        );
        assert_eq!(event.kind, EventKind::TaskClaimed);
        assert_eq!(event.payload, payload);
        assert_eq!(event.epic_name.as_deref(), Some("e1"));
        assert_eq!(
            event.task_id.as_ref().map(TaskId::as_str),
            Some("aaaaaaaaaaaa")
        );
    }

    #[test]
    #[should_panic(expected = "kind")]
    fn event_new_panics_on_kind_payload_mismatch_in_debug() {
        // Only meaningful in debug builds; release skips the assert and the
        // event is still constructed. This test guards the dev-time check.
        let _ = Event::new(
            EventKind::TaskCompleted,
            None,
            None,
            EventPayload::TaskUnblocked,
            at(),
        );
    }
}
