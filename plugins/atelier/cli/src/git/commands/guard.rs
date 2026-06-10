//! `guard` command — the single dispatch point for all guard targets (#777).
//! Branch targets (write/commit) route to `core::guard::GuardService`, the
//! `pr` target routes to `core::pr_guard::PrGuardService`. Both collapse into
//! a `GuardDecision` so the CLI layer only maps allow/block to exit codes.

use crate::git::core::guard::GuardService;
use crate::git::core::pr_guard::PrGuardService;
use crate::git::types::{GuardCommandTarget, GuardDecision, GuardInput, PrGuardInput};

pub struct GuardCommandDeps<'a> {
    pub branch_guard: &'a dyn GuardService,
    pub pr_guard: &'a dyn PrGuardService,
}

/// Guard request as parsed by the CLI: the target (with its tool payload) plus
/// the branch-guard configuration, which only branch targets consume.
pub struct GuardCommandInput {
    pub target: GuardCommandTarget,
    pub project_dir: String,
    pub create_branch_script: String,
    pub default_branch: Option<String>,
    pub protected_branches: Option<Vec<String>>,
}

/// Routes the target to its guard service and returns the unified decision.
pub fn run(deps: &GuardCommandDeps, input: &GuardCommandInput) -> GuardDecision {
    match &input.target {
        GuardCommandTarget::Pr { command } => check_pr(deps.pr_guard, command.clone()),
        GuardCommandTarget::Branch(target) => {
            let out = deps.branch_guard.check(&GuardInput {
                target: target.clone(),
                project_dir: input.project_dir.clone(),
                create_branch_script: input.create_branch_script.clone(),
                default_branch: input.default_branch.clone(),
                protected_branches: input.protected_branches.clone(),
            });
            GuardDecision {
                allowed: out.allowed,
                reason: out.reason,
            }
        }
    }
}

/// PR-guard check without branch configuration — shared by the `guard pr`
/// dispatch above and the legacy `pr-guard` alias.
pub fn check_pr(pr_guard: &dyn PrGuardService, command: Option<String>) -> GuardDecision {
    let out = pr_guard.check(&PrGuardInput {
        tool_command: command,
    });
    GuardDecision {
        allowed: out.allowed,
        reason: out.reason,
    }
}
