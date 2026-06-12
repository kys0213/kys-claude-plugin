//! Blackbox tests for `autopilot hook` guard logic (#776).
//!
//! Ports the deterministic behavior of `hooks/guard-pr-base.sh` and
//! `hooks/protect-stagnation.sh` into testable CLI command logic. Tests
//! exercise only the public API (payload parse → decision) with in-memory
//! mocks per `.claude/rules/rust-cli.md`.

mod mock_fs;

use std::sync::Arc;

use atelier::autopilot::cmd::check::stagnation::StagnationConfig;
use atelier::autopilot::cmd::hook::{
    extract_claim_task_id, guard_pr_base, protect_stagnation_check, HookToolPayload,
};
use atelier::autopilot::domain::{TaskId, TaskSource};
use atelier::autopilot::ports::task_store::{NewWatchTask, TaskStore};
use atelier::autopilot::store::InMemoryTaskStore;
use chrono::{DateTime, TimeZone, Utc};
use mock_fs::MockFs;

const PROJECT_DIR: &str = "/proj";
const CONFIG_NAME: &str = "github-autopilot.local.md";

fn config_path() -> String {
    format!("{PROJECT_DIR}/{CONFIG_NAME}")
}

fn frontmatter(body: &str) -> String {
    format!("---\n{body}\n---\n\n# autopilot config\n")
}

fn mcp_payload(base: &str) -> HookToolPayload {
    HookToolPayload::parse(&format!(
        r#"{{"tool_name":"mcp__github__create_pull_request","tool_input":{{"base":"{base}","title":"t"}}}}"#
    ))
}

fn bash_payload(command: &str) -> HookToolPayload {
    HookToolPayload::parse(&format!(
        r#"{{"tool_name":"Bash","tool_input":{{"command":"{command}"}}}}"#
    ))
}

// ── payload parsing ────────────────────────────────────────────────────

#[test]
fn payload_parse_extracts_tool_name_command_and_base() {
    let p = HookToolPayload::parse(
        r#"{"tool_name":"Bash","tool_input":{"command":"gh pr create","base":"develop"}}"#,
    );
    assert_eq!(p.tool_name.as_deref(), Some("Bash"));
    assert_eq!(p.command.as_deref(), Some("gh pr create"));
    assert_eq!(p.pr_base.as_deref(), Some("develop"));
}

#[test]
fn payload_parse_swallows_garbage() {
    let p = HookToolPayload::parse("not json at all");
    assert!(p.tool_name.is_none());
    assert!(p.command.is_none());
    assert!(p.pr_base.is_none());
}

// ── guard-pr-base: config absent / non-PR calls ────────────────────────

#[test]
fn guard_allows_when_config_missing() {
    let fs = MockFs::new();
    let d = guard_pr_base(
        &fs,
        PROJECT_DIR.as_ref(),
        CONFIG_NAME,
        &mcp_payload("develop"),
    );
    assert!(d.allowed, "non-autopilot project must pass: {:?}", d.reason);
    assert_eq!(d.exit_code(), 0);
}

#[test]
fn guard_allows_non_pr_bash_command() {
    let fs = MockFs::new().with_file(&config_path(), &frontmatter("work_branch: epic/foo"));
    let d = guard_pr_base(
        &fs,
        PROJECT_DIR.as_ref(),
        CONFIG_NAME,
        &bash_payload("cargo test"),
    );
    assert!(d.allowed);
}

#[test]
fn guard_allows_pr_create_without_base() {
    let fs = MockFs::new().with_file(&config_path(), &frontmatter("work_branch: epic/foo"));
    let d = guard_pr_base(
        &fs,
        PROJECT_DIR.as_ref(),
        CONFIG_NAME,
        &bash_payload("gh pr create --title hi"),
    );
    assert!(d.allowed, "no explicit base → allow: {:?}", d.reason);
}

#[test]
fn guard_allows_unrecognized_tool() {
    let fs = MockFs::new().with_file(&config_path(), &frontmatter("work_branch: epic/foo"));
    let p = HookToolPayload::parse(
        r#"{"tool_name":"Write","tool_input":{"file_path":"a.rs","base":"x"}}"#,
    );
    let d = guard_pr_base(&fs, PROJECT_DIR.as_ref(), CONFIG_NAME, &p);
    assert!(d.allowed);
}

// ── guard-pr-base: expected-base resolution ────────────────────────────

#[test]
fn guard_blocks_mcp_base_mismatching_work_branch() {
    let fs = MockFs::new().with_file(&config_path(), &frontmatter("work_branch: epic/foo"));
    let d = guard_pr_base(&fs, PROJECT_DIR.as_ref(), CONFIG_NAME, &mcp_payload("main"));
    assert!(!d.allowed);
    assert_eq!(d.exit_code(), 2);
    let reason = d.reason.unwrap();
    assert!(reason.contains("epic/foo"), "reason mentions expected base");
    assert!(reason.contains("main"), "reason mentions actual base");
}

#[test]
fn guard_allows_mcp_base_matching_work_branch() {
    let fs = MockFs::new().with_file(
        &config_path(),
        &frontmatter("work_branch: \"epic/foo\"\nbranch_strategy: draft-main"),
    );
    let d = guard_pr_base(
        &fs,
        PROJECT_DIR.as_ref(),
        CONFIG_NAME,
        &mcp_payload("epic/foo"),
    );
    assert!(d.allowed, "{:?}", d.reason);
}

#[test]
fn guard_develop_strategy_expects_develop() {
    let fs = MockFs::new().with_file(
        &config_path(),
        &frontmatter("branch_strategy: draft-develop-main"),
    );
    let blocked = guard_pr_base(
        &fs,
        PROJECT_DIR.as_ref(),
        CONFIG_NAME,
        &bash_payload("gh pr create --base main --title t"),
    );
    assert!(!blocked.allowed);

    let allowed = guard_pr_base(
        &fs,
        PROJECT_DIR.as_ref(),
        CONFIG_NAME,
        &bash_payload("gh pr create --base develop --title t"),
    );
    assert!(allowed.allowed, "{:?}", allowed.reason);
}

#[test]
fn guard_defaults_to_main_without_strategy() {
    let fs = MockFs::new().with_file(&config_path(), &frontmatter("label_prefix: autopilot"));
    let blocked = guard_pr_base(
        &fs,
        PROJECT_DIR.as_ref(),
        CONFIG_NAME,
        &bash_payload("gh pr create --base=develop"),
    );
    assert!(!blocked.allowed, "develop against default main must block");

    let allowed = guard_pr_base(
        &fs,
        PROJECT_DIR.as_ref(),
        CONFIG_NAME,
        &bash_payload("gh pr create --base=main"),
    );
    assert!(allowed.allowed, "{:?}", allowed.reason);
}

#[test]
fn guard_parses_base_equals_form() {
    let fs = MockFs::new().with_file(&config_path(), &frontmatter("work_branch: epic/foo"));
    let d = guard_pr_base(
        &fs,
        PROJECT_DIR.as_ref(),
        CONFIG_NAME,
        &bash_payload("gh pr create --base=epic/foo --title t"),
    );
    assert!(d.allowed, "{:?}", d.reason);
}

// ── guard-pr-base: #758 — unrecognized branch_strategy is an error ─────

#[test]
fn guard_blocks_on_unrecognized_branch_strategy() {
    let fs = MockFs::new().with_file(
        &config_path(),
        &frontmatter("branch_strategy: draft-develup-main"),
    );
    let d = guard_pr_base(
        &fs,
        PROJECT_DIR.as_ref(),
        CONFIG_NAME,
        &bash_payload("gh pr create --base main"),
    );
    assert!(
        !d.allowed,
        "typo'd strategy must not silently fall back to main"
    );
    let reason = d.reason.unwrap();
    assert!(
        reason.contains("draft-develup-main"),
        "reason names the bad value: {reason}"
    );
    assert!(
        reason.contains("draft-develop-main"),
        "reason lists supported values: {reason}"
    );
}

#[test]
fn guard_ignores_broken_strategy_for_non_pr_calls() {
    let fs = MockFs::new().with_file(
        &config_path(),
        &frontmatter("branch_strategy: draft-develup-main"),
    );
    let d = guard_pr_base(
        &fs,
        PROJECT_DIR.as_ref(),
        CONFIG_NAME,
        &bash_payload("git status"),
    );
    assert!(d.allowed, "config error only surfaces on PR creation");
}

// ── guard-pr-base: frontmatter parsing edges ───────────────────────────

#[test]
fn guard_ignores_keys_outside_frontmatter() {
    let content = "---\nlabel_prefix: x\n---\n\nwork_branch: epic/should-not-apply\n";
    let fs = MockFs::new().with_file(&config_path(), content);
    // Outside-frontmatter work_branch ignored → expected base falls back to main.
    let d = guard_pr_base(&fs, PROJECT_DIR.as_ref(), CONFIG_NAME, &mcp_payload("main"));
    assert!(d.allowed, "{:?}", d.reason);
}

// ── protect-stagnation: claim-command parsing ──────────────────────────

#[test]
fn claim_id_none_for_unrelated_command() {
    assert_eq!(extract_claim_task_id(Some("cargo test")), None);
    assert_eq!(extract_claim_task_id(None), None);
}

#[test]
fn claim_id_none_for_epic_claim_surface() {
    // Current CLI claims by epic, not by task id — no id to check.
    assert_eq!(
        extract_claim_task_id(Some("atelier autopilot task claim --epic foo")),
        None
    );
}

#[test]
fn claim_id_extracted_from_positional_form() {
    assert_eq!(
        extract_claim_task_id(Some("autopilot task claim abc123def456")),
        Some("abc123def456".to_string())
    );
    assert_eq!(
        extract_claim_task_id(Some(
            "cd x && atelier autopilot task claim abc123def456 --json"
        )),
        Some("abc123def456".to_string())
    );
}

#[test]
fn claim_id_extracted_from_task_flag() {
    assert_eq!(
        extract_claim_task_id(Some("autopilot task claim --task abc123def456")),
        Some("abc123def456".to_string())
    );
    assert_eq!(
        extract_claim_task_id(Some("autopilot task claim --task=abc123def456")),
        Some("abc123def456".to_string())
    );
}

#[test]
fn claim_id_requires_claim_context() {
    // `--task <id>` on a non-claim command must not trigger the guard.
    assert_eq!(
        extract_claim_task_id(Some("autopilot check stagnation --task abc123def456")),
        None
    );
}

// ── protect-stagnation: stagnation banding via in-memory store ─────────

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap()
}

fn seed_task(store: &dyn TaskStore, id: &str, simhash: u64, fingerprint: &str) {
    let nt = NewWatchTask {
        id: TaskId::from_raw(id),
        epic_name: "epic-a".to_string(),
        source: TaskSource::Human,
        fingerprint: fingerprint.to_string(),
        title: format!("task {id}"),
        body: None,
        simhash: Some(simhash),
        affected_paths: Some(vec!["src/cmd/task.rs".into()]),
    };
    store.upsert_watch_task(nt, t0()).expect("seed task");
}

fn store_with_similar(priors: u64) -> Arc<dyn TaskStore> {
    let store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
    let base: u64 = 0xA3F2_B81C_4D5E_6F1B;
    for i in 0..priors {
        seed_task(
            store.as_ref(),
            &format!("aaaaaaaaaa{i:02}"),
            base ^ (i & 0x3),
            &format!("fp:{i}"),
        );
    }
    seed_task(store.as_ref(), "bbbbbbbbbb99", base, "fp:current");
    store
}

#[test]
fn stagnation_ok_band_allows() {
    let store = store_with_similar(1);
    let d = protect_stagnation_check(store.as_ref(), &StagnationConfig::default(), "bbbbbbbbbb99")
        .expect("check runs");
    assert!(d.allowed);
    assert_eq!(d.exit_code(), 0);
}

#[test]
fn stagnation_band_blocks_with_redirect_prompt() {
    let store = store_with_similar(3);
    let d = protect_stagnation_check(store.as_ref(), &StagnationConfig::default(), "bbbbbbbbbb99")
        .expect("check runs");
    assert!(!d.allowed);
    assert_eq!(d.exit_code(), 2);
    let reason = d.reason.unwrap();
    assert!(
        reason.contains("[STAGNATION DETECTED] task bbbbbbbbbb99"),
        "redirect header present: {reason}"
    );
    assert!(
        reason.contains("DO NOT proceed with the same approach"),
        "redirect instruction present: {reason}"
    );
    assert!(
        reason.contains("Persona shift"),
        "persona suggestion present: {reason}"
    );
}

#[test]
fn escalate_band_blocks_with_human_review_prompt() {
    let store = store_with_similar(5);
    let d = protect_stagnation_check(store.as_ref(), &StagnationConfig::default(), "bbbbbbbbbb99")
        .expect("check runs");
    assert!(!d.allowed);
    let reason = d.reason.unwrap();
    assert!(
        reason.contains("[STAGNATION ESCALATED] task bbbbbbbbbb99"),
        "escalate header present: {reason}"
    );
    assert!(
        reason.contains("task escalate bbbbbbbbbb99"),
        "manual escalate instruction present: {reason}"
    );
}

#[test]
fn unknown_task_id_is_an_error_for_caller_to_allow() {
    // The CLI edge maps Err → allow (best-effort parity with the bash
    // hook's `*) exit 0` arm); the service itself just reports the error.
    let store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
    let result =
        protect_stagnation_check(store.as_ref(), &StagnationConfig::default(), "feedfacefeed");
    assert!(result.is_err());
}
