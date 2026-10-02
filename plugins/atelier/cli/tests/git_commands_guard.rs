//! Dispatch tests for the unified `guard` command (#777): branch targets must
//! route to the branch guard, the pr target to the PR guard — verified through
//! the public command API with stub services.

use atelier::git::commands::guard::{
    check_pr, run, GuardCommandDeps, GuardCommandInput, GuardTargetKind, HookPayload,
};
use atelier::git::core::guard::GuardService;
use atelier::git::core::pr_guard::PrGuardService;
use atelier::git::types::{
    GuardCommandTarget, GuardDecision, GuardInput, GuardOutput, GuardTarget, GuardVerdict,
    OpaqueExecPolicy, PrGuardInput, PrGuardOutput,
};

/// Branch guard stub: blocks, echoing the received target in the reason so
/// tests can assert what reached the service.
struct StubBranchGuard;

impl GuardService for StubBranchGuard {
    fn check(&self, input: &GuardInput) -> GuardOutput {
        GuardOutput {
            verdict: GuardVerdict::Block,
            reason: Some(format!("branch-guard: {:?}", input.target)),
            current_branch: None,
            rule: None,
            repo_root: None,
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
        create_branch_script: "git switch -c".to_string(),
        default_branch: None,
        protected_branches: None,
        opaque_exec: OpaqueExecPolicy::Ask,
        home: None,
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
    assert_eq!(decision.verdict, GuardVerdict::Block);
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
            cwd: None,
            command: Some("git commit -m x".to_string()),
        })),
    );
    assert_eq!(decision.verdict, GuardVerdict::Block);
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
    assert_eq!(decision.verdict, GuardVerdict::Block);
    let reason = decision.reason.unwrap();
    assert!(reason.starts_with("pr-guard:"));
    assert!(reason.contains("gh pr create --title x"));
}

#[test]
fn check_pr_maps_output_to_decision() {
    let pr = StubPrGuard;
    let decision = check_pr(&pr, None);
    assert_eq!(decision.verdict, GuardVerdict::Block);
    assert!(decision.reason.unwrap().starts_with("pr-guard:"));
}

// ---- #778: PreToolUse payload parsing / target binding / exit mapping ----

#[test]
fn hook_payload_parses_command_and_file_path() {
    let payload = HookPayload::parse(
        r#"{"tool_input":{"command":"git commit -m x","file_path":"src/main.rs"}}"#,
    );
    assert_eq!(payload.command.as_deref(), Some("git commit -m x"));
    assert_eq!(payload.file_path.as_deref(), Some("src/main.rs"));
}

#[test]
fn hook_payload_parses_top_level_cwd() {
    let payload = HookPayload::parse(r#"{"cwd":"/work/sub","tool_input":{"command":"ls"}}"#);
    assert_eq!(payload.cwd.as_deref(), Some("/work/sub"));
    assert_eq!(HookPayload::parse(r#"{"tool_input":{}}"#).cwd, None);
}

#[test]
fn block_decision_supplies_stderr_message_and_others_do_not() {
    let decision = |verdict| GuardDecision {
        verdict,
        reason: Some("why".to_string()),
    };
    assert_eq!(decision(GuardVerdict::Block).stderr_message(), Some("why"));
    assert_eq!(decision(GuardVerdict::Ask).stderr_message(), None);
    assert_eq!(decision(GuardVerdict::Allow).stderr_message(), None);
}

#[test]
fn opaque_exec_policy_and_home_reach_the_branch_guard() {
    struct Echo;
    impl GuardService for Echo {
        fn check(&self, input: &GuardInput) -> GuardOutput {
            GuardOutput {
                verdict: GuardVerdict::Allow,
                reason: Some(format!("{:?} {:?}", input.opaque_exec, input.home)),
                current_branch: None,
                rule: None,
                repo_root: None,
                default_branch: None,
            }
        }
    }
    let pr = StubPrGuard;
    let echo = Echo;
    let mut input = input_with(GuardCommandTarget::Branch(GuardTarget::Write {
        file_path: None,
    }));
    input.opaque_exec = OpaqueExecPolicy::Block;
    input.home = Some("/home/u".to_string());
    let decision = run(
        &GuardCommandDeps {
            branch_guard: &echo,
            pr_guard: &pr,
        },
        &input,
    );
    assert_eq!(decision.reason.as_deref(), Some("Block Some(\"/home/u\")"));
}

#[test]
fn hook_payload_swallows_malformed_json() {
    assert_eq!(HookPayload::parse("not json"), HookPayload::default());
    assert_eq!(HookPayload::parse(""), HookPayload::default());
}

#[test]
fn hook_payload_missing_fields_are_none() {
    let payload = HookPayload::parse(r#"{"tool_input":{}}"#);
    assert_eq!(payload, HookPayload::default());
}

#[test]
fn guard_target_kind_parses_known_names_only() {
    assert_eq!(
        GuardTargetKind::parse("write"),
        Some(GuardTargetKind::Write)
    );
    assert_eq!(
        GuardTargetKind::parse("commit"),
        Some(GuardTargetKind::Commit)
    );
    assert_eq!(GuardTargetKind::parse("pr"), Some(GuardTargetKind::Pr));
    assert_eq!(GuardTargetKind::parse("push"), None);
    assert_eq!(GuardTargetKind::parse(""), None);
}

#[test]
fn guard_target_kind_binds_only_its_payload_field() {
    let payload = HookPayload {
        command: Some("git commit".to_string()),
        file_path: Some("src/main.rs".to_string()),
        cwd: Some("/work/sub".to_string()),
    };
    assert_eq!(
        GuardTargetKind::Write.into_target(payload.clone()),
        GuardCommandTarget::Branch(GuardTarget::Write {
            file_path: Some("src/main.rs".to_string()),
        })
    );
    assert_eq!(
        GuardTargetKind::Commit.into_target(payload.clone()),
        GuardCommandTarget::Branch(GuardTarget::Commit {
            command: Some("git commit".to_string()),
            cwd: Some("/work/sub".to_string()),
        })
    );
    assert_eq!(
        GuardTargetKind::Pr.into_target(payload),
        GuardCommandTarget::Pr {
            command: Some("git commit".to_string()),
        }
    );
}

#[test]
fn guard_decision_exit_code_maps_hook_contract() {
    let allow = GuardDecision {
        verdict: GuardVerdict::Allow,
        reason: None,
    };
    let block = GuardDecision {
        verdict: GuardVerdict::Block,
        reason: Some("blocked".to_string()),
    };
    assert_eq!(allow.exit_code(), 0);
    assert_eq!(block.exit_code(), 2);
}

#[test]
fn ask_decision_exits_zero_and_emits_permission_json() {
    let reason = "확인 필요 \"quoted\"".to_string();
    let ask = GuardDecision {
        verdict: GuardVerdict::Ask,
        reason: Some(reason.clone()),
    };
    assert_eq!(ask.exit_code(), 0);
    let json: serde_json::Value = serde_json::from_str(&ask.ask_stdout().unwrap()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "ask",
                "permissionDecisionReason": reason,
                "additionalContext": reason,
            }
        })
    );
}

#[test]
fn allow_and_block_decisions_emit_no_ask_stdout() {
    let allow = GuardDecision {
        verdict: GuardVerdict::Allow,
        reason: None,
    };
    let block = GuardDecision {
        verdict: GuardVerdict::Block,
        reason: Some("blocked".to_string()),
    };
    assert_eq!(allow.ask_stdout(), None);
    assert_eq!(block.ask_stdout(), None);
}

#[test]
fn pr_guard_output_maps_to_block_or_allow() {
    let pr_output = |allowed| PrGuardOutput {
        allowed,
        reason: None,
        pr_number: None,
    };
    assert_eq!(
        GuardDecision::from(pr_output(false)).verdict,
        GuardVerdict::Block
    );
    assert_eq!(
        GuardDecision::from(pr_output(true)).verdict,
        GuardVerdict::Allow
    );
}
