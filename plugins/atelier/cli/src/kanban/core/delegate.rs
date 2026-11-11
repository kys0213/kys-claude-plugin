//! The delegate session and what a person's `/kanban delegate …` asks of it:
//! which live session is the delegate, what the command's words mean, and
//! when the daily report falls due.

use super::board::{Board, Card, Column};
use super::ledger::{EntryKind, DAY_MS};
use super::matchkey::digest;
use super::precedent::Proxied;
use serde::{Deserialize, Serialize};

const VERSION: u64 = 1;
const ID_BYTES: usize = 4;
const MINUTE_MS: u64 = 60 * 1_000;

/// The live session a person turned into the delegate last, and the other
/// live ones still turned on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Delegate {
    pub session_id: Option<String>,
    pub since: Option<u64>,
    pub conflicts: Vec<String>,
}

impl Delegate {
    pub fn is(&self, session: &str) -> bool {
        self.session_id.as_deref() == Some(session)
    }
}

/// The delegate of the sessions on `board`; a stale session is not live.
pub fn delegate_of(board: &Board) -> Delegate {
    let mut on: Vec<(&Card, u64)> = board
        .columns
        .iter()
        .filter(|(column, _)| *column != Column::Stale)
        .flat_map(|(_, cards)| cards)
        .filter_map(|card| card.delegate_since.map(|since| (card, since)))
        .collect();
    on.sort_by(|(a, a_since), (b, b_since)| {
        b_since
            .cmp(a_since)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
    let mut on = on.into_iter();
    let Some((first, since)) = on.next() else {
        return Delegate::default();
    };
    Delegate {
        session_id: Some(first.session_id.clone()),
        since: Some(since),
        conflicts: on.map(|(card, _)| card.session_id.clone()).collect(),
    }
}

/// What `/kanban <args>` asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegateCommand {
    On,
    Off,
    Act(Proxied),
    Shadow(EntryKind),
}

const USAGE: &str = "`/kanban delegate on|off`, `/kanban delegate act question|permission`, `/kanban delegate shadow question|permission|decision` 중 하나예요";

impl DelegateCommand {
    pub fn parse(args: &str) -> Result<DelegateCommand, String> {
        let words: Vec<&str> = args.split_whitespace().collect();
        let refuse = || format!("{USAGE} (받은 값: {})", args.trim());
        match words.as_slice() {
            ["delegate", "on"] => Ok(DelegateCommand::On),
            ["delegate", "off"] => Ok(DelegateCommand::Off),
            ["delegate", "act", "question"] => Ok(DelegateCommand::Act(Proxied::Question)),
            ["delegate", "act", "permission"] => Ok(DelegateCommand::Act(Proxied::Permission)),
            ["delegate", "act", "decision"] => {
                Err("결정 요청은 아직 대신 답하지 않아요".to_string())
            }
            ["delegate", "shadow", kind] => EntryKind::parse(kind)
                .map(DelegateCommand::Shadow)
                .ok_or_else(refuse),
            _ => Err(refuse()),
        }
    }
}

/// How a person names a kind of wait.
pub fn kind_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Question => "질문",
        EntryKind::Permission => "권한 요청",
        EntryKind::Decision => "결정 요청",
    }
}

/// The local time of day the daily report falls due, as `HH:MM`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportAt {
    minutes: u64,
}

impl ReportAt {
    pub fn parse(raw: &str) -> Result<ReportAt, String> {
        let refuse = || format!("--report-at 은 `HH:MM`(00:00~23:59) 이에요 (받은 값: {raw})");
        let (hours, minutes) = raw.split_once(':').ok_or_else(refuse)?;
        let two_digits = |part: &str| part.len() == 2 && part.chars().all(|c| c.is_ascii_digit());
        if !(two_digits(hours) && two_digits(minutes)) {
            return Err(refuse());
        }
        let hours: u64 = hours.parse().map_err(|_| refuse())?;
        let minutes: u64 = minutes.parse().map_err(|_| refuse())?;
        if hours > 23 || minutes > 59 {
            return Err(refuse());
        }
        Ok(ReportAt {
            minutes: hours * 60 + minutes,
        })
    }

    /// The latest time at or before `now` the clock read this time of day,
    /// `offset_minutes` east of UTC.
    pub fn last_due(self, now: u64, offset_minutes: i64) -> u64 {
        let offset = offset_minutes * MINUTE_MS as i64;
        let local = now as i64 + offset;
        let day = local.div_euclid(DAY_MS as i64) * DAY_MS as i64;
        let mut due = day + (self.minutes * MINUTE_MS) as i64;
        if due > local {
            due -= DAY_MS as i64;
        }
        (due - offset).max(0) as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReportTag {
    Report,
}

/// A daily report the delegate session gave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    v: u64,
    #[serde(rename = "type")]
    tag: ReportTag,
    pub id: String,
    pub at: u64,
}

impl Report {
    pub fn new(at: u64) -> Report {
        Report {
            v: VERSION,
            tag: ReportTag::Report,
            id: format!("P{at}-{}", digest(&["report", &at.to_string()], ID_BYTES)),
            at,
        }
    }

    pub fn parse(line: &str) -> Option<Report> {
        serde_json::from_str::<Report>(line)
            .ok()
            .filter(|r| r.v == VERSION)
    }

    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("a report serializes")
    }
}

/// The newest report in `lines` at or before `now`.
pub fn last_report<'a>(lines: impl IntoIterator<Item = &'a str>, now: u64) -> Option<u64> {
    lines
        .into_iter()
        .filter_map(Report::parse)
        .map(|r| r.at)
        .filter(|at| *at <= now)
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: u64 = 60 * MINUTE_MS;

    #[test]
    fn the_report_time_falls_on_the_local_day() {
        let at = ReportAt::parse("18:00").expect("valid");
        let midnight_utc = 1_799_971_200_000;

        assert_eq!(
            at.last_due(midnight_utc + 18 * HOUR, 0),
            midnight_utc + 18 * HOUR
        );
        assert_eq!(
            at.last_due(midnight_utc + 18 * HOUR - 1, 0),
            midnight_utc - 6 * HOUR
        );
        assert_eq!(
            at.last_due(midnight_utc + 9 * HOUR, 540),
            midnight_utc + 9 * HOUR
        );
        assert_eq!(
            at.last_due(midnight_utc + 23 * HOUR, -300),
            midnight_utc + 23 * HOUR
        );
    }

    #[test]
    fn only_hh_mm_is_a_report_time() {
        for raw in ["6pm", "18", "24:00", "18:60", "8:00", "18:00:00"] {
            assert!(ReportAt::parse(raw).is_err(), "{raw}");
        }
        assert!(ReportAt::parse("00:00").is_ok());
    }
}
