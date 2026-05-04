//! Event log query CLI (spec §6.4).
//!
//! Thin adapter over [`EventLog::list_events`]. The CLI does not synthesize
//! events — it only renders what the store recorded — so filters here mirror
//! [`EventFilter`] one-to-one.

use std::io::Write;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};
use serde::Serialize;

use crate::cmd::output::write_json;
use crate::domain::{Event, EventKind, TaskId};
use crate::ports::task_store::{EventFilter, EventLog};

const PAYLOAD_PREVIEW_LIMIT: usize = 40;

#[derive(Subcommand)]
pub enum EventsCommands {
    /// List events in chronological order (oldest first), with filters
    List(ListArgs),
}

#[derive(Args)]
pub struct ListArgs {
    /// Filter by epic name
    #[arg(long)]
    pub epic: Option<String>,
    /// Filter by task id
    #[arg(long)]
    pub task: Option<String>,
    /// Filter by event kind. Repeatable; unknown kinds cause exit 1.
    #[arg(long = "kind")]
    pub kinds: Vec<String>,
    /// Earliest timestamp (RFC3339); inclusive
    #[arg(long)]
    pub since: Option<String>,
    /// Maximum number of events to return
    #[arg(long)]
    pub limit: Option<u32>,
    /// Output JSON instead of a human-readable table
    #[arg(long)]
    pub json: bool,
}

pub struct EventsService<'a> {
    store: &'a dyn EventLog,
}

impl<'a> EventsService<'a> {
    pub fn new(store: &'a dyn EventLog) -> Self {
        Self { store }
    }

    pub fn list(&self, args: &ListArgs, out: &mut dyn Write) -> Result<i32> {
        let kinds = match parse_kinds(&args.kinds) {
            Ok(k) => k,
            Err(unknown) => {
                writeln!(out, "unknown event kind '{unknown}'")?;
                return Ok(1);
            }
        };
        let since = match args.since.as_deref().map(parse_since).transpose() {
            Ok(s) => s,
            Err(e) => {
                writeln!(out, "{e}")?;
                return Ok(1);
            }
        };

        let filter = EventFilter {
            epic: args.epic.clone(),
            task: args.task.as_deref().map(TaskId::from_raw),
            kinds,
            since,
            limit: args.limit,
        };
        let events = self.store.list_events(filter).context("listing events")?;

        if args.json {
            return write_json(
                out,
                &events.iter().map(EventRecord::from).collect::<Vec<_>>(),
            )
            .map(|()| 0);
        }
        render_table(&events, out)?;
        Ok(0)
    }
}

fn parse_kinds(raw: &[String]) -> Result<Vec<EventKind>, String> {
    raw.iter()
        .map(|s| EventKind::parse(s).ok_or_else(|| s.clone()))
        .collect()
}

fn parse_since(raw: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .with_context(|| format!("parsing --since '{raw}' as RFC3339"))
}

fn render_table(events: &[Event], out: &mut dyn Write) -> Result<()> {
    if events.is_empty() {
        writeln!(out, "(no events)")?;
        return Ok(());
    }
    writeln!(
        out,
        "AT                            KIND                  EPIC                TASK          PAYLOAD-SUMMARY"
    )?;
    for e in events {
        writeln!(
            out,
            "{:<28}  {:<20}  {:<18}  {:<12}  {}",
            e.at.to_rfc3339(),
            e.kind.as_str(),
            e.epic_name.as_deref().unwrap_or("-"),
            e.task_id.as_ref().map(TaskId::as_str).unwrap_or("-"),
            payload_preview(&e.payload),
        )?;
    }
    Ok(())
}

fn payload_preview(value: &serde_json::Value) -> String {
    let s = if value.is_null() {
        "-".to_string()
    } else {
        value.to_string()
    };
    if s.chars().count() <= PAYLOAD_PREVIEW_LIMIT {
        return s;
    }
    let truncated: String = s.chars().take(PAYLOAD_PREVIEW_LIMIT).collect();
    format!("{truncated}…")
}

#[derive(Debug, Serialize)]
struct EventRecord<'a> {
    at: String,
    kind: &'static str,
    epic: Option<&'a str>,
    task: Option<&'a str>,
    payload: &'a serde_json::Value,
}

impl<'a> From<&'a Event> for EventRecord<'a> {
    fn from(e: &'a Event) -> Self {
        Self {
            at: e.at.to_rfc3339(),
            kind: e.kind.as_str(),
            epic: e.epic_name.as_deref(),
            task: e.task_id.as_ref().map(TaskId::as_str),
            payload: &e.payload,
        }
    }
}

pub fn events_service(store: &dyn EventLog) -> EventsService<'_> {
    EventsService::new(store)
}
