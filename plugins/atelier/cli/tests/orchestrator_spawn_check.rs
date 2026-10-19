use assert_cmd::Command;
use serde_json::Value;

fn atelier() -> Command {
    Command::cargo_bin("atelier").expect("locate `atelier` cargo binary")
}

struct Verdict {
    deny: Option<String>,
    warnings: Vec<String>,
}

fn verdict_for(stdin: &str) -> Verdict {
    let output = atelier()
        .args(["orchestrator", "spawn-check"])
        .write_stdin(stdin)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).expect("valid JSON on stdout");
    let deny = match &value["deny"] {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => panic!("deny is null or a string, got {other}"),
    };
    let warnings = value["warnings"]
        .as_array()
        .expect("warnings is an array")
        .iter()
        .map(|w| w.as_str().expect("warning is a string").to_string())
        .collect();
    Verdict { deny, warnings }
}

fn warnings_for(stdin: &str) -> Vec<String> {
    let v = verdict_for(stdin);
    assert_eq!(v.deny, None, "unexpected deny");
    v.warnings
}

fn deny_for(stdin: &str) -> String {
    let v = verdict_for(stdin);
    assert!(
        v.warnings.is_empty(),
        "a denied dispatch never runs, so it carries no post-run warning: {:?}",
        v.warnings
    );
    v.deny.expect("dispatch should be denied")
}

fn payload(model: &str, parent_model: &str, fork: bool) -> String {
    format!(
        r#"{{"model":{model},"resolved_model":"claude-opus-5-5","parent_model":"{parent_model}","fork":{fork},"subagent_type":"general-purpose"}}"#
    )
}

fn payload_resolved(model: &str, resolved: &str, parent_model: &str) -> String {
    format!(
        r#"{{"model":{model},"resolved_model":{resolved},"parent_model":"{parent_model}","fork":false,"subagent_type":"general-purpose"}}"#
    )
}

// --- Rule A: model unspecified is denied -----------------------------------

#[test]
fn inherit_denied_when_resolved_equals_parent() {
    let d = deny_for(&payload_resolved(
        "null",
        "\" Claude-Opus-5-5 \"",
        "claude-opus-5-5",
    ));
    assert!(d.contains("상속"), "{d}");
    assert!(d.contains("Claude-Opus-5-5"), "{d}");
}

#[test]
fn inherit_allowed_when_definition_picked_lower_model() {
    let w = warnings_for(&payload_resolved(
        "null",
        "\"claude-haiku-4-5-20251001\"",
        "claude-opus-5-5",
    ));
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn inherit_denied_when_resolved_missing() {
    let d = deny_for(&payload_resolved("null", "null", "claude-opus-5-5"));
    assert!(d.contains("부모 model(claude-opus-5-5)"), "{d}");
}

#[test]
fn fable_parent_inheriting_fable_is_denied_with_cap_hint() {
    let d = deny_for(&payload_resolved(
        "null",
        "\"claude-fable-5-1\"",
        "claude-fable-5-1",
    ));
    assert!(d.contains("`opus`"), "{d}");
}

#[test]
fn deny_hint_omits_cap_when_parent_tier_undetectable() {
    let d = deny_for(&payload_resolved("null", "null", "gpt-5"));
    assert!(!d.contains("상한"), "{d}");
}

#[test]
fn opus_parent_definition_picking_fable_yields_cap_warning_only() {
    let w = warnings_for(&payload_resolved(
        "null",
        "\"claude-fable-5-1\"",
        "claude-opus-5-5",
    ));
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("집행 위임 상한"));
    assert!(w[0].contains("실행됐습니다"));
}

#[test]
fn explicit_model_ignores_resolved_model() {
    let w = warnings_for(&payload_resolved(
        "\"opus\"",
        "\"claude-fable-5-1\"",
        "claude-fable-5-1",
    ));
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn cap_warning_for_explicit_model_says_delegated() {
    let w = warnings_for(&payload("\"claude-fable-5-1\"", "claude-opus-5-5", false));
    assert_eq!(w.len(), 1);
    assert!(w[0].contains("위임했습니다"));
}

#[test]
fn messages_are_self_contained() {
    let deny = deny_for(&payload_resolved(
        "null",
        "\"claude-fable-5-1\"",
        "claude-fable-5-1",
    ));
    let cap = warnings_for(&payload("\"claude-fable-5-1\"", "claude-opus-5-5", false));
    assert_eq!(cap.len(), 1);
    let cap = &cap[0];
    for m in [&deny, cap] {
        assert!(m.starts_with("[atelier]"), "{m}");
        assert!(!m.contains("불변식"), "{m}");
        assert!(!m.contains("표 1"), "{m}");
        assert!(!m.contains("시작 tier"), "{m}");
    }
    assert!(deny.contains("다시 호출"), "{deny}");
    assert!(deny.contains("model"), "{deny}");
    assert!(deny.contains("effort"), "{deny}");
    assert!(cap.contains("다시 실행할 필요는 없"), "{cap}");
    assert!(cap.contains("다음 dispatch 부터"), "{cap}");
    assert!(cap.contains("`opus`"), "{cap}");
    assert!(!cap.contains("Opus"), "{cap}");
    assert!(cap.contains("자문 소집이라면 무시해도"), "{cap}");
}

#[test]
fn rule_a_denies_null_model() {
    deny_for(&payload("null", "claude-opus-5-5", false));
}

#[test]
fn rule_a_denies_blank_model() {
    deny_for(&payload("\"   \"", "claude-opus-5-5", false));
}

#[test]
fn rule_a_denies_inherit_case_insensitive() {
    deny_for(&payload("\"INHERIT\"", "claude-opus-5-5", false));
}

#[test]
fn rule_a_allows_fork() {
    let warnings = warnings_for(&payload("null", "claude-opus-5-5", true));
    assert!(warnings.is_empty());
}

#[test]
fn rule_a_allows_given_model() {
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
    assert!(warnings[0].contains("집행 위임 상한"));
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
fn missing_model_key_still_denies_rule_a() {
    let stdin = r#"{"resolved_model":null,"parent_model":"claude-opus-5-5","fork":false,"subagent_type":"general-purpose"}"#;
    let d = deny_for(stdin);
    assert!(d.contains("상속"), "{d}");
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
