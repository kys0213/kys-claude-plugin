//! log-cleanup cron job — Log file cleanup + worktree TTL cleanup.
//!
//! Global cron job that:
//! 1. Delegates log file cleanup to the existing `daemon::log::cleanup_old_logs`
//! 2. Cleans up preserved worktrees that exceed the TTL (default 7 days)
//!
//! Worktrees are preserved on task failure for debugging. This module
//! ensures they don't accumulate indefinitely.

use std::path::Path;
use std::time::{Duration, SystemTime};

use tracing::{info, warn};

/// Default TTL for preserved worktrees in days.
pub const DEFAULT_WORKTREE_TTL_DAYS: u32 = 7;

/// Result of worktree cleanup.
#[derive(Debug, Default)]
pub struct WorktreeCleanupResult {
    /// Number of worktrees removed.
    pub removed: u32,
    /// Number of worktrees that failed to remove.
    pub errors: u32,
}

/// Remove preserved worktrees that exceed the TTL.
///
/// Scans `workspaces/<repo>/<worktree>` directories, skipping `main` and `claw`
/// (the base clone and claw workspace). For each remaining directory, checks
/// if its modification time exceeds `ttl_days` and removes it.
pub fn cleanup_stale_worktrees(workspaces_dir: &Path, ttl_days: u32) -> WorktreeCleanupResult {
    cleanup_stale_worktrees_with_now(workspaces_dir, ttl_days, SystemTime::now())
}

/// Testable inner implementation with injectable "now" timestamp.
fn cleanup_stale_worktrees_with_now(
    workspaces_dir: &Path,
    ttl_days: u32,
    now: SystemTime,
) -> WorktreeCleanupResult {
    let mut result = WorktreeCleanupResult::default();

    let ttl = Duration::from_secs(u64::from(ttl_days) * 24 * 60 * 60);

    let repo_entries = match std::fs::read_dir(workspaces_dir) {
        Ok(entries) => entries,
        Err(_) => return result,
    };

    for repo_entry in repo_entries.filter_map(Result::ok) {
        if !repo_entry.file_type().is_ok_and(|ft| ft.is_dir()) {
            continue;
        }

        let repo_dir = repo_entry.path();
        let wt_entries = match std::fs::read_dir(&repo_dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for wt_entry in wt_entries.filter_map(Result::ok) {
            if !wt_entry.file_type().is_ok_and(|ft| ft.is_dir()) {
                continue;
            }

            let wt_name = wt_entry.file_name();
            let wt_name_str = wt_name.to_string_lossy();

            // Skip non-worktree directories
            if matches!(wt_name_str.as_ref(), "main" | "claw") || wt_name_str.starts_with('.') {
                continue;
            }

            // Check modification time
            let metadata = match wt_entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let modified = match metadata.modified() {
                Ok(t) => t,
                Err(_) => continue,
            };

            let age = match now.duration_since(modified) {
                Ok(d) => d,
                Err(_) => continue,
            };

            if age > ttl {
                let wt_path = wt_entry.path();
                info!(
                    "removing stale worktree: {} (age: {}d, ttl: {}d)",
                    wt_path.display(),
                    age.as_secs() / 86400,
                    ttl_days,
                );
                match std::fs::remove_dir_all(&wt_path) {
                    Ok(()) => result.removed += 1,
                    Err(e) => {
                        warn!("failed to remove worktree {}: {e}", wt_path.display());
                        result.errors += 1;
                    }
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_worktree(base: &Path, repo: &str, name: &str) -> std::path::PathBuf {
        let path = base.join(repo).join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }

    /// Simulate a stale worktree by using a far-future "now" with the injectable helper.
    /// This avoids needing the `filetime` crate to set modification times.
    fn far_future() -> SystemTime {
        // 30 days from now — any worktree created "now" will appear 30 days old
        SystemTime::now() + Duration::from_secs(30 * 86400)
    }

    #[test]
    fn removes_stale_worktrees() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();

        let stale_wt = create_worktree(ws, "org-repo", "issue-42");

        // Use far-future "now" so the just-created dir appears >7 days old
        let result = cleanup_stale_worktrees_with_now(ws, 7, far_future());
        assert_eq!(result.removed, 1);
        assert!(!stale_wt.exists());
    }

    #[test]
    fn keeps_recent_worktrees() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();

        let fresh_wt = create_worktree(ws, "org-repo", "issue-43");

        // Use real "now" — just created, within TTL
        let result = cleanup_stale_worktrees(ws, 7);
        assert_eq!(result.removed, 0);
        assert!(fresh_wt.exists());
    }

    #[test]
    fn skips_main_and_claw_directories() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();

        let main_dir = create_worktree(ws, "org-repo", "main");
        let claw_dir = create_worktree(ws, "org-repo", "claw");

        // Even with far-future "now", main and claw should be preserved
        let result = cleanup_stale_worktrees_with_now(ws, 7, far_future());
        assert_eq!(result.removed, 0);
        assert!(main_dir.exists());
        assert!(claw_dir.exists());
    }

    #[test]
    fn skips_dotfiles() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();

        let dot_dir = create_worktree(ws, "org-repo", ".git");

        let result = cleanup_stale_worktrees_with_now(ws, 7, far_future());
        assert_eq!(result.removed, 0);
        assert!(dot_dir.exists());
    }

    #[test]
    fn empty_workspaces_dir_returns_zero() {
        let tmp = TempDir::new().unwrap();
        let result = cleanup_stale_worktrees(tmp.path(), 7);
        assert_eq!(result.removed, 0);
        assert_eq!(result.errors, 0);
    }

    #[test]
    fn nonexistent_dir_returns_zero() {
        let result = cleanup_stale_worktrees(Path::new("/nonexistent/dir"), 7);
        assert_eq!(result.removed, 0);
        assert_eq!(result.errors, 0);
    }

    #[test]
    fn mixed_stale_and_fresh() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();

        create_worktree(ws, "org-repo", "issue-1");
        let fresh = create_worktree(ws, "org-repo", "issue-2");

        // Both are just created; use a "now" that is just barely past TTL for issue-1
        // but since both have same mtime (just created), both would be removed.
        // Instead, test with real now to verify fresh dirs are kept.
        let result = cleanup_stale_worktrees(ws, 7);
        assert_eq!(result.removed, 0);
        assert!(fresh.exists());

        // Now with far-future, both should be removed
        let result2 = cleanup_stale_worktrees_with_now(ws, 7, far_future());
        assert_eq!(result2.removed, 2);
    }

    #[test]
    fn zero_ttl_removes_all_worktrees() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();

        create_worktree(ws, "org-repo", "issue-99");

        // TTL=0 + future now means any worktree is stale
        let future_now = SystemTime::now() + Duration::from_secs(10);
        let result = cleanup_stale_worktrees_with_now(ws, 0, future_now);
        assert_eq!(result.removed, 1);
    }
}
