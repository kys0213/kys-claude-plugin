//! What a card shows when it is opened: titled sections of lines, the
//! decision a turn ended on, and how the last kanban request went.

use super::duration::age_label;
use super::progress::{
    first_field, item_text, live_items, tool_line, Activity, ItemStatus, WorkItem,
};
use super::prompt::PromptFact;
use super::request::{strip_agent_header, RequestKind, Status, Via};
use super::text::{cut, fold, strip_controls, strip_reminders};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub ask: String,
    pub recap: Option<String>,
    pub options: Vec<String>,
    pub at: u64,
    pub orig_len: Option<DecisionLengths>,
}

/// The lengths before the mod cut the texts, in UTF-16 code units; `options`
/// has one length for every option offered, kept or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionLengths {
    pub ask: u64,
    pub options: Vec<u64>,
}

/// A status a newer CLI wrote that this one does not know is shown as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryStatus {
    Known(Status),
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    pub id: String,
    pub kind: Option<RequestKind>,
    pub status: DeliveryStatus,
    pub via: Option<Via>,
    pub at: u64,
}

/// What the session waits for or does, as far as the detail differs by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Permission,
    Question,
    Decision,
    Input,
    Working,
    Idle,
    Stale,
}

pub struct DetailFacts<'a> {
    /// The session title, else its first prompt, as written.
    pub work: Option<&'a str>,
    pub cwd: &'a str,
    /// The person's prompts before `last_prompt`, oldest first.
    pub earlier_prompts: &'a [&'a PromptFact],
    pub last_prompt: Option<&'a str>,
    pub last_answer: Option<&'a str>,
    pub pending_context: Option<&'a str>,
    pub permission: Option<&'a Activity>,
    pub decision: Option<&'a Decision>,
    pub tasks: &'a [WorkItem],
    pub todos: &'a [WorkItem],
    pub recent: &'a [Activity],
    pub inbox: Option<&'a Delivery>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: &'static str,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detail {
    pub sections: Vec<Section>,
    pub decision: Option<Decision>,
    pub delivery: Option<String>,
}

const PROMPT_CHARS: usize = 300;
const REPLY_LINES: usize = 6;
const LINE_CHARS: usize = 200;
const WAIT_CHARS: usize = 120;
const LAST_LINE_CHARS: usize = 60;
const TODO_LINES: usize = 8;
const RECENT_LINES: usize = 5;
const EARLIER_CHARS: usize = 120;

/// Tried in order for what a permission request is about; paths stay whole.
pub const WAIT_FIELDS: [&str; 8] = [
    "command",
    "file_path",
    "notebook_path",
    "url",
    "pattern",
    "query",
    "prompt",
    "description",
];

pub fn clean(raw: &str) -> String {
    strip_controls(strip_agent_header(strip_reminders(raw).trim_start()))
}

pub fn prompt_lines(raw: Option<&str>) -> Vec<String> {
    raw.map(folded)
        .filter(|p| !p.is_empty())
        .map(|p| vec![cut(&p, PROMPT_CHARS)])
        .unwrap_or_default()
}

pub fn reply_lines(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    clean(raw)
        .lines()
        .map(fold)
        .filter(|l| !l.is_empty())
        .take(REPLY_LINES)
        .map(|l| cut(&l, LINE_CHARS))
        .collect()
}

pub fn folded(raw: &str) -> String {
    fold(&clean(raw))
}

fn work_lines(facts: &DetailFacts) -> Vec<String> {
    let shown: Vec<String> = facts
        .earlier_prompts
        .iter()
        .map(|p| p.text.as_str())
        .chain(facts.last_prompt)
        .map(folded)
        .collect();
    let work = facts
        .work
        .map(folded)
        .filter(|w| !w.is_empty() && !shown.contains(w))
        .map(|w| cut(&w, LINE_CHARS));
    let place = Some(facts.cwd)
        .filter(|cwd| !cwd.is_empty())
        .map(|cwd| format!("위치: {cwd}"));
    work.into_iter().chain(place).collect()
}

fn earlier_lines(earlier: &[&PromptFact], now: u64) -> Vec<String> {
    earlier
        .iter()
        .map(|p| (folded(&p.text), p.at))
        .filter(|(text, _)| !text.is_empty())
        .map(|(text, at)| {
            format!(
                "{} · {}",
                cut(&text, EARLIER_CHARS),
                age_label(now.saturating_sub(at))
            )
        })
        .collect()
}

fn wait_lines(permission: Option<&Activity>) -> Vec<String> {
    let Some(permission) = permission else {
        return Vec::new();
    };
    let target = first_field(&permission.input, &WAIT_FIELDS).map(|(_, value)| value);
    vec![match target {
        Some(target) => format!("{}: {}", permission.tool, cut(&target, WAIT_CHARS)),
        None => permission.tool.clone(),
    }]
}

fn mark(status: ItemStatus) -> &'static str {
    match status {
        ItemStatus::Completed => "✓",
        ItemStatus::InProgress => "▸",
        _ => "·",
    }
}

fn todo_lines(tasks: &[WorkItem], todos: &[WorkItem]) -> Vec<String> {
    let live = live_items(tasks, todos);
    let line = |item: &&WorkItem| {
        item_text(item).map(|t| format!("{} {}", mark(item.status), cut(&t, LINE_CHARS)))
    };
    if live.len() <= TODO_LINES {
        return live.iter().filter_map(line).collect();
    }
    let done = live
        .iter()
        .filter(|i| i.status == ItemStatus::Completed)
        .count();
    let open = live
        .iter()
        .filter(|i| i.status != ItemStatus::Completed)
        .take(TODO_LINES)
        .filter_map(line);
    std::iter::once(format!("✓ 완료 {done}개"))
        .chain(open)
        .collect()
}

fn recent_lines(recent: &[Activity], now: u64) -> Vec<String> {
    let mut newest_first: Vec<&Activity> = recent.iter().collect();
    newest_first.sort_by(|a, b| b.at.cmp(&a.at));
    newest_first
        .into_iter()
        .take(RECENT_LINES)
        .map(|a| format!("{} · {}", tool_line(a), age_label(now.saturating_sub(a.at))))
        .collect()
}

fn delivery_line(inbox: &Delivery, now: u64) -> String {
    let kind = match inbox.kind {
        Some(RequestKind::Stop) => "중지 요청",
        Some(RequestKind::Permit) => "권한 답",
        Some(RequestKind::Answer) => "질문 답",
        Some(RequestKind::Decide) => "결정 답",
        Some(RequestKind::Prompt) | None => "보낸 요청",
    };
    let status = match (&inbox.status, inbox.kind) {
        (DeliveryStatus::Known(Status::Delivered), _) => "전달됐어요",
        (DeliveryStatus::Known(Status::Busy), _) => "작업 중이라 받지 않았어요",
        (DeliveryStatus::Known(Status::Expired), _) => "시간이 지나 버렸어요",
        (DeliveryStatus::Known(Status::Stale), Some(RequestKind::Answer)) => {
            "끝난 질문이라 버렸어요"
        }
        (DeliveryStatus::Known(Status::Stale), Some(RequestKind::Decide)) => {
            "끝난 결정이라 버렸어요"
        }
        (DeliveryStatus::Known(Status::Stale), _) => "끝난 턴이라 버렸어요",
        (DeliveryStatus::Known(Status::Invalid), Some(RequestKind::Answer)) => {
            "답 수가 맞지 않아 버렸어요"
        }
        (DeliveryStatus::Known(Status::Invalid), _) => "invalid",
        (DeliveryStatus::Known(Status::Refused), _) => "대리 조건이 맞지 않아 버렸어요",
        (DeliveryStatus::Unknown(raw), _) => raw,
    };
    format!(
        "{kind}: {status} · {}",
        age_label(now.saturating_sub(inbox.at))
    )
}

pub fn last_line(last_answer: Option<&str>) -> Option<String> {
    let line = clean(last_answer?)
        .lines()
        .map(fold)
        .find(|l| !l.is_empty())?;
    Some(format!("직전: {}", cut(&line, LAST_LINE_CHARS)))
}

pub fn build(phase: Phase, facts: &DetailFacts, now: u64) -> Detail {
    let work = ("작업", work_lines(facts));
    let earlier = ("이전 요청", earlier_lines(facts.earlier_prompts, now));
    let prompt = prompt_lines(facts.last_prompt);
    let pending = reply_lines(facts.pending_context);
    let todos = ("할 일", todo_lines(facts.tasks, facts.todos));
    let sections: Vec<(&'static str, Vec<String>)> = match phase {
        Phase::Permission => vec![
            ("기다리는 것", wait_lines(facts.permission)),
            earlier,
            ("요청", prompt),
            ("기다리기 직전 응답", pending),
        ],
        Phase::Question | Phase::Input => {
            vec![earlier, ("요청", prompt), ("기다리기 직전 응답", pending)]
        }
        Phase::Decision => vec![earlier, ("요청", prompt)],
        Phase::Working => vec![
            earlier,
            ("요청", prompt),
            todos,
            ("최근 도구", recent_lines(facts.recent, now)),
        ],
        Phase::Idle => vec![
            earlier,
            ("직전 요청", prompt),
            ("직전 응답", reply_lines(facts.last_answer)),
            todos,
        ],
        Phase::Stale => vec![
            earlier,
            ("마지막 요청", prompt),
            ("마지막 응답", reply_lines(facts.last_answer)),
            todos,
        ],
    };
    Detail {
        sections: std::iter::once(work)
            .chain(sections)
            .filter(|(_, lines)| !lines.is_empty())
            .map(|(title, lines)| Section { title, lines })
            .collect(),
        decision: facts.decision.filter(|_| phase == Phase::Decision).cloned(),
        delivery: facts.inbox.map(|inbox| delivery_line(inbox, now)),
    }
}
