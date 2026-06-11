//! Dispatch tests for the unified `guard` command (#777): branch targets must
//! route to the branch guard, the pr target to the PR guard — verified through
//! the public command API with stub services.

use atelier::git::commands::guard::{check_pr, run, GuardCommandDeps, GuardCommandInput};
use atelier::git::core::guard::GuardService;
use atelier::git::core::pr_guard::PrGuardService;
use atelier::git::types::{
    GuardCommandTarget, GuardInput, GuardOutput, GuardTarget, PrGuardInput, PrGuardOutput,
};

/// Branch guard stub: blocks, echoing the received target in the reason so
/// tests can assert what reached the service.
struct StubBranchGuard;

impl GuardService for StubBranchGuard {
    fn check(&self, input: &GuardInput) -> GuardOutput {
        GuardOutput {
            allowed: false,
            reason: Some(format!("branch-guard: {:?}", input.target)),
            current_branch: None,
            default_branch: None,
        }
    }
}

/// PR guard stub: blocks, echoing the received command.
struct StubPrGuard;

impl PrGuardService for StubPrGuard {
    fn check(&self, input: &PrGuardInput) -> PrGuardOutput {
        PrGuardOutput {
            allowed: false,
            reason: Some(format!("pr-guard: {:?}", input.tool_command)),
            pr_number: Some(7),
        }
    }
}

fn deps<'a>(branch: &'a StubBranchGuard, pr: &'a StubPrGuard) -> GuardCommandDeps<'a> {
    GuardCommandDeps {
        branch_guard: branch,
        pr_guard: pr,
    }
}

fn input_with(target: GuardCommandTarget) -> GuardCommandInput {
    GuardCommandInput {
        target,
        project_dir: "/tmp/test".to_string(),
        create_branch_script: "atelier git branch".to_string(),
        default_branch: None,
        protected_branches: None,
    }
}

#[test]
fn write_target_routes_to_branch_guard() {
    let branch = StubBranchGuard;
    let pr = StubPrGuard;
    let decision = run(
        &deps(&branch, &pr),
        &input_with(GuardCommandTarget::Branch(GuardTarget::Write {
            file_path: Some("src/main.rs".to_string()),
        })),
    );
    assert!(!decision.allowed);
    let reason = decision.reason.unwrap();
    assert!(reason.starts_with("branch-guard:"));
    assert!(reason.contains("src/main.rs"));
}

#[test]
fn commit_target_routes_to_branch_guard() {
    let branch = StubBranchGuard;
    let pr = StubPrGuard;
    let decision = run(
        &deps(&branch, &pr),
        &input_with(GuardCommandTarget::Branch(GuardTarget::Commit {
            command: Some("git commit -m x".to_string()),
        })),
    );
    assert!(!decision.allowed);
    let reason = decision.reason.unwrap();
    assert!(reason.starts_with("branch-guard:"));
    assert!(reason.contains("git commit -m x"));
}

#[test]
fn pr_target_routes_to_pr_guard() {
    let branch = StubBranchGuard;
    let pr = StubPrGuard;
    let decision = run(
        &deps(&branch, &pr),
        &input_with(GuardCommandTarget::Pr {
            command: Some("gh pr create --title x".to_string()),
        }),
    );
    assert!(!decision.allowed);
    let reason = decision.reason.unwrap();
    assert!(reason.starts_with("pr-guard:"));
    assert!(reason.contains("gh pr create --title x"));
}

#[test]
fn check_pr_maps_output_to_decision() {
    let pr = StubPrGuard;
    let decision = check_pr(&pr, None);
    assert!(!decision.allowed);
    assert!(decision.reason.unwrap().starts_with("pr-guard:"));
}
