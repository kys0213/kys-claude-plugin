//! Operator-facing diagnostics and lifecycle CLI for the task store.
//!
//! These commands read, mutate, or override the local SQLite cache without
//! touching git or GitHub — useful both for the implementer-loop integration
//! (`add`, `claim`, `complete`, `fail`, `release`, `escalate`) and for
//! debugging stuck epics (`force-status`).

use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use anyhow::{Context, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::cmd::output::write_json;
use crate::cmd::simhash;
use crate::domain::{DomainError, Task, TaskFailureOutcome, TaskId, TaskSource, TaskStatus};
use crate::ports::clock::{Clock, StdClock};
use crate::ports::task_store::{NewWatchTask, TaskStore, TaskStoreError, UpsertOutcome};

/// Default `max_attempts` used when no config / explicit override is present.
/// `main.rs` now resolves this from `autopilot.toml` (`epic.default_max_attempts`)
/// and threads it into [`TaskService::with_max_attempts`].
pub const DEFAULT_MAX_ATTEMPTS: u32 = 3;

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum TaskStatusArg {
    Pending,
    Ready,
    Wip,
    Blocked,
    Done,
    Escalated,
}

impl From<TaskStatusArg> for TaskStatus {
    fn from(s: TaskStatusArg) -> TaskStatus {
        match s {
            TaskStatusArg::Pending => TaskStatus::Pending,
            TaskStatusArg::Ready => TaskStatus::Ready,
            TaskStatusArg::Wip => TaskStatus::Wip,
            TaskStatusArg::Blocked => TaskStatus::Blocked,
            TaskStatusArg::Done => TaskStatus::Done,
            TaskStatusArg::Escalated => TaskStatus::Escalated,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum TaskSourceArg {
    Human,
    GapWatch,
    QaBoost,
    CiWatch,
}

impl From<TaskSourceArg> for TaskSource {
    fn from(s: TaskSourceArg) -> TaskSource {
        match s {
            TaskSourceArg::Human => TaskSource::Human,
            TaskSourceArg::GapWatch => TaskSource::GapWatch,
            TaskSourceArg::QaBoost => TaskSource::QaBoost,
            TaskSourceArg::CiWatch => TaskSource::CiWatch,
        }
    }
}

pub struct TaskService<'a> {
    store: &'a dyn TaskStore,
    clock: &'a dyn Clock,
    max_attempts: u32,
}

impl<'a> TaskService<'a> {
    pub fn new(store: &'a dyn TaskStore, clock: &'a dyn Clock) -> Self {
        Self::with_max_attempts(store, clock, DEFAULT_MAX_ATTEMPTS)
    }

    pub fn with_max_attempts(
        store: &'a dyn TaskStore,
        clock: &'a dyn Clock,
        max_attempts: u32,
    ) -> Self {
        Self {
            store,
            clock,
            max_attempts,
        }
    }

    pub fn list(
        &self,
        epic: &str,
        status: Option<TaskStatusArg>,
        json: bool,
        out: &mut dyn Write,
    ) -> Result<i32> {
        let tasks = self
            .store
            .list_tasks_by_epic(epic, status.map(TaskStatus::from))
            .with_context(|| format!("listing tasks for epic '{epic}'"))?;
        if json {
            return write_json(out, &tasks).map(|()| 0);
        }
        if tasks.is_empty() {
            writeln!(out, "(no tasks)")?;
        } else {
            writeln!(out, "ID            STATUS     ATTEMPTS  TITLE")?;
            for t in &tasks {
                writeln!(
                    out,
                    "{:<12}  {:<9}  {:>8}  {}",
                    t.id.as_str(),
                    t.status.as_str(),
                    t.attempts,
                    t.title
                )?;
            }
        }
        Ok(0)
    }

    pub fn show(&self, task_id: &str, json: bool, out: &mut dyn Write) -> Result<i32> {
        let id = TaskId::from_raw(task_id);
        let task = self
            .store
            .get_task(&id)
            .with_context(|| format!("fetching task '{task_id}'"))?;
        match task {
            Some(t) => {
                render_task(&t, json, out)?;
                Ok(0)
            }
            None => {
                writeln!(out, "task '{task_id}' not found")?;
                Ok(1)
            }
        }
    }

    pub fn force_status(
        &self,
        task_id: &str,
        to: TaskStatusArg,
        reason: Option<&str>,
        out: &mut dyn Write,
    ) -> Result<i32> {
        let id = TaskId::from_raw(task_id);
        let now = self.clock.now();
        let target = TaskStatus::from(to);
        self.store
            .force_status(&id, target, reason.unwrap_or(""), now)
            .with_context(|| format!("forcing status of task '{task_id}'"))?;
        match reason {
            Some(r) => writeln!(
                out,
                "task '{task_id}' status forced to {} ({r})",
                target.as_str()
            )?,
            None => writeln!(out, "task '{task_id}' status forced to {}", target.as_str())?,
        }
        Ok(0)
    }

    /// Inserts (or detects duplicate of) a watch-style task on `epic`.
    /// Auto-derives a fingerprint from `title + body` when `fingerprint` is
    /// `None`, so callers don't need to mirror the simhash recipe.
    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &self,
        epic: &str,
        task_id: &str,
        title: &str,
        body: Option<&str>,
        fingerprint: Option<&str>,
        source: TaskSourceArg,
        out: &mut dyn Write,
    ) -> Result<i32> {
        let now = self.clock.now();
        let fp = fingerprint
            .map(str::to_string)
            .unwrap_or_else(|| derive_fingerprint(title, body));
        let nt = NewWatchTask {
            id: TaskId::from_raw(task_id),
            epic_name: epic.to_string(),
            source: source.into(),
            fingerprint: fp,
            title: title.to_string(),
            body: body.map(str::to_string),
        };
        match self.store.upsert_watch_task(nt, now) {
            Ok(UpsertOutcome::Inserted(id)) => {
                writeln!(out, "inserted task {}", id.as_str())?;
                Ok(0)
            }
            Ok(UpsertOutcome::DuplicateFingerprint(id)) => {
                writeln!(out, "duplicate of task {}", id.as_str())?;
                Ok(0)
            }
            Err(TaskStoreError::Domain(DomainError::DuplicateTaskId(_))) => {
                writeln!(out, "task '{task_id}' already exists")?;
                Ok(1)
            }
            Err(e) => Err(e).with_context(|| format!("adding task '{task_id}' to epic '{epic}'")),
        }
    }

    /// Reads `path` as JSONL where each line describes a single watch task.
    /// Lines that fail to parse return Err (no partial commit guarantee — the
    /// underlying store still inserts each accepted line individually, which
    /// matches the spec for `add-batch` per §6.6).
    pub fn add_batch(&self, epic: &str, path: &Path, out: &mut dyn Write) -> Result<i32> {
        let file =
            std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
        let reader = BufReader::new(file);
        let now = self.clock.now();

        let mut inserted = 0u32;
        let mut duplicates = 0u32;
        for (lineno, line) in reader.lines().enumerate() {
            let line = line.with_context(|| format!("reading line {}", lineno + 1))?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parsed: BatchLine = serde_json::from_str(trimmed)
                .with_context(|| format!("parsing line {}: {trimmed}", lineno + 1))?;
            let source = match parsed.source.as_deref() {
                Some(s) => TaskSource::parse(s).ok_or_else(|| {
                    anyhow::anyhow!("unknown source '{s}' on line {}", lineno + 1)
                })?,
                None => TaskSource::Human,
            };
            let title = parsed.title;
            let body = parsed.body;
            let fp = parsed
                .fingerprint
                .unwrap_or_else(|| derive_fingerprint(&title, body.as_deref()));
            let nt = NewWatchTask {
                id: TaskId::from_raw(&parsed.id),
                epic_name: epic.to_string(),
                source,
                fingerprint: fp,
                title,
                body,
            };
            match self
                .store
                .upsert_watch_task(nt, now)
                .with_context(|| format!("inserting task on line {}", lineno + 1))?
            {
                UpsertOutcome::Inserted(_) => inserted += 1,
                UpsertOutcome::DuplicateFingerprint(_) => duplicates += 1,
            }
        }
        writeln!(out, "inserted: {inserted}, duplicates: {duplicates}")?;
        Ok(0)
    }

    pub fn find_by_pr(&self, pr_number: u64, json: bool, out: &mut dyn Write) -> Result<i32> {
        match self
            .store
            .find_task_by_pr(pr_number)
            .with_context(|| format!("finding task for PR #{pr_number}"))?
        {
            Some(t) => {
                render_task(&t, json, out)?;
                Ok(0)
            }
            None => {
                writeln!(out, "no task owns PR #{pr_number}")?;
                Ok(1)
            }
        }
    }

    pub fn claim(&self, epic: &str, json: bool, out: &mut dyn Write) -> Result<i32> {
        let now = self.clock.now();
        match self
            .store
            .claim_next_task(epic, now)
            .with_context(|| format!("claiming next task on epic '{epic}'"))?
        {
            Some(t) => {
                render_task(&t, json, out)?;
                Ok(0)
            }
            None => {
                writeln!(out, "(no ready tasks on epic '{epic}')")?;
                Ok(1)
            }
        }
    }

    /// Reaps Wip tasks whose claim went stale (worker crashed / ctrl-C /
    /// worktree destroyed) by reverting them to Ready. Same effect as
    /// per-task `release` but driven by a `before` cutoff. Emits a
    /// `TaskReleasedStale` event per recovered task. Always exits 0 on the
    /// happy path — empty recovery is normal.
    pub fn release_stale(
        &self,
        before_seconds: i64,
        json: bool,
        out: &mut dyn Write,
    ) -> Result<i32> {
        let now = self.clock.now();
        let cutoff = now - chrono::Duration::seconds(before_seconds);
        let recovered = self
            .store
            .release_stale(cutoff, now)
            .with_context(|| format!("releasing stale Wip tasks older than {before_seconds}s"))?;
        if json {
            let ids: Vec<&str> = recovered.iter().map(|i| i.as_str()).collect();
            write_json(out, &ids)?;
        } else if recovered.is_empty() {
            writeln!(out, "released 0 stale tasks")?;
        } else {
            writeln!(out, "released {} stale tasks:", recovered.len())?;
            for id in &recovered {
                writeln!(out, "  {}", id.as_str())?;
            }
        }
        Ok(0)
    }

    pub fn release(&self, task_id: &str, out: &mut dyn Write) -> Result<i32> {
        let id = TaskId::from_raw(task_id);
        let now = self.clock.now();
        match self.store.release_claim(&id, now) {
            Ok(()) => {
                writeln!(out, "released task {task_id}")?;
                Ok(0)
            }
            Err(TaskStoreError::NotFound(_)) => {
                writeln!(out, "task '{task_id}' not found")?;
                Ok(1)
            }
            Err(TaskStoreError::Domain(DomainError::RequiresStatus(_, _, actual))) => {
                writeln!(
                    out,
                    "task '{task_id}' cannot be released from {} (must be wip)",
                    actual.as_str()
                )?;
                Ok(1)
            }
            Err(e) => Err(e).with_context(|| format!("releasing task '{task_id}'")),
        }
    }

    pub fn complete(&self, task_id: &str, pr: u64, out: &mut dyn Write) -> Result<i32> {
        let id = TaskId::from_raw(task_id);
        let now = self.clock.now();
        match self.store.complete_task_and_unblock(&id, pr, now) {
            Ok(report) => {
                writeln!(
                    out,
                    "completed task {} (PR #{pr})",
                    report.completed.as_str()
                )?;
                if report.newly_ready.is_empty() {
                    writeln!(out, "newly ready: (none)")?;
                } else {
                    let ids: Vec<&str> = report.newly_ready.iter().map(|i| i.as_str()).collect();
                    writeln!(out, "newly ready: {}", ids.join(", "))?;
                }
                Ok(0)
            }
            Err(TaskStoreError::NotFound(_)) => {
                writeln!(out, "task '{task_id}' not found")?;
                Ok(1)
            }
            Err(e) => Err(e).with_context(|| format!("completing task '{task_id}'")),
        }
    }

    pub fn fail(&self, task_id: &str, out: &mut dyn Write) -> Result<i32> {
        let id = TaskId::from_raw(task_id);
        let now = self.clock.now();
        let outcome = self
            .store
            .mark_task_failed(&id, self.max_attempts, now)
            .with_context(|| format!("failing task '{task_id}'"))?;
        write_json(out, &FailReport::from(outcome))?;
        Ok(0)
    }

    pub fn escalate(&self, task_id: &str, issue: u64, out: &mut dyn Write) -> Result<i32> {
        let id = TaskId::from_raw(task_id);
        let now = self.clock.now();
        match self.store.escalate_task(&id, issue, now) {
            Ok(()) => {
                writeln!(out, "task '{task_id}' escalated to issue #{issue}")?;
                Ok(0)
            }
            Err(TaskStoreError::NotFound(_)) => {
                writeln!(out, "task '{task_id}' not found")?;
                Ok(1)
            }
            Err(e) => Err(e).with_context(|| format!("escalating task '{task_id}'")),
        }
    }
}

#[derive(Debug, Deserialize)]
struct BatchLine {
    id: String,
    title: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    fingerprint: Option<String>,
    #[serde(default)]
    source: Option<String>,
    // Unknown fields (e.g. section/requirement on watch payloads) are silently
    // accepted by serde, keeping the JSONL contract forward-compatible.
}

#[derive(Debug, Serialize)]
struct FailReport {
    outcome: &'static str,
    attempts: u32,
}

impl From<TaskFailureOutcome> for FailReport {
    fn from(o: TaskFailureOutcome) -> Self {
        match o {
            TaskFailureOutcome::Retried { attempts } => Self {
                outcome: "retried",
                attempts,
            },
            TaskFailureOutcome::Escalated { attempts } => Self {
                outcome: "escalated",
                attempts,
            },
        }
    }
}

pub fn task_service<'a>(store: &'a dyn TaskStore, clock: &'a dyn Clock) -> TaskService<'a> {
    TaskService::new(store, clock)
}

/// Parses a Go-style duration (`30s`, `5m`, `1h`, `1d`, `1w`, `2h30m`,
/// `2d12h`, `1w3d`, `2d12h30m`) to seconds.
///
/// Supported units (fixed length only — month/year are intentionally
/// omitted because they're variable-length and ambiguous):
/// - `s` = 1 second
/// - `m` = 60 seconds
/// - `h` = 3 600 seconds
/// - `d` = 86 400 seconds
/// - `w` = 604 800 seconds
///
/// Unit ordering is **not** enforced — the result is the sum of every
/// `<count><unit>` part regardless of order. Empty input, missing units
/// (`90`), unknown units (`1y`, `1mo`), zero/negative totals, and
/// integer overflow all return `Err` with a message naming the offending
/// fragment.
pub fn parse_duration_seconds(s: &str) -> std::result::Result<i64, String> {
    if s.is_empty() {
        return Err("empty duration".to_string());
    }
    let mut total: i64 = 0;
    let mut acc: i64 = 0;
    let mut saw_digit = false;
    for c in s.chars() {
        if let Some(d) = c.to_digit(10) {
            acc = acc
                .checked_mul(10)
                .and_then(|v| v.checked_add(d as i64))
                .ok_or_else(|| format!("duration overflow: {s}"))?;
            saw_digit = true;
            continue;
        }
        if !saw_digit {
            return Err(format!("invalid duration '{s}': unit '{c}' without count"));
        }
        let mult: i64 = match c {
            's' => 1,
            'm' => 60,
            'h' => 3_600,
            'd' => 86_400,
            'w' => 604_800,
            _ => return Err(format!("invalid duration '{s}': unknown unit '{c}'")),
        };
        let part = acc
            .checked_mul(mult)
            .ok_or_else(|| format!("duration overflow: {s}"))?;
        total = total
            .checked_add(part)
            .ok_or_else(|| format!("duration overflow: {s}"))?;
        acc = 0;
        saw_digit = false;
    }
    if saw_digit {
        return Err(format!(
            "invalid duration '{s}': trailing digits without unit"
        ));
    }
    if total <= 0 {
        return Err(format!("duration must be positive: {s}"));
    }
    Ok(total)
}

pub fn default_clock() -> StdClock {
    StdClock
}

fn render_task(t: &Task, json: bool, out: &mut dyn Write) -> Result<()> {
    if json {
        return write_json(out, t);
    }
    print_task_human(t, out)
}

fn derive_fingerprint(title: &str, body: Option<&str>) -> String {
    let composite = format!("{title}\n{}", body.unwrap_or(""));
    simhash::format_simhash(simhash::weighted_simhash(&simhash::tokenize_weighted(
        &composite,
    )))
}

fn print_task_human(t: &Task, out: &mut dyn Write) -> Result<()> {
    writeln!(out, "id:              {}", t.id.as_str())?;
    writeln!(out, "epic:            {}", t.epic_name)?;
    writeln!(out, "status:          {}", t.status.as_str())?;
    writeln!(out, "source:          {}", t.source.as_str())?;
    writeln!(out, "attempts:        {}", t.attempts)?;
    writeln!(out, "title:           {}", t.title)?;
    if let Some(b) = &t.branch {
        writeln!(out, "branch:          {b}")?;
    }
    if let Some(pr) = t.pr_number {
        writeln!(out, "pr_number:       {pr}")?;
    }
    if let Some(issue) = t.escalated_issue {
        writeln!(out, "escalated_issue: {issue}")?;
    }
    if let Some(fp) = &t.fingerprint {
        writeln!(out, "fingerprint:     {fp}")?;
    }
    if let Some(body) = &t.body {
        writeln!(out, "---")?;
        writeln!(out, "{body}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::clock::FixedClock;
    use crate::ports::task_store::{EpicPlan, NewTask};
    use crate::store::InMemoryTaskStore;
    use chrono::TimeZone;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn fixture() -> (Arc<dyn TaskStore>, FixedClock) {
        let store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
        let clock = FixedClock::new(chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
        let now = clock.now();
        store
            .insert_epic_with_tasks(
                EpicPlan {
                    epic: crate::domain::Epic {
                        name: "e".to_string(),
                        spec_path: PathBuf::from("spec/x.md"),
                        branch: "epic/e".to_string(),
                        status: crate::domain::EpicStatus::Active,
                        created_at: now,
                        completed_at: None,
                    },
                    tasks: vec![NewTask {
                        id: TaskId::from_raw("a1b2c3d4e5f6"),
                        source: crate::domain::TaskSource::Decompose,
                        fingerprint: None,
                        title: "first".to_string(),
                        body: None,
                    }],
                    deps: vec![],
                },
                now,
            )
            .unwrap();
        (store, clock)
    }

    #[test]
    fn list_renders_human_table() {
        let (store, clock) = fixture();
        let svc = TaskService::new(store.as_ref(), &clock);
        let mut buf: Vec<u8> = Vec::new();
        let code = svc.list("e", None, false, &mut buf).unwrap();
        assert_eq!(code, 0);
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("a1b2c3d4e5f6"));
        assert!(s.contains("ready"));
        assert!(s.contains("first"));
    }

    #[test]
    fn list_renders_json() {
        let (store, clock) = fixture();
        let svc = TaskService::new(store.as_ref(), &clock);
        let mut buf: Vec<u8> = Vec::new();
        svc.list("e", None, true, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert!(parsed.is_array());
    }

    #[test]
    fn show_returns_1_when_missing() {
        let (store, clock) = fixture();
        let svc = TaskService::new(store.as_ref(), &clock);
        let mut buf: Vec<u8> = Vec::new();
        let code = svc.show("does-not-exist", false, &mut buf).unwrap();
        assert_eq!(code, 1);
    }

    #[test]
    fn force_status_overrides_lifecycle() {
        let (store, clock) = fixture();
        let svc = TaskService::new(store.as_ref(), &clock);
        let mut buf: Vec<u8> = Vec::new();
        svc.force_status(
            "a1b2c3d4e5f6",
            TaskStatusArg::Done,
            Some("manual"),
            &mut buf,
        )
        .unwrap();
        let task = store
            .get_task(&TaskId::from_raw("a1b2c3d4e5f6"))
            .unwrap()
            .unwrap();
        assert_eq!(task.status, TaskStatus::Done);
        // Output uses canonical lowercase status (TaskStatus::as_str), matching
        // the rest of the CLI. Specifically NOT the {:?} debug form (`Done`).
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("forced to done"), "stdout: {s}");
        assert!(s.contains("(manual)"), "stdout: {s}");
    }

    #[test]
    fn force_status_without_reason_omits_parens() {
        let (store, clock) = fixture();
        let svc = TaskService::new(store.as_ref(), &clock);
        let mut buf: Vec<u8> = Vec::new();
        svc.force_status("a1b2c3d4e5f6", TaskStatusArg::Wip, None, &mut buf)
            .unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("forced to wip"), "stdout: {s}");
        assert!(
            !s.contains('('),
            "stdout should not contain '(' when reason is None: {s}"
        );
    }
}
