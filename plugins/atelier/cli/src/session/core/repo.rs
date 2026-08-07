//! Repository reads the session commands consume. The trait keeps the commands
//! testable in memory (DIP); `GitRepoReader` shells out to git.
//!
//! Every read is pinned to `project_dir` via `git -C`: a Stop hook's process
//! cwd can be a worktree or a subagent's directory, not the project (#780).
//! Every failure collapses to "nothing" — these back an advisory hook that must
//! stay silent in an empty repo, outside a repo, or after a rebase dropped the
//! baseline commit.

use crate::git::core::shell::exec;
use std::collections::BTreeSet;

pub trait RepoReader {
    fn is_inside_work_tree(&self) -> bool;
    /// Current `HEAD` commit, `None` in an empty repo (or outside one).
    fn head(&self) -> Option<String>;
    /// Paths with staged, unstaged or untracked changes.
    fn dirty_files(&self) -> BTreeSet<String>;
    /// Paths changed by commits between `base_head` and `HEAD`. Empty when the
    /// base commit no longer resolves (rebase, amend, dropped branch).
    fn files_changed_since(&self, base_head: &str) -> BTreeSet<String>;
}

/// Real reader bound to a project directory.
pub struct GitRepoReader {
    project_dir: String,
}

pub fn create_repo_reader(project_dir: impl Into<String>) -> GitRepoReader {
    GitRepoReader {
        project_dir: project_dir.into(),
    }
}

impl GitRepoReader {
    /// `git -C <project_dir> <args...>` → (stdout, exit code).
    fn git(&self, args: &[&str]) -> (String, i32) {
        let mut full = vec!["git", "-C", self.project_dir.as_str()];
        full.extend_from_slice(args);
        let result = exec(&full, None);
        (result.stdout, result.exit_code)
    }
}

/// Parses NUL-separated `git status --porcelain -z` output into paths.
/// Rename/copy entries carry the original path as an extra field, which is
/// consumed and dropped — the new path is what the session changed.
fn parse_status_z(raw: &str) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    let mut fields = raw.split('\0').filter(|f| !f.is_empty());
    while let Some(entry) = fields.next() {
        // Every record is `XY <path>`; anything shorter is malformed.
        if entry.len() < 4 || !entry.is_char_boundary(3) {
            continue;
        }
        let (status, path) = entry.split_at(3);
        if status.starts_with('R') || status.starts_with('C') {
            let _ = fields.next();
        }
        paths.insert(path.to_string());
    }
    paths
}

/// Parses NUL-separated path lists (`--name-only -z`).
fn parse_paths_z(raw: &str) -> BTreeSet<String> {
    raw.split('\0')
        .filter(|p| !p.is_empty())
        .map(|p| p.to_string())
        .collect()
}

impl RepoReader for GitRepoReader {
    fn is_inside_work_tree(&self) -> bool {
        let (stdout, exit) = self.git(&["rev-parse", "--is-inside-work-tree"]);
        exit == 0 && stdout.trim() == "true"
    }

    fn head(&self) -> Option<String> {
        let (stdout, exit) = self.git(&["rev-parse", "HEAD"]);
        let head = stdout.trim().to_string();
        (exit == 0 && !head.is_empty()).then_some(head)
    }

    fn dirty_files(&self) -> BTreeSet<String> {
        let (stdout, exit) = self.git(&["status", "--porcelain", "-z"]);
        if exit != 0 {
            return BTreeSet::new();
        }
        parse_status_z(&stdout)
    }

    fn files_changed_since(&self, base_head: &str) -> BTreeSet<String> {
        let range = format!("{base_head}..HEAD");
        let (stdout, exit) = self.git(&["diff", "--name-only", "-z", &range]);
        if exit != 0 {
            return BTreeSet::new();
        }
        parse_paths_z(&stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_entries_and_drops_rename_origin() {
        let raw = " M src/lib.rs\0?? new.txt\0R  src/new.rs\0src/old.rs\0";
        let paths = parse_status_z(raw);
        assert_eq!(
            paths.into_iter().collect::<Vec<_>>(),
            vec!["new.txt", "src/lib.rs", "src/new.rs"]
        );
    }

    #[test]
    fn parses_empty_status_as_no_paths() {
        assert!(parse_status_z("").is_empty());
    }
}
