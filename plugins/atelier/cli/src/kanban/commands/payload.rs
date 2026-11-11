//! Stdin payloads: `kanban board` takes `{"sessions": [<v1 record>, ...]}`,
//! `kanban answer` takes `{"questions": [...], "picks": [...]}`. A record this
//! version cannot read is dropped here, before the board sees it.
//!
//! The field readers answer `None` for a field of the wrong type, which drops
//! the record, and read an absent field the same as `null`.

use crate::kanban::core::answer::Pick;
use crate::kanban::core::background::{AgentFact, AgentStatus};
use crate::kanban::core::board::{Event, SessionFact};
use crate::kanban::core::detail::{Decision, Delivery, DeliveryStatus};
use crate::kanban::core::progress::{Activity, ItemStatus, WorkItem};
use crate::kanban::core::prompt::PromptFact;
use crate::kanban::core::question::{Ask, Question, QuestionOption};
use crate::kanban::core::request::{RequestKind, Status};
use serde_json::{Map, Value};

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
        repo_root: nullable_string(value, "repo_root")?,
        remote: nullable_string(value, "remote")?,
        permission_mode: nullable_string(value, "permission_mode")?,
        title: nullable_string(value, "title")?,
        first_prompt: nullable_string(value, "first_prompt")?,
        last_prompt: nullable_string(value, "last_prompt")?,
        prompts: optional_array(value, "prompts")?
            .iter()
            .map(parse_prompt)
            .collect::<Option<Vec<_>>>()?,
        last_answer: nullable_string(value, "last_answer")?,
        pending_context: nullable_string(value, "pending_context")?,
        turn_id: nullable_string(value, "turn_id")?,
        started_at: value.get("started_at")?.as_u64()?,
        heartbeat_at: value.get("heartbeat_at")?.as_u64()?,
        ended_reason: nullable_string(value, "ended_reason")?,
        events: value
            .get("events")?
            .as_array()?
            .iter()
            .map(parse_event)
            .collect::<Option<Vec<_>>>()?,
        activity: optional(value, "activity", parse_activity)?,
        recent: optional_array(value, "recent")?
            .iter()
            .map(parse_activity)
            .collect::<Option<Vec<_>>>()?,
        permission: optional(value, "permission", parse_activity)?,
        permission_id: match value.get("permission") {
            Some(permission @ Value::Object(_)) => nullable_string(permission, "id")?,
            _ => None,
        },
        decision: optional(value, "decision", parse_decision)?,
        inbox: optional(value, "inbox", parse_delivery)?,
        tasks: optional_object(value, "tasks")?
            .map(parse_tasks)
            .unwrap_or(Some(Vec::new()))?,
        todos: optional_array(value, "todos")?
            .iter()
            .map(parse_todo)
            .collect::<Option<Vec<_>>>()?,
        asks: optional_object(value, "asks")?
            .map(|asks| {
                asks.iter()
                    .map(|(id, ask)| parse_ask(id, ask))
                    .collect::<Option<Vec<_>>>()
            })
            .unwrap_or(Some(Vec::new()))?,
        agents: optional_array(value, "agents")?
            .iter()
            .map(parse_agent)
            .collect::<Option<Vec<_>>>()?,
    })
}

fn parse_agent(value: &Value) -> Option<AgentFact> {
    Some(AgentFact {
        status: AgentStatus::parse(value.get("status")?.as_str()?),
        teammate: value.get("teammate")?.as_bool()?,
    })
}

fn parse_activity(value: &Value) -> Option<Activity> {
    let input = value
        .get("input")?
        .as_object()?
        .iter()
        .map(|(name, v)| Some((name.clone(), v.as_str()?.to_string())))
        .collect::<Option<Vec<_>>>()?;
    Some(Activity {
        tool: value.get("tool")?.as_str()?.to_string(),
        input,
        at: value.get("at")?.as_u64()?,
    })
}

fn parse_prompt(value: &Value) -> Option<PromptFact> {
    Some(PromptFact {
        text: value.get("text")?.as_str()?.to_string(),
        origin: value.get("origin")?.as_str()?.to_string(),
        at: value.get("at")?.as_u64()?,
    })
}

fn parse_decision(value: &Value) -> Option<Decision> {
    Some(Decision {
        ask: value.get("ask")?.as_str()?.to_string(),
        recap: nullable_string(value, "recap")?,
        options: optional_array(value, "options")?
            .iter()
            .map(|o| Some(o.as_str()?.to_string()))
            .collect::<Option<Vec<_>>>()?,
        at: value.get("at")?.as_u64()?,
    })
}

fn parse_delivery(value: &Value) -> Option<Delivery> {
    let status = value.get("status")?.as_str()?;
    Some(Delivery {
        id: value.get("id")?.as_str()?.to_string(),
        kind: RequestKind::parse(value.get("kind")?.as_str()?),
        status: Status::parse(status)
            .map(DeliveryStatus::Known)
            .unwrap_or_else(|| DeliveryStatus::Unknown(status.to_string())),
        at: value.get("at")?.as_u64()?,
    })
}

fn optional<T>(value: &Value, key: &str, parse: fn(&Value) -> Option<T>) -> Option<Option<T>> {
    match value.get(key) {
        None | Some(Value::Null) => Some(None),
        Some(field) => parse(field).map(Some),
    }
}

/// Task ids are the engine's counters ("1", "2", ... "10"), so they are
/// ordered as numbers where they are numbers.
fn parse_tasks(tasks: &Map<String, Value>) -> Option<Vec<WorkItem>> {
    let mut entries: Vec<(&String, &Value)> = tasks.iter().collect();
    entries.sort_by_key(|(id, _)| (id.parse::<u64>().unwrap_or(u64::MAX), (*id).clone()));
    entries
        .into_iter()
        .map(|(_, task)| {
            Some(WorkItem {
                status: ItemStatus::parse(task.get("status")?.as_str()?),
                subject: Some(task.get("subject")?.as_str()?.to_string()),
                active: nullable_string(task, "active")?,
            })
        })
        .collect()
}

fn parse_todo(value: &Value) -> Option<WorkItem> {
    Some(WorkItem {
        status: ItemStatus::parse(value.get("status")?.as_str()?),
        subject: None,
        active: Some(value.get("active")?.as_str()?.to_string()),
    })
}

fn parse_ask(tool_use_id: &str, value: &Value) -> Option<Ask> {
    Some(Ask {
        tool_use_id: tool_use_id.to_string(),
        questions: parse_questions(value.get("questions")?)?,
        at: value.get("at")?.as_u64()?,
    })
}

fn parse_questions(value: &Value) -> Option<Vec<Question>> {
    value.as_array()?.iter().map(parse_question).collect()
}

fn parse_question(value: &Value) -> Option<Question> {
    Some(Question {
        question: value.get("question")?.as_str()?.to_string(),
        header: value.get("header")?.as_str()?.to_string(),
        multi_select: value.get("multi_select")?.as_bool()?,
        options: value
            .get("options")?
            .as_array()?
            .iter()
            .map(|option| {
                Some(QuestionOption {
                    label: option.get("label")?.as_str()?.to_string(),
                    description: nullable_string(option, "description")?,
                })
            })
            .collect::<Option<Vec<_>>>()?,
    })
}

pub fn parse_answer_request(raw: &str) -> Result<(Vec<Question>, Vec<Pick>), String> {
    let value: Value = serde_json::from_str(raw).map_err(|e| format!("stdin is not JSON: {e}"))?;
    let questions = value
        .get("questions")
        .and_then(parse_questions)
        .ok_or_else(|| "stdin has no readable `questions` array".to_string())?;
    let picks = value
        .get("picks")
        .and_then(Value::as_array)
        .and_then(|picks| picks.iter().map(parse_pick).collect::<Option<Vec<_>>>())
        .ok_or_else(|| "stdin has no readable `picks` array".to_string())?;
    Ok((questions, picks))
}

fn parse_pick(value: &Value) -> Option<Pick> {
    Some(Pick {
        options: value
            .get("options")?
            .as_array()?
            .iter()
            .map(|label| Some(label.as_str()?.to_string()))
            .collect::<Option<Vec<_>>>()?,
        text: nullable_string(value, "text")?,
    })
}

fn parse_event(value: &Value) -> Option<Event> {
    Some(Event {
        name: value.get("name")?.as_str()?.to_string(),
        detail: nullable_string(value, "detail")?,
        at: value.get("at")?.as_u64()?,
    })
}

fn optional_object<'a>(value: &'a Value, key: &str) -> Option<Option<&'a Map<String, Value>>> {
    match value.get(key) {
        None | Some(Value::Null) => Some(None),
        Some(Value::Object(map)) => Some(Some(map)),
        Some(_) => None,
    }
}

fn optional_array<'a>(value: &'a Value, key: &str) -> Option<&'a [Value]> {
    match value.get(key) {
        None | Some(Value::Null) => Some(&[]),
        Some(Value::Array(items)) => Some(items),
        Some(_) => None,
    }
}

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
