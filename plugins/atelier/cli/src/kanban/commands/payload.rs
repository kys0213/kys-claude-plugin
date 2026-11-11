//! Stdin payloads: `kanban board` takes `{"sessions": [<v1 record>, ...]}`,
//! `kanban answer` takes `{"questions": [...], "picks": [...]}`, `inbox take`
//! what the session waits on and the nonces the pane left for it,
//! `inbox wait-permit` those nonces, `guard-bash` the Bash tool's input,
//! `ledger record` what a mod knows of one answer. A record this version cannot
//! read is dropped here, before the board sees it.
//!
//! The field readers answer `None` for a field of the wrong type, which drops
//! the record, and read an absent field the same as `null`.

use crate::kanban::core::answer::Pick;
use crate::kanban::core::background::{AgentFact, AgentStatus};
use crate::kanban::core::board::{Event, PermissionAsk, SessionFact};
use crate::kanban::core::detail::{Decision, DecisionLengths, Delivery, DeliveryStatus};
use crate::kanban::core::ledger::{Answered, DecisionBy, Facts, OpenCall, PermitBy, QuestionBy};
use crate::kanban::core::progress::{Activity, ItemStatus, WorkItem};
use crate::kanban::core::prompt::PromptFact;
use crate::kanban::core::question::{Ask, AskLengths, Question, QuestionOption};
use crate::kanban::core::request::{
    check_id, LeftNonce, Nonce, OpenAsk, PaneNonces, RequestKind, Status, Verdict, Via, Waits,
};
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

/// One session's record copy; `None` when it is not JSON or not a record this
/// version reads.
pub fn parse_record_copy(raw: &str) -> Option<SessionFact> {
    parse_record(&serde_json::from_str(raw).ok()?)
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
        permission_input_len: match value.get("permission") {
            Some(permission @ Value::Object(_)) => {
                optional(permission, "input_len", parse_field_lengths)?
            }
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
        orig_len: optional(value, "orig_len", parse_decision_lengths)?,
    })
}

fn parse_decision_lengths(value: &Value) -> Option<DecisionLengths> {
    Some(DecisionLengths {
        ask: value.get("ask")?.as_u64()?,
        options: parse_lengths(value.get("options")?)?,
    })
}

fn parse_lengths(value: &Value) -> Option<Vec<u64>> {
    value.as_array()?.iter().map(Value::as_u64).collect()
}

fn parse_field_lengths(value: &Value) -> Option<Vec<(String, u64)>> {
    value
        .as_object()?
        .iter()
        .map(|(name, len)| Some((name.clone(), len.as_u64()?)))
        .collect()
}

fn parse_ask_lengths(value: &Value) -> Option<AskLengths> {
    Some(AskLengths {
        question: parse_lengths(value.get("question")?)?,
        labels: value
            .get("labels")?
            .as_array()?
            .iter()
            .map(parse_lengths)
            .collect::<Option<Vec<_>>>()?,
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
        orig_len: optional(value, "orig_len", parse_ask_lengths)?,
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

/// What `inbox take` waits on: `{"asks": [{"id", "questions": [...]}],
/// "decision": {"at", ...} | null}`. Empty stdin waits on nothing.
pub fn parse_waits(raw: &str) -> Result<Waits, String> {
    if raw.trim().is_empty() {
        return Ok(Waits::default());
    }
    let value: Value = serde_json::from_str(raw).map_err(|e| format!("stdin is not JSON: {e}"))?;
    let asks = optional_array(&value, "asks")
        .and_then(|asks| asks.iter().map(parse_open_ask).collect::<Option<Vec<_>>>())
        .ok_or_else(|| "stdin has no readable `asks` array".to_string())?;
    let decision_at = optional_object(&value, "decision")
        .and_then(|decision| match decision {
            None => Some(None),
            Some(fields) => Some(Some(fields.get("at")?.as_u64()?.to_string())),
        })
        .ok_or_else(|| "stdin has no readable `decision`".to_string())?;
    Ok(Waits { asks, decision_at })
}

fn parse_open_ask(value: &Value) -> Option<OpenAsk> {
    Some(OpenAsk {
        id: value.get("id")?.as_str()?.to_string(),
        questions: value.get("questions")?.as_array()?.len(),
    })
}

/// The nonces the pane left for the session, beside what it waits on:
/// `{"nonces": [{"kind", "nonce"}]}`, `kind` an answer's request kind. An entry
/// that does not read proves nothing and is skipped. Empty stdin left none.
pub fn parse_pane_nonces(raw: &str) -> Result<PaneNonces, String> {
    if raw.trim().is_empty() {
        return Ok(PaneNonces::default());
    }
    let value: Value = serde_json::from_str(raw).map_err(|e| format!("stdin is not JSON: {e}"))?;
    let left = optional_array(&value, "nonces")
        .ok_or_else(|| "stdin has no readable `nonces` array".to_string())?;
    Ok(PaneNonces::new(
        left.iter().filter_map(parse_left_nonce).collect(),
    ))
}

fn parse_left_nonce(value: &Value) -> Option<LeftNonce> {
    Some(LeftNonce {
        kind: RequestKind::parse(value.get("kind")?.as_str()?)?,
        nonce: Nonce::parse(value.get("nonce")?.as_str()?).ok()?,
    })
}

/// The Bash tool's input: `{"command": ...}`.
pub fn parse_bash_command(raw: &str) -> Result<String, String> {
    let value: Value = serde_json::from_str(raw).map_err(|e| format!("stdin is not JSON: {e}"))?;
    value
        .get("command")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "stdin has no `command` string".to_string())
}

/// What `ledger record` takes: when, the session and where it runs, the wait
/// as its record keeps it under `question`, `permission` or `decision`, and
/// `answer`, how the answer came in.
pub fn parse_ledger_facts(raw: &str) -> Result<Facts, String> {
    let value: Value = serde_json::from_str(raw).map_err(|e| format!("stdin is not JSON: {e}"))?;
    let at = value
        .get("at")
        .and_then(Value::as_u64)
        .ok_or_else(|| "stdin has no `at` time".to_string())?;
    let session = value
        .get("session")
        .and_then(Value::as_str)
        .ok_or_else(|| "stdin has no `session`".to_string())?;
    check_id("session", session)?;
    let cwd = value
        .get("cwd")
        .and_then(Value::as_str)
        .ok_or_else(|| "stdin has no `cwd`".to_string())?;
    let repo_root = nullable_string(&value, "repo_root")
        .ok_or_else(|| "`repo_root` is not a string".to_string())?;
    let permission_mode = nullable_string(&value, "permission_mode")
        .ok_or_else(|| "`permission_mode` is not a string".to_string())?;
    let answer = value
        .get("answer")
        .filter(|a| a.is_object())
        .ok_or_else(|| "stdin has no `answer` object".to_string())?;
    let way = Way::read(answer)?;
    let waits: Vec<&str> = ["question", "permission", "decision"]
        .into_iter()
        .filter(|kind| !matches!(value.get(*kind), None | Some(Value::Null)))
        .collect();
    let answered = match waits.as_slice() {
        ["question"] => parse_answered_question(&value["question"], answer, way),
        ["permission"] => parse_answered_permission(&value["permission"], answer, way),
        ["decision"] => parse_answered_decision(&value["decision"], answer, way),
        _ => Err(
            "stdin names no wait, or more than one: give one of `question`, `permission`, `decision`"
                .to_string(),
        ),
    }?;
    Ok(Facts {
        at,
        session: session.to_string(),
        repo_root,
        cwd: cwd.to_string(),
        permission_mode,
        answered,
    })
}

/// What `ledger bind` takes: the permission request's `tool` and the
/// session's `calls` in flight, `{"tool_use_id", "tool", "permission"}` each,
/// `permission` whether a request is already bound to it.
pub fn parse_bind_facts(raw: &str) -> Result<(String, Vec<OpenCall>), String> {
    let value: Value = serde_json::from_str(raw).map_err(|e| format!("stdin is not JSON: {e}"))?;
    let tool = value
        .get("tool")
        .and_then(Value::as_str)
        .ok_or_else(|| "stdin has no `tool`".to_string())?;
    let calls = value
        .get("calls")
        .and_then(Value::as_array)
        .and_then(|calls| {
            calls
                .iter()
                .map(parse_open_call)
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| "stdin has no readable `calls` array".to_string())?;
    Ok((tool.to_string(), calls))
}

fn parse_open_call(value: &Value) -> Option<OpenCall> {
    Some(OpenCall {
        tool_use_id: value.get("tool_use_id")?.as_str()?.to_string(),
        tool: value.get("tool")?.as_str()?.to_string(),
        has_permission: value.get("permission")?.as_bool()?,
    })
}

/// How an answer came in: the asking session's own screen, or its inbox as
/// the CLI judged it there. A `via` it did not name proves nothing.
#[derive(Clone, Copy)]
enum Way {
    Session,
    Inbox(Via),
}

impl Way {
    fn read(answer: &Value) -> Result<Way, String> {
        match answer.get("path").and_then(Value::as_str) {
            Some("session") => Ok(Way::Session),
            Some("inbox") => Ok(Way::Inbox(
                match answer.get("via").and_then(Value::as_str) {
                    Some("pane") => Via::Pane,
                    Some("agent") => Via::Agent,
                    _ => Via::Terminal,
                },
            )),
            _ => Err("`answer.path` is `session` or `inbox`".to_string()),
        }
    }
}

fn required_bool(value: &Value, key: &str, what: &str) -> Result<bool, String> {
    value
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("`{what}` is not a boolean"))
}

fn dialog_answers(answer: &Value, answers: &[Value]) -> Result<Vec<Option<String>>, String> {
    let is_error = required_bool(answer, "is_error", "answer.is_error")?;
    let denied = required_bool(answer, "denied", "answer.denied")?;
    if is_error || denied {
        return Ok(Vec::new());
    }
    Ok(answers
        .iter()
        .map(|a| a.as_str().map(str::to_string))
        .collect())
}

fn inbox_answers(answers: &[Value]) -> Result<Vec<Option<String>>, String> {
    answers
        .iter()
        .map(|a| match a {
            Value::Null => Some(None),
            Value::String(s) => Some(Some(s.clone())),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| "`answer.answers` is not a list of answers".to_string())
}

fn parse_answered_question(wait: &Value, answer: &Value, way: Way) -> Result<Answered, String> {
    let unreadable = || "`question` is not a recorded question".to_string();
    let id = wait
        .get("tool_use_id")
        .and_then(Value::as_str)
        .ok_or_else(unreadable)?;
    let ask = parse_ask(id, wait).ok_or_else(unreadable)?;
    let given = answer
        .get("answers")
        .and_then(Value::as_array)
        .ok_or_else(|| "`answer.answers` is not a list".to_string())?;
    let (by, answers) = match way {
        Way::Session => (
            QuestionBy::Dialog {
                afk: answer.get("afk_timeout_ms").is_some_and(Value::is_number),
            },
            dialog_answers(answer, given)?,
        ),
        Way::Inbox(via) => (QuestionBy::Inbox(via), inbox_answers(given)?),
    };
    Ok(Answered::Question { ask, by, answers })
}

fn parse_answered_permission(wait: &Value, answer: &Value, way: Way) -> Result<Answered, String> {
    let unreadable = || "`permission` is not a recorded permission request".to_string();
    let ask = PermissionAsk {
        id: nullable_string(wait, "id").ok_or_else(unreadable)?,
        request: parse_activity(wait).ok_or_else(unreadable)?,
        input_len: optional(wait, "input_len", parse_field_lengths).ok_or_else(unreadable)?,
    };
    let by = match way {
        Way::Session => PermitBy::Dialog {
            ran: match answer.get("post") {
                None | Some(Value::Null) => false,
                Some(post)
                    if matches!(post.as_str(), Some("PostToolUse" | "PostToolUseFailure")) =>
                {
                    true
                }
                Some(_) => {
                    return Err(
                        "`answer.post` is `PostToolUse`, `PostToolUseFailure` or null".to_string(),
                    )
                }
            },
            is_error: answer
                .get("is_error")
                .and_then(Value::as_bool)
                .ok_or_else(|| "`answer.is_error` is not a boolean".to_string())?,
        },
        Way::Inbox(via) => PermitBy::Inbox {
            via,
            verdict: answer
                .get("decision")
                .and_then(Value::as_str)
                .and_then(Verdict::parse)
                .ok_or_else(|| "`answer.decision` is `allow` or `deny`".to_string())?,
        },
    };
    Ok(Answered::Permission { ask, by })
}

fn parse_answered_decision(wait: &Value, answer: &Value, way: Way) -> Result<Answered, String> {
    let decision =
        parse_decision(wait).ok_or_else(|| "`decision` is not a recorded decision".to_string())?;
    let text = answer
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| "`answer.text` is not a string".to_string())?;
    let by = match way {
        Way::Session => DecisionBy::Prompt {
            origin: answer
                .get("origin")
                .and_then(Value::as_str)
                .ok_or_else(|| "`answer.origin` is not a string".to_string())?
                .to_string(),
        },
        Way::Inbox(via) => DecisionBy::Inbox(via),
    };
    Ok(Answered::Decision {
        decision,
        by,
        text: text.to_string(),
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
