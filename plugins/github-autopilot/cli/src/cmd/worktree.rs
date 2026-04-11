use crate::git::GitOps;
use anyhow::Result;

pub struct WorktreeService {
    git: Box<dyn GitOps>,
}

pub struct CleanupResult {
    pub worktree_removed: bool,
    pub branch_deleted: bool,
}

impl WorktreeService {
    pub fn new(git: Box<dyn GitOps>) -> Self {
        Self { git }
    }

    /// CLI entry point: clean up and print summary.
    pub fn cleanup(&self, branch: &str) -> Result<i32> {
        let result = self.cleanup_branch(branch)?;

        if result.worktree_removed {
            eprintln!("Removed worktree for branch '{branch}'");
        }
        if result.branch_deleted {
            eprintln!("Deleted local branch '{branch}'");
        }

        Ok(0)
    }

    /// Clean up the worktree associated with the given branch, delete the local
    /// branch, and prune stale worktree metadata.
    pub fn cleanup_branch(&self, branch: &str) -> Result<CleanupResult> {
        let entries = self.git.worktree_list()?;
        let mut worktree_removed = false;

        for entry in &entries {
            if entry.branch.as_deref() == Some(branch) {
                worktree_removed = self.git.worktree_remove(&entry.path).is_ok();
                break;
            }
        }

        let branch_deleted = self.git.branch_delete(branch).is_ok();
        let _ = self.git.worktree_prune();

        Ok(CleanupResult {
            worktree_removed,
            branch_deleted,
        })
    }
}
