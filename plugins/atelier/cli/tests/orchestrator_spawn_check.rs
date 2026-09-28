//! End-to-end black-box tests for `atelier orchestrator spawn-check`. Same
//! hard contract as `session` (see `tests/session_cli.rs`): this binary
//! backs a function-hooks adapter that must never see a nonzero exit, so
//! unparseable stdin and unknown subcommands must still succeed.

use assert_cmd::Command;
use serde_json::Value;

fn atelier() -> Command {
    Command::cargo_bin("atelier").expect("locate `atelier` cargo binary")
}

fn warnings_for(stdin: &str) -> Vec<String> {
    let output = atelier()
        .args(["orchestrator", "spawn-check"])
        .write_stdin(stdin)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).expect("valid JSON on stdout");
    value["warnings"]
        .as_array()
        .expect("warnings is an array")
        .iter()
        .map(|w| w.as_str().expect("warning is a string").to_string())
        .collect()
}

fn payload(model: &str, parent_model: &str, fork: bool) -> String {
    format!(
        r#"{{"model":{model},"resolved_model":"claude-opus-5-5","parent_model":"{parent_model}","fork":{fork},"subagent_type":"general-purpose"}}"#
    )
}

// --- Rule A: model unspecified ---------------------------------------------

#[test]
fn rule_a_warns_on_null_model() {
    let warnings = warnings_for(&payload("null", "claude-opus-5-5", false));
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("model 을 지정하지 않은"));
}

#[test]
fn rule_a_warns_on_blank_model() {
    let warnings = warnings_for(&payload("\"   \"", "claude-opus-5-5", false));
    assert_eq!(warnings.len(), 1);
}

#[test]
fn rule_a_warns_on_inherit_case_insensitive() {
    let warnings = warnings_for(&payload("\"INHERIT\"", "claude-opus-5-5", false));
    assert_eq!(warnings.len(), 1);
}

#[test]
fn rule_a_silent_on_fork() {
    let warnings = warnings_for(&payload("null", "claude-opus-5-5", true));
    assert!(warnings.is_empty());
}

#[test]
fn rule_a_silent_when_model_given() {
    let warnings = warnings_for(&payload("\"claude-opus-5-5\"", "claude-opus-5-5", false));
    assert!(warnings.is_empty());
}

// --- Rule B: tier cap exceeded ----------------------------------------------

#[test]
fn rule_b_fable_parent_allows_opus_delegation() {
    let warnings = warnings_for(&payload("\"claude-opus-5-5\"", "claude-fable-5-1", false));
    assert!(warnings.is_empty());
}

#[test]
fn rule_b_fable_parent_warns_on_fable_delegation() {
    let warnings = warnings_for(&payload("\"claude-fable-5-1\"", "claude-fable-5-1", false));
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("tier 상한 초과"));
}

#[test]
fn rule_b_opus_parent_warns_on_fable_delegation() {
    let warnings = warnings_for(&payload("\"claude-fable-5-1\"", "claude-opus-5-5", false));
    assert_eq!(warnings.len(), 1);
}

#[test]
fn rule_b_opus_parent_allows_opus_delegation() {
    let warnings = warnings_for(&payload("\"claude-opus-5-5\"", "claude-opus-5-5", false));
    assert!(warnings.is_empty());
}

#[test]
fn rule_b_sonnet_parent_warns_on_opus_delegation() {
    let warnings = warnings_for(&payload("\"claude-opus-5-5\"", "claude-sonnet-5", false));
    assert_eq!(warnings.len(), 1);
}

#[test]
fn rule_b_sonnet_parent_allows_sonnet_delegation() {
    let warnings = warnings_for(&payload("\"claude-sonnet-5\"", "claude-sonnet-5", false));
    assert!(warnings.is_empty());
}

#[test]
fn rule_b_sonnet_parent_allows_haiku_delegation() {
    let warnings = warnings_for(&payload(
        "\"claude-haiku-4-5-20251001\"",
        "claude-sonnet-5",
        false,
    ));
    assert!(warnings.is_empty());
}

#[test]
fn rule_b_haiku_parent_warns_on_sonnet_delegation() {
    let warnings = warnings_for(&payload(
        "\"claude-sonnet-5\"",
        "claude-haiku-4-5-20251001",
        false,
    ));
    assert_eq!(warnings.len(), 1);
}

#[test]
fn rule_b_silent_when_model_tier_undetectable() {
    let warnings = warnings_for(&payload("\"gpt-5\"", "claude-opus-5-5", false));
    assert!(warnings.is_empty());
}

#[test]
fn rule_b_silent_when_parent_tier_undetectable() {
    let warnings = warnings_for(&payload("\"claude-opus-5-5\"", "gpt-5", false));
    assert!(warnings.is_empty());
}

#[test]
fn rule_b_silent_on_fork_even_above_cap() {
    let warnings = warnings_for(&payload("\"claude-fable-5-1\"", "claude-sonnet-5", true));
    assert!(warnings.is_empty());
}

// --- Full model-id tier detection -------------------------------------------

#[test]
fn full_model_ids_are_all_detected() {
    assert!(warnings_for(&payload("\"claude-opus-5-5\"", "claude-fable-5-1", false)).is_empty());
    assert!(warnings_for(&payload("\"claude-sonnet-5\"", "claude-sonnet-5", false)).is_empty());
    assert!(warnings_for(&payload(
        "\"claude-haiku-4-5-20251001\"",
        "claude-haiku-4-5-20251001",
        false
    ))
    .is_empty());
    assert_eq!(
        warnings_for(&payload("\"claude-fable-5-1\"", "claude-opus-5-5", false)).len(),
        1
    );
}

// --- Rule B: opus parent within cap -----------------------------------------

#[test]
fn rule_b_opus_parent_allows_sonnet_delegation() {
    let warnings = warnings_for(&payload("\"sonnet\"", "claude-opus-5-5", false));
    assert!(warnings.is_empty());
}

#[test]
fn rule_b_opus_parent_allows_haiku_delegation() {
    let warnings = warnings_for(&payload(
        "\"claude-haiku-4-5-20251001\"",
        "claude-opus-5-5",
        false,
    ));
    assert!(warnings.is_empty());
}

// --- Parse failure ------------------------------------------------------------

#[test]
fn unparseable_stdin_yields_empty_warnings() {
    assert!(warnings_for("not json").is_empty());
}

#[test]
fn empty_stdin_yields_empty_warnings() {
    assert!(warnings_for("").is_empty());
}

// --- Strict input schema ------------------------------------------------------

#[test]
fn fork_as_string_yields_no_verdict() {
    let stdin = r#"{"model":null,"resolved_model":null,"parent_model":"claude-opus-5-5","fork":"true","subagent_type":"x"}"#;
    assert!(warnings_for(stdin).is_empty());
}

#[test]
fn missing_fork_yields_no_verdict() {
    let stdin = r#"{"model":null,"resolved_model":null,"parent_model":"claude-opus-5-5","subagent_type":"x"}"#;
    assert!(warnings_for(stdin).is_empty());
}

#[test]
fn missing_parent_model_yields_no_verdict() {
    let stdin = r#"{"model":null,"resolved_model":null,"fork":false,"subagent_type":"x"}"#;
    assert!(warnings_for(stdin).is_empty());
}

#[test]
fn parent_model_as_number_yields_no_verdict() {
    let stdin = r#"{"model":null,"resolved_model":null,"parent_model":123,"fork":false,"subagent_type":"x"}"#;
    assert!(warnings_for(stdin).is_empty());
}

#[test]
fn model_as_number_yields_no_verdict() {
    let stdin = r#"{"model":123,"resolved_model":null,"parent_model":"claude-opus-5-5","fork":false,"subagent_type":"x"}"#;
    assert!(warnings_for(stdin).is_empty());
}

#[test]
fn resolved_model_as_bool_yields_no_verdict() {
    let stdin = r#"{"model":null,"resolved_model":true,"parent_model":"claude-opus-5-5","fork":false,"subagent_type":"x"}"#;
    assert!(warnings_for(stdin).is_empty());
}

#[test]
fn subagent_type_as_number_yields_no_verdict() {
    let stdin = r#"{"model":null,"resolved_model":null,"parent_model":"claude-opus-5-5","fork":false,"subagent_type":123}"#;
    assert!(warnings_for(stdin).is_empty());
}

#[test]
fn missing_model_key_still_warns_rule_a() {
    let stdin = r#"{"resolved_model":null,"parent_model":"claude-opus-5-5","fork":false,"subagent_type":"general-purpose"}"#;
    let warnings = warnings_for(stdin);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("model 을 지정하지 않은"));
}

#[test]
fn null_model_still_warns_rule_a() {
    let warnings = warnings_for(&payload("null", "claude-opus-5-5", false));
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("model 을 지정하지 않은"));
}

// --- Hook contract: never a nonzero exit -------------------------------------

#[test]
fn unknown_subcommand_exits_zero() {
    atelier()
        .args(["orchestrator", "no-such-command"])
        .write_stdin("")
        .assert()
        .success();
}

#[test]
fn help_exits_zero() {
    atelier()
        .args(["orchestrator", "--help"])
        .write_stdin("")
        .assert()
        .success();
}
