//! Ledger-native stagnation detection (`autopilot check stagnation`).
//!
//! Per `plans/ledger-stagnation-redesign.md`, the GitHub-issue-body
//! fingerprint flow is retired and stagnation is now grouped over ledger
//! tasks via a hybrid (simhash hamming distance OR path Jaccard) similarity
//! query. The CLI is a deterministic primitive: same input → same output,
//! threshold decisions live in the calling Skill (CLAUDE.md "책임 경계").
//!
//! The service:
//! 1. Fetches the current task by id.
//! 2. Asks the [`TaskStore`] for similar tasks via
//!    [`TaskRepo::list_similar_tasks`] (hybrid OR; set is the union).
//! 3. Counts the candidates `N` and bands the result:
//!    - `N < n_threshold` → `status="ok"`, exit 0.
//!    - `n_threshold ≤ N < n_escalate` → `status="stagnation"`, exit 4.
//!    - `N ≥ n_escalate` → `status="escalate"`, exit 5.
//! 4. Emits the spec §3.10 JSON to `out` so hooks / Skills can read it
//!    directly without touching SQLite.
//!
//! `recommended_persona` follows the same rotation as
//! `cmd::issue::filter_comments` (`PERSONAS` array) so a human reading
//! both surfaces sees a consistent "next persona" suggestion.

use std::io::Write;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::cmd::output::write_json;
use crate::domain::event::{EventKind, EventPayload};
use crate::domain::simhash::{hamming_distance, jaccard_similarity};
use crate::domain::{Task, TaskId, TaskStatus};
use crate::ports::task_store::{EventFilter, TaskStore};

/// Exit code: `n_threshold ≤ N < n_escalate` similar tasks detected. Hook
/// converts this into a redirect prompt without auto-escalating.
pub const EXIT_STAGNATION: i32 = 4;

/// Exit code: `N ≥ n_escalate` similar tasks detected. Hook should call
/// `autopilot task escalate` automatically.
pub const EXIT_ESCALATE: i32 = 5;

/// Persona rotation order. Mirrors `cmd::issue::PERSONAS` so the same
/// `consecutive_failures → persona` mapping appears in both filter-comments
/// and ledger stagnation outputs.
pub const PERSONAS: &[&str] = &[
    "hacker",
    "researcher",
    "simplifier",
    "architect",
    "contrarian",
];

/// Configuration for [`StagnationService::check`]. Every field is required
/// — the CLI surface fills defaults via clap (`max-distance=3`,
/// `min-jaccard=0.5`, `n-threshold=3`, `n-escalate=5`).
#[derive(Debug, Clone, Copy)]
pub struct StagnationConfig {
    /// Hamming distance upper bound (inclusive) for the simhash dimension.
    /// Spec §3.4 default `T = 3`.
    pub max_distance: u32,
    /// Jaccard ratio lower bound (inclusive) for the path-set dimension.
    /// Spec §3.4 default `J = 0.5`.
    pub min_jaccard: f64,
    /// `N`: candidate count that flips status from `ok` to `stagnation`.
    /// Default 3.
    pub n_threshold: u32,
    /// `N_esc`: candidate count that flips status to `escalate`. Default 5.
    /// Must satisfy `n_escalate >= n_threshold`; not asserted because clap
    /// already accepts both as user input — invalid combinations are
    /// surfaced to the operator via the band logic (escalate dominates).
    pub n_escalate: u32,
}

impl Default for StagnationConfig {
    fn default() -> Self {
        Self {
            max_distance: 3,
            min_jaccard: 0.5,
            n_threshold: 3,
            n_escalate: 5,
        }
    }
}

/// Stagnation status banding. Drives both the CLI exit code and the JSON
/// `status` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StagnationStatus {
    Ok,
    Stagnation,
    Escalate,
}

impl StagnationStatus {
    pub fn exit_code(self) -> i32 {
        match self {
            StagnationStatus::Ok => 0,
            StagnationStatus::Stagnation => EXIT_STAGNATION,
            StagnationStatus::Escalate => EXIT_ESCALATE,
        }
    }
}

/// Outcome label for a single similar task. Mirrors spec §3.10
/// (`failed | completed | wip | ...`). Derived from the live `TaskStatus`,
/// not from event history — the events log is consulted only to extract
/// `failure_reason`.
fn outcome_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Done => "completed",
        TaskStatus::Escalated => "failed",
        TaskStatus::Wip => "wip",
        TaskStatus::Ready => "ready",
        TaskStatus::Pending => "pending",
        TaskStatus::Blocked => "blocked",
    }
}

/// Reads the most recent `TaskFailed` event payload for a task and returns
/// a short human-readable reason string. The current `EventPayload::TaskFailed`
/// only carries `is_final` and `attempts` — there is no free-form `reason`
/// field today, so we synthesize one. Callers should treat `None` as
/// "task hasn't failed yet".
fn derive_failure_reason(store: &dyn TaskStore, task_id: &TaskId) -> Option<String> {
    let events = store
        .list_events(EventFilter {
            task: Some(task_id.clone()),
            kinds: vec![EventKind::TaskFailed],
            ..Default::default()
        })
        .ok()?;
    let last = events.last()?;
    if let EventPayload::TaskFailed { is_final, attempts } = &last.payload {
        let phase = if *is_final { "final" } else { "retry" };
        Some(format!("{phase} after {attempts} attempts"))
    } else {
        None
    }
}

/// Service that orchestrates ledger-based stagnation detection. Holds only
/// references — the caller decides the lifetime of the store. Mirrors the
/// shape of [`crate::cmd::task::TaskService`] so wiring in `main.rs` is
/// uniform.
pub struct StagnationService<'a> {
    store: &'a dyn TaskStore,
}

impl<'a> StagnationService<'a> {
    pub fn new(store: &'a dyn TaskStore) -> Self {
        Self { store }
    }

    /// Runs the full check pipeline for `task_id` and writes the spec §3.10
    /// JSON to `out`. Returns the exit code (0 / 4 / 5) for `main.rs` to
    /// propagate to the OS.
    ///
    /// Errors:
    /// - `task_id` not found → wraps a `TaskStoreError::NotFound` for the
    ///   `main.rs` mapper, which exits 2 by default.
    /// - missing `simhash` and `affected_paths` on the current task → still
    ///   runs the query (treated as `simhash = 0`, `paths = []`); typically
    ///   yields zero candidates → `status="ok"` exit 0.
    pub fn check(
        &self,
        task_id: &TaskId,
        config: &StagnationConfig,
        out: &mut dyn Write,
    ) -> Result<i32> {
        let current = self
            .store
            .get_task(task_id)
            .with_context(|| format!("loading task '{task_id}'"))?
            .ok_or_else(|| anyhow::anyhow!("task '{task_id}' not found"))?;

        let cur_simhash = current.simhash.unwrap_or(0);
        let cur_paths: Vec<String> = current.affected_paths.clone().unwrap_or_default();

        let similar = self
            .store
            .list_similar_tasks(
                &current.id,
                cur_simhash,
                config.max_distance,
                &cur_paths,
                config.min_jaccard,
            )
            .with_context(|| format!("listing similar tasks for '{task_id}'"))?;

        let n = similar.len() as u32;
        let status = if n >= config.n_escalate {
            StagnationStatus::Escalate
        } else if n >= config.n_threshold {
            StagnationStatus::Stagnation
        } else {
            StagnationStatus::Ok
        };

        let report = build_report(self.store, &current, &similar, status);
        write_json(out, &report)?;
        Ok(status.exit_code())
    }
}

// ── JSON report shaping ────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct CurrentTaskJson {
    id: String,
    simhash: Option<String>,
    affected_paths: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SimilarityJson {
    simhash_distance: Option<u32>,
    jaccard: f64,
    shared_paths: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SimilarTaskJson {
    id: String,
    title: String,
    similarity: SimilarityJson,
    outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure_reason: Option<String>,
    completed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
struct PatternJson {
    shared_paths: Vec<String>,
    common_failure_categories: Vec<String>,
    consecutive_failures: u32,
}

#[derive(Debug, Serialize)]
struct StagnationReport {
    status: StagnationStatus,
    current_task: CurrentTaskJson,
    similar_tasks: Vec<SimilarTaskJson>,
    pattern: PatternJson,
    recommended_persona: Option<&'static str>,
}

fn build_report(
    store: &dyn TaskStore,
    current: &Task,
    similar: &[Task],
    status: StagnationStatus,
) -> StagnationReport {
    let cur_paths_owned: Vec<String> = current.affected_paths.clone().unwrap_or_default();

    let mut similar_json: Vec<SimilarTaskJson> = Vec::with_capacity(similar.len());
    let mut failures_among_similar: u32 = 0;
    for t in similar {
        let simhash_distance = match (current.simhash, t.simhash) {
            (Some(a), Some(b)) => Some(hamming_distance(a, b)),
            _ => None,
        };
        let other_paths = t.affected_paths.clone().unwrap_or_default();
        let jaccard = jaccard_similarity(&cur_paths_owned, &other_paths);
        let shared_paths: Vec<String> = match &t.affected_paths {
            Some(p) => p
                .iter()
                .filter(|x| cur_paths_owned.iter().any(|c| c == *x))
                .cloned()
                .collect(),
            None => Vec::new(),
        };
        if matches!(t.status, TaskStatus::Escalated) {
            failures_among_similar += 1;
        }
        let failure_reason = derive_failure_reason(store, &t.id);
        similar_json.push(SimilarTaskJson {
            id: t.id.as_str().to_string(),
            title: t.title.clone(),
            similarity: SimilarityJson {
                simhash_distance,
                jaccard,
                shared_paths,
            },
            outcome: outcome_label(t.status),
            failure_reason,
            completed_at: t.updated_at,
        });
    }

    // Pattern.shared_paths = paths present in EVERY similar task AND in
    // the current task. Determined intersectionally; degrades gracefully
    // when no candidate has a path set (in which case `all` over an empty
    // iter is vacuously true — guard explicitly).
    let any_candidate_has_paths = similar
        .iter()
        .any(|t| t.affected_paths.as_ref().is_some_and(|p| !p.is_empty()));
    let pattern_shared: Vec<String> = if !cur_paths_owned.is_empty() && any_candidate_has_paths {
        cur_paths_owned
            .iter()
            .filter(|p| {
                similar
                    .iter()
                    .filter_map(|t| t.affected_paths.as_ref())
                    .all(|sp| sp.iter().any(|s| s == *p))
            })
            .cloned()
            .collect()
    } else {
        Vec::new()
    };

    // The persona table is keyed by `consecutive` per `cmd::issue.rs:553`:
    // 2 → PERSONAS[0], 3 → PERSONAS[1], capped at the last entry. We use
    // the count of similar tasks (which already excludes the current one)
    // plus 1 for the current attempt, mirroring the "consecutive failures
    // including this one" semantic.
    let _ = failures_among_similar; // reserved for future
    let recommended_persona = match status {
        StagnationStatus::Ok => None,
        _ => {
            let consecutive = (similar.len() as u32 + 1).max(2);
            let idx = (consecutive as usize - 2).min(PERSONAS.len() - 1);
            Some(PERSONAS[idx])
        }
    };

    StagnationReport {
        status,
        current_task: CurrentTaskJson {
            id: current.id.as_str().to_string(),
            simhash: current.simhash.map(|h| format!("0x{h:016x}")),
            affected_paths: cur_paths_owned,
        },
        similar_tasks: similar_json,
        pattern: PatternJson {
            shared_paths: pattern_shared,
            common_failure_categories: Vec::new(),
            consecutive_failures: similar.len() as u32,
        },
        recommended_persona,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_label_maps_terminal_states() {
        assert_eq!(outcome_label(TaskStatus::Done), "completed");
        assert_eq!(outcome_label(TaskStatus::Escalated), "failed");
        assert_eq!(outcome_label(TaskStatus::Wip), "wip");
    }

    #[test]
    fn config_defaults_match_spec() {
        let c = StagnationConfig::default();
        assert_eq!(c.max_distance, 3);
        assert!((c.min_jaccard - 0.5).abs() < 1e-9);
        assert_eq!(c.n_threshold, 3);
        assert_eq!(c.n_escalate, 5);
    }

    #[test]
    fn status_exit_codes() {
        assert_eq!(StagnationStatus::Ok.exit_code(), 0);
        assert_eq!(StagnationStatus::Stagnation.exit_code(), 4);
        assert_eq!(StagnationStatus::Escalate.exit_code(), 5);
    }
}
