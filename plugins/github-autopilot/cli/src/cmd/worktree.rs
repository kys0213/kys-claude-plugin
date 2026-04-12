use crate::git::GitOps;
use anyhow::Result;

pub struct WorktreeService {
    git: Box<dyn GitOps>,
}

pub struct CleanupResult {
    pub worktree_removed: bool,
    pub branch_deleted: bool,
}

pub struct StaleCleanupEntry {
    pub branch: String,
    pub path: String,
    pub had_uncommitted: bool,
    pub worktree_removed: bool,
}

impl WorktreeService {
    pub fn new(git: Box<dyn GitOps>) -> Self {
        Self { git }
    }

    /// CLI entry point: clean up stale draft worktrees and print summary.
    pub fn cleanup_stale_cmd(&self) -> Result<i32> {
        let entries = self.cleanup_stale()?;

        if entries.is_empty() {
            eprintln!("No stale draft worktrees found");
        } else {
            for entry in &entries {
                if entry.had_uncommitted {
                    eprintln!(
                        "Committed partial work in '{}' (branch '{}')",
                        entry.path, entry.branch
                    );
                }
                if entry.worktree_removed {
                    eprintln!("Removed worktree for branch '{}'", entry.branch);
                }
            }
        }

        Ok(0)
    }

    /// Clean up all draft/* worktrees. Uncommitted changes are preserved
    /// as partial commits before removing the worktree. Branches are kept
    /// so the next cycle can resume work.
    pub fn cleanup_stale(&self) -> Result<Vec<StaleCleanupEntry>> {
        let entries = self.git.worktree_list()?;
        let mut results = Vec::new();

        for entry in &entries {
            let branch = match &entry.branch {
                Some(b) if b.starts_with("draft/") => b,
                _ => continue,
            };

            let mut had_uncommitted = false;

            // Best-effort: commit uncommitted changes before removing
            if self
                .git
                .has_uncommitted_changes(&entry.path)
                .unwrap_or(false)
            {
                let msg = format!("wip: partial work for {branch}");
                if self.git.commit_all_in_worktree(&entry.path, &msg).is_ok() {
                    had_uncommitted = true;
                }
            }

            let worktree_removed = self.git.worktree_remove(&entry.path).is_ok();

            results.push(StaleCleanupEntry {
                branch: branch.clone(),
                path: entry.path.clone(),
                had_uncommitted,
                worktree_removed,
            });
        }

        if !results.is_empty() {
            let _ = self.git.worktree_prune();
        }

        Ok(results)
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
