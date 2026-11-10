//! What a card shows when it is opened: titled sections of lines, the
//! decision a turn ended on, and how the last kanban request went.

use super::duration::age_label;
use super::progress::{tool_line, Activity, ItemStatus, WorkItem};
use super::text::{cut, fold, strip_controls, strip_reminders};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub ask: String,
    pub recap: Option<String>,
    pub options: Vec<String>,
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    pub id: String,
    pub kind: String,
    pub status: String,
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

/// The facts a detail is drawn from.
pub struct DetailFacts<'a> {
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

/// Tried in order for what a permission request is about; paths stay whole.
const WAIT_FIELDS: [&str; 8] = [
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
    strip_controls(&strip_reminders(raw))
}

fn prompt_lines(raw: Option<&str>) -> Vec<String> {
    raw.map(|p| fold(&clean(p)))
        .filter(|p| !p.is_empty())
        .map(|p| vec![cut(&p, PROMPT_CHARS)])
        .unwrap_or_default()
}

fn reply_lines(raw: Option<&str>) -> Vec<String> {
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

fn wait_lines(permission: Option<&Activity>) -> Vec<String> {
    let Some(permission) = permission else {
        return Vec::new();
    };
    let target = WAIT_FIELDS.iter().find_map(|field| {
        permission
            .input
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, value)| fold(value))
            .filter(|v| !v.is_empty())
    });
    vec![match target {
        Some(target) => format!("{}: {}", permission.tool, cut(&target, WAIT_CHARS)),
        None => permission.tool.clone(),
    }]
}

fn item_text(item: &WorkItem) -> Option<String> {
    let text = match item.status {
        ItemStatus::InProgress => item.active.as_deref().or(item.subject.as_deref()),
        _ => item.subject.as_deref().or(item.active.as_deref()),
    };
    text.map(fold).filter(|t| !t.is_empty())
}

fn mark(status: ItemStatus) -> &'static str {
    match status {
        ItemStatus::Completed => "✓",
        ItemStatus::InProgress => "▸",
        _ => "·",
    }
}

fn todo_lines(tasks: &[WorkItem], todos: &[WorkItem]) -> Vec<String> {
    let items = if tasks.is_empty() { todos } else { tasks };
    let live: Vec<&WorkItem> = items
        .iter()
        .filter(|i| i.status != ItemStatus::Deleted)
        .collect();
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
    let kind = match inbox.kind.as_str() {
        "stop" => "중지 요청",
        _ => "보낸 요청",
    };
    let status = match inbox.status.as_str() {
        "delivered" => "전달됐어요",
        "busy" => "작업 중이라 받지 않았어요",
        "expired" => "시간이 지나 버렸어요",
        "stale" => "끝난 턴이라 버렸어요",
        other => other,
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
    let prompt = prompt_lines(facts.last_prompt);
    let pending = reply_lines(facts.pending_context);
    let sections: Vec<(&'static str, Vec<String>)> = match phase {
        Phase::Permission => vec![
            ("기다리는 것", wait_lines(facts.permission)),
            ("요청", prompt),
            ("기다리기 직전 응답", pending),
        ],
        Phase::Question | Phase::Input => vec![("요청", prompt), ("기다리기 직전 응답", pending)],
        Phase::Decision => vec![("요청", prompt)],
        Phase::Working => vec![
            ("요청", prompt),
            ("할 일", todo_lines(facts.tasks, facts.todos)),
            ("최근 도구", recent_lines(facts.recent, now)),
        ],
        Phase::Idle => vec![
            ("직전 요청", prompt),
            ("직전 응답", reply_lines(facts.last_answer)),
        ],
        Phase::Stale => vec![
            ("마지막 요청", prompt),
            ("마지막 응답", reply_lines(facts.last_answer)),
        ],
    };
    Detail {
        sections: sections
            .into_iter()
            .filter(|(_, lines)| !lines.is_empty())
            .map(|(title, lines)| Section { title, lines })
            .collect(),
        decision: facts.decision.filter(|_| phase == Phase::Decision).cloned(),
        delivery: facts.inbox.map(|inbox| delivery_line(inbox, now)),
    }
}
