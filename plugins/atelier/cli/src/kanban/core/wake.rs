//! When the delegate session's mod wakes the main agent and with what: the
//! other sessions' waits, the model answers they dropped and the daily
//! report, said as session and wait ids only. Something said goes as a
//! prompt of its own while the person leaves the session alone, else beside
//! the person's next prompt; it counts as said once the mod delivered it.

use super::board::{Board, Column, SessionFact, WaitOn, Waiting};
use super::delegate::Delegate;
use super::detail::{Delivery, DeliveryStatus};
use super::prompt::{decides_as_person, PERSON_ORIGINS};
use super::request::{check_id, RequestKind, Status, Via};

const GATHER_MS: u64 = 10 * 1_000;
const QUIET_MS: u64 = 30 * 1_000;
const HOLD_MS: u64 = 3 * 60 * 1_000;
const MAX_KEPT: usize = 200;
const HEADER: &str = "[칸반 위임]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub id: String,
    pub since: u64,
}

/// What the mod keeps between wakes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WakeState {
    pub pending: Vec<Pending>,
    /// What a prompt or a person's prompt already carried.
    pub delivered: Vec<String>,
}

/// The daily report due, and when the last one was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportDue {
    pub due_at: u64,
    pub last: Option<u64>,
}

pub struct WakeInput<'a> {
    pub me: &'a str,
    pub now: u64,
    /// The length of the draft in the delegate session's prompt box.
    pub draft_len: u64,
    pub state: WakeState,
    pub report: Option<ReportDue>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wake {
    pub delegate: bool,
    pub submit: Option<String>,
    pub context: Option<String>,
    pub deliver_ids: Vec<String>,
    pub state: WakeState,
    pub report_due: bool,
}

pub fn attach_to() -> &'static [&'static str] {
    &PERSON_ORIGINS
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Section {
    Failure,
    Wait,
    Report,
}

struct Item {
    id: String,
    section: Section,
    line: String,
}

fn wait_item(session: &str, waiting: &Waiting) -> Option<Item> {
    let (kind, id) = match &waiting.on {
        WaitOn::Question(ask) => ("question", Some(ask.tool_use_id.clone())),
        WaitOn::Permission(ask) => ("permission", ask.id.clone()),
        WaitOn::Decision(decision) => ("decision", Some(decision.at.to_string())),
        WaitOn::Input => ("input", None),
    };
    if id
        .as_deref()
        .is_some_and(|id| check_id("wait", id).is_err())
    {
        return None;
    }
    let key = id.clone().unwrap_or_else(|| format!("@{}", waiting.since));
    Some(Item {
        id: format!("wait:{session}:{kind}:{key}"),
        section: Section::Wait,
        line: format!("- {session} {kind} {}", id.as_deref().unwrap_or("-")),
    })
}

fn dropped(status: &DeliveryStatus) -> Option<Status> {
    match status {
        DeliveryStatus::Known(
            status @ (Status::Expired | Status::Stale | Status::Invalid | Status::Refused),
        ) => Some(*status),
        DeliveryStatus::Known(Status::Delivered | Status::Busy) | DeliveryStatus::Unknown(_) => {
            None
        }
    }
}

fn failure_item(session: &str, inbox: &Delivery, since: u64) -> Option<Item> {
    let answer = matches!(
        inbox.kind,
        Some(RequestKind::Answer | RequestKind::Decide | RequestKind::Permit)
    );
    let status = dropped(&inbox.status)?;
    if !(answer && inbox.via == Some(Via::Agent) && inbox.at >= since)
        || check_id("request", &inbox.id).is_err()
    {
        return None;
    }
    let kind = inbox.kind.map_or("", |k| k.id());
    Some(Item {
        id: format!("fail:{session}:{}", inbox.id),
        section: Section::Failure,
        line: format!("- {session} {kind} {} {}", inbox.id, status.id()),
    })
}

fn report_item(report: ReportDue) -> Item {
    Item {
        id: format!("report:{}", report.due_at),
        section: Section::Report,
        line: format!(
            "{HEADER} 일일 보고 — 직전 보고 이후(--since {})",
            report.last.unwrap_or(0)
        ),
    }
}

fn items(board: &Board, delegate: &Delegate, input: &WakeInput) -> Vec<Item> {
    let since = delegate.since.unwrap_or(0);
    let others = board
        .columns
        .iter()
        .flat_map(|(column, cards)| cards.iter().map(move |card| (*column, card)))
        .filter(|(_, card)| {
            card.session_id != input.me && check_id("session", &card.session_id).is_ok()
        });
    let mut found = Vec::new();
    for (column, card) in others {
        if let (Column::NeedsInput, Some(waiting)) = (column, &card.waiting) {
            found.extend(wait_item(&card.session_id, waiting));
        }
        if let Some(inbox) = &card.inbox {
            found.extend(failure_item(&card.session_id, inbox, since));
        }
    }
    found.extend(input.report.map(report_item));
    found
}

fn last_person_prompt(me: &SessionFact) -> Option<u64> {
    me.prompts
        .iter()
        .filter(|p| decides_as_person(&p.origin))
        .map(|p| p.at)
        .max()
}

fn say(offered: &[(&Item, u64)]) -> String {
    let mut sections: Vec<String> = Vec::new();
    for section in [Section::Failure, Section::Wait, Section::Report] {
        let lines: Vec<&str> = offered
            .iter()
            .filter(|(item, _)| item.section == section)
            .map(|(item, _)| item.line.as_str())
            .collect();
        if lines.is_empty() {
            continue;
        }
        sections.push(match section {
            Section::Failure => {
                format!("{HEADER} 전달 실패 {}건\n{}", lines.len(), lines.join("\n"))
            }
            Section::Wait => format!("{HEADER} 입력 대기 {}건\n{}", lines.len(), lines.join("\n")),
            Section::Report => lines.join("\n"),
        });
    }
    sections.join("\n\n")
}

fn newest(mut kept: Vec<Pending>) -> Vec<Pending> {
    kept.sort_by(|a, b| a.since.cmp(&b.since).then_with(|| a.id.cmp(&b.id)));
    let over = kept.len().saturating_sub(MAX_KEPT);
    kept.split_off(over)
}

pub fn wake(board: &Board, delegate: &Delegate, me: &SessionFact, input: WakeInput) -> Wake {
    if !delegate.is(input.me) {
        return Wake::default();
    }
    let now = input.now;
    let found = items(board, delegate, &input);
    let is_current = |id: &str| found.iter().any(|item| item.id == id);
    let mut delivered: Vec<String> = input
        .state
        .delivered
        .iter()
        .filter(|id| is_current(id))
        .cloned()
        .collect();
    delivered.dedup();
    let over = delivered.len().saturating_sub(MAX_KEPT);
    let delivered = delivered.split_off(over);
    let pending = newest(
        found
            .iter()
            .filter(|item| !delivered.contains(&item.id))
            .map(|item| Pending {
                id: item.id.clone(),
                since: input
                    .state
                    .pending
                    .iter()
                    .find(|p| p.id == item.id)
                    .map_or(now, |p| p.since.min(now)),
            })
            .collect(),
    );
    let since_of = |item: &Item| pending.iter().find(|p| p.id == item.id).map(|p| p.since);
    let mut waiting: Vec<(&Item, u64)> = found
        .iter()
        .filter_map(|item| since_of(item).map(|since| (item, since)))
        .collect();
    waiting.sort_by(|(a, a_since), (b, b_since)| {
        (a.section, a_since, &a.id).cmp(&(b.section, b_since, &b.id))
    });
    let gathered = waiting
        .iter()
        .filter(|(item, _)| item.section == Section::Wait)
        .map(|(_, since)| *since)
        .min()
        .is_some_and(|first| now - first >= GATHER_MS);
    let offered: Vec<(&Item, u64)> = waiting
        .into_iter()
        .filter(|(item, _)| item.section != Section::Wait || gathered)
        .collect();
    let mut wake = Wake {
        delegate: true,
        report_due: input.report.is_some(),
        state: WakeState { pending, delivered },
        ..Wake::default()
    };
    if offered.is_empty() {
        return wake;
    }
    let turn_running = me.turn_id.is_some();
    let quiet = input.draft_len == 0
        && last_person_prompt(me).is_none_or(|at| now.saturating_sub(at) >= QUIET_MS);
    let held_too_long = offered.iter().any(|(_, since)| now - since >= HOLD_MS);
    let text = say(&offered);
    wake.deliver_ids = offered.iter().map(|(item, _)| item.id.clone()).collect();
    if !turn_running && (quiet || held_too_long) {
        wake.submit = Some(text);
    } else {
        wake.context = Some(text);
    }
    wake
}
