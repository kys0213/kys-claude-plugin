//! Relays: a person's answer to a question the main agent asked about another
//! session's wait.

use super::board::{PermissionAsk, WaitOn, Waiting};
use super::detail::Decision;
use super::ledger::{DecidedBy, Entry, EntryKind};
use super::matchkey::{answer_fp, digest};
use super::precedent::Book;
use super::question::{Ask, Question, QuestionOption};
use super::request::{check_id, Verdict};
use super::text::{defuse_marker, js_len, short_id, MARKER_WORD};
use super::wait::{permission_line, Subject, Wait};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const VERSION: u64 = 1;
pub const RELAY_TTL_MS: u64 = 30 * 60 * 1_000;
const ALLOW_LABEL: &str = "허용";
const DENY_LABEL: &str = "거부";
const ID_BYTES: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub session: String,
    pub wait: String,
    pub kind: EntryKind,
}

impl Target {
    pub fn to_json(&self) -> Value {
        json!({ "session": self.session, "wait": self.wait, "kind": self.kind.id() })
    }

    pub fn from_json(value: &Value) -> Result<Target, String> {
        let field = |name: &str| {
            value
                .get(name)
                .and_then(Value::as_str)
                .ok_or_else(|| format!("relay target has no `{name}`"))
        };
        let session = field("session")?;
        let wait = field("wait")?;
        check_id("session", session)?;
        check_id("wait", wait)?;
        let kind = EntryKind::parse(field("kind")?)
            .ok_or_else(|| "relay target `kind` is question, permission or decision".to_string())?;
        Ok(Target {
            session: session.to_string(),
            wait: wait.to_string(),
            kind,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub target: Target,
    pub recommended: Vec<Option<String>>,
}

impl Frame {
    pub fn to_json(&self) -> Value {
        let mut value = self.target.to_json();
        value["recommended"] = json!(self.recommended);
        value
    }

    pub fn from_json(value: &Value) -> Result<Frame, String> {
        let refuse = || "relay target `recommended` is a list of labels or nulls".to_string();
        let recommended = value
            .get("recommended")
            .and_then(Value::as_array)
            .ok_or_else(refuse)?
            .iter()
            .map(|r| match r {
                Value::Null => Ok(None),
                Value::String(label) => Ok(Some(label.clone())),
                _ => Err(refuse()),
            })
            .collect::<Result<_, _>>()?;
        Ok(Frame {
            target: Target::from_json(value)?,
            recommended,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relay {
    pub id: String,
}

impl Relay {
    pub fn new(id: &str) -> Result<Relay, String> {
        check_id("ledger entry", id)?;
        Ok(Relay { id: id.to_string() })
    }

    pub fn to_json(&self) -> Value {
        json!({ "relay": self.id })
    }

    pub fn from_json(value: &Value) -> Option<Relay> {
        Relay::new(value.get("relay")?.as_str()?).ok()
    }
}

pub fn relay_answer(kind: EntryKind, picked: &str) -> String {
    match (kind, picked.trim()) {
        (EntryKind::Permission, ALLOW_LABEL) => Verdict::Allow.id().to_string(),
        (EntryKind::Permission, DENY_LABEL) => Verdict::Deny.id().to_string(),
        _ => picked.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum UsedTag {
    RelayUsed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayUse {
    v: u64,
    #[serde(rename = "type")]
    tag: UsedTag,
    pub id: String,
    pub at: u64,
    pub relay: String,
    pub session: String,
    pub wait: String,
}

impl RelayUse {
    pub fn parse(line: &str) -> Option<RelayUse> {
        serde_json::from_str::<RelayUse>(line)
            .ok()
            .filter(|u| u.v == VERSION)
    }

    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("a relay use serializes")
    }
}

pub fn spend(relay: &Relay, target: &Target, now: u64) -> RelayUse {
    let suffix = digest(
        &[&relay.id, &target.session, &target.wait, &now.to_string()],
        ID_BYTES,
    );
    RelayUse {
        v: VERSION,
        tag: UsedTag::RelayUsed,
        id: format!("U{now}-{suffix}"),
        at: now,
        relay: relay.id.clone(),
        session: target.session.clone(),
        wait: target.wait.clone(),
    }
}

fn is_relay(entry: &Entry) -> bool {
    entry.target_session.is_some()
        && entry.target_wait.is_some()
        && entry.decided_by == Some(DecidedBy::Human)
}

fn group<'a>(book: &'a Book, first: &Entry) -> Vec<&'a Entry> {
    let mut group: Vec<&Entry> = book
        .entries()
        .iter()
        .filter(|e| {
            is_relay(e)
                && e.session == first.session
                && e.wait_id == first.wait_id
                && e.at == first.at
                && e.target_session == first.target_session
                && e.target_wait == first.target_wait
        })
        .collect();
    group.sort_by_key(|e| e.q_index);
    group
}

pub fn answered<'a>(book: &'a Book, relay: &Relay) -> Option<Vec<&'a Entry>> {
    let entry = book.entry(&relay.id).filter(|e| is_relay(e))?;
    Some(group(book, entry))
}

fn is_used(book: &Book, group: &[&Entry]) -> bool {
    book.relay_uses()
        .iter()
        .any(|u| group.iter().any(|e| e.id == u.relay))
}

fn is_fresh(at: u64, now: u64) -> bool {
    at <= now && now - at <= RELAY_TTL_MS
}

pub fn check(
    book: &Book,
    relay: &Relay,
    target: &Target,
    answers: &[String],
    now: u64,
) -> Result<(), String> {
    let id = &relay.id;
    let entry = book
        .entry(id)
        .ok_or_else(|| format!("장부에 물어 받은 답 `{id}` 가 없어요"))?;
    if !is_relay(entry) {
        return Err(format!("`{id}` 는 물어 받은 답이 아니에요"));
    }
    let group = group(book, entry);
    if is_used(book, &group) {
        return Err(format!("물어 받은 답 `{id}` 는 이미 전달했어요"));
    }
    if !is_fresh(entry.at, now) {
        return Err(format!("물어 받은 답 `{id}` 는 받은 지 30분이 지났어요"));
    }
    let bound = entry.target_session.as_deref() == Some(target.session.as_str())
        && entry.target_wait.as_deref() == Some(target.wait.as_str())
        && entry.kind == target.kind;
    if !bound {
        return Err(format!("물어 받은 답 `{id}` 는 다른 대기의 답이에요"));
    }
    if group.len() != answers.len() {
        return Err("답 수가 물어 받은 답과 달라요".to_string());
    }
    for (index, (kept, sent)) in group.iter().zip(answers).enumerate() {
        if answer_fp(&relay_answer(target.kind, sent)) != kept.answer_fp {
            return Err(format!("{}번 답이 물어 받은 답과 달라요", index + 1));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unused {
    pub id: String,
    pub answers: Vec<String>,
    pub at: u64,
}

pub fn unused(book: &Book, session: &str, wait: &str, now: u64) -> Vec<Unused> {
    let mut found: Vec<Unused> = book
        .entries()
        .iter()
        .filter(|e| {
            is_relay(e)
                && e.q_index.unwrap_or(0) == 0
                && e.target_session.as_deref() == Some(session)
                && e.target_wait.as_deref() == Some(wait)
                && is_fresh(e.at, now)
        })
        .filter_map(|first| {
            let group = group(book, first);
            (!is_used(book, &group)).then(|| Unused {
                id: first.id.clone(),
                answers: group.iter().map(|e| e.answer.clone()).collect(),
                at: first.at,
            })
        })
        .collect();
    found.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.id.cmp(&a.id)));
    found
}

pub struct Shown<'a> {
    pub label: &'a str,
    pub session_id: &'a str,
    pub waiting: &'a Waiting,
    pub wait: &'a Wait,
}

fn marker(text: &str) -> Option<(&str, &str)> {
    let line = text.lines().next()?.trim();
    let inner = line
        .strip_prefix('[')?
        .strip_prefix(MARKER_WORD)?
        .strip_prefix(' ')?
        .strip_suffix(']')?;
    let mut words = inner.split(' ');
    let (session, wait) = (words.next()?, words.next()?);
    let named = words.next().is_none()
        && check_id("session", session).is_ok()
        && check_id("wait", wait).is_ok();
    named.then_some((session, wait))
}

fn cut_note(stored: &str, orig: Option<u64>) -> Option<String> {
    let kept = js_len(stored);
    match orig {
        None => Some(" (잘림 — 원래 길이를 몰라요)".to_string()),
        Some(orig) if kept < orig => Some(format!(" (잘림 — 원래 {orig}자 중 앞 {kept}자)")),
        Some(_) => None,
    }
}

struct Asked {
    kind: EntryKind,
    blocks: Vec<String>,
    options: Vec<Offered>,
    multi_select: Vec<bool>,
    cut: Option<String>,
}

enum Offered {
    Any,
    /// The wait's own labels, each with what the main agent wrote of it.
    Labels(Vec<QuestionOption>),
    /// Labels whose descriptions say what the answer does, whatever the main
    /// agent wrote of them.
    Fixed(Vec<QuestionOption>),
}

fn options_line(labels: impl Iterator<Item = String>) -> String {
    format!("선택지: {}", labels.collect::<Vec<_>>().join(" / "))
}

const VERDICTS: [(&str, &str); 2] = [
    (ALLOW_LABEL, "위 머리말의 명령을 실행해요"),
    (DENY_LABEL, "실행하지 않아요"),
];

fn verdict_options() -> Vec<QuestionOption> {
    VERDICTS
        .map(|(label, description)| QuestionOption {
            label: label.to_string(),
            description: Some(description.to_string()),
        })
        .to_vec()
}

fn asked_question(ask: &Ask) -> Asked {
    let lengths = ask.orig_len.as_ref();
    let cut = ask
        .questions
        .iter()
        .enumerate()
        .flat_map(|(i, q)| {
            let question = (
                q.question.as_str(),
                lengths.and_then(|l| l.question.get(i).copied()),
            );
            let labels = q.options.iter().enumerate().map(move |(j, o)| {
                let orig = lengths.and_then(|l| l.labels.get(i)?.get(j).copied());
                (o.label.as_str(), orig)
            });
            std::iter::once(question).chain(labels)
        })
        .find_map(|(text, orig)| cut_note(text, orig));
    Asked {
        kind: EntryKind::Question,
        blocks: ask
            .questions
            .iter()
            .map(|q| {
                let labels = q.options.iter().map(|o| o.label.clone());
                format!("{}\n{}", q.question, options_line(labels))
            })
            .collect(),
        options: ask
            .questions
            .iter()
            .map(|q| Offered::Labels(q.options.clone()))
            .collect(),
        multi_select: ask.questions.iter().map(|q| q.multi_select).collect(),
        cut,
    }
}

fn asked_permission(ask: &PermissionAsk) -> Asked {
    let input = &ask.request.input;
    let (block, field) = permission_line(&ask.request.tool, input);
    let cut = field.and_then(|field| {
        let (_, value) = input.iter().find(|(name, _)| name == field)?;
        let orig = ask
            .input_len
            .as_ref()
            .and_then(|lengths| lengths.iter().find(|(name, _)| name == field))
            .map(|(_, len)| *len);
        cut_note(value, orig)
    });
    Asked {
        kind: EntryKind::Permission,
        blocks: vec![block],
        options: vec![Offered::Fixed(verdict_options())],
        multi_select: vec![false],
        cut,
    }
}

fn asked_decision(decision: &Decision) -> Asked {
    let lengths = decision.orig_len.as_ref();
    let options = decision.options.iter().enumerate().map(|(j, option)| {
        (
            option.as_str(),
            lengths.and_then(|l| l.options.get(j).copied()),
        )
    });
    let cut = std::iter::once((decision.ask.as_str(), lengths.map(|l| l.ask)))
        .chain(options)
        .find_map(|(text, orig)| cut_note(text, orig));
    let block = if decision.options.is_empty() {
        decision.ask.clone()
    } else {
        format!(
            "{}\n{}",
            decision.ask,
            options_line(decision.options.iter().cloned())
        )
    };
    let options = if (2..=4).contains(&decision.options.len()) {
        Offered::Labels(
            decision
                .options
                .iter()
                .map(|label| QuestionOption {
                    label: label.clone(),
                    description: None,
                })
                .collect(),
        )
    } else {
        Offered::Any
    };
    Asked {
        kind: EntryKind::Decision,
        blocks: vec![block],
        options: vec![options],
        multi_select: vec![false],
        cut,
    }
}

fn asked(on: &WaitOn) -> Option<Asked> {
    match on {
        WaitOn::Question(ask) => Some(asked_question(ask)),
        WaitOn::Permission(ask) => Some(asked_permission(ask)),
        WaitOn::Decision(decision) => Some(asked_decision(decision)),
        WaitOn::Input => None,
    }
}

fn kind_label(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Question => "질문",
        EntryKind::Permission => "권한 요청",
        EntryKind::Decision => "결정 요청",
    }
}

fn defused_option(option: &QuestionOption) -> QuestionOption {
    QuestionOption {
        label: option.label.clone(),
        description: option.description.as_deref().map(defuse_marker),
    }
}

fn sorted_labels(options: &[QuestionOption]) -> Vec<&str> {
    let mut labels: Vec<&str> = options.iter().map(|o| o.label.as_str()).collect();
    labels.sort_unstable();
    labels
}

fn options_for(given: &[QuestionOption], offered: &Offered) -> Vec<QuestionOption> {
    let (required, own_descriptions) = match offered {
        Offered::Any => return given.iter().map(defused_option).collect(),
        Offered::Labels(required) => (required, true),
        Offered::Fixed(required) => (required, false),
    };
    if sorted_labels(given) != sorted_labels(required) {
        return required.iter().map(defused_option).collect();
    }
    given
        .iter()
        .map(|option| {
            let source = if own_descriptions {
                option
            } else {
                required
                    .iter()
                    .find(|r| r.label == option.label)
                    .unwrap_or(option)
            };
            defused_option(source)
        })
        .collect()
}

fn joined(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|p| !p.trim().is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn frame(
    questions: &[Question],
    board: &[Shown],
) -> Result<Option<(Vec<Question>, Frame)>, String> {
    let Some((session, wait)) = questions.first().and_then(|q| marker(&q.question)) else {
        return Ok(None);
    };
    let shown = board
        .iter()
        .find(|s| s.session_id == session && s.wait.id.as_deref() == Some(wait))
        .ok_or_else(|| format!("표지가 가리키는 대기 `{session} {wait}` 를 찾지 못했어요"))?;
    let Some(asked) = asked(&shown.waiting.on) else {
        return Ok(None);
    };
    if matches!(shown.wait.subject, Subject::Input) || asked.blocks.len() != questions.len() {
        return Ok(None);
    }
    let short = short_id(shown.session_id);
    let header = format!(
        "칸반 대기 · {} · {short} · {}{}",
        defuse_marker(shown.label),
        kind_label(asked.kind),
        asked.cut.as_deref().unwrap_or_default()
    );
    let rewritten: Vec<Question> = questions
        .iter()
        .enumerate()
        .map(|(i, given)| {
            let block = defuse_marker(&asked.blocks[i]);
            let text = if i == 0 {
                let body = given.question.split_once('\n').map_or("", |(_, rest)| rest);
                let head = format!("{header}\n{block}");
                joined(&[&head, &defuse_marker(body)])
            } else {
                joined(&[&block, &defuse_marker(&given.question)])
            };
            Question {
                question: text,
                header: defuse_marker(&given.header),
                multi_select: asked.multi_select[i],
                options: options_for(&given.options, &asked.options[i]),
            }
        })
        .collect();
    let recommended = rewritten
        .iter()
        .map(|q| q.options.first().map(|o| o.label.clone()))
        .collect();
    let target = Target {
        session: session.to_string(),
        wait: wait.to_string(),
        kind: asked.kind,
    };
    Ok(Some((
        rewritten,
        Frame {
            target,
            recommended,
        },
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_is_the_first_line_alone_with_two_ids() {
        assert_eq!(marker("[kanban-wait s w]\nbody"), Some(("s", "w")));
        assert_eq!(marker("  [kanban-wait s w]  "), Some(("s", "w")));
        assert_eq!(marker("x [kanban-wait s w]"), None);
        assert_eq!(marker("[kanban-wait s w x]"), None);
        assert_eq!(marker("[kanban-wait s]"), None);
        assert_eq!(marker("[kanban-wait s w/x]"), None);
        assert_eq!(marker("body\n[kanban-wait s w]"), None);
    }

    #[test]
    fn a_permission_label_stands_for_its_verdict() {
        assert_eq!(relay_answer(EntryKind::Permission, "허용"), "allow");
        assert_eq!(relay_answer(EntryKind::Permission, "거부"), "deny");
        assert_eq!(relay_answer(EntryKind::Permission, "allow"), "allow");
        assert_eq!(relay_answer(EntryKind::Question, "허용"), "허용");
    }
}
