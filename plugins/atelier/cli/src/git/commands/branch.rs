//! `branch` command — port of `git-utils/src/commands/branch.ts`. Creates a
//! new branch off a (detected or supplied) base after sync. `fetch`/`pull`
//! failures are ignored; the final `checkout -b` failure is returned as a
//! handled error, while a base-checkout failure propagates (outer `Err`),
//! matching the TS try/catch placement.

use crate::git::core::git::{BranchLocation, CheckoutOptions, GitService};
use crate::git::types::{BranchInput, BranchOutput, CmdResult};

pub struct BranchDeps<'a> {
    pub git: &'a dyn GitService,
}

/// Runs the branch command.
pub fn run(deps: &BranchDeps, input: &BranchInput) -> Result<CmdResult<BranchOutput>, String> {
    if input.branch_name.trim().is_empty() {
        return Ok(CmdResult::Err("Branch name is required".to_string()));
    }

    if deps.git.has_uncommitted_changes() {
        return Ok(CmdResult::Err(
            "Uncommitted changes detected. Please commit or stash first.".to_string(),
        ));
    }

    // Determine base branch.
    let base_branch = match &input.base_branch {
        Some(b) => b.clone(),
        None => match deps.git.detect_default_branch() {
            Ok(b) => b,
            Err(e) => return Ok(CmdResult::Err(e)),
        },
    };

    if !deps.git.branch_exists(&base_branch, BranchLocation::Any) {
        return Ok(CmdResult::Err(format!(
            "Base branch '{base_branch}' does not exist locally or remotely."
        )));
    }

    if deps
        .git
        .branch_exists(&input.branch_name, BranchLocation::Local)
    {
        return Ok(CmdResult::Err(format!(
            "Branch '{}' already exists.",
            input.branch_name
        )));
    }

    // Fetch (ignore errors — fetch() already swallows them).
    let _ = deps.git.fetch(None);

    // Checkout base + pull, or create tracking branch from remote.
    let local_exists = deps.git.branch_exists(&base_branch, BranchLocation::Local);
    if local_exists {
        // Not wrapped in try/catch in TS → propagate on failure.
        deps.git.checkout(&base_branch, None)?;
        let _ = deps.git.pull(&base_branch);
    } else {
        deps.git.checkout(
            &base_branch,
            Some(&CheckoutOptions {
                create: true,
                track: Some(format!("origin/{base_branch}")),
            }),
        )?;
    }

    // Create the new branch (handled error on failure).
    if let Err(e) = deps.git.checkout(
        &input.branch_name,
        Some(&CheckoutOptions {
            create: true,
            track: None,
        }),
    ) {
        return Ok(CmdResult::Err(e));
    }

    Ok(CmdResult::Ok(BranchOutput {
        branch_name: input.branch_name.clone(),
        base_branch,
    }))
}
