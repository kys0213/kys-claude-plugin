//! Black-box tests for the `session push-check` decision. Both readers are
//! in-memory doubles, so every rule — and the order the reads happen in — is
//! pinned without a repository, a remote or a `gh` login.

mod session_mocks;

use atelier::session::commands::payload::SessionPayload;
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

// ---------------------------------------------------------------------------
// Ordering contract
//
// The order the rules answer in is part of the contract, not an optimisation:
// each case below satisfies *several* silent rules at once and pins which one
// is allowed to answer, so reordering `decide` fails a test rather than
// quietly changing which reads a Stop pays for.
// ---------------------------------------------------------------------------

#[test]
fn stop_hook_active_answers_before_any_git_read() {
    // Not just "no gh call": an already-continuing Stop must cost nothing at
    // all, so no branch read may happen either.
    let branch = MemBranchSync::default();
    let pr = MemOpenPr::open(42);

    assert_eq!(
        check(&branch, &pr, true),
        PushCheckDecision::Silent(SilentReason::StopHookActive)
    );
    assert_eq!(branch.reads.get(), 0, "must not read git");
    assert_eq!(pr.calls.get(), 0, "must not call gh");
}

#[test]
fn outside_a_work_tree_answers_before_every_later_rule() {
    // Also mid-rebase, also on the default branch, also without an upstream,
    // and a PR is open — the work-tree rule still owns the answer.
    let branch = MemBranchSync {
        inside_work_tree: false,
        rebase: true,
        current_branch: "main".to_string(),
        default_branch: Some("main".to_string()),
        divergence: None,
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
fn a_special_state_answers_before_the_default_branch_rule() {
    let branch = MemBranchSync {
        rebase: true,
        current_branch: "main".to_string(),
        default_branch: Some("main".to_string()),
        ..MemBranchSync::default()
    };
    assert_silent_without_gh(
        &branch,
        &MemOpenPr::open(42),
        false,
        SilentReason::SpecialState,
    );
}

#[test]
fn the_default_branch_answers_before_the_upstream_rules() {
    // A default branch with no upstream is silent *because* it is the default
    // branch — being on it is decided before any divergence is considered.
    let branch = MemBranchSync {
        current_branch: "main".to_string(),
        default_branch: Some("main".to_string()),
        divergence: None,
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
fn a_missing_upstream_is_not_the_same_as_being_in_sync() {
    // `None` is "never pushed", `(0, 0)` is "already pushed everything" — the
    // two must not collapse into one reason.
    let no_upstream = MemBranchSync {
        divergence: None,
        ..MemBranchSync::default()
    };
    let in_sync = MemBranchSync {
        divergence: Some((0, 0)),
        ..MemBranchSync::default()
    };
    assert_silent_without_gh(
        &no_upstream,
        &MemOpenPr::open(42),
        false,
        SilentReason::NoUpstream,
    );
    assert_silent_without_gh(
        &in_sync,
        &MemOpenPr::open(42),
        false,
        SilentReason::NothingToPush,
    );
}

#[test]
fn blocks_on_a_single_unpushed_commit() {
    // The boundary the rule turns on: one commit ahead, nothing behind.
    let branch = MemBranchSync {
        current_branch: "feature/one".to_string(),
        divergence: Some((0, 1)),
        ..MemBranchSync::default()
    };
    assert_eq!(
        check(&branch, &MemOpenPr::open(3), false),
        PushCheckDecision::Block {
            branch: "feature/one".to_string(),
            pr_number: 3,
            ahead: 1,
            behind: 0,
        }
    );
}

// ---------------------------------------------------------------------------
// Block document shape
// ---------------------------------------------------------------------------

#[test]
fn block_json_carries_exactly_the_decision_and_reason_keys() {
    // Claude Code reads the Stop document by key; an extra key is a contract
    // change, so the object's shape is asserted, not only its two values.
    let json = render_block_json(&check(
        &MemBranchSync::default(),
        &MemOpenPr::open(11),
        false,
    ))
    .expect("block json");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let object = parsed.as_object().expect("JSON object");

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["decision", "reason"]);
    assert!(object["reason"].is_string());
}

#[test]
fn block_reason_omits_the_behind_line_when_nothing_is_behind() {
    // The remote-only count is reported only when there is one — a "0개" line
    // would read as drift that is not there.
    let branch = MemBranchSync {
        divergence: Some((0, 2)),
        ..MemBranchSync::default()
    };
    let json = render_block_json(&check(&branch, &MemOpenPr::open(11), false)).expect("block json");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let reason = parsed["reason"].as_str().expect("reason string");

    assert!(reason.contains("로컬 전용 커밋: 2개"), "{reason}");
    assert!(!reason.contains("원격 전용"), "{reason}");
}

// ---------------------------------------------------------------------------
// Payload → decision
//
// `stop_hook_active` reaches the decision only through `SessionPayload::parse`,
// so the parse defaults and the wiring are pinned end to end rather than by
// handing the flag straight to `run`.
// ---------------------------------------------------------------------------

/// Parses a raw hook payload and runs the check the way the CLI edge does.
fn check_payload(raw: &str, branch: &MemBranchSync, pr: &MemOpenPr) -> PushCheckDecision {
    check(branch, pr, SessionPayload::parse(raw).stop_hook_active)
}

#[test]
fn a_payload_marking_the_stop_hook_active_stays_silent() {
    let branch = MemBranchSync::default();
    let pr = MemOpenPr::open(42);

    assert_eq!(
        check_payload(r#"{"stop_hook_active":true}"#, &branch, &pr),
        PushCheckDecision::Silent(SilentReason::StopHookActive)
    );
    assert_eq!(branch.reads.get(), 0);
    assert_eq!(pr.calls.get(), 0);
}

#[test]
fn payloads_without_a_usable_flag_leave_the_check_armed() {
    // Absent, explicitly false, non-boolean and unparseable stdin all mean
    // "not a continuation": defaulting the flag on would silence the hook
    // everywhere, which is the failure mode that matters.
    for raw in [
        r#"{"session_id":"sess-abc12345","cwd":"/repo"}"#,
        r#"{"stop_hook_active":false}"#,
        r#"{"stop_hook_active":"true"}"#,
        r#"{"stop_hook_active":null}"#,
        "not json at all",
        "",
    ] {
        let branch = MemBranchSync {
            divergence: Some((0, 2)),
            ..MemBranchSync::default()
        };
        assert_eq!(
            check_payload(raw, &branch, &MemOpenPr::open(9)),
            PushCheckDecision::Block {
                branch: "feature/x".to_string(),
                pr_number: 9,
                ahead: 2,
                behind: 0,
            },
            "payload: {raw}"
        );
    }
}
