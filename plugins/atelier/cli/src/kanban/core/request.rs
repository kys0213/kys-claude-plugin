//! Requests one session leaves for another through its inbox: a prompt to
//! run or a stop for the running turn, and which of them a session takes.

use serde_json::{json, Value};

const VERSION: u64 = 1;
const MAX_TEXT_CHARS: usize = 8_000;
const MAX_ID_CHARS: usize = 128;
const MAX_SLUG_CHARS: usize = 40;

/// Who wrote a prompt: a person on the board, or a model through the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sender {
    Pane,
    Agent,
}

impl Sender {
    pub fn id(self) -> &'static str {
        match self {
            Sender::Pane => "pane",
            Sender::Agent => "agent",
        }
    }

    pub fn parse(raw: &str) -> Option<Sender> {
        match raw {
            "pane" => Some(Sender::Pane),
            "agent" => Some(Sender::Agent),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Prompt {
        body: String,
        from: Sender,
        from_label: Option<String>,
    },
    Stop {
        turn_id: String,
    },
}

impl Request {
    pub fn kind(&self) -> &'static str {
        match self {
            Request::Prompt { .. } => "prompt",
            Request::Stop { .. } => "stop",
        }
    }

    pub fn prompt(body: &str, from: Sender, from_label: Option<&str>) -> Result<Request, String> {
        let body = body.trim();
        if body.is_empty() {
            return Err("보낼 글이 비어 있어요".to_string());
        }
        if body.chars().count() > MAX_TEXT_CHARS {
            return Err(format!("보낼 글이 {MAX_TEXT_CHARS}자를 넘어요"));
        }
        Ok(Request::Prompt {
            body: body.to_string(),
            from,
            from_label: from_label
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string),
        })
    }

    pub fn stop(turn_id: &str) -> Result<Request, String> {
        check_id("turn", turn_id)?;
        Ok(Request::Stop {
            turn_id: turn_id.to_string(),
        })
    }

    pub fn to_json(&self) -> Value {
        match self {
            Request::Prompt {
                body,
                from,
                from_label,
            } => json!({
                "v": VERSION,
                "kind": "prompt",
                "text": body,
                "from": from.id(),
                "from_label": from_label,
            }),
            Request::Stop { turn_id } => {
                json!({ "v": VERSION, "kind": "stop", "turn_id": turn_id })
            }
        }
    }

    pub fn parse(raw: &str) -> Option<Request> {
        let value: Value = serde_json::from_str(raw).ok()?;
        if value.get("v")?.as_u64()? != VERSION {
            return None;
        }
        match value.get("kind")?.as_str()? {
            "prompt" => Request::prompt(
                value.get("text")?.as_str()?,
                Sender::parse(value.get("from")?.as_str()?)?,
                value.get("from_label").and_then(Value::as_str),
            )
            .ok(),
            "stop" => Request::stop(value.get("turn_id")?.as_str()?).ok(),
            _ => None,
        }
    }
}

/// The prompt a model-written request runs as: under a header naming where it
/// came from. A person's words from the board run as typed.
pub fn prompt_text(body: &str, from: Sender, from_label: Option<&str>) -> String {
    match from {
        Sender::Pane => body.to_string(),
        Sender::Agent => format!("{}\n\n{body}", agent_header(from_label)),
    }
}

const HEADER_OPEN: &str = "[칸반 요청 · ";

pub fn agent_header(from_label: Option<&str>) -> String {
    match from_label {
        Some(label) => format!("{HEADER_OPEN}{label} 세션의 오케스트레이터]"),
        None => format!("{HEADER_OPEN}오케스트레이터]"),
    }
}

/// The prompt without the header `agent_header` put in front of it.
pub fn strip_agent_header(text: &str) -> &str {
    text.strip_prefix(HEADER_OPEN)
        .and_then(|rest| rest.split_once(']'))
        .map(|(_, body)| body.trim_start())
        .unwrap_or(text)
}

pub fn check_id(what: &str, id: &str) -> Result<(), String> {
    let ok = !id.is_empty()
        && id.len() <= MAX_ID_CHARS
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        Err(format!("{what} id `{id}` 는 영문·숫자·`-`·`_` 만 받아요"))
    }
}

pub fn check_slug(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name.len() <= MAX_SLUG_CHARS
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "worktree 이름 `{name}` 은 소문자·숫자·`-` 만, {MAX_SLUG_CHARS}자까지 받아요"
        ))
    }
}

/// One inbox file as found: its id, how old it is, and the request if it
/// could be read.
pub struct Found {
    pub id: String,
    pub age_ms: u64,
    pub request: Option<Request>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Taken {
    Prompt {
        text: String,
        body: String,
        from: Sender,
    },
    Stop {
        turn_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Delivered,
    Busy,
    Expired,
    Stale,
    Invalid,
}

impl Status {
    pub fn id(self) -> &'static str {
        match self {
            Status::Delivered => "delivered",
            Status::Busy => "busy",
            Status::Expired => "expired",
            Status::Stale => "stale",
            Status::Invalid => "invalid",
        }
    }
}

pub struct Outcome {
    pub id: String,
    pub kind: &'static str,
    pub status: Status,
}

/// Decides each request in the order found: a stop acts only on the turn it
/// names, a prompt only while no turn runs and none was taken before it, and
/// nothing older than `ttl_ms`.
pub fn take(
    found: Vec<Found>,
    running_turn: Option<&str>,
    ttl_ms: u64,
) -> (Vec<Taken>, Vec<Outcome>) {
    let mut taken = Vec::new();
    let mut outcomes = Vec::new();
    let mut prompted = false;
    for item in found {
        let (kind, status) = match item.request {
            None => ("unknown", Status::Invalid),
            Some(request) => {
                let kind = request.kind();
                let status = if item.age_ms > ttl_ms {
                    Status::Expired
                } else {
                    match request {
                        Request::Stop { turn_id } if running_turn == Some(turn_id.as_str()) => {
                            taken.push(Taken::Stop { turn_id });
                            Status::Delivered
                        }
                        Request::Stop { .. } => Status::Stale,
                        Request::Prompt { .. } if running_turn.is_some() || prompted => {
                            Status::Busy
                        }
                        Request::Prompt {
                            body,
                            from,
                            from_label,
                        } => {
                            prompted = true;
                            taken.push(Taken::Prompt {
                                text: prompt_text(&body, from, from_label.as_deref()),
                                body,
                                from,
                            });
                            Status::Delivered
                        }
                    }
                };
                (kind, status)
            }
        };
        outcomes.push(Outcome {
            id: item.id,
            kind,
            status,
        });
    }
    (taken, outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_round_trips_through_its_json() {
        let prompt = Request::prompt(" hi ", Sender::Agent, Some("main")).expect("prompt");
        assert_eq!(Request::parse(&prompt.to_json().to_string()), Some(prompt));
        let stop = Request::stop("t-1").expect("stop");
        assert_eq!(Request::parse(&stop.to_json().to_string()), Some(stop));
    }

    #[test]
    fn an_expired_stop_is_expired_not_stale() {
        let found = vec![Found {
            id: "1".to_string(),
            age_ms: 10,
            request: Some(Request::stop("t").expect("stop")),
        }];
        let (taken, outcomes) = take(found, Some("t"), 5);
        assert!(taken.is_empty());
        assert_eq!(outcomes[0].status, Status::Expired);
    }
}
