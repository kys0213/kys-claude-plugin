//! Stdin payload for `kanban board`: `{"sessions": [<v1 record>, ...]}`.
//!
//! A record this version cannot read (another `v`, a missing or wrong-typed
//! field) is skipped, never expired, so a newer module's records survive an
//! older CLI.

use crate::kanban::core::board::{Event, SessionFact};
use serde_json::Value;

const VERSION: u64 = 1;

pub fn parse_sessions(raw: &str) -> Result<Vec<SessionFact>, String> {
    let value: Value = serde_json::from_str(raw).map_err(|e| format!("stdin is not JSON: {e}"))?;
    let records = value
        .get("sessions")
        .and_then(Value::as_array)
        .ok_or_else(|| "stdin has no `sessions` array".to_string())?;
    Ok(records.iter().filter_map(parse_record).collect())
}

fn parse_record(value: &Value) -> Option<SessionFact> {
    if value.get("v")?.as_u64()? != VERSION {
        return None;
    }
    Some(SessionFact {
        session_id: value.get("session_id")?.as_str()?.to_string(),
        cwd: value.get("cwd")?.as_str()?.to_string(),
        repo: nullable_string(value, "repo")?,
        title: nullable_string(value, "title")?,
        first_prompt: nullable_string(value, "first_prompt")?,
        started_at: value.get("started_at")?.as_u64()?,
        heartbeat_at: value.get("heartbeat_at")?.as_u64()?,
        ended_reason: nullable_string(value, "ended_reason")?,
        events: value
            .get("events")?
            .as_array()?
            .iter()
            .map(parse_event)
            .collect::<Option<Vec<_>>>()?,
    })
}

fn parse_event(value: &Value) -> Option<Event> {
    Some(Event {
        name: value.get("name")?.as_str()?.to_string(),
        detail: nullable_string(value, "detail")?,
        at: value.get("at")?.as_u64()?,
    })
}

/// Outer `None` rejects a wrong-typed field; absent and `null` both read as
/// unset.
fn nullable_string(value: &Value, key: &str) -> Option<Option<String>> {
    match value.get(key) {
        None | Some(Value::Null) => Some(None),
        Some(Value::String(s)) => Some(Some(s.clone())),
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORD: &str = r#"{"v":1,"session_id":"s","cwd":"/w","started_at":1,"heartbeat_at":2,"events":[{"name":"turn.start","at":3}]}"#;

    #[test]
    fn optional_fields_may_be_absent() {
        let sessions = parse_sessions(&format!(r#"{{"sessions":[{RECORD}]}}"#)).expect("valid");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].repo, None);
        assert_eq!(sessions[0].events[0].detail, None);
    }

    #[test]
    fn a_wrong_typed_optional_field_skips_the_record() {
        let bad = RECORD.replace(r#""cwd":"/w""#, r#""cwd":"/w","repo":7"#);
        let sessions = parse_sessions(&format!(r#"{{"sessions":[{bad}]}}"#)).expect("valid");
        assert!(sessions.is_empty());
    }

    #[test]
    fn a_malformed_event_skips_the_record() {
        let bad = RECORD.replace(r#""at":3"#, r#""at":"3""#);
        let sessions = parse_sessions(&format!(r#"{{"sessions":[{bad}]}}"#)).expect("valid");
        assert!(sessions.is_empty());
    }
}
