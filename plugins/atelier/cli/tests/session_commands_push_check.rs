//! Black-box tests for the `session push-check` decision. The git and forge
//! reads come from the shared git-subsystem mocks, so every rule — and the
//! exact reads each one pays for — is pinned without a repository, a remote or
//! a `gh` login.

mod git_mocks;

use atelier::git::types::Divergence;
use atelier::session::commands::payload::SessionPayload;
use atelier::session::commands::push_check::{
    render_block_json, run, PushCheckDecision, PushCheckDeps, SilentReason,
};
use git_mocks::{MockGit, MockGitHub, Recorder};
use std::rc::Rc;

fn div(behind: u32, ahead: u32) -> Divergence {
    Divergence { behind, ahead }
}

/// One Stop's world. The defaults describe the case the rule exists for — a
/// feature branch one commit ahead of its upstream with an open PR — so each
/// test names only what it varies.
struct Scenario {
    stop_hook_active: bool,
    /// `None` models a drift that could not be read at all.
    divergence: Option<Divergence>,
    rebase: bool,
    merge: bool,
    /// Empty means detached HEAD, exactly as `branch --show-current` reports.
    current_branch: String,
    default_branch: Result<String, String>,
    open_pr: Option<i64>,
}

impl Default for Scenario {
    fn default() -> Self {
        Scenario {
            stop_hook_active: false,
            divergence: Some(div(0, 1)),
            rebase: false,
            merge: false,
            current_branch: "feature/x".to_string(),
            default_branch: Ok("main".to_string()),
            open_pr: Some(42),
        }
    }
}

/// Runs the check over mocks built from `s`, returning the decision *and* the
/// reads it paid for, in order. Both halves are the contract: which rule
/// answers, and what a Stop costs before it does.
fn check(s: Scenario) -> (PushCheckDecision, Vec<String>) {
    let calls = Rc::new(Recorder::default());
    let (rec_div, rec_state, rec_default, rec_pr) = (
        Rc::clone(&calls),
        Rc::clone(&calls),
        Rc::clone(&calls),
        Rc::clone(&calls),
    );
    let Scenario {
        stop_hook_active,
        divergence,
        rebase,
        merge,
        current_branch,
        default_branch,
        open_pr,
    } = s;

    let git = MockGit {
        upstream_divergence: Box::new(move || {
            rec_div.push("upstream_divergence");
            divergence
        }),
        special_state_flags: Box::new(move || {
            rec_state.push("get_special_state");
            (rebase, merge)
        }),
        current_branch: Box::new(move || current_branch.clone()),
        detect_default_branch: Box::new(move || {
            rec_default.push("detect_default_branch");
            default_branch.clone()
        }),
        ..Default::default()
    };
    let gh = MockGitHub {
        detect_current_pr_number: Box::new(move || {
            rec_pr.push("open_pr_number");
            Ok(open_pr)
        }),
        ..Default::default()
    };

    let deps = PushCheckDeps {
        git: &git,
        open_pr: &gh,
    };
    let decision = run(&deps, stop_hook_active);
    (decision, calls.snapshot())
}

fn decide(s: Scenario) -> PushCheckDecision {
    check(s).0
}

/// Asserts which rule answered *and* that nothing beyond `reads` was consulted.
fn assert_silent(s: Scenario, expected: SilentReason, reads: &[&str]) {
    let (decision, calls) = check(s);
    assert_eq!(decision, PushCheckDecision::Silent(expected));
    assert_eq!(calls, reads, "reads a silent Stop paid for");
}

/// The `reason` string of the block `s` produces.
fn block_reason(s: Scenario) -> String {
    let json = render_block_json(&decide(s)).expect("block json");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    parsed["reason"]
        .as_str()
        .expect("reason string")
        .to_string()
}

#[test]
fn blocks_on_a_single_unpushed_commit_with_an_open_pr() {
    // The boundary the rule turns on: one commit ahead, nothing behind.
    let (decision, calls) = check(Scenario {
        current_branch: "feature/one".to_string(),
        divergence: Some(div(0, 1)),
        open_pr: Some(3),
        ..Default::default()
    });

    assert_eq!(
        decision,
        PushCheckDecision::Block {
            branch: "feature/one".to_string(),
            pr_number: 3,
            ahead: 1,
            behind: 0,
        }
    );
    assert_eq!(
        calls,
        [
            "upstream_divergence",
            "get_special_state",
            "detect_default_branch",
            "open_pr_number"
        ]
    );
}

#[test]
fn stop_hook_active_answers_before_any_read() {
    // Blocking a Stop that is itself the continuation of a block is the
    // documented infinite loop — and it must cost nothing at all to refuse.
    assert_silent(
        Scenario {
            stop_hook_active: true,
            ..Default::default()
        },
        SilentReason::StopHookActive,
        &[],
    );
}

#[test]
fn an_unreadable_divergence_is_not_the_same_as_being_in_sync() {
    // `None` is "cannot tell" — no upstream, outside a work tree, detached
    // HEAD; `(0, 0)` is "already pushed everything". Distinct reasons, and
    // either way the single `rev-list` is the whole cost of the Stop.
    assert_silent(
        Scenario {
            divergence: None,
            ..Default::default()
        },
        SilentReason::DivergenceUnknown,
        &["upstream_divergence"],
    );
    assert_silent(
        Scenario {
            divergence: Some(div(2, 0)),
            ..Default::default()
        },
        SilentReason::NothingToPush,
        &["upstream_divergence"],
    );
}

#[test]
fn an_unreadable_divergence_answers_before_every_later_rule() {
    // Mid-rebase, on the default branch, and a PR is open — the one read that
    // already answered is still the only one paid for.
    assert_silent(
        Scenario {
            divergence: None,
            rebase: true,
            current_branch: "main".to_string(),
            ..Default::default()
        },
        SilentReason::DivergenceUnknown,
        &["upstream_divergence"],
    );
}

#[test]
fn silent_during_rebase_merge_or_detached_head() {
    for state in [
        Scenario {
            rebase: true,
            ..Default::default()
        },
        Scenario {
            merge: true,
            ..Default::default()
        },
        Scenario {
            current_branch: String::new(),
            ..Default::default()
        },
    ] {
        assert_silent(
            state,
            SilentReason::SpecialState,
            &["upstream_divergence", "get_special_state"],
        );
    }
}

#[test]
fn the_default_branch_answers_after_the_divergence_and_before_the_forge() {
    assert_silent(
        Scenario {
            current_branch: "main".to_string(),
            divergence: Some(div(0, 2)),
            ..Default::default()
        },
        SilentReason::DefaultBranch,
        &[
            "upstream_divergence",
            "get_special_state",
            "detect_default_branch",
        ],
    );
}

#[test]
fn proceeds_when_default_branch_detection_fails() {
    // Detection failure must not disarm the check: the decisive gate is the
    // open PR, which the default branch cannot have for itself.
    assert!(matches!(
        decide(Scenario {
            default_branch: Err("no remote".to_string()),
            open_pr: Some(7),
            ..Default::default()
        }),
        PushCheckDecision::Block { pr_number: 7, .. }
    ));
}

#[test]
fn silent_when_no_pr_is_open() {
    // Nobody is waiting on the commits. The forge *is* consulted here — this
    // is the one path that reaches it without blocking.
    assert_silent(
        Scenario {
            open_pr: None,
            ..Default::default()
        },
        SilentReason::NoOpenPr,
        &[
            "upstream_divergence",
            "get_special_state",
            "detect_default_branch",
            "open_pr_number",
        ],
    );
}

#[test]
fn silent_decisions_render_no_json() {
    let decision = decide(Scenario {
        open_pr: None,
        ..Default::default()
    });
    assert_eq!(render_block_json(&decision), None);
}

// ---------------------------------------------------------------------------
// Block document
// ---------------------------------------------------------------------------

#[test]
fn block_reason_names_the_branch_pr_and_counts_and_points_at_the_next_step() {
    let reason = block_reason(Scenario {
        current_branch: "feature/y".to_string(),
        divergence: Some(div(0, 2)),
        open_pr: Some(11),
        ..Default::default()
    });

    assert!(reason.starts_with("[push-check]"), "{reason}");
    assert!(reason.contains("feature/y"), "{reason}");
    assert!(reason.contains("#11"), "{reason}");
    assert!(reason.contains("로컬 전용 커밋: 2개"), "{reason}");
    assert!(reason.contains("git push"), "{reason}");
    // The delegation pointer is the only policy the reason carries.
    assert!(reason.contains("push 권한 거부 시 사용자 위임"), "{reason}");
    // A "0개" remote-only line would read as drift that is not there, and
    // nothing is behind, so nothing suggests a rebase.
    assert!(!reason.contains("원격 전용"), "{reason}");
    assert!(!reason.contains("rebase"), "{reason}");
}

#[test]
fn block_reason_defers_the_rebase_judgement_to_the_git_skill_when_behind() {
    let reason = block_reason(Scenario {
        divergence: Some(div(4, 2)),
        ..Default::default()
    });

    assert!(reason.contains("원격 전용 커밋: 4개"), "{reason}");
    assert!(reason.contains("열린 PR 최신화 원칙"), "{reason}");
    // The policy body itself lives in the skill and is never restated here —
    // two copies of a rule drift apart.
    assert!(!reason.contains("--force-with-lease"), "{reason}");
}

#[test]
fn block_json_carries_exactly_the_decision_and_reason_keys() {
    // Claude Code reads the Stop document by key; an extra key is a contract
    // change, so the object's shape is asserted, not only its two values.
    let json = render_block_json(&decide(Scenario::default())).expect("block json");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let object = parsed.as_object().expect("JSON object");

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["decision", "reason"]);
    assert_eq!(object["decision"], "block");
    assert!(object["reason"].is_string());
}

// ---------------------------------------------------------------------------
// Payload → decision
// ---------------------------------------------------------------------------

/// Runs the check the way the CLI edge does: the flag reaches the decision only
/// through `SessionPayload::parse`, so the parse defaults are pinned end to end
/// rather than by handing the flag straight to `run`.
fn check_payload(raw: &str) -> PushCheckDecision {
    decide(Scenario {
        stop_hook_active: SessionPayload::parse(raw).stop_hook_active,
        ..Default::default()
    })
}

#[test]
fn only_an_explicitly_true_stop_hook_flag_disarms_the_check() {
    assert_eq!(
        check_payload(r#"{"stop_hook_active":true}"#),
        PushCheckDecision::Silent(SilentReason::StopHookActive)
    );
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
        assert!(
            matches!(
                check_payload(raw),
                PushCheckDecision::Block {
                    pr_number: 42,
                    ahead: 1,
                    ..
                }
            ),
            "payload: {raw}"
        );
    }
}
