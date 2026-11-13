//! Requests one session leaves for another through its inbox: a prompt to
//! run or a stop for the running turn, and which of them a session takes.

use super::precedent::{Auth, SessionPlace};
use super::question::Question;
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

/// The one-time token the pane leaves with an answer; carried, never judged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nonce(String);

impl Nonce {
    pub fn parse(raw: &str) -> Result<Nonce, String> {
        check_id("nonce", raw)?;
        Ok(Nonce(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn read(value: &Value) -> Result<Option<Nonce>, String> {
        match value.get("nonce") {
            None => Ok(None),
            Some(Value::String(raw)) => Nonce::parse(raw).map(Some),
            Some(other) => Err(format!("nonce `{other}` 는 문자열이 아니에요")),
        }
    }
}

/// Where an answer came from: the pane that left the nonce it carries, a
/// model, or a person whose way in nothing proves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Pane,
    Terminal,
    Agent,
}

impl Via {
    pub fn id(self) -> &'static str {
        match self {
            Via::Pane => "pane",
            Via::Terminal => "terminal",
            Via::Agent => "agent",
        }
    }

    pub fn parse(raw: &str) -> Option<Via> {
        [Via::Pane, Via::Terminal, Via::Agent]
            .into_iter()
            .find(|via| via.id() == raw)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftNonce {
    pub kind: RequestKind,
    pub nonce: Nonce,
}

impl LeftNonce {
    pub fn to_json(&self) -> Value {
        json!({ "kind": self.kind.id(), "nonce": self.nonce.as_str() })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaneNonces {
    left: Vec<LeftNonce>,
    used: Vec<LeftNonce>,
}

impl PaneNonces {
    pub fn new(left: Vec<LeftNonce>) -> PaneNonces {
        PaneNonces {
            left,
            used: Vec::new(),
        }
    }

    fn proving(&self, kind: RequestKind, from: Sender, nonce: Option<&Nonce>) -> Option<usize> {
        let nonce = nonce.filter(|_| from == Sender::Pane)?;
        self.left
            .iter()
            .position(|left| left.kind == kind && left.nonce == *nonce)
    }

    pub fn via(&self, kind: RequestKind, from: Sender, nonce: Option<&Nonce>) -> Via {
        match (from, self.proving(kind, from, nonce)) {
            (Sender::Agent, _) => Via::Agent,
            (Sender::Pane, Some(_)) => Via::Pane,
            (Sender::Pane, None) => Via::Terminal,
        }
    }

    /// `via`, and a nonce that proved the pane is used up: a second answer
    /// carrying it comes through the terminal.
    pub fn spend(&mut self, kind: RequestKind, from: Sender, nonce: Option<&Nonce>) -> Via {
        let via = self.via(kind, from, nonce);
        if let Some(at) = self.proving(kind, from, nonce) {
            self.used.push(self.left.remove(at));
        }
        via
    }

    pub fn used(&self) -> &[LeftNonce] {
        &self.used
    }
}

fn with_nonce(mut value: Value, nonce: Option<&Nonce>) -> Value {
    if let (Some(nonce), Some(fields)) = (nonce, value.as_object_mut()) {
        fields.insert("nonce".to_string(), json!(nonce.as_str()));
    }
    value
}

fn with_auth(mut value: Value, auth: Option<&Auth>) -> Value {
    if let (Some(auth), Some(fields)) = (auth, value.as_object_mut()) {
        fields.insert("auth".to_string(), auth.to_json());
    }
    value
}

/// `Some(None)` without an `auth`; `None` for one that does not read.
fn read_auth(value: &Value) -> Option<Option<Auth>> {
    match value.get("auth") {
        None => Some(None),
        Some(auth) => Auth::from_json(auth).map(Some),
    }
}

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
        nonce: Option<Nonce>,
    },
    Stop {
        turn_id: String,
    },
    Permit {
        request_id: String,
        verdict: Verdict,
        from: Sender,
        nonce: Option<Nonce>,
        auth: Option<Auth>,
    },
    /// One answer per question of the open AskUserQuestion call it names.
    Answer {
        tool_use_id: String,
        answers: Vec<String>,
        from: Sender,
        nonce: Option<Nonce>,
        auth: Option<Auth>,
    },
    /// The reply to the decision a turn ended on, named by when it was left.
    Decide {
        decision_at: String,
        text: String,
        from: Sender,
        nonce: Option<Nonce>,
        auth: Option<Auth>,
    },
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

    pub fn answerer(&self) -> Option<(Sender, Option<&Nonce>)> {
        match self {
            Request::Permit { from, nonce, .. }
            | Request::Answer { from, nonce, .. }
            | Request::Decide { from, nonce, .. } => Some((*from, nonce.as_ref())),
            Request::Prompt { .. } | Request::Stop { .. } => None,
        }
    }

    pub fn permit(
        request_id: &str,
        verdict: &str,
        from: Sender,
        nonce: Option<Nonce>,
    ) -> Result<Request, String> {
        check_id("permission", request_id)?;
        let verdict = Verdict::parse(verdict)
            .ok_or_else(|| format!("--decision 은 allow 또는 deny 예요 (받은 값: {verdict})"))?;
        Ok(Request::Permit {
            request_id: request_id.to_string(),
            verdict,
            from,
            nonce,
            auth: None,
        })
    }

    pub fn answer(
        tool_use_id: &str,
        answers: &[String],
        from: Sender,
        nonce: Option<Nonce>,
    ) -> Result<Request, String> {
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
            nonce,
            auth: None,
        })
    }

    pub fn with_auth(mut self, given: Option<Auth>) -> Request {
        if let Request::Permit { auth, .. }
        | Request::Answer { auth, .. }
        | Request::Decide { auth, .. } = &mut self
        {
            *auth = given;
        }
        self
    }

    pub fn decide(
        decision_at: &str,
        text: &str,
        from: Sender,
        nonce: Option<Nonce>,
    ) -> Result<Request, String> {
        check_id("decision", decision_at)?;
        Ok(Request::Decide {
            decision_at: decision_at.to_string(),
            text: check_text("결정 답", text)?,
            from,
            nonce,
            auth: None,
        })
    }

    pub fn prompt(
        body: &str,
        from: Sender,
        from_label: Option<&str>,
        nonce: Option<Nonce>,
    ) -> Result<Request, String> {
        Ok(Request::Prompt {
            body: check_text("보낼 글", body)?,
            from,
            from_label: from_label
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string),
            nonce,
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
                nonce,
            } => with_nonce(
                json!({
                    "v": VERSION,
                    "kind": "prompt",
                    "text": body,
                    "from": from.id(),
                    "from_label": from_label,
                }),
                nonce.as_ref(),
            ),
            Request::Stop { turn_id } => {
                json!({ "v": VERSION, "kind": "stop", "turn_id": turn_id })
            }
            Request::Permit {
                request_id,
                verdict,
                from,
                nonce,
                auth,
            } => with_auth(
                with_nonce(
                    json!({
                        "v": VERSION,
                        "kind": "permit",
                        "request_id": request_id,
                        "decision": verdict.id(),
                        "from": from.id(),
                    }),
                    nonce.as_ref(),
                ),
                auth.as_ref(),
            ),
            Request::Answer {
                tool_use_id,
                answers,
                from,
                nonce,
                auth,
            } => with_auth(
                with_nonce(
                    json!({
                        "v": VERSION,
                        "kind": "answer",
                        "tool_use_id": tool_use_id,
                        "answers": answers,
                        "from": from.id(),
                    }),
                    nonce.as_ref(),
                ),
                auth.as_ref(),
            ),
            Request::Decide {
                decision_at,
                text,
                from,
                nonce,
                auth,
            } => with_auth(
                with_nonce(
                    json!({
                        "v": VERSION,
                        "kind": "decide",
                        "decision_at": decision_at,
                        "text": text,
                        "from": from.id(),
                    }),
                    nonce.as_ref(),
                ),
                auth.as_ref(),
            ),
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
                Nonce::read(&value).ok()?,
            )
            .ok(),
            RequestKind::Stop => Request::stop(value.get("turn_id")?.as_str()?).ok(),
            RequestKind::Permit => Request::permit(
                value.get("request_id")?.as_str()?,
                value.get("decision")?.as_str()?,
                // A permit with no `from` reads as the pane.
                match value.get("from") {
                    None => Sender::Pane,
                    Some(_) => from()?,
                },
                Nonce::read(&value).ok()?,
            )
            .ok()?
            .with_auth(read_auth(&value)?)
            .into(),
            RequestKind::Answer => Request::answer(
                value.get("tool_use_id")?.as_str()?,
                &strings(value.get("answers")?)?,
                from()?,
                Nonce::read(&value).ok()?,
            )
            .ok()?
            .with_auth(read_auth(&value)?)
            .into(),
            RequestKind::Decide => Request::decide(
                value.get("decision_at")?.as_str()?,
                value.get("text")?.as_str()?,
                from()?,
                Nonce::read(&value).ok()?,
            )
            .ok()?
            .with_auth(read_auth(&value)?)
            .into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAsk {
    pub id: String,
    pub questions: usize,
    pub whole: Option<Vec<Question>>,
}

/// What the taking session waits on now: an answer or a decide that names
/// anything else is stale.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Waits {
    pub asks: Vec<OpenAsk>,
    /// The id of the decision its last turn ended on.
    pub decision_at: Option<String>,
    pub place: Option<SessionPlace>,
}

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
        via: Via,
    },
    Stop {
        turn_id: String,
    },
    Answer {
        tool_use_id: String,
        answers: Vec<String>,
        from: Sender,
        via: Via,
        auth: Option<Auth>,
    },
    /// Runs as a prompt, the text as typed.
    Decide {
        decision_at: String,
        text: String,
        from: Sender,
        via: Via,
        auth: Option<Auth>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Delivered,
    Busy,
    Expired,
    Stale,
    Invalid,
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
    /// `None` for a request that answers no wait.
    pub via: Option<Via>,
    pub reason: Option<String>,
}

pub struct Took {
    pub taken: Vec<Taken>,
    pub outcomes: Vec<Outcome>,
    pub used: Vec<LeftNonce>,
}

pub const UNVOUCHED: &str = "메인 에이전트의 답에 선례도 물어 받은 답도 없어요";

#[derive(Debug, Clone, Copy)]
pub enum Answering<'a> {
    Ask(&'a OpenAsk),
    Decision(&'a str),
}

pub type Vet<'a> = &'a dyn Fn(Answering, &[String], &Auth) -> Result<(), String>;

struct Taker<'a> {
    running_turn: Option<&'a str>,
    waits: &'a Waits,
    nonces: PaneNonces,
    vet: Vet<'a>,
    prompted: bool,
    answered: Vec<String>,
    taken: Vec<Taken>,
    refusal: Option<String>,
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
            Request::Decide {
                from: Sender::Agent,
                auth,
                ..
            } if auth.as_ref().and_then(Auth::relay).is_none() => {
                self.refuse("결정 요청은 대신 답하지 않아요".to_string())
            }
            Request::Answer {
                tool_use_id,
                answers,
                from,
                nonce,
                auth,
            } => self.answer(tool_use_id, answers, from, nonce, auth),
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
                nonce,
                auth,
            } => {
                let on = Answering::Decision(&decision_at);
                let vouched = self.vouched(on, std::slice::from_ref(&text), from, auth.as_ref());
                if let Err(reason) = vouched {
                    return self.refuse(reason);
                }
                self.prompted = true;
                let via = self.nonces.spend(RequestKind::Decide, from, nonce.as_ref());
                self.deliver(Taken::Decide {
                    decision_at,
                    text,
                    from,
                    via,
                    auth: auth.filter(|_| from == Sender::Agent),
                })
            }
            Request::Prompt {
                body,
                from,
                from_label,
                nonce,
            } => {
                self.prompted = true;
                let via = self.nonces.spend(RequestKind::Prompt, from, nonce.as_ref());
                self.deliver(Taken::Prompt {
                    text: prompt_text(&body, from, from_label.as_deref()),
                    body,
                    from,
                    via,
                })
            }
        }
    }

    fn refuse(&mut self, reason: String) -> Status {
        self.refusal = Some(reason);
        Status::Refused
    }

    fn vouched(
        &self,
        on: Answering,
        answers: &[String],
        from: Sender,
        auth: Option<&Auth>,
    ) -> Result<(), String> {
        match (from, auth) {
            (Sender::Pane, _) => Ok(()),
            (Sender::Agent, None) => Err(UNVOUCHED.to_string()),
            (Sender::Agent, Some(auth)) => (self.vet)(on, answers, auth),
        }
    }

    fn answer(
        &mut self,
        tool_use_id: String,
        answers: Vec<String>,
        from: Sender,
        nonce: Option<Nonce>,
        auth: Option<Auth>,
    ) -> Status {
        let open = self
            .waits
            .asks
            .iter()
            .find(|ask| ask.id == tool_use_id && !self.answered.contains(&ask.id));
        let ask = match open {
            None => return Status::Stale,
            Some(ask) if ask.questions != answers.len() => return Status::Invalid,
            Some(ask) => ask,
        };
        if let Err(reason) = self.vouched(Answering::Ask(ask), &answers, from, auth.as_ref()) {
            return self.refuse(reason);
        }
        self.answered.push(tool_use_id.clone());
        let via = self.nonces.spend(RequestKind::Answer, from, nonce.as_ref());
        self.deliver(Taken::Answer {
            tool_use_id,
            answers,
            from,
            via,
            auth: auth.filter(|_| from == Sender::Agent),
        })
    }
}

pub struct Taking<'a> {
    pub running_turn: Option<&'a str>,
    pub waits: &'a Waits,
    pub nonces: PaneNonces,
    pub ttl_ms: u64,
    pub vet: Vet<'a>,
}

pub fn take(found: Vec<Found>, taking: Taking) -> Took {
    let ttl_ms = taking.ttl_ms;
    let mut taker = Taker {
        running_turn: taking.running_turn,
        waits: taking.waits,
        nonces: taking.nonces,
        vet: taking.vet,
        prompted: false,
        answered: Vec::new(),
        taken: Vec::new(),
        refusal: None,
    };
    let outcomes = found
        .into_iter()
        .map(|item| {
            let kind = item.request.as_ref().map(Request::kind);
            let via = item.request.as_ref().and_then(|request| {
                let (from, nonce) = request.answerer()?;
                Some(taker.nonces.via(request.kind(), from, nonce))
            });
            let status = match item.request {
                None => Status::Invalid,
                Some(_) if item.age_ms > ttl_ms => Status::Expired,
                Some(request) => taker.judge(request),
            };
            Outcome {
                id: item.id,
                kind,
                status,
                via,
                reason: taker.refusal.take(),
            }
        })
        .collect();
    Took {
        used: taker.nonces.used().to_vec(),
        taken: taker.taken,
        outcomes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_round_trips_through_its_json() {
        let prompt = Request::prompt(" hi ", Sender::Agent, Some("main"), None).expect("prompt");
        assert_eq!(Request::parse(&prompt.to_json().to_string()), Some(prompt));
        let nonce = Nonce::parse("n-1").ok();
        let prompt = Request::prompt("hi", Sender::Pane, None, nonce).expect("prompt");
        assert_eq!(Request::parse(&prompt.to_json().to_string()), Some(prompt));
        let stop = Request::stop("t-1").expect("stop");
        assert_eq!(Request::parse(&stop.to_json().to_string()), Some(stop));
        let answers = ["Red".to_string()];
        let nonce = Nonce::parse("n-1").ok();
        let answer = Request::answer("tu-1", &answers, Sender::Pane, nonce).expect("answer");
        assert_eq!(Request::parse(&answer.to_json().to_string()), Some(answer));
        let decide = Request::decide("1700", "네", Sender::Pane, None).expect("decide");
        assert_eq!(Request::parse(&decide.to_json().to_string()), Some(decide));
    }

    #[test]
    fn an_expired_stop_is_expired_not_stale() {
        let found = vec![Found {
            id: "1".to_string(),
            age_ms: 10,
            request: Some(Request::stop("t").expect("stop")),
        }];
        let took = take(
            found,
            Taking {
                running_turn: Some("t"),
                waits: &Waits::default(),
                nonces: PaneNonces::default(),
                ttl_ms: 5,
                vet: &|_, _, _| Ok(()),
            },
        );
        assert!(took.taken.is_empty());
        assert_eq!(took.outcomes[0].status, Status::Expired);
    }
}
