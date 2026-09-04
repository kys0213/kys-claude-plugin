//! Branch synchronisation reads consumed by `session push-check`: is this a
//! repository, is it mid-rebase, what branch is it on, what is the default
//! branch, and how far has the branch drifted from its upstream.
//!
//! Deliberately separate from `RepoReader` (ISP): that trait answers "what did
//! this session change" and every one of its methods is a `git status`/`git
//! diff` read. Nothing in push-check needs those, and nothing in
//! simplify-check needs these — widening one trait would make each command
//! depend on reads it never issues.
//!
//! `GitBranchSyncReader` shells out; like `GitRepoReader` every read is pinned
//! to `project_dir` (`git -C`, or the service's pinned cwd) because a Stop
//! hook's process cwd can be a worktree or a subagent's directory, not the
//! project (#780). Every failure collapses to "unknown", which the decision
//! then reads as "stay silent".

use crate::git::core::git::{create_git_service, GitService, RealGitService};
use crate::git::types::GitSpecialState;
use crate::shared::shell::exec;

/// How far the current branch has drifted from its upstream, as
/// `(behind, ahead)` — commits only on the upstream, and commits only here.
pub type Divergence = (u32, u32);

pub trait BranchSyncReader {
    fn is_inside_work_tree(&self) -> bool;

    /// Rebase/merge/detached *and* the current branch in one snapshot. There is
    /// no separate `current_branch` read: `GitSpecialState` already carries it
    /// precisely so this costs one subprocess rather than two (#778).
    fn special_state(&self) -> GitSpecialState;

    /// The repository's default branch, `None` when detection fails (no
    /// remote, offline, non-standard name that no probe resolved).
    fn default_branch(&self) -> Option<String>;

    /// `(behind, ahead)` against `@{upstream}`, or `None` when the branch has
    /// no upstream configured (never pushed) — absence is not zero drift.
    fn upstream_divergence(&self) -> Option<Divergence>;
}

/// Real reader bound to a project directory.
pub struct GitBranchSyncReader {
    project_dir: String,
    /// Answers the three reads the git subsystem already owns, so the two
    /// subsystems cannot disagree about "inside a repo", "mid-rebase" or
    /// "default branch".
    git: RealGitService,
}

pub fn create_branch_sync_reader(project_dir: impl Into<String>) -> GitBranchSyncReader {
    let project_dir = project_dir.into();
    GitBranchSyncReader {
        git: create_git_service(Some(project_dir.clone())),
        project_dir,
    }
}

/// Parses `git rev-list --left-right --count <upstream>...HEAD` output —
/// two counts, left (behind) then right (ahead), separated by whitespace.
fn parse_divergence(raw: &str) -> Option<Divergence> {
    let mut fields = raw.split_whitespace();
    let behind = fields.next()?.parse().ok()?;
    let ahead = fields.next()?.parse().ok()?;
    Some((behind, ahead))
}

impl BranchSyncReader for GitBranchSyncReader {
    fn is_inside_work_tree(&self) -> bool {
        GitService::is_inside_work_tree(&self.git)
    }

    fn special_state(&self) -> GitSpecialState {
        GitService::get_special_state(&self.git)
    }

    fn default_branch(&self) -> Option<String> {
        GitService::detect_default_branch(&self.git).ok()
    }

    fn upstream_divergence(&self) -> Option<Divergence> {
        let result = exec(
            &[
                "git",
                "-C",
                self.project_dir.as_str(),
                "rev-list",
                "--left-right",
                "--count",
                "@{upstream}...HEAD",
            ],
            None,
        );
        if result.exit_code != 0 {
            // No upstream configured is the common case here, and git also
            // exits non-zero in an empty repo or on a detached HEAD.
            return None;
        }
        parse_divergence(&result.stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_divergence;

    #[test]
    fn parses_behind_then_ahead() {
        assert_eq!(parse_divergence("2\t3\n"), Some((2, 3)));
        assert_eq!(parse_divergence("0 0"), Some((0, 0)));
    }

    #[test]
    fn rejects_incomplete_or_non_numeric_output() {
        assert_eq!(parse_divergence(""), None);
        assert_eq!(parse_divergence("3"), None);
        assert_eq!(parse_divergence("a\tb"), None);
    }
}
