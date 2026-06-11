//! `guard` command — the single dispatch point for all guard targets (#777).
//! Branch targets (write/commit) route to `core::guard::GuardService`, the
//! `pr` target routes to `core::pr_guard::PrGuardService`. Both collapse into
//! a `GuardDecision` so the CLI layer only maps allow/block to exit codes.

use crate::git::core::guard::GuardService;
use crate::git::core::pr_guard::PrGuardService;
use crate::git::types::{GuardCommandTarget, GuardDecision, GuardInput, GuardTarget, PrGuardInput};

/// PreToolUse hook payload fields the guard targets consume. `parse` is
/// swallow-all — any read/JSON failure yields all-`None`, preserving the TS
/// `readHookStdin` behavior. Lives in the command layer so the payload schema
/// is deterministic, testable logic; the CLI edge only reads stdin (#778).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HookPayload {
    pub command: Option<String>,
    pub file_path: Option<String>,
}

impl HookPayload {
    pub fn parse(raw: &str) -> HookPayload {
        match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(v) => HookPayload {
                command: v["tool_input"]["command"].as_str().map(|s| s.to_string()),
                file_path: v["tool_input"]["file_path"].as_str().map(|s| s.to_string()),
            },
            Err(_) => HookPayload::default(),
        }
    }
}

/// Guard target name as parsed from argv. Splitting parse from payload
/// binding lets the CLI reject an invalid target *before* consuming stdin
/// (usage error must not block on a missing pipe).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardTargetKind {
    Write,
    Commit,
    Pr,
}

impl GuardTargetKind {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "write" => Some(Self::Write),
            "commit" => Some(Self::Commit),
            "pr" => Some(Self::Pr),
            _ => None,
        }
    }

    /// Binds exactly the payload field this target's check consumes.
    pub fn into_target(self, payload: HookPayload) -> GuardCommandTarget {
        match self {
            Self::Write => GuardCommandTarget::Branch(GuardTarget::Write {
                file_path: payload.file_path,
            }),
            Self::Commit => GuardCommandTarget::Branch(GuardTarget::Commit {
                command: payload.command,
            }),
            Self::Pr => GuardCommandTarget::Pr {
                command: payload.command,
            },
        }
    }
}

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
        GuardCommandTarget::Branch(target) => deps
            .branch_guard
            .check(&GuardInput {
                target: target.clone(),
                project_dir: input.project_dir.clone(),
                create_branch_script: input.create_branch_script.clone(),
                default_branch: input.default_branch.clone(),
                protected_branches: input.protected_branches.clone(),
            })
            .into(),
    }
}

/// PR-guard check without branch configuration — shared by the `guard pr`
/// dispatch above and the legacy `pr-guard` alias.
pub fn check_pr(pr_guard: &dyn PrGuardService, command: Option<String>) -> GuardDecision {
    pr_guard
        .check(&PrGuardInput {
            tool_command: command,
        })
        .into()
}
