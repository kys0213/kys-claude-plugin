//! The decision ledger: one entry per answer a session took for one of its
//! waits, with who decided it judged from the way the answer came in, what was
//! asked and answered masked, and the wait as it was then. Kept one month per
//! file of JSON lines.

use super::allowlist::RealPaths;
use super::board::{PermissionAsk, WaitOn, Waiting};
use super::detail::Decision;
use super::mask::mask;
use super::matchkey::{answer_fp, digest, sentence};
use super::prompt::decides_as_person;
use super::question::Ask;
use super::request::{Verdict, Via};
use super::text::cut;
use super::wait::{derive, Blocked, Place, Subject, Wait};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const VERSION: u64 = 1;
const MAX_TEXT_CHARS: usize = 500;
const ID_BYTES: usize = 4;
const DAY_MS: u64 = 24 * 60 * 60 * 1_000;
pub const MAX_AGE_MS: u64 = 180 * DAY_MS;
pub const MAX_BYTES: u64 = 50 * 1024 * 1024;
pub const DEFAULT_LIMIT: usize = 50;
pub const MAX_LIMIT: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Question,
    Permission,
    Decision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecidedBy {
    Human,
    Proxy,
}

/// The way an answer came in: the asking session's own screen or prompt, or
/// an inbox answer as `inbox take`/`wait-permit` judged it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryVia {
    Session,
    Pane,
    Terminal,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Used,
}

macro_rules! named {
    ($ty:ty, $($variant:ident = $id:literal),+ $(,)?) => {
        impl $ty {
            pub fn id(self) -> &'static str {
                match self { $(Self::$variant => $id),+ }
            }

            pub fn parse(raw: &str) -> Option<Self> {
                match raw { $($id => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}

named!(
    EntryKind,
    Question = "question",
    Permission = "permission",
    Decision = "decision"
);
named!(DecidedBy, Human = "human", Proxy = "proxy");
named!(
    EntryVia,
    Session = "session",
    Pane = "pane",
    Terminal = "terminal",
    Agent = "agent"
);

/// What was asked, masked and cut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Asked {
    Question {
        question: String,
        header: String,
        options: Vec<String>,
        multi_select: bool,
    },
    Permission {
        tool: String,
        target: String,
    },
    Decision {
        ask: String,
        options: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub v: u64,
    pub id: String,
    pub at: u64,
    pub kind: EntryKind,
    pub repo: String,
    pub session: String,
    pub wait_id: Option<String>,
    /// The question's place in its AskUserQuestion call; each question of a
    /// call is an entry of its own under the call's `wait_id`.
    pub q_index: Option<usize>,
    pub q_count: Option<usize>,
    pub key: Option<String>,
    pub key_label: Option<String>,
    pub asked: Asked,
    pub answer: String,
    pub answer_fp: String,
    pub answer_masked: bool,
    pub free_text: bool,
    pub decided_by: Option<DecidedBy>,
    pub via: EntryVia,
    pub inferred: bool,
    pub origin: Option<String>,
    /// Whether the wait was one a precedent may answer when it was answered,
    /// and why not.
    pub proxyable: bool,
    pub blocked: Vec<String>,
    pub permission_mode: Option<String>,
    pub outcome: Outcome,
    pub recommended: Option<String>,
    pub approved: bool,
    pub precedents: Vec<String>,
    pub reason: Option<String>,
    pub relay: Option<String>,
    pub target_session: Option<String>,
    pub target_wait: Option<String>,
}

impl Entry {
    /// `None` for a line that is not an entry this version reads.
    pub fn parse(line: &str) -> Option<Entry> {
        serde_json::from_str::<Entry>(line)
            .ok()
            .filter(|e| e.v == VERSION)
    }

    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("an entry serializes")
    }

    fn searchable(&self) -> String {
        let asked = serde_json::to_string(&self.asked).unwrap_or_default();
        [
            self.key_label.as_deref().unwrap_or(""),
            &asked,
            &self.answer,
        ]
        .join("\n")
        .to_lowercase()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionBy {
    /// The asking session's own dialog; `afk` when it resolved itself while
    /// nobody was at the keyboard.
    Dialog {
        afk: bool,
    },
    Inbox(Via),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermitBy {
    /// The engine's own dialog: whether the tool then ran (a PostToolUse or
    /// PostToolUseFailure of its call came), and whether the call ended in an
    /// error.
    Dialog {
        ran: bool,
        is_error: bool,
    },
    Inbox {
        via: Via,
        verdict: Verdict,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionBy {
    /// The next prompt while the decision was open, with its `origin.kind`.
    Prompt {
        origin: String,
    },
    Inbox(Via),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answered {
    /// One answer per question, `None` for a question left unanswered.
    Question {
        ask: Ask,
        by: QuestionBy,
        answers: Vec<Option<String>>,
    },
    Permission {
        ask: PermissionAsk,
        by: PermitBy,
    },
    Decision {
        decision: Decision,
        by: DecisionBy,
        text: String,
    },
}

/// What a mod knows of one answer its session took, for `ledger record`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub at: u64,
    pub session: String,
    pub repo_root: Option<String>,
    pub cwd: String,
    pub permission_mode: Option<String>,
    pub answered: Answered,
}

impl Facts {
    fn place(&self) -> Place<'_> {
        Place {
            repo_root: self.repo_root.as_deref(),
            cwd: &self.cwd,
            permission_mode: self.permission_mode.as_deref(),
        }
    }

    fn wait_on(&self) -> WaitOn {
        match &self.answered {
            Answered::Question { ask, .. } => WaitOn::Question(ask.clone()),
            Answered::Permission { ask, .. } => WaitOn::Permission(ask.clone()),
            Answered::Decision { decision, .. } => WaitOn::Decision(decision.clone()),
        }
    }
}

struct Source {
    decided_by: Option<DecidedBy>,
    via: EntryVia,
    inferred: bool,
}

impl Source {
    fn session(person: bool) -> Source {
        Source {
            decided_by: person.then_some(DecidedBy::Human),
            via: EntryVia::Session,
            inferred: false,
        }
    }

    fn inbox(via: Via) -> Source {
        let (decided_by, via) = match via {
            Via::Pane => (Some(DecidedBy::Human), EntryVia::Pane),
            Via::Terminal => (None, EntryVia::Terminal),
            Via::Agent => (Some(DecidedBy::Proxy), EntryVia::Agent),
        };
        Source {
            decided_by,
            via,
            inferred: false,
        }
    }

    fn inferred(mut self) -> Source {
        self.inferred = true;
        self
    }
}

/// One answer as it reads before masking, and what it answered.
struct Draft {
    kind: EntryKind,
    wait_id: Option<String>,
    question: Option<(usize, usize)>,
    key: Option<String>,
    key_label: Option<String>,
    asked: Asked,
    answer: String,
    /// The labels a picked answer would be one of; `None` where there are none.
    options: Option<Vec<String>>,
    source: Source,
    origin: Option<String>,
}

fn kept(text: &str) -> String {
    cut(&mask(text), MAX_TEXT_CHARS)
}

fn kept_asked(asked: Asked) -> Asked {
    let all = |texts: Vec<String>| texts.iter().map(|t| kept(t)).collect();
    match asked {
        Asked::Question {
            question,
            header,
            options,
            multi_select,
        } => Asked::Question {
            question: kept(&question),
            header: kept(&header),
            options: all(options),
            multi_select,
        },
        Asked::Permission { tool, target } => Asked::Permission {
            tool: kept(&tool),
            target: kept(&target),
        },
        Asked::Decision { ask, options } => Asked::Decision {
            ask: kept(&ask),
            options: all(options),
        },
    }
}

fn is_free_text(answer: &str, options: Option<&[String]>) -> bool {
    options.is_some_and(|options| {
        let said = sentence(answer);
        !options.iter().any(|o| sentence(o) == said)
    })
}

fn finish(draft: Draft, facts: &Facts, wait: &Wait) -> Entry {
    let masked = mask(&draft.answer);
    let q_index = draft.question.map(|(index, _)| index.to_string());
    let asked = serde_json::to_string(&draft.asked).expect("what was asked serializes");
    let suffix = digest(
        &[
            &facts.session,
            draft.wait_id.as_deref().unwrap_or(""),
            q_index.as_deref().unwrap_or(""),
            &facts.at.to_string(),
            draft.key.as_deref().unwrap_or(""),
            &asked,
            &draft.answer,
        ],
        ID_BYTES,
    );
    Entry {
        v: VERSION,
        id: format!("L{}-{suffix}", facts.at),
        at: facts.at,
        kind: draft.kind,
        repo: facts.place().repo().to_string(),
        session: facts.session.clone(),
        wait_id: draft.wait_id,
        q_index: draft.question.map(|(index, _)| index),
        q_count: draft.question.map(|(_, count)| count),
        key: draft.key,
        key_label: draft.key_label.as_deref().map(kept),
        asked: kept_asked(draft.asked),
        answer_fp: answer_fp(&draft.answer),
        answer_masked: masked != draft.answer,
        free_text: is_free_text(&draft.answer, draft.options.as_deref()),
        answer: cut(&masked, MAX_TEXT_CHARS),
        decided_by: draft.source.decided_by,
        via: draft.source.via,
        inferred: draft.source.inferred,
        origin: draft.origin,
        proxyable: wait.proxyable(),
        blocked: wait.blocked.iter().map(Blocked::id).collect(),
        permission_mode: facts.permission_mode.clone(),
        outcome: Outcome::Used,
        recommended: None,
        approved: false,
        precedents: Vec::new(),
        reason: None,
        relay: None,
        target_session: None,
        target_wait: None,
    }
}

fn question_drafts(
    ask: &Ask,
    by: QuestionBy,
    answers: &[Option<String>],
    wait: &Wait,
) -> Vec<Draft> {
    let Subject::Question(questions) = &wait.subject else {
        return Vec::new();
    };
    let count = questions.len();
    questions
        .iter()
        .enumerate()
        .filter_map(|(index, shown)| {
            let answer = answers.get(index)?.as_deref()?;
            let options: Vec<String> = shown
                .question
                .options
                .iter()
                .map(|o| o.label.clone())
                .collect();
            let source = match by {
                QuestionBy::Dialog { afk } => Source::session(!afk),
                QuestionBy::Inbox(via) => Source::inbox(via),
            };
            Some(Draft {
                kind: EntryKind::Question,
                wait_id: Some(ask.tool_use_id.clone()),
                question: Some((index, count)),
                key: shown.key.clone(),
                key_label: Some(shown.key_label.clone()),
                asked: Asked::Question {
                    question: shown.question.question.clone(),
                    header: shown.question.header.clone(),
                    options: options.clone(),
                    multi_select: shown.question.multi_select,
                },
                answer: answer.to_string(),
                options: Some(options),
                source,
                origin: None,
            })
        })
        .collect()
}

fn permission_draft(ask: &PermissionAsk, by: PermitBy, wait: &Wait) -> Option<Draft> {
    let Subject::Permission(shown) = &wait.subject else {
        return None;
    };
    let (verdict, source) = match by {
        PermitBy::Dialog { ran: true, .. } => (Verdict::Allow, Source::session(true)),
        PermitBy::Dialog {
            ran: false,
            is_error: true,
        } => (Verdict::Deny, Source::session(true).inferred()),
        PermitBy::Dialog {
            ran: false,
            is_error: false,
        } => return None,
        PermitBy::Inbox { via, verdict } => (verdict, Source::inbox(via)),
    };
    Some(Draft {
        kind: EntryKind::Permission,
        wait_id: ask.id.clone(),
        question: None,
        key: shown.key.clone(),
        key_label: shown.key_label.clone(),
        asked: Asked::Permission {
            tool: shown.tool.clone(),
            target: shown.target.clone(),
        },
        answer: verdict.id().to_string(),
        options: None,
        source,
        origin: None,
    })
}

fn decision_draft(decision: &Decision, by: &DecisionBy, text: &str, wait: &Wait) -> Option<Draft> {
    let Subject::Decision(shown) = &wait.subject else {
        return None;
    };
    let (source, origin) = match by {
        DecisionBy::Prompt { origin } => (
            Source::session(decides_as_person(origin)),
            Some(origin.clone()),
        ),
        DecisionBy::Inbox(via) => (Source::inbox(*via), None),
    };
    Some(Draft {
        kind: EntryKind::Decision,
        wait_id: Some(decision.at.to_string()),
        question: None,
        key: shown.key.clone(),
        key_label: Some(sentence(&shown.ask)),
        asked: Asked::Decision {
            ask: shown.ask.clone(),
            options: shown.options.clone(),
        },
        answer: text.to_string(),
        options: Some(shown.options.clone()),
        source,
        origin,
    })
}

/// What one answer leaves in the ledger.
pub fn entries(facts: &Facts, paths: &dyn RealPaths) -> Vec<Entry> {
    let waiting = Waiting {
        on: facts.wait_on(),
        since: facts.at,
        request: None,
        context: None,
    };
    let wait = derive(&waiting, &facts.place(), paths);
    let drafts = match &facts.answered {
        Answered::Question { ask, by, answers } => question_drafts(ask, *by, answers, &wait),
        Answered::Permission { ask, by } => permission_draft(ask, *by, &wait).into_iter().collect(),
        Answered::Decision { decision, by, text } => decision_draft(decision, by, text, &wait)
            .into_iter()
            .collect(),
    };
    drafts
        .into_iter()
        .map(|draft| finish(draft, facts, &wait))
        .collect()
}

/// A tool call in flight in a session, and whether a permission request is
/// already bound to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCall {
    pub tool_use_id: String,
    pub tool: String,
    pub has_permission: bool,
}

/// The call a permission request for `tool` belongs to: the one call of that
/// tool still without a request. With none, or more than one, it belongs to
/// no call, since the request names no call itself.
pub fn permission_call<'a>(tool: &str, calls: &'a [OpenCall]) -> Option<&'a str> {
    let mut open = calls
        .iter()
        .filter(|call| call.tool == tool && !call.has_permission);
    match (open.next(), open.next()) {
        (Some(call), None) => Some(&call.tool_use_id),
        _ => None,
    }
}

/// `YYYY-MM` of a unix-millisecond time, in UTC.
pub fn month_of(at: u64) -> String {
    let days = (at / DAY_MS) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}")
}

/// One month file's lines, in the order they were added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthFile {
    pub month: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pruned {
    pub entries: usize,
    pub files: usize,
}

fn line_at(line: &str) -> Option<u64> {
    serde_json::from_str::<Value>(line)
        .ok()?
        .get("at")?
        .as_u64()
}

/// The months kept within `max_age_ms` and `max_bytes`, and what was dropped.
pub fn prune(
    mut months: Vec<MonthFile>,
    now: u64,
    max_age_ms: u64,
    max_bytes: u64,
) -> (Vec<MonthFile>, Pruned) {
    months.sort_by(|a, b| a.month.cmp(&b.month));
    let had = months.len();
    let oldest = now.saturating_sub(max_age_ms);
    let mut dropped = 0;
    for month in &mut months {
        let before = month.lines.len();
        month
            .lines
            .retain(|line| line_at(line).is_some_and(|at| at >= oldest));
        dropped += before - month.lines.len();
    }
    let size = |line: &String| line.len() as u64 + 1;
    let mut total: u64 = months.iter().flat_map(|m| &m.lines).map(size).sum();
    for month in &mut months {
        while total > max_bytes && !month.lines.is_empty() {
            total -= size(&month.lines.remove(0));
            dropped += 1;
        }
    }
    months.retain(|m| !m.lines.is_empty());
    let pruned = Pruned {
        entries: dropped,
        files: had - months.len(),
    };
    (months, pruned)
}

/// What `ledger list` keeps; an absent field keeps every entry.
#[derive(Debug, Clone, Default)]
pub struct ListQuery {
    pub since: Option<u64>,
    pub until: Option<u64>,
    pub kinds: Vec<EntryKind>,
    pub repo: Option<String>,
    pub session: Option<String>,
    pub decided_by: Option<DecidedBy>,
    pub via: Option<EntryVia>,
    pub key: Option<String>,
    /// Matched case-insensitively against what was asked and answered.
    pub search: Option<String>,
    pub target_session: Option<String>,
    pub target_wait: Option<String>,
    pub limit: usize,
}

fn same(wanted: Option<&str>, value: Option<&str>) -> bool {
    wanted.is_none_or(|wanted| value == Some(wanted))
}

impl ListQuery {
    fn keeps(&self, entry: &Entry) -> bool {
        self.since.is_none_or(|since| entry.at >= since)
            && self.until.is_none_or(|until| entry.at <= until)
            && (self.kinds.is_empty() || self.kinds.contains(&entry.kind))
            && same(self.repo.as_deref(), Some(&entry.repo))
            && same(self.session.as_deref(), Some(&entry.session))
            && self
                .decided_by
                .is_none_or(|by| entry.decided_by == Some(by))
            && self.via.is_none_or(|via| entry.via == via)
            && same(self.key.as_deref(), entry.key.as_deref())
            && same(
                self.target_session.as_deref(),
                entry.target_session.as_deref(),
            )
            && same(self.target_wait.as_deref(), entry.target_wait.as_deref())
            && self
                .search
                .as_deref()
                .is_none_or(|search| entry.searchable().contains(&search.to_lowercase()))
    }
}

/// The entries the query keeps, newest first, at most `limit`.
pub fn select(entries: Vec<Entry>, query: &ListQuery) -> Vec<Entry> {
    let mut kept: Vec<Entry> = entries.into_iter().filter(|e| query.keeps(e)).collect();
    kept.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.id.cmp(&a.id)));
    kept.truncate(query.limit);
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_month_is_the_utc_calendar_month() {
        assert_eq!(month_of(0), "1970-01");
        assert_eq!(month_of(1_800_000_000_000), "2027-01");
        assert_eq!(month_of(951_782_400_000), "2000-02");
        assert_eq!(month_of(951_868_800_000), "2000-03");
    }
}
