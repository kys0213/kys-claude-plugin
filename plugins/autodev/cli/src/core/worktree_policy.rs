//! Worktree lifecycle policy — phase별 worktree 보존/정리 규칙.
//!
//! v5 스펙의 worktree 생명주기 규칙을 코어 도메인에 정의한다.
//! Task가 완료될 때 TaskStatus에 따라 worktree를 보존할지 정리할지 결정한다.
//!
//! | Phase / Status | Worktree Action |
//! |----------------|-----------------|
//! | Running        | Preserve (생성/유지) |
//! | Completed      | Cleanup (정상 완료) |
//! | Done           | Cleanup |
//! | Skipped        | Cleanup |
//! | Failed         | Preserve (디버깅용) |
//! | HITL           | Preserve (사람 확인 대기) |
//! | Retry          | Preserve (재시도 시 재사용) |

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Worktree 정리/보존 결정.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeAction {
    /// Worktree를 보존한다 (Failed, HITL, Retry 등).
    Preserve,
    /// Worktree를 정리한다 (Done, Skipped, Completed 등).
    Cleanup,
}

impl fmt::Display for WorktreeAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorktreeAction::Preserve => write!(f, "preserve"),
            WorktreeAction::Cleanup => write!(f, "cleanup"),
        }
    }
}

/// TaskStatus에 따른 worktree lifecycle 정책.
///
/// v5 스펙 원칙: "Done이 되어야만 정리한다" + "Failed는 디버깅을 위해 보존"
///
/// - Completed: 작업이 정상 완료됨 → 정리
/// - Skipped: 건너뜀 (preflight 실패 등) → 정리
/// - Failed: 실패 → 보존 (디버깅용)
pub fn resolve_action(status: &super::task::TaskStatus) -> WorktreeAction {
    match status {
        super::task::TaskStatus::Completed => WorktreeAction::Cleanup,
        super::task::TaskStatus::Skipped(_) => WorktreeAction::Cleanup,
        super::task::TaskStatus::Failed(_) => WorktreeAction::Preserve,
    }
}

/// TTL 초과 worktree 경로 목록을 반환한다.
///
/// `workspaces_root` 하위의 `<repo>/<task_id>` 디렉토리를 스캔하여
/// 수정 시간이 TTL을 초과한 worktree를 식별한다.
/// `main`, `claw`, `.`으로 시작하는 디렉토리는 worktree가 아니므로 제외한다.
pub fn find_stale_worktrees(workspaces_root: &Path, ttl: Duration) -> Vec<PathBuf> {
    let mut stale = Vec::new();
    let now = SystemTime::now();

    let repo_dirs = match std::fs::read_dir(workspaces_root) {
        Ok(dirs) => dirs,
        Err(_) => return stale,
    };

    for repo_entry in repo_dirs.flatten() {
        if !repo_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }

        let wt_dirs = match std::fs::read_dir(repo_entry.path()) {
            Ok(dirs) => dirs,
            Err(_) => continue,
        };

        for wt_entry in wt_dirs.flatten() {
            if !wt_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }

            let wt_name = wt_entry.file_name().to_string_lossy().to_string();
            // Skip known non-worktree dirs
            if matches!(wt_name.as_str(), "main" | "claw") || wt_name.starts_with('.') {
                continue;
            }

            let modified = wt_entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(now);

            if let Ok(age) = now.duration_since(modified) {
                if age > ttl {
                    stale.push(wt_entry.path());
                }
            }
        }
    }

    stale.sort();
    stale
}

/// 기본 TTL: 7일 (v5 스펙의 log-cleanup cron TTL).
pub const DEFAULT_TTL_SECS: u64 = 7 * 24 * 60 * 60;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::task::{SkipReason, TaskStatus};

    // ═══════════════════════════════════════════════
    // resolve_action tests
    // ═══════════════════════════════════════════════

    #[test]
    fn completed_task_triggers_cleanup() {
        let action = resolve_action(&TaskStatus::Completed);
        assert_eq!(action, WorktreeAction::Cleanup);
    }

    #[test]
    fn skipped_task_triggers_cleanup() {
        let status = TaskStatus::Skipped(SkipReason::AlreadyProcessed);
        let action = resolve_action(&status);
        assert_eq!(action, WorktreeAction::Cleanup);
    }

    #[test]
    fn skipped_preflight_triggers_cleanup() {
        let status = TaskStatus::Skipped(SkipReason::PreflightFailed("issue closed".into()));
        let action = resolve_action(&status);
        assert_eq!(action, WorktreeAction::Cleanup);
    }

    #[test]
    fn failed_task_preserves_worktree() {
        let status = TaskStatus::Failed("agent timeout".into());
        let action = resolve_action(&status);
        assert_eq!(action, WorktreeAction::Preserve);
    }

    #[test]
    fn action_display() {
        assert_eq!(WorktreeAction::Preserve.to_string(), "preserve");
        assert_eq!(WorktreeAction::Cleanup.to_string(), "cleanup");
    }

    // ═══════════════════════════════════════════════
    // find_stale_worktrees tests
    // ═══════════════════════════════════════════════

    #[test]
    fn find_stale_returns_empty_for_missing_dir() {
        let result = find_stale_worktrees(Path::new("/nonexistent"), Duration::from_secs(0));
        assert!(result.is_empty());
    }

    #[test]
    fn find_stale_skips_main_and_claw_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_dir = tmp.path().join("org-repo");
        std::fs::create_dir_all(repo_dir.join("main")).unwrap();
        std::fs::create_dir_all(repo_dir.join("claw")).unwrap();
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();

        // TTL=0 means everything is stale
        let result = find_stale_worktrees(tmp.path(), Duration::from_secs(0));
        assert!(result.is_empty());
    }

    #[test]
    fn find_stale_detects_old_worktrees() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_dir = tmp.path().join("org-repo");
        let wt_dir = repo_dir.join("issue-42");
        std::fs::create_dir_all(&wt_dir).unwrap();

        // TTL=0 means everything is immediately stale
        let result = find_stale_worktrees(tmp.path(), Duration::from_secs(0));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], wt_dir);
    }

    #[test]
    fn find_stale_excludes_recent_worktrees() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_dir = tmp.path().join("org-repo");
        std::fs::create_dir_all(repo_dir.join("issue-42")).unwrap();

        // TTL = 1 hour, worktree was just created
        let result = find_stale_worktrees(tmp.path(), Duration::from_secs(3600));
        assert!(result.is_empty());
    }

    #[test]
    fn find_stale_multiple_repos() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("repo-a").join("issue-1")).unwrap();
        std::fs::create_dir_all(tmp.path().join("repo-a").join("main")).unwrap();
        std::fs::create_dir_all(tmp.path().join("repo-b").join("pr-5")).unwrap();
        std::fs::create_dir_all(tmp.path().join("repo-b").join("claw")).unwrap();

        let result = find_stale_worktrees(tmp.path(), Duration::from_secs(0));
        assert_eq!(result.len(), 2);
        // Sorted output
        assert!(result[0].ends_with("issue-1"));
        assert!(result[1].ends_with("pr-5"));
    }
}
