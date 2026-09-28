//! Stdin payload for `orchestrator spawn-check`.

use crate::orchestrator::core::spawn_check::SpawnFacts;

/// Parses the `agent.spawn` facts JSON. Returns `None` when stdin is empty
/// or not valid JSON — the hook contract treats that case as "nothing to
/// check" (`{"warnings":[]}`), not as facts defaulted enough to run the
/// rules on: an all-`None`/`false`/`""` `SpawnFacts` would itself trip rule A
/// (no model, not a fork). Once the JSON parses, every *field* is still read
/// leniently: missing or wrong-typed becomes `None`/`false`/`""`.
pub fn parse_spawn_facts(raw: &str) -> Option<SpawnFacts> {
    let value = serde_json::from_str::<serde_json::Value>(raw).ok()?;
    Some(SpawnFacts {
        model: value["model"]
            .as_str()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty()),
        resolved_model: value["resolved_model"]
            .as_str()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty()),
        parent_model: value["parent_model"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        fork: value["fork"].as_bool().unwrap_or(false),
        subagent_type: value["subagent_type"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    })
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
    fn missing_fields_default_leniently() {
        let facts = parse_spawn_facts("{}").expect("valid JSON");
        assert_eq!(facts.model, None);
        assert_eq!(facts.resolved_model, None);
        assert_eq!(facts.parent_model, "");
        assert!(!facts.fork);
        assert_eq!(facts.subagent_type, "");
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
