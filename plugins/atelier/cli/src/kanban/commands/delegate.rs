//! The delegate commands: a person's `/kanban delegate …` as the mod hands
//! it over with where it came from, the wake of the delegate session, the
//! daily report it gave, and the delegate as `status` shows it.

use crate::kanban::commands::ledger_store::{read_book, LedgerStore};
use crate::kanban::commands::payload::{parse_record_copy, parse_wake};
use crate::kanban::commands::records::{RecordStore, EXPIRE_AFTER_MS, STALE_AFTER_MS};
use crate::kanban::core::board::{build, Board, BoardParams, SessionFact};
use crate::kanban::core::delegate::{
    delegate_of, kind_name, last_report, Delegate, DelegateCommand, Report, ReportAt,
};
use crate::kanban::core::ledger::{month_of, EntryKind};
use crate::kanban::core::precedent::{mode, Book, Mode, Setting, SettingScope, SettingValue};
use crate::kanban::core::prompt::decides_as_person;
use crate::kanban::core::replay::{kind_stats, KindStats, KINDS};
use crate::kanban::core::request::check_id;
use crate::kanban::core::text::short_id;
use crate::kanban::core::wake::{attach_to, wake, ReportDue, Wake, WakeInput};
use serde_json::{json, Map, Value};

pub struct KanbanDelegate<'a> {
    pub records: &'a dyn RecordStore,
    pub ledger: &'a dyn LedgerStore,
}

/// A `/kanban <args>` run as the mod saw it.
pub struct Typed<'a> {
    pub session: &'a str,
    pub now: u64,
    /// The run's `origin.kind`.
    pub origin: &'a str,
    pub args: &'a str,
}

/// When `delegate wake` runs and the report time it keeps.
pub struct WakeClock<'a> {
    pub session: &'a str,
    pub now: u64,
    pub report_at: &'a str,
    pub utc_offset: i64,
}

fn answer(result: Result<Value, String>) -> Value {
    result.unwrap_or_else(|e| json!({ "error": e }))
}

fn board_params(now: u64) -> BoardParams {
    BoardParams {
        now,
        stale_after: STALE_AFTER_MS,
        expire_after: EXPIRE_AFTER_MS,
        self_id: None,
    }
}

fn comparison_line(stats: &KindStats) -> String {
    let advice = if stats.promotable() {
        "올려도 좋아요"
    } else {
        "아직 아니에요"
    };
    format!(
        "지금 비교 {} · 불일치 {} · 최근 5건 중 {}건 일치 · 권고: {advice}",
        stats.progress(),
        stats.disagreed(),
        stats.recent_agreed,
    )
}

fn modes_line(book: &Book, now: u64) -> String {
    KINDS
        .iter()
        .map(|kind| {
            let step = match mode(book, *kind, now) {
                Mode::Act => "대신 답하기",
                Mode::Shadow => "지켜보기",
            };
            format!("{} {step}", kind_name(*kind))
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

impl KanbanDelegate<'_> {
    fn board(&self, now: u64) -> Result<Board, String> {
        let sessions: Vec<SessionFact> = self
            .records
            .read_all()?
            .iter()
            .filter_map(|raw| parse_record_copy(raw))
            .collect();
        Ok(build(&sessions, &board_params(now)))
    }

    fn set(&self, setting: Setting) -> Result<(), String> {
        self.ledger
            .append(&month_of(setting.at), &[setting.to_line()])
    }

    /// A person's `/kanban <args>`: what to tell them, and for `on` and `off`
    /// the session's new `delegate_since`.
    pub fn command(&self, typed: &Typed) -> Value {
        answer((|| {
            check_id("session", typed.session)?;
            let command = DelegateCommand::parse(typed.args)?;
            if !decides_as_person(typed.origin) {
                return Err(format!(
                    "사람이 입력한 `/kanban delegate` 만 받아요 (출처: {})",
                    typed.origin
                ));
            }
            let now = typed.now;
            let setting = |scope: SettingScope, value: SettingValue, session: Option<&str>| {
                Setting::new(now, scope, value, typed.origin, session)
            };
            match command {
                DelegateCommand::On => {
                    let book = read_book(self.ledger)?;
                    let before = self
                        .board(now)
                        .map(|board| delegate_of(&board))
                        .unwrap_or_default();
                    let stopped: Vec<String> = before
                        .session_id
                        .into_iter()
                        .chain(before.conflicts)
                        .filter(|id| id != typed.session)
                        .map(|id| short_id(&id))
                        .collect();
                    self.set(setting(
                        SettingScope::Delegate,
                        SettingValue::On,
                        Some(typed.session),
                    ))?;
                    let mut text = format!(
                        "칸반 위임을 켰어요 — 이 세션이 다른 세션의 입력 대기를 받아요. 단계: {}.",
                        modes_line(&book, now)
                    );
                    if !stopped.is_empty() {
                        text.push_str(&format!(
                            " 먼저 켠 위임 세션({})은 이제 멈춰요.",
                            stopped.join(", ")
                        ));
                    }
                    Ok(json!({ "text": text, "delegate_since": now }))
                }
                DelegateCommand::Off => {
                    self.set(setting(
                        SettingScope::Delegate,
                        SettingValue::Off,
                        Some(typed.session),
                    ))?;
                    Ok(json!({ "text": "칸반 위임을 껐어요.", "delegate_since": null }))
                }
                DelegateCommand::Act(proxied) => {
                    let kind = proxied.kind();
                    let stats = kind_stats(&read_book(self.ledger)?, kind, now);
                    self.set(setting(kind.into(), Mode::Act.into(), None))?;
                    Ok(json!({
                        "text": format!(
                            "{}을 대신 답하기로 올렸어요. {}.",
                            kind_name(kind),
                            comparison_line(&stats)
                        )
                    }))
                }
                DelegateCommand::Shadow(kind) => {
                    self.set(setting(kind.into(), Mode::Shadow.into(), None))?;
                    Ok(json!({
                        "text": format!("{}을 지켜보기로 내렸어요.", kind_name(kind))
                    }))
                }
            }
        })())
    }

    fn report_due(&self, at: ReportAt, clock: &WakeClock) -> Result<Option<ReportDue>, String> {
        let due_at = at.last_due(clock.now, clock.utc_offset);
        let lines = |months: Vec<crate::kanban::core::ledger::MonthFile>| -> Vec<String> {
            months.into_iter().flat_map(|m| m.lines).collect()
        };
        let recent = lines(self.ledger.read_from(&month_of(due_at))?);
        let since_due = last_report(recent.iter().map(String::as_str), clock.now);
        if since_due.is_some_and(|last| last >= due_at) {
            return Ok(None);
        }
        let all = lines(self.ledger.read()?);
        Ok(Some(ReportDue {
            due_at,
            last: last_report(all.iter().map(String::as_str), clock.now),
        }))
    }

    pub fn wake(&self, raw: &str, clock: &WakeClock) -> Value {
        answer((|| {
            check_id("session", clock.session)?;
            let stdin = parse_wake(raw)?;
            let board = build(&stdin.sessions, &board_params(clock.now));
            let delegate = delegate_of(&board);
            let me = stdin
                .sessions
                .iter()
                .find(|s| s.session_id == clock.session);
            let report_at = ReportAt::parse(clock.report_at);
            let woke = match me {
                Some(me) if delegate.is(clock.session) => wake(
                    &board,
                    &delegate,
                    me,
                    WakeInput {
                        me: clock.session,
                        now: clock.now,
                        draft_len: stdin.draft_len,
                        state: stdin.state,
                        report: report_at
                            .as_ref()
                            .ok()
                            .and_then(|at| self.report_due(*at, clock).unwrap_or(None)),
                    },
                ),
                _ => Wake::default(),
            };
            let mut out = wake_json(&woke);
            out["report_at_error"] = json!(report_at.err());
            Ok(out)
        })())
    }

    pub fn report(&self, now: u64) -> Value {
        answer((|| {
            let report = Report::new(now);
            self.ledger.append(&month_of(now), &[report.to_line()])?;
            Ok(json!({ "reported": { "id": report.id, "at": report.at } }))
        })())
    }
}

fn wake_json(woke: &Wake) -> Value {
    json!({
        "delegate": woke.delegate,
        "submit": woke.submit,
        "context": woke.context,
        "deliver_ids": woke.deliver_ids,
        "attach_to": attach_to(),
        "report_due": woke.report_due,
        "state": {
            "pending": woke
                .state
                .pending
                .iter()
                .map(|p| json!({ "id": p.id, "since": p.since }))
                .collect::<Vec<_>>(),
            "delivered": woke.state.delivered,
        },
    })
}

/// The board's delegate: who it is, since when, and the others turned on.
pub fn delegate_json(delegate: &Delegate) -> Value {
    json!({
        "session_id": delegate.session_id,
        "since": delegate.since,
        "conflicts": delegate.conflicts,
    })
}

/// The delegate as `status` shows it: also each kind's mode and how far its
/// comparisons are; those are null when the ledger cannot be read.
pub fn status_delegate_json(delegate: &Delegate, book: Option<&Book>, now: u64) -> Value {
    let mut out = delegate_json(delegate);
    let per_kind = |value: &dyn Fn(&Book, EntryKind) -> Value| -> Value {
        match book {
            Some(book) => Value::Object(
                KINDS
                    .iter()
                    .map(|kind| (kind.id().to_string(), value(book, *kind)))
                    .collect::<Map<String, Value>>(),
            ),
            None => Value::Null,
        }
    };
    out["modes"] = per_kind(&|book, kind| json!(mode(book, kind, now).id()));
    out["progress"] = per_kind(&|book, kind| json!(kind_stats(book, kind, now).progress()));
    out
}
