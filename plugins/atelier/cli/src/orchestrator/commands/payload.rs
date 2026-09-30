//! Stdin payload for `orchestrator spawn-check`.

use crate::orchestrator::core::spawn_check::SpawnFacts;
use serde_json::Value;

pub fn parse_spawn_facts(raw: &str) -> Option<SpawnFacts> {
    let value = serde_json::from_str::<Value>(raw).ok()?;

    let fork = value.get("fork")?.as_bool()?;
    let parent_model = value.get("parent_model")?.as_str()?.to_string();
    let model = optional_nullable_string(&value, "model")?.into_specified();
    let resolved_model = optional_nullable_string(&value, "resolved_model")?.into_specified();
    let subagent_type = optional_string(&value, "subagent_type")?;

    Some(SpawnFacts {
        model,
        resolved_model,
        parent_model,
        fork,
        subagent_type,
    })
}

enum NullableField {
    Unspecified,
    Value(String),
}

impl NullableField {
    fn into_specified(self) -> Option<String> {
        match self {
            NullableField::Unspecified => None,
            NullableField::Value(s) if s.is_empty() => None,
            NullableField::Value(s) => Some(s),
        }
    }
}

fn optional_nullable_string(value: &Value, key: &str) -> Option<NullableField> {
    match value.get(key) {
        None | Some(Value::Null) => Some(NullableField::Unspecified),
        Some(Value::String(s)) => Some(NullableField::Value(s.clone())),
        Some(_) => None,
    }
}

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
