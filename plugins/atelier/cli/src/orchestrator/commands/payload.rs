//! Stdin payload for `orchestrator spawn-check`.

use crate::orchestrator::core::spawn_check::SpawnFacts;
use serde_json::Value;

/// Parses the `agent.spawn` facts JSON. Returns `None` when stdin is empty,
/// not valid JSON, or the JSON does not satisfy the schema below — the hook
/// contract treats every one of those cases as "nothing to check"
/// (`{"warnings":[]}`), never as facts defaulted enough to run the rules on.
///
/// The caller (the function hooks module) always sends well-typed facts, so
/// this parses strictly rather than leniently: a wrong-typed field is the
/// caller's bug, and guessing a default for it would only manufacture a
/// false-positive warning.
///
/// - `fork`: JSON bool, required. Missing or wrong-typed → reject.
/// - `parent_model`: JSON string, required. Missing or wrong-typed → reject.
/// - `model`, `resolved_model`: JSON string, `null`, or absent — all three
///   mean "not specified". Any other type → reject.
/// - `subagent_type`: JSON string or absent (absent defaults to `""`). Any
///   other type, including `null` → reject.
pub fn parse_spawn_facts(raw: &str) -> Option<SpawnFacts> {
    let value = serde_json::from_str::<Value>(raw).ok()?;

    let fork = value.get("fork")?.as_bool()?;
    let parent_model = value.get("parent_model")?.as_str()?.to_string();
    let model = optional_nullable_string(&value, "model")?;
    let resolved_model = optional_nullable_string(&value, "resolved_model")?;
    let subagent_type = optional_string(&value, "subagent_type")?;

    Some(SpawnFacts {
        model,
        resolved_model,
        parent_model,
        fork,
        subagent_type,
    })
}

/// Reads a field that may be a JSON string, explicit `null`, or absent — all
/// three are valid and mean "not specified" (empty string also collapses to
/// "not specified"). Any other JSON type means the payload is malformed, so
/// this returns `None` to signal "give up parsing the whole payload".
fn optional_nullable_string(value: &Value, key: &str) -> Option<Option<String>> {
    match value.get(key) {
        None => Some(None),
        Some(Value::Null) => Some(None),
        Some(Value::String(s)) => Some(Some(s.clone()).filter(|s| !s.is_empty())),
        Some(_) => None,
    }
}

/// Reads a field that may be a JSON string or absent (absent defaults to
/// `""`). Any other JSON type — including explicit `null` — means the
/// payload is malformed, so this returns `None` ("give up parsing").
fn optional_string(value: &Value, key: &str) -> Option<String> {
    match value.get(key) {
        None => Some(String::new()),
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_failure_yields_none() {
        assert_eq!(parse_spawn_facts("not json"), None);
        assert_eq!(parse_spawn_facts(""), None);
    }

    #[test]
    fn missing_required_fields_reject_the_payload() {
        // `{}` is missing `fork` and `parent_model`, both required.
        assert_eq!(parse_spawn_facts("{}"), None);
    }

    #[test]
    fn optional_fields_default_when_absent_or_null() {
        let facts = parse_spawn_facts(
            r#"{"fork":false,"parent_model":"claude-opus-5-5","model":null,"resolved_model":null}"#,
        )
        .expect("valid JSON");
        assert_eq!(facts.model, None);
        assert_eq!(facts.resolved_model, None);
        assert_eq!(facts.parent_model, "claude-opus-5-5");
        assert!(!facts.fork);
        assert_eq!(facts.subagent_type, "");
    }

    #[test]
    fn wrong_typed_fork_rejects_the_payload() {
        assert_eq!(
            parse_spawn_facts(
                r#"{"fork":"true","parent_model":"claude-opus-5-5","subagent_type":"x"}"#
            ),
            None
        );
    }

    #[test]
    fn wrong_typed_parent_model_rejects_the_payload() {
        assert_eq!(
            parse_spawn_facts(r#"{"fork":false,"parent_model":123}"#),
            None
        );
    }

    #[test]
    fn wrong_typed_model_rejects_the_payload() {
        assert_eq!(
            parse_spawn_facts(r#"{"fork":false,"parent_model":"claude-opus-5-5","model":123}"#),
            None
        );
    }

    #[test]
    fn wrong_typed_resolved_model_rejects_the_payload() {
        assert_eq!(
            parse_spawn_facts(
                r#"{"fork":false,"parent_model":"claude-opus-5-5","resolved_model":true}"#
            ),
            None
        );
    }

    #[test]
    fn wrong_typed_subagent_type_rejects_the_payload() {
        assert_eq!(
            parse_spawn_facts(
                r#"{"fork":false,"parent_model":"claude-opus-5-5","subagent_type":123}"#
            ),
            None
        );
    }

    #[test]
    fn null_subagent_type_rejects_the_payload() {
        assert_eq!(
            parse_spawn_facts(
                r#"{"fork":false,"parent_model":"claude-opus-5-5","subagent_type":null}"#
            ),
            None
        );
    }

    #[test]
    fn parses_full_payload() {
        let facts = parse_spawn_facts(
            r#"{"model":"sonnet","resolved_model":"claude-sonnet-5","parent_model":"claude-opus-5-5","fork":true,"subagent_type":"general-purpose"}"#,
        )
        .expect("valid JSON");
        assert_eq!(facts.model.as_deref(), Some("sonnet"));
        assert_eq!(facts.resolved_model.as_deref(), Some("claude-sonnet-5"));
        assert_eq!(facts.parent_model, "claude-opus-5-5");
        assert!(facts.fork);
        assert_eq!(facts.subagent_type, "general-purpose");
    }
}
