//! Black-box tests for the `session push-check` decision. Both readers are
//! in-memory doubles, so every rule — and the order the reads happen in — is
//! pinned without a repository, a remote or a `gh` login.

mod session_mocks;

use atelier::session::commands::push_check::{
    render_block_json, run, PushCheckDecision, PushCheckDeps, SilentReason,
};
use session_mocks::{MemBranchSync, MemOpenPr};

/// Runs the check over the two doubles, returning the decision and leaving the
/// PR reader available for its call counter.
fn check(branch: &MemBranchSync, pr: &MemOpenPr, stop_hook_active: bool) -> PushCheckDecision {
    let deps = PushCheckDeps {
        branch,
        open_pr: pr,
    };
    run(&deps, stop_hook_active)
}

/// Asserts the decision is `Silent` for `expected` *and* that the silent path
/// never consulted the forge — the network contract, not just the output.
fn assert_silent_without_gh(
    branch: &MemBranchSync,
    pr: &MemOpenPr,
    stop_hook_active: bool,
    expected: SilentReason,
) {
    assert_eq!(
        check(branch, pr, stop_hook_active),
        PushCheckDecision::Silent(expected)
    );
    assert_eq!(pr.calls.get(), 0, "silent path must not call gh");
}

#[test]
fn blocks_when_open_pr_and_local_commits_are_ahead() {
    let branch = MemBranchSync {
        current_branch: "feature/x".to_string(),
        divergence: Some((0, 3)),
        ..MemBranchSync::default()
    };
    let pr = MemOpenPr::open(42);

    assert_eq!(
        check(&branch, &pr, false),
        PushCheckDecision::Block {
            branch: "feature/x".to_string(),
            pr_number: 42,
            ahead: 3,
            behind: 0,
        }
    );
    assert_eq!(pr.calls.get(), 1);
}

#[test]
fn silent_when_stop_hook_already_active() {
    // Blocking a Stop that is itself the continuation of a block is the
    // documented infinite loop.
    assert_silent_without_gh(
        &MemBranchSync::default(),
        &MemOpenPr::open(42),
        true,
        SilentReason::StopHookActive,
    );
}

#[test]
fn silent_outside_a_work_tree() {
    let branch = MemBranchSync {
        inside_work_tree: false,
        ..MemBranchSync::default()
    };
    assert_silent_without_gh(
        &branch,
        &MemOpenPr::open(42),
        false,
        SilentReason::OutsideWorkTree,
    );
}

#[test]
fn silent_during_rebase_merge_or_detached_head() {
    for state in [
        MemBranchSync {
            rebase: true,
            ..MemBranchSync::default()
        },
        MemBranchSync {
            merge: true,
            ..MemBranchSync::default()
        },
        MemBranchSync {
            current_branch: String::new(),
            ..MemBranchSync::default()
        },
    ] {
        assert_silent_without_gh(
            &state,
            &MemOpenPr::open(42),
            false,
            SilentReason::SpecialState,
        );
    }
}

#[test]
fn silent_on_the_default_branch() {
    let branch = MemBranchSync {
        current_branch: "main".to_string(),
        default_branch: Some("main".to_string()),
        divergence: Some((0, 2)),
        ..MemBranchSync::default()
    };
    assert_silent_without_gh(
        &branch,
        &MemOpenPr::open(42),
        false,
        SilentReason::DefaultBranch,
    );
}

#[test]
fn proceeds_when_default_branch_detection_fails() {
    // Detection failure must not disarm the check: the decisive gate is the
    // open PR, which the default branch cannot have for itself.
    let branch = MemBranchSync {
        default_branch: None,
        divergence: Some((0, 1)),
        ..MemBranchSync::default()
    };
    let pr = MemOpenPr::open(7);

    assert!(matches!(
        check(&branch, &pr, false),
        PushCheckDecision::Block { pr_number: 7, .. }
    ));
}

#[test]
fn silent_when_branch_has_no_upstream() {
    let branch = MemBranchSync {
        divergence: None,
        ..MemBranchSync::default()
    };
    assert_silent_without_gh(
        &branch,
        &MemOpenPr::open(42),
        false,
        SilentReason::NoUpstream,
    );
}

#[test]
fn silent_when_upstream_already_has_every_commit() {
    let branch = MemBranchSync {
        divergence: Some((2, 0)),
        ..MemBranchSync::default()
    };
    assert_silent_without_gh(
        &branch,
        &MemOpenPr::open(42),
        false,
        SilentReason::NothingToPush,
    );
}

#[test]
fn silent_when_the_pr_lookup_fails() {
    // Safe mode, as in the PR guard: an unanswerable forge never blocks.
    let pr = MemOpenPr::failing();
    assert_eq!(
        check(&MemBranchSync::default(), &pr, false),
        PushCheckDecision::Silent(SilentReason::PrLookupFailed)
    );
    assert_eq!(pr.calls.get(), 1);
}

#[test]
fn silent_when_no_pr_is_open() {
    let pr = MemOpenPr::none();
    assert_eq!(
        check(&MemBranchSync::default(), &pr, false),
        PushCheckDecision::Silent(SilentReason::NoOpenPr)
    );
    assert_eq!(pr.calls.get(), 1);
}

#[test]
fn block_reason_names_the_branch_pr_and_ahead_count() {
    let branch = MemBranchSync {
        current_branch: "feature/y".to_string(),
        divergence: Some((0, 2)),
        ..MemBranchSync::default()
    };
    let json = render_block_json(&check(&branch, &MemOpenPr::open(11), false)).expect("block json");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    assert_eq!(parsed["decision"], "block");
    let reason = parsed["reason"].as_str().expect("reason string");
    assert!(reason.starts_with("[push-check]"), "{reason}");
    assert!(reason.contains("feature/y"), "{reason}");
    assert!(reason.contains("#11"), "{reason}");
    assert!(reason.contains("2개"), "{reason}");
    assert!(reason.contains("git push"), "{reason}");
    // The permission-denied delegation is policy, not decoration.
    assert!(reason.contains("permission denied"), "{reason}");
    assert!(reason.contains("위임"), "{reason}");
    // Nothing to rebase when the branch is not behind.
    assert!(!reason.contains("rebase"), "{reason}");
}

#[test]
fn block_reason_asks_for_a_rebase_judgement_when_behind() {
    let branch = MemBranchSync {
        divergence: Some((4, 2)),
        ..MemBranchSync::default()
    };
    let json = render_block_json(&check(&branch, &MemOpenPr::open(11), false)).expect("block json");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let reason = parsed["reason"].as_str().expect("reason string");

    assert!(reason.contains("4개"), "{reason}");
    assert!(reason.contains("rebase"), "{reason}");
    assert!(reason.contains("--force-with-lease"), "{reason}");
}

#[test]
fn silent_decisions_render_no_json() {
    let decision = check(&MemBranchSync::default(), &MemOpenPr::none(), false);
    assert_eq!(render_block_json(&decision), None);
}
