//! The one forge read `session push-check` makes: does the current branch have
//! an open PR.
//!
//! Its own trait rather than a method on `BranchSyncReader` (ISP): this is the
//! only read in the command that costs a network round-trip, and keeping it
//! separate is what lets the decision prove — by type, and in tests by a call
//! counter — that no silent path ever reaches `gh`.
//!
//! `GhOpenPrReader` delegates to the git subsystem's `GitHubService` so
//! "which PR is open on this branch" is answered in exactly one place.

use crate::git::core::github::{create_github_service, GitHubService, RealGitHubService};

pub trait OpenPrReader {
    /// The open PR number for the current branch, `None` when there is none.
    /// `Err` when the lookup itself failed (no `gh`, not authenticated,
    /// offline) — the caller treats that as "unknown", never as "no PR".
    fn open_pr_number(&self) -> Result<Option<i64>, String>;
}

/// Real reader bound to a project directory — `gh` resolves the repository
/// and the branch from its cwd, so the anchor is not optional (#780).
pub struct GhOpenPrReader {
    github: RealGitHubService,
}

pub fn create_open_pr_reader(project_dir: impl Into<String>) -> GhOpenPrReader {
    GhOpenPrReader {
        github: create_github_service(Some(project_dir.into())),
    }
}

impl OpenPrReader for GhOpenPrReader {
    fn open_pr_number(&self) -> Result<Option<i64>, String> {
        GitHubService::detect_current_pr_number(&self.github)
    }
}
