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

/// A person's answer to a permission dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny,
}

impl Verdict {
    pub fn id(self) -> &'static str {
        match self {
            Verdict::Allow => "allow",
            Verdict::Deny => "deny",
        }
    }

    pub fn parse(raw: &str) -> Option<Verdict> {
        match raw {
            "allow" => Some(Verdict::Allow),
            "deny" => Some(Verdict::Deny),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestKind {
    Prompt,
    Stop,
    Permit,
    Answer,
    Decide,
}

impl RequestKind {
    pub fn id(self) -> &'static str {
        match self {
            RequestKind::Prompt => "prompt",
            RequestKind::Stop => "stop",
            RequestKind::Permit => "permit",
            RequestKind::Answer => "answer",
            RequestKind::Decide => "decide",
        }
    }

    pub fn parse(raw: &str) -> Option<RequestKind> {
        [
            RequestKind::Prompt,
            RequestKind::Stop,
            RequestKind::Permit,
            RequestKind::Answer,
            RequestKind::Decide,
        ]
        .into_iter()
        .find(|k| k.id() == raw)
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
    /// The answer to the permission request the asking session named.
    Permit {
        request_id: String,
        verdict: Verdict,
        from: Sender,
    },
    /// One answer per question of the open AskUserQuestion call it names.
    Answer {
        tool_use_id: String,
        answers: Vec<String>,
        from: Sender,
    },
    /// The reply to the decision a turn ended on, named by when it was left.
    Decide {
        decision_at: String,
        text: String,
        from: Sender,
    },
}

/// Refuses a model's answer: nothing yet can tell one worth taking.
pub fn check_answerer(from: Sender) -> Result<(), String> {
    match from {
        Sender::Pane => Ok(()),
        Sender::Agent => Err(
            "메인 에이전트의 답(`--from agent`)은 아직 받지 않아요 — 장부가 필요해요".to_string(),
        ),
    }
}

fn check_text(what: &str, text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(format!("{what}이 비어 있어요"));
    }
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(format!("{what}이 {MAX_TEXT_CHARS}자를 넘어요"));
    }
    Ok(text.to_string())
}

fn strings(value: &Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|a| Some(a.as_str()?.to_string()))
        .collect()
}

impl Request {
    pub fn kind(&self) -> RequestKind {
        match self {
            Request::Prompt { .. } => RequestKind::Prompt,
            Request::Stop { .. } => RequestKind::Stop,
            Request::Permit { .. } => RequestKind::Permit,
            Request::Answer { .. } => RequestKind::Answer,
            Request::Decide { .. } => RequestKind::Decide,
        }
    }

    pub fn permit(request_id: &str, verdict: &str, from: Sender) -> Result<Request, String> {
        check_id("permission", request_id)?;
        let verdict = Verdict::parse(verdict)
            .ok_or_else(|| format!("--decision 은 allow 또는 deny 예요 (받은 값: {verdict})"))?;
        Ok(Request::Permit {
            request_id: request_id.to_string(),
            verdict,
            from,
        })
    }

    pub fn answer(tool_use_id: &str, answers: &[String], from: Sender) -> Result<Request, String> {
        check_id("question", tool_use_id)?;
        if answers.is_empty() {
            return Err("답이 없어요 — 질문마다 --answer 를 하나씩 주세요".to_string());
        }
        Ok(Request::Answer {
            tool_use_id: tool_use_id.to_string(),
            answers: answers
                .iter()
                .map(|a| check_text("답", a).map(|_| a.clone()))
                .collect::<Result<_, _>>()?,
            from,
        })
    }

    pub fn decide(decision_at: &str, text: &str, from: Sender) -> Result<Request, String> {
        check_id("decision", decision_at)?;
        Ok(Request::Decide {
            decision_at: decision_at.to_string(),
            text: check_text("결정 답", text)?,
            from,
        })
    }

    pub fn prompt(body: &str, from: Sender, from_label: Option<&str>) -> Result<Request, String> {
        Ok(Request::Prompt {
            body: check_text("보낼 글", body)?,
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
            Request::Permit {
                request_id,
                verdict,
                from,
            } => json!({
                "v": VERSION,
                "kind": "permit",
                "request_id": request_id,
                "decision": verdict.id(),
                "from": from.id(),
            }),
            Request::Answer {
                tool_use_id,
                answers,
                from,
            } => json!({
                "v": VERSION,
                "kind": "answer",
                "tool_use_id": tool_use_id,
                "answers": answers,
                "from": from.id(),
            }),
            Request::Decide {
                decision_at,
                text,
                from,
            } => json!({
                "v": VERSION,
                "kind": "decide",
                "decision_at": decision_at,
                "text": text,
                "from": from.id(),
            }),
        }
    }

    pub fn parse(raw: &str) -> Option<Request> {
        let value: Value = serde_json::from_str(raw).ok()?;
        if value.get("v")?.as_u64()? != VERSION {
            return None;
        }
        let from = || Sender::parse(value.get("from")?.as_str()?);
        match RequestKind::parse(value.get("kind")?.as_str()?)? {
            RequestKind::Prompt => Request::prompt(
                value.get("text")?.as_str()?,
                from()?,
                value.get("from_label").and_then(Value::as_str),
            )
            .ok(),
            RequestKind::Stop => Request::stop(value.get("turn_id")?.as_str()?).ok(),
            RequestKind::Permit => Request::permit(
                value.get("request_id")?.as_str()?,
                value.get("decision")?.as_str()?,
                // A permit with no sender was left by a pane that named none.
                match value.get("from") {
                    None => Sender::Pane,
                    Some(_) => from()?,
                },
            )
            .ok(),
            RequestKind::Answer => Request::answer(
                value.get("tool_use_id")?.as_str()?,
                &strings(value.get("answers")?)?,
                from()?,
            )
            .ok(),
            RequestKind::Decide => Request::decide(
                value.get("decision_at")?.as_str()?,
                value.get("text")?.as_str()?,
                from()?,
            )
            .ok(),
        }
    }
}

/// An AskUserQuestion call the taking session has open, and how many
/// questions it asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAsk {
    pub id: String,
    pub questions: usize,
}

/// What the taking session waits on now: an answer or a decide that names
/// anything else is stale.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Waits {
    pub asks: Vec<OpenAsk>,
    /// The id of the decision its last turn ended on.
    pub decision_at: Option<String>,
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
    Answer {
        tool_use_id: String,
        answers: Vec<String>,
        from: Sender,
    },
    /// Runs as a prompt, the text as typed.
    Decide {
        decision_at: String,
        text: String,
        from: Sender,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Delivered,
    Busy,
    Expired,
    Stale,
    Invalid,
    /// A model's answer the taking session would not take.
    Refused,
}

impl Status {
    pub fn id(self) -> &'static str {
        match self {
            Status::Delivered => "delivered",
            Status::Busy => "busy",
            Status::Expired => "expired",
            Status::Stale => "stale",
            Status::Invalid => "invalid",
            Status::Refused => "refused",
        }
    }

    pub fn parse(raw: &str) -> Option<Status> {
        [
            Status::Delivered,
            Status::Busy,
            Status::Expired,
            Status::Stale,
            Status::Invalid,
            Status::Refused,
        ]
        .into_iter()
        .find(|s| s.id() == raw)
    }
}

pub struct Outcome {
    pub id: String,
    /// `None` for a request that could not be read.
    pub kind: Option<RequestKind>,
    pub status: Status,
}

/// Which requests one take acts on: at most one prompt, and each open
/// question answered once.
struct Taker<'a> {
    running_turn: Option<&'a str>,
    waits: &'a Waits,
    prompted: bool,
    answered: Vec<String>,
    taken: Vec<Taken>,
}

impl Taker<'_> {
    fn deliver(&mut self, taken: Taken) -> Status {
        self.taken.push(taken);
        Status::Delivered
    }

    fn is_busy(&self) -> bool {
        self.running_turn.is_some() || self.prompted
    }

    fn judge(&mut self, request: Request) -> Status {
        match request {
            Request::Stop { turn_id } if self.running_turn == Some(turn_id.as_str()) => {
                self.deliver(Taken::Stop { turn_id })
            }
            Request::Stop { .. } | Request::Permit { .. } => Status::Stale,
            Request::Answer {
                from: Sender::Agent,
                ..
            }
            | Request::Decide {
                from: Sender::Agent,
                ..
            } => Status::Refused,
            Request::Answer {
                tool_use_id,
                answers,
                from,
            } => self.answer(tool_use_id, answers, from),
            Request::Decide { decision_at, .. }
                if self.waits.decision_at.as_deref() != Some(decision_at.as_str()) =>
            {
                Status::Stale
            }
            Request::Decide { .. } | Request::Prompt { .. } if self.is_busy() => Status::Busy,
            Request::Decide {
                decision_at,
                text,
                from,
            } => {
                self.prompted = true;
                self.deliver(Taken::Decide {
                    decision_at,
                    text,
                    from,
                })
            }
            Request::Prompt {
                body,
                from,
                from_label,
            } => {
                self.prompted = true;
                self.deliver(Taken::Prompt {
                    text: prompt_text(&body, from, from_label.as_deref()),
                    body,
                    from,
                })
            }
        }
    }

    fn answer(&mut self, tool_use_id: String, answers: Vec<String>, from: Sender) -> Status {
        let open = self
            .waits
            .asks
            .iter()
            .find(|ask| ask.id == tool_use_id && !self.answered.contains(&ask.id));
        match open {
            None => Status::Stale,
            Some(ask) if ask.questions != answers.len() => Status::Invalid,
            Some(_) => {
                self.answered.push(tool_use_id.clone());
                self.deliver(Taken::Answer {
                    tool_use_id,
                    answers,
                    from,
                })
            }
        }
    }
}

pub fn take(
    found: Vec<Found>,
    running_turn: Option<&str>,
    waits: &Waits,
    ttl_ms: u64,
) -> (Vec<Taken>, Vec<Outcome>) {
    let mut taker = Taker {
        running_turn,
        waits,
        prompted: false,
        answered: Vec::new(),
        taken: Vec::new(),
    };
    let outcomes = found
        .into_iter()
        .map(|item| {
            let kind = item.request.as_ref().map(Request::kind);
            let status = match item.request {
                None => Status::Invalid,
                Some(_) if item.age_ms > ttl_ms => Status::Expired,
                Some(request) => taker.judge(request),
            };
            Outcome {
                id: item.id,
                kind,
                status,
            }
        })
        .collect();
    (taker.taken, outcomes)
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
        let answers = ["Red".to_string()];
        let answer = Request::answer("tu-1", &answers, Sender::Pane).expect("answer");
        assert_eq!(Request::parse(&answer.to_json().to_string()), Some(answer));
        let decide = Request::decide("1700", "네", Sender::Pane).expect("decide");
        assert_eq!(Request::parse(&decide.to_json().to_string()), Some(decide));
    }

    #[test]
    fn an_expired_stop_is_expired_not_stale() {
        let found = vec![Found {
            id: "1".to_string(),
            age_ms: 10,
            request: Some(Request::stop("t").expect("stop")),
        }];
        let (taken, outcomes) = take(found, Some("t"), &Waits::default(), 5);
        assert!(taken.is_empty());
        assert_eq!(outcomes[0].status, Status::Expired);
    }
}
