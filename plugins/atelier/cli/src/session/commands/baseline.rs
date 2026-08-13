//! `session baseline` command — records the repository state a session starts
//! from. Runs on SessionStart, and again from the Stop path as self-healing
//! when the plugin was installed mid-session.

use crate::session::commands::SessionDeps;
use crate::session::core::baseline::{is_valid_session_id, Baseline};

/// Snapshots the repository and stores it as the session baseline, but only if
/// this session has none yet.
///
/// Returns nothing: every way this can decline — a rejected id, no repository,
/// a failed write — is a silent no-op, and both callers act the same on all of
/// them. This command never blocks a session.
pub fn run(deps: &SessionDeps, session_id: &str) {
    if !is_valid_session_id(session_id) {
        return;
    }
    // SessionStart re-fires on resume/compact/clear, and `save_if_absent`
    // discards the snapshot every time after it. Ask the store first: one file
    // read settles it, where the snapshot below costs three git processes.
    if deps.store.load(session_id).is_some() {
        return;
    }
    if !deps.repo.is_inside_work_tree() {
        return;
    }
    let snapshot = Baseline {
        head: deps.repo.head(),
        dirty: deps.repo.dirty_files(),
        notified: false,
    };
    // Still `save_if_absent`, not `save`: the early return above is a fast
    // path, this is the guarantee that a concurrent write is never clobbered.
    let _ = deps.store.save_if_absent(session_id, &snapshot);
}
