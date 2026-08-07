//! `session baseline` command — records the repository state a session starts
//! from. Runs on SessionStart, and again from the Stop path as self-healing
//! when the plugin was installed mid-session.

use crate::session::commands::SessionDeps;
use crate::session::core::baseline::{is_valid_session_id, Baseline};

/// Why a baseline was not written. Every variant is a silent no-op for the
/// caller — this command never blocks a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// Missing id, or one that fails the path-traversal guard.
    InvalidSessionId,
    NotAGitRepo,
    StoreError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaselineOutcome {
    Recorded,
    /// Already recorded earlier in this session (resume/compact/clear re-fire).
    AlreadyPresent,
    Skipped(SkipReason),
}

/// Snapshots the repository and stores it as the session baseline, but only if
/// this session has none yet.
pub fn run(deps: &SessionDeps, session_id: &str) -> BaselineOutcome {
    if !is_valid_session_id(session_id) {
        return BaselineOutcome::Skipped(SkipReason::InvalidSessionId);
    }
    if !deps.repo.is_inside_work_tree() {
        return BaselineOutcome::Skipped(SkipReason::NotAGitRepo);
    }
    let snapshot = Baseline {
        head: deps.repo.head(),
        dirty: deps.repo.dirty_files(),
        notified: false,
    };
    match deps.store.save_if_absent(session_id, &snapshot) {
        Ok(true) => BaselineOutcome::Recorded,
        Ok(false) => BaselineOutcome::AlreadyPresent,
        Err(_) => BaselineOutcome::Skipped(SkipReason::StoreError),
    }
}
