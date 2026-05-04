//! Epic ledger subcommands.
//!
//! Pure ledger surface: every operation is a thin adapter over `TaskStore`.
//! Agents own spec decomposition, git, and GitHub; this module only
//! persists state.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::cmd::output::write_json;
use crate::domain::{DomainError, Epic, EpicStatus, TaskId, TaskSource, TaskStatus};
use crate::ports::clock::Clock;
use crate::ports::task_store::{
    EpicPlan, NewTask, ReconciliationPlan, RemotePrState, RemoteTaskState, TaskStore,
    TaskStoreError,
};

#[derive(Subcommand)]
pub enum EpicCommands {
    /// Begin tracking a new epic
    Create(CreateArgs),
    /// List epics
    List(ListArgs),
    /// Show a single epic
    Get(GetArgs),
    /// Task status counts for an epic (or all active epics)
    Status(StatusArgs),
    /// Mark epic as completed
    Complete(NameArg),
    /// Mark epic as abandoned
    Abandon(NameArg),
    /// Apply a reconciliation plan (JSONL on disk) to an epic
    Reconcile(ReconcileArgs),
    /// Look up the active epic owning a spec path
    FindBySpecPath(FindBySpecPathArgs),
}

#[derive(Args)]
pub struct CreateArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub spec: PathBuf,
    /// Defaults to `epic/<NAME>` when omitted.
    #[arg(long)]
    pub branch: Option<String>,
}

#[derive(Args)]
pub struct ListArgs {
    #[arg(long)]
    pub status: Option<EpicStatusFilter>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct GetArgs {
    pub name: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct StatusArgs {
    pub name: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct NameArg {
    pub name: String,
}

#[derive(Args)]
pub struct ReconcileArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub plan: PathBuf,
}

#[derive(Args)]
pub struct FindBySpecPathArgs {
    pub spec: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum EpicStatusFilter {
    Active,
    Completed,
    Abandoned,
    All,
}

impl EpicStatusFilter {
    pub fn to_status(self) -> Option<EpicStatus> {
        match self {
            Self::Active => Some(EpicStatus::Active),
            Self::Completed => Some(EpicStatus::Completed),
            Self::Abandoned => Some(EpicStatus::Abandoned),
            Self::All => None,
        }
    }
}

pub struct EpicService<'a> {
    store: &'a dyn TaskStore,
    clock: &'a dyn Clock,
}

impl<'a> EpicService<'a> {
    pub fn new(store: &'a dyn TaskStore, clock: &'a dyn Clock) -> Self {
        Self { store, clock }
    }

    pub fn create(
        &self,
        name: &str,
        spec_path: &Path,
        branch: Option<&str>,
        out: &mut dyn Write,
    ) -> Result<i32> {
        let now = self.clock.now();
        let epic = Epic {
            name: name.to_string(),
            spec_path: spec_path.to_path_buf(),
            branch: branch
                .map(str::to_string)
                .unwrap_or_else(|| format!("epic/{name}")),
            status: EpicStatus::Active,
            created_at: now,
            completed_at: None,
        };
        let plan = EpicPlan {
            epic,
            tasks: vec![],
            deps: vec![],
        };
        match self.store.insert_epic_with_tasks(plan, now) {
            Ok(()) => {
                writeln!(out, "epic '{name}' created")?;
                Ok(0)
            }
            Err(TaskStoreError::Domain(DomainError::EpicAlreadyExists(n, st))) => {
                writeln!(out, "epic '{n}' already exists ({})", st.as_str())?;
                Ok(1)
            }
            Err(e) => Err(e).with_context(|| format!("creating epic '{name}'")),
        }
    }

    pub fn list(
        &self,
        status: Option<EpicStatusFilter>,
        json: bool,
        out: &mut dyn Write,
    ) -> Result<i32> {
        let filter = status.and_then(EpicStatusFilter::to_status);
        let epics = self.store.list_epics(filter).context("listing epics")?;
        if json {
            return write_json(out, &epics).map(|()| 0);
        }
        if epics.is_empty() {
            writeln!(out, "(no epics)")?;
            return Ok(0);
        }
        writeln!(
            out,
            "NAME                STATUS      BRANCH                    SPEC"
        )?;
        for e in &epics {
            writeln!(
                out,
                "{:<18}  {:<10}  {:<24}  {}",
                e.name,
                e.status.as_str(),
                e.branch,
                e.spec_path.display()
            )?;
        }
        Ok(0)
    }

    pub fn get(&self, name: &str, json: bool, out: &mut dyn Write) -> Result<i32> {
        let Some(epic) = self
            .store
            .get_epic(name)
            .with_context(|| format!("fetching epic '{name}'"))?
        else {
            writeln!(out, "epic '{name}' not found")?;
            return Ok(1);
        };
        render_epic(&epic, json, out)?;
        Ok(0)
    }

    pub fn status(&self, name: Option<&str>, json: bool, out: &mut dyn Write) -> Result<i32> {
        let epics: Vec<Epic> = match name {
            Some(n) => {
                let Some(e) = self.store.get_epic(n)? else {
                    writeln!(out, "epic '{n}' not found")?;
                    return Ok(1);
                };
                vec![e]
            }
            None => self.store.list_epics(Some(EpicStatus::Active))?,
        };

        let mut reports: Vec<EpicStatusReport> = Vec::with_capacity(epics.len());
        for e in &epics {
            let tasks = self.store.list_tasks_by_epic(&e.name, None)?;
            let mut counts = StatusCounts::default();
            for t in &tasks {
                match t.status {
                    TaskStatus::Pending => counts.pending += 1,
                    TaskStatus::Ready => counts.ready += 1,
                    TaskStatus::Wip => counts.wip += 1,
                    TaskStatus::Blocked => counts.blocked += 1,
                    TaskStatus::Done => counts.done += 1,
                    TaskStatus::Escalated => counts.escalated += 1,
                }
            }
            reports.push(EpicStatusReport {
                epic: e.name.clone(),
                status: e.status,
                total: tasks.len(),
                counts,
            });
        }

        if json {
            return write_json(out, &reports).map(|()| 0);
        }
        if reports.is_empty() {
            writeln!(out, "(no active epics)")?;
            return Ok(0);
        }
        writeln!(
            out,
            "EPIC               STATUS     PEND READY  WIP  BLK DONE  ESC TOTAL"
        )?;
        for r in &reports {
            writeln!(
                out,
                "{:<18} {:<10} {:>4} {:>5} {:>4} {:>4} {:>4} {:>4} {:>5}",
                r.epic,
                r.status.as_str(),
                r.counts.pending,
                r.counts.ready,
                r.counts.wip,
                r.counts.blocked,
                r.counts.done,
                r.counts.escalated,
                r.total
            )?;
        }
        Ok(0)
    }

    pub fn complete(&self, name: &str, out: &mut dyn Write) -> Result<i32> {
        self.set_status(name, EpicStatus::Completed, out)
    }

    pub fn abandon(&self, name: &str, out: &mut dyn Write) -> Result<i32> {
        self.set_status(name, EpicStatus::Abandoned, out)
    }

    fn set_status(&self, name: &str, target: EpicStatus, out: &mut dyn Write) -> Result<i32> {
        let now = self.clock.now();
        match self.store.set_epic_status(name, target, now) {
            Ok(()) => {
                writeln!(out, "epic '{name}' {}", target.as_str())?;
                Ok(0)
            }
            Err(TaskStoreError::NotFound(_)) => {
                writeln!(out, "epic '{name}' not found")?;
                Ok(1)
            }
            Err(e) => {
                Err(e).with_context(|| format!("setting epic '{name}' to {}", target.as_str()))
            }
        }
    }

    pub fn find_by_spec_path(
        &self,
        spec_path: &Path,
        json: bool,
        out: &mut dyn Write,
    ) -> Result<i32> {
        match self.store.find_active_by_spec_path(spec_path) {
            Ok(Some(e)) => {
                render_epic(&e, json, out)?;
                Ok(0)
            }
            Ok(None) => {
                writeln!(out, "(no active epic for {})", spec_path.display())?;
                Ok(1)
            }
            Err(TaskStoreError::Domain(DomainError::Inconsistency(msg))) => {
                writeln!(out, "inconsistency: {msg}")?;
                Ok(3)
            }
            Err(e) => Err(e)
                .with_context(|| format!("finding active epic for spec '{}'", spec_path.display())),
        }
    }

    pub fn reconcile(&self, name: &str, plan_path: &Path, out: &mut dyn Write) -> Result<i32> {
        let now = self.clock.now();
        let existing = match self
            .store
            .get_epic(name)
            .with_context(|| format!("fetching epic '{name}' for reconcile"))?
        {
            Some(e) => e,
            None => {
                writeln!(
                    out,
                    "epic '{name}' not found — run `autopilot epic create` first"
                )?;
                return Ok(1);
            }
        };
        let plan = parse_reconcile_jsonl(plan_path, existing)
            .with_context(|| format!("parsing reconcile plan {}", plan_path.display()))?;
        match self.store.apply_reconciliation(plan, now) {
            Ok(()) => {
                writeln!(out, "epic '{name}' reconciled")?;
                Ok(0)
            }
            Err(TaskStoreError::Domain(DomainError::DepCycle(cycle))) => {
                writeln!(out, "dependency cycle: {cycle:?}")?;
                Ok(1)
            }
            Err(e) => Err(e).with_context(|| format!("reconciling epic '{name}'")),
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize)]
struct StatusCounts {
    pending: u32,
    ready: u32,
    wip: u32,
    blocked: u32,
    done: u32,
    escalated: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct EpicStatusReport {
    epic: String,
    status: EpicStatus,
    total: usize,
    counts: StatusCounts,
}

fn render_epic(e: &Epic, json: bool, out: &mut dyn Write) -> Result<()> {
    if json {
        return write_json(out, e);
    }
    writeln!(out, "name:         {}", e.name)?;
    writeln!(out, "status:       {}", e.status.as_str())?;
    writeln!(out, "branch:       {}", e.branch)?;
    writeln!(out, "spec_path:    {}", e.spec_path.display())?;
    writeln!(out, "created_at:   {}", e.created_at.to_rfc3339())?;
    if let Some(ts) = e.completed_at {
        writeln!(out, "completed_at: {}", ts.to_rfc3339())?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PlanLine {
    Task {
        id: String,
        title: String,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        fingerprint: Option<String>,
        #[serde(default)]
        source: Option<String>,
    },
    Dep {
        task: String,
        depends_on: String,
    },
    RemoteState {
        task_id: String,
        branch_exists: bool,
        #[serde(default)]
        pr: Option<PlanPr>,
    },
    OrphanBranch {
        #[serde(rename = "ref")]
        branch_ref: String,
    },
}

#[derive(Debug, Deserialize)]
struct PlanPr {
    number: u64,
    #[serde(default)]
    merged: bool,
    #[serde(default)]
    closed: bool,
}

fn parse_reconcile_jsonl(plan_path: &Path, existing: Epic) -> Result<ReconciliationPlan> {
    let file = std::fs::File::open(plan_path)
        .with_context(|| format!("opening {}", plan_path.display()))?;
    let reader = BufReader::new(file);

    let mut tasks: BTreeMap<String, NewTask> = BTreeMap::new();
    let mut deps: Vec<(TaskId, TaskId)> = Vec::new();
    let mut remote_state: Vec<RemoteTaskState> = Vec::new();
    let mut orphan_branches: Vec<String> = Vec::new();

    for (lineno, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("reading line {}", lineno + 1))?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parsed: PlanLine = serde_json::from_str(trimmed)
            .with_context(|| format!("parsing line {}: {trimmed}", lineno + 1))?;
        match parsed {
            PlanLine::Task {
                id,
                title,
                body,
                fingerprint,
                source,
            } => {
                let source = source
                    .as_deref()
                    .map(|s| {
                        TaskSource::parse(s).ok_or_else(|| {
                            anyhow::anyhow!("unknown source '{s}' on line {}", lineno + 1)
                        })
                    })
                    .transpose()?
                    .unwrap_or(TaskSource::Decompose);
                let nt = NewTask {
                    id: TaskId::from_raw(&id),
                    source,
                    fingerprint,
                    title,
                    body,
                };
                if tasks.insert(id.clone(), nt).is_some() {
                    anyhow::bail!("duplicate task id '{id}' on line {}", lineno + 1);
                }
            }
            PlanLine::Dep { task, depends_on } => {
                deps.push((TaskId::from_raw(&task), TaskId::from_raw(&depends_on)));
            }
            PlanLine::RemoteState {
                task_id,
                branch_exists,
                pr,
            } => {
                remote_state.push(RemoteTaskState {
                    task_id: TaskId::from_raw(&task_id),
                    branch_exists,
                    pr: pr.map(|p| RemotePrState {
                        number: p.number,
                        merged: p.merged,
                        closed: p.closed,
                    }),
                });
            }
            PlanLine::OrphanBranch { branch_ref } => {
                orphan_branches.push(branch_ref);
            }
        }
    }

    Ok(ReconciliationPlan {
        epic: Epic {
            status: EpicStatus::Active,
            ..existing
        },
        tasks: tasks.into_values().collect(),
        deps,
        remote_state,
        orphan_branches,
    })
}

pub fn epic_service<'a>(store: &'a dyn TaskStore, clock: &'a dyn Clock) -> EpicService<'a> {
    EpicService::new(store, clock)
}
