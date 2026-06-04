//! `branch` command, ported from git-utils `commands/branch.ts`
//! (originally `create-branch.sh`). Creates a new branch off a base branch,
//! validating preconditions and delegating all git work to [`GitService`].

use crate::git::core::git::{BranchLocation, GitService};
use crate::git::types::{BranchInput, BranchOutput};

/// The `branch` command, backed by an injected [`GitService`].
pub struct BranchCommand<'a> {
    git: &'a dyn GitService,
}

impl<'a> BranchCommand<'a> {
    pub fn new(git: &'a dyn GitService) -> Self {
        Self { git }
    }

    pub fn run(&self, input: &BranchInput) -> Result<BranchOutput, String> {
        if input.branch_name.trim().is_empty() {
            return Err("Branch name is required".to_string());
        }

        if self
            .git
            .has_uncommitted_changes()
            .map_err(|e| e.to_string())?
        {
            return Err("Uncommitted changes detected. Please commit or stash first.".to_string());
        }

        // Determine base branch (explicit > detected).
        let base = match &input.base_branch {
            Some(b) => b.clone(),
            None => self
                .git
                .detect_default_branch()
                .map_err(|e| e.to_string())?,
        };

        if !self
            .git
            .branch_exists(&base, BranchLocation::Any)
            .map_err(|e| e.to_string())?
        {
            return Err(format!(
                "Base branch '{base}' does not exist locally or remotely."
            ));
        }

        if self
            .git
            .branch_exists(&input.branch_name, BranchLocation::Local)
            .map_err(|e| e.to_string())?
        {
            return Err(format!("Branch '{}' already exists.", input.branch_name));
        }

        // Fetch (best-effort).
        let _ = self.git.fetch(None);

        // Sync the base: checkout + pull if local, otherwise create a tracking
        // branch from the remote.
        let local_exists = self
            .git
            .branch_exists(&base, BranchLocation::Local)
            .map_err(|e| e.to_string())?;
        if local_exists {
            self.git
                .checkout(&base, false, None)
                .map_err(|e| e.to_string())?;
            let _ = self.git.pull(&base);
        } else {
            let track = format!("origin/{base}");
            self.git
                .checkout(&base, true, Some(&track))
                .map_err(|e| e.to_string())?;
        }

        // Create and switch to the new branch.
        self.git
            .checkout(&input.branch_name, true, None)
            .map_err(|e| e.to_string())?;

        Ok(BranchOutput {
            branch_name: input.branch_name.clone(),
            base_branch: base,
        })
    }
}
