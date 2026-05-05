//! Ledger event detector for `autopilot watch`.
//!
//! Polls the SQLite events table on each tick and emits the high-level
//! ledger events that Monitor's dispatcher consumes:
//!
//! | Stdout shape | Source |
//! |---|---|
//! | `TASK_READY epic=<EPIC> task_id=<ID>` | `task_inserted` / `task_unblocked` events whose target task is currently `Ready` |
//! | `EPIC_DONE epic=<EPIC> total=<N>` | `task_completed` event when every task in the epic is now `Done` |
//! | `STALE_WIP candidates=<JSON> epic=<EPIC>` | `list_stale(now - threshold)` result, deduplicated per task id |
//!
//! Idempotency is preserved across ticks (and daemon restarts) by tracking:
//! - `last_event_at`: only events strictly newer than this `at` are considered
//! - `seen_event_keys`: events whose `at == last_event_at` (same-second ties) — prevents replay when many events share a timestamp
//! - `epics_done`: epics for which `EPIC_DONE` has already been emitted
//! - `stale_seen`: task ids that have already produced a `STALE_WIP` line
//!
//! Per `CLAUDE.md` "책임 경계", this is deterministic state tracking — judgment
//! (whether to release / fail / escalate a stale task) belongs to Skill/Agent.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::cmd::watch::WatchEvent;
use crate::domain::{EventKind, TaskId, TaskStatus};
use crate::ports::task_store::{EventFilter, TaskStore};

/// Persisted ledger detector state. Serialized into the existing
/// `watch.json` file alongside push / CI / issue cursors.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LedgerState {
    /// Timestamp of the most recent event observed in any prior tick.
    /// `None` means "never polled yet" — first tick will seed it.
    #[serde(default)]
    pub last_event_at: Option<DateTime<Utc>>,
    /// Event keys (`(kind, task_id, at)`) already emitted at exactly
    /// `last_event_at`. Required because multiple events can share a
    /// second-level timestamp; without this we would either replay or
    /// drop events on the boundary.
    #[serde(default)]
    pub seen_keys: BTreeSet<String>,
    /// Epics for which `EPIC_DONE` has already been emitted. Stays set
    /// across restarts so the event fires exactly once per epic.
    #[serde(default)]
    pub epics_done: BTreeSet<String>,
    /// Task ids already reported as `STALE_WIP`. Reset when the task
    /// leaves Wip (release / complete / fail) on the next tick.
    #[serde(default)]
    pub stale_seen: BTreeSet<String>,
}

impl LedgerState {
    /// Seeds `last_event_at` to `now` so the first tick after a fresh
    /// daemon start does not emit historical events.
    pub fn seed(&mut self, now: DateTime<Utc>) {
        if self.last_event_at.is_none() {
            self.last_event_at = Some(now);
        }
    }

    /// Clamps a future `last_event_at` cursor down to `now` and clears
    /// any dedupe keys tied to the now-stale boundary timestamp.
    ///
    /// Returns the original future timestamp when a clamp happens (so
    /// callers can warn once), or `None` when the cursor is already
    /// `<= now` (the normal case — no-op, no warn).
    ///
    /// Why this exists: `last_event_at` is loaded from `watch.json` and
    /// fed to `TaskStore::list_events` as `WHERE at >= since`. If it's
    /// ever in the future (NTP correction, OS clock manipulation,
    /// `watch.json` copied across hosts), every freshly-inserted event
    /// gets filtered out and `TASK_READY` / `EPIC_DONE` / `STALE_WIP`
    /// emission freezes until real time catches up. Clamping at the read
    /// boundary unfreezes emission immediately while preserving history.
    pub fn clamp_future_cursor(&mut self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self.last_event_at {
            Some(at) if at > now => {
                self.last_event_at = Some(now);
                // The dedupe set is keyed by the (now-stale) future
                // timestamp; clearing avoids accidentally suppressing
                // legitimate events that happen to land at `now`.
                self.seen_keys.clear();
                Some(at)
            }
            _ => None,
        }
    }
}

fn key_of(kind: EventKind, task_id: Option<&TaskId>, at: DateTime<Utc>) -> String {
    format!(
        "{}|{}|{}",
        kind.as_str(),
        task_id.map(|t| t.as_str()).unwrap_or(""),
        at.timestamp_micros()
    )
}

/// Runs one detection tick: queries the store for new ledger events and
/// stale Wip tasks, mutates `state` to remember what was emitted, and
/// returns the WatchEvents to print.
///
/// Pure with respect to time (`now` is injected) and the store
/// (`&dyn TaskStore`), so blackbox tests drive it with `InMemoryTaskStore`
/// and a `FixedClock`.
pub fn detect_ledger_events(
    store: &dyn TaskStore,
    state: &mut LedgerState,
    now: DateTime<Utc>,
    stale_threshold_secs: i64,
) -> Vec<WatchEvent> {
    let mut out: Vec<WatchEvent> = Vec::new();

    // ── Phase 1: deltas from events table ──
    if let Some(events) = fetch_new_events(store, state) {
        // `events` is ordered by `at ASC` (sqlite ORDER BY id ASC matches insertion order).
        let boundary = state.last_event_at;
        let mut max_at = boundary.unwrap_or(now);
        let mut new_keys: BTreeSet<String> = BTreeSet::new();

        for ev in &events {
            let key = key_of(ev.kind, ev.task_id.as_ref(), ev.at);
            // Skip if already emitted in a prior tick (same-timestamp boundary).
            if Some(ev.at) == boundary && state.seen_keys.contains(&key) {
                continue;
            }

            match ev.kind {
                EventKind::TaskInserted | EventKind::TaskUnblocked => {
                    if let Some(task_id) = &ev.task_id {
                        if let Ok(Some(task)) = store.get_task(task_id) {
                            if task.status == TaskStatus::Ready {
                                out.push(WatchEvent::TaskReady {
                                    epic: task.epic_name,
                                    task_id: task_id.as_str().to_string(),
                                });
                            }
                        }
                    }
                }
                EventKind::TaskCompleted => {
                    if let Some(epic) = &ev.epic_name {
                        if !state.epics_done.contains(epic) {
                            if let Ok(tasks) = store.list_tasks_by_epic(epic, None) {
                                if !tasks.is_empty()
                                    && tasks.iter().all(|t| t.status == TaskStatus::Done)
                                {
                                    out.push(WatchEvent::EpicDone {
                                        epic: epic.clone(),
                                        total: tasks.len() as u64,
                                    });
                                    state.epics_done.insert(epic.clone());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            if ev.at > max_at {
                max_at = ev.at;
                new_keys.clear();
            }
            if ev.at == max_at {
                new_keys.insert(key);
            }
        }

        // Persist cursor: keep state.seen_keys for ties at max_at; if max_at
        // advanced we replaced the set; if no events arrived we keep old keys.
        if !events.is_empty() {
            state.last_event_at = Some(max_at);
            state.seen_keys = state.seen_keys.union(&new_keys).cloned().collect();
            // Drop keys older than max_at (they can't conflict anymore).
            let suffix = format!("|{}", max_at.timestamp_micros());
            state.seen_keys.retain(|k| k.ends_with(&suffix));
        }
    }

    // ── Phase 2: stale Wip from current state (not events) ──
    let stale_before = now - chrono::Duration::seconds(stale_threshold_secs);
    if let Ok(stale) = store.list_stale(stale_before) {
        let current_ids: HashSet<String> =
            stale.iter().map(|t| t.id.as_str().to_string()).collect();
        // Forget ids that are no longer stale (task moved out of Wip).
        state.stale_seen.retain(|id| current_ids.contains(id));

        // Group remaining (un-emitted) candidates by epic, preserving id order.
        let mut by_epic: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for t in &stale {
            let id = t.id.as_str().to_string();
            if state.stale_seen.contains(&id) {
                continue;
            }
            by_epic.entry(t.epic_name.clone()).or_default().push(id);
        }
        for (epic, ids) in by_epic {
            for id in &ids {
                state.stale_seen.insert(id.clone());
            }
            out.push(WatchEvent::StaleWip {
                epic,
                candidates: ids,
            });
        }
    }

    out
}

fn fetch_new_events(
    store: &dyn TaskStore,
    state: &LedgerState,
) -> Option<Vec<crate::domain::Event>> {
    let filter = EventFilter {
        epic: None,
        task: None,
        kinds: vec![
            EventKind::TaskInserted,
            EventKind::TaskUnblocked,
            EventKind::TaskCompleted,
        ],
        since: state.last_event_at,
        limit: None,
    };
    store.list_events(filter).ok()
}
