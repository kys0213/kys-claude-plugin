mod mock_git;

use autopilot::cmd::worktree::WorktreeService;
use mock_git::MockGit;

#[test]
fn cleanup_removes_worktree_and_branch_when_found() {
    let git = MockGit::new()
        .with_worktree("/repo/.claude/worktrees/agent-1", Some("feature/issue-42"))
        .with_worktree("/repo/.claude/worktrees/agent-2", Some("draft/issue-99"));

    let svc = WorktreeService::new(Box::new(git));
    let result = svc.cleanup_branch("feature/issue-42").unwrap();

    assert!(result.worktree_removed);
    assert!(result.branch_deleted);
}

#[test]
fn cleanup_skips_remove_when_branch_not_found() {
    let git =
        MockGit::new().with_worktree("/repo/.claude/worktrees/agent-1", Some("feature/issue-42"));

    let svc = WorktreeService::new(Box::new(git));
    let result = svc.cleanup_branch("feature/issue-99").unwrap();

    assert!(!result.worktree_removed);
    assert!(result.branch_deleted);
}

#[test]
fn cleanup_handles_detached_worktree() {
    let git = MockGit::new().with_worktree("/repo/.claude/worktrees/agent-1", None);

    let svc = WorktreeService::new(Box::new(git));
    let result = svc.cleanup_branch("feature/issue-42").unwrap();

    assert!(!result.worktree_removed);
}

#[test]
fn cleanup_handles_empty_worktree_list() {
    let git = MockGit::new();

    let svc = WorktreeService::new(Box::new(git));
    let result = svc.cleanup_branch("feature/issue-42").unwrap();

    assert!(!result.worktree_removed);
    assert!(result.branch_deleted);
}

#[test]
fn cleanup_reports_false_when_worktree_remove_fails() {
    let git = MockGit::new()
        .with_worktree("/repo/.claude/worktrees/agent-1", Some("feature/issue-42"))
        .with_fail_worktree_remove();

    let svc = WorktreeService::new(Box::new(git));
    let result = svc.cleanup_branch("feature/issue-42").unwrap();

    assert!(!result.worktree_removed);
    assert!(result.branch_deleted);
}

#[test]
fn cleanup_reports_false_when_branch_delete_fails() {
    let git = MockGit::new()
        .with_worktree("/repo/.claude/worktrees/agent-1", Some("feature/issue-42"))
        .with_fail_branch_delete();

    let svc = WorktreeService::new(Box::new(git));
    let result = svc.cleanup_branch("feature/issue-42").unwrap();

    assert!(result.worktree_removed);
    assert!(!result.branch_deleted);
}

#[test]
fn cleanup_propagates_worktree_list_error() {
    let git = MockGit::new().with_fail_worktree_list();

    let svc = WorktreeService::new(Box::new(git));
    let result = svc.cleanup_branch("feature/issue-42");

    assert!(result.is_err());
}

// --- cleanup_stale tests ---

#[test]
fn cleanup_stale_commits_and_removes_draft_worktrees() {
    let git = MockGit::new()
        .with_worktree("/repo/.claude/worktrees/a1", Some("draft/issue-42"))
        .with_uncommitted_worktree("/repo/.claude/worktrees/a1")
        .with_worktree("/repo/.claude/worktrees/a2", Some("draft/issue-99"))
        .with_uncommitted_worktree("/repo/.claude/worktrees/a2");

    let svc = WorktreeService::new(Box::new(git));
    let results = svc.cleanup_stale().unwrap();

    assert_eq!(results.len(), 2);
    assert!(results[0].had_uncommitted);
    assert!(results[0].worktree_removed);
    assert!(results[1].had_uncommitted);
    assert!(results[1].worktree_removed);
}

#[test]
fn cleanup_stale_skips_worktree_without_uncommitted_changes() {
    let git = MockGit::new().with_worktree("/repo/.claude/worktrees/a1", Some("draft/issue-42"));

    let svc = WorktreeService::new(Box::new(git));
    let results = svc.cleanup_stale().unwrap();

    assert_eq!(results.len(), 1);
    assert!(!results[0].had_uncommitted);
    assert!(results[0].worktree_removed);
}

#[test]
fn cleanup_stale_ignores_non_draft_worktrees() {
    let git = MockGit::new()
        .with_worktree("/repo", Some("main"))
        .with_worktree("/repo/.claude/worktrees/a1", Some("feature/issue-42"))
        .with_worktree("/repo/.claude/worktrees/a2", Some("draft/issue-99"));

    let svc = WorktreeService::new(Box::new(git));
    let results = svc.cleanup_stale().unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].branch, "draft/issue-99");
}

#[test]
fn cleanup_stale_returns_empty_when_no_worktrees() {
    let git = MockGit::new();

    let svc = WorktreeService::new(Box::new(git));
    let results = svc.cleanup_stale().unwrap();

    assert!(results.is_empty());
}

#[test]
fn cleanup_stale_continues_when_commit_fails() {
    let git = MockGit::new()
        .with_worktree("/repo/.claude/worktrees/a1", Some("draft/issue-42"))
        .with_uncommitted_worktree("/repo/.claude/worktrees/a1")
        .with_fail_commit();

    let svc = WorktreeService::new(Box::new(git));
    let results = svc.cleanup_stale().unwrap();

    assert_eq!(results.len(), 1);
    assert!(!results[0].had_uncommitted); // commit failed, so not marked
    assert!(results[0].worktree_removed);
}
