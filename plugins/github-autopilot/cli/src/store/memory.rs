use std::collections::BTreeMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};

use crate::domain::{
    DomainError, Epic, EpicStatus, Event, EventKind, Task, TaskFailureOutcome, TaskGraph, TaskId,
    TaskStatus,
};
use crate::ports::task_store::{
    EpicPlan, EpicRepo, EventFilter, EventLog, NewWatchTask, ReconciliationPlan, RemotePrState,
    Result, TaskRepo, TaskStoreError, UnblockReport, UpsertOutcome,
};

#[derive(Default)]
struct State {
    epics: BTreeMap<String, Epic>,
    tasks: BTreeMap<TaskId, Task>,
    deps: Vec<(TaskId, TaskId)>,
    events: Vec<Event>,
}

#[derive(Default)]
pub struct InMemoryTaskStore {
    state: Mutex<State>,
}

impl InMemoryTaskStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InMemoryTaskStore {
    fn deps_of(state: &State, id: &TaskId) -> Vec<TaskId> {
        state
            .deps
            .iter()
            .filter(|(t, _)| t == id)
            .map(|(_, d)| d.clone())
            .collect()
    }

    fn dependents_of(state: &State, id: &TaskId) -> Vec<TaskId> {
        state
            .deps
            .iter()
            .filter(|(_, d)| d == id)
            .map(|(t, _)| t.clone())
            .collect()
    }

    fn all_deps_done(state: &State, id: &TaskId) -> bool {
        Self::deps_of(state, id).iter().all(|d| {
            state
                .tasks
                .get(d)
                .map(|t| t.status == TaskStatus::Done)
                .unwrap_or(false)
        })
    }

    fn push_event(state: &mut State, event: Event) {
        state.events.push(event);
    }

    fn make_event(
        kind: EventKind,
        epic: Option<String>,
        task: Option<TaskId>,
        payload: serde_json::Value,
        at: DateTime<Utc>,
    ) -> Event {
        Event {
            task_id: task,
            epic_name: epic,
            kind,
            payload,
            at,
        }
    }
}

impl EpicRepo for InMemoryTaskStore {
    fn upsert_epic(&self, epic: &Epic) -> Result<()> {
        let mut s = self.state.lock().expect("poisoned");
        s.epics.insert(epic.name.clone(), epic.clone());
        Ok(())
    }

    fn get_epic(&self, name: &str) -> Result<Option<Epic>> {
        let s = self.state.lock().expect("poisoned");
        Ok(s.epics.get(name).cloned())
    }

    fn list_epics(&self, status: Option<EpicStatus>) -> Result<Vec<Epic>> {
        let s = self.state.lock().expect("poisoned");
        let out: Vec<Epic> = s
            .epics
            .values()
            .filter(|e| status.is_none_or(|st| e.status == st))
            .cloned()
            .collect();
        Ok(out)
    }

    fn set_epic_status(&self, name: &str, status: EpicStatus, at: DateTime<Utc>) -> Result<()> {
        let mut s = self.state.lock().expect("poisoned");
        let epic = s
            .epics
            .get_mut(name)
            .ok_or_else(|| TaskStoreError::NotFound(format!("epic '{name}'")))?;
        epic.status = status;
        if matches!(status, EpicStatus::Completed | EpicStatus::Abandoned) {
            epic.completed_at = Some(at);
        }
        let kind = match status {
            EpicStatus::Active => EventKind::EpicStarted,
            EpicStatus::Completed => EventKind::EpicCompleted,
            EpicStatus::Abandoned => EventKind::EpicAbandoned,
        };
        let event = InMemoryTaskStore::make_event(
            kind,
            Some(name.to_string()),
            None,
            serde_json::json!({}),
            at,
        );
        InMemoryTaskStore::push_event(&mut s, event);
        Ok(())
    }
}

impl TaskRepo for InMemoryTaskStore {
    fn insert_epic_with_tasks(&self, plan: EpicPlan, now: DateTime<Utc>) -> Result<()> {
        // Validate task id uniqueness
        let mut seen = std::collections::BTreeSet::new();
        for t in &plan.tasks {
            if !seen.insert(t.id.clone()) {
                return Err(DomainError::DuplicateTaskId(t.id.clone()).into());
            }
        }
        // Validate deps reference plan tasks
        for (a, b) in &plan.deps {
            if !seen.contains(a) {
                return Err(DomainError::UnknownDepTarget(a.clone()).into());
            }
            if !seen.contains(b) {
                return Err(DomainError::UnknownDepTarget(b.clone()).into());
            }
        }
        // Validate cycle
        let graph = TaskGraph::build(plan.deps.iter().cloned());
        if let Some(cycle) = graph.detect_cycle() {
            return Err(DomainError::DepCycle(cycle).into());
        }

        let mut s = self.state.lock().expect("poisoned");

        if let Some(existing) = s.epics.get(&plan.epic.name) {
            return Err(
                DomainError::EpicAlreadyExists(plan.epic.name.clone(), existing.status).into(),
            );
        }

        // Insert/replace epic as active
        let epic = Epic {
            status: EpicStatus::Active,
            ..plan.epic.clone()
        };
        s.epics.insert(epic.name.clone(), epic.clone());

        // Insert tasks (status pending, attempts 0)
        for nt in &plan.tasks {
            let task = Task {
                id: nt.id.clone(),
                epic_name: epic.name.clone(),
                source: nt.source,
                fingerprint: nt.fingerprint.clone(),
                title: nt.title.clone(),
                body: nt.body.clone(),
                status: TaskStatus::Pending,
                attempts: 0,
                branch: None,
                pr_number: None,
                escalated_issue: None,
                created_at: now,
                updated_at: now,
            };
            s.tasks.insert(task.id.clone(), task);
        }

        // Insert deps
        for (a, b) in plan.deps {
            s.deps.push((a, b));
        }

        // Promote entry-points to Ready
        let plan_ids: Vec<TaskId> = plan.tasks.iter().map(|t| t.id.clone()).collect();
        let entry_points: Vec<TaskId> = plan_ids
            .iter()
            .filter(|id| !s.deps.iter().any(|(t, _)| t == *id))
            .cloned()
            .collect();
        for id in &entry_points {
            if let Some(t) = s.tasks.get_mut(id) {
                t.status = TaskStatus::Ready;
                t.updated_at = now;
            }
        }

        // Events
        InMemoryTaskStore::push_event(
            &mut s,
            InMemoryTaskStore::make_event(
                EventKind::EpicStarted,
                Some(epic.name.clone()),
                None,
                serde_json::json!({}),
                now,
            ),
        );
        for nt in &plan.tasks {
            InMemoryTaskStore::push_event(
                &mut s,
                InMemoryTaskStore::make_event(
                    EventKind::TaskInserted,
                    Some(epic.name.clone()),
                    Some(nt.id.clone()),
                    serde_json::json!({"source": nt.source.as_str()}),
                    now,
                ),
            );
        }
        Ok(())
    }

    fn get_task(&self, id: &TaskId) -> Result<Option<Task>> {
        let s = self.state.lock().expect("poisoned");
        Ok(s.tasks.get(id).cloned())
    }

    fn list_tasks_by_epic(&self, epic: &str, status: Option<TaskStatus>) -> Result<Vec<Task>> {
        let s = self.state.lock().expect("poisoned");
        let out: Vec<Task> = s
            .tasks
            .values()
            .filter(|t| t.epic_name == epic)
            .filter(|t| status.is_none_or(|st| t.status == st))
            .cloned()
            .collect();
        Ok(out)
    }

    fn find_by_fingerprint(&self, epic: &str, fingerprint: &str) -> Result<Option<Task>> {
        let s = self.state.lock().expect("poisoned");
        Ok(s.tasks
            .values()
            .find(|t| t.epic_name == epic && t.fingerprint.as_deref() == Some(fingerprint))
            .cloned())
    }

    fn upsert_watch_task(&self, task: NewWatchTask, now: DateTime<Utc>) -> Result<UpsertOutcome> {
        let mut s = self.state.lock().expect("poisoned");

        // Duplicate fingerprint check
        if let Some(existing) = s
            .tasks
            .values()
            .find(|t| {
                t.epic_name == task.epic_name
                    && t.fingerprint.as_deref() == Some(task.fingerprint.as_str())
            })
            .cloned()
        {
            InMemoryTaskStore::push_event(
                &mut s,
                InMemoryTaskStore::make_event(
                    EventKind::WatchDuplicate,
                    Some(task.epic_name.clone()),
                    Some(existing.id.clone()),
                    serde_json::json!({"fingerprint": task.fingerprint}),
                    now,
                ),
            );
            return Ok(UpsertOutcome::DuplicateFingerprint(existing.id));
        }

        // Determine initial status: Ready if no deps, else Pending. New watch tasks have no deps
        // by definition (deps come only from spec decomposition).
        let initial_status = TaskStatus::Ready;

        let new_task = Task {
            id: task.id.clone(),
            epic_name: task.epic_name.clone(),
            source: task.source,
            fingerprint: Some(task.fingerprint.clone()),
            title: task.title.clone(),
            body: task.body.clone(),
            status: initial_status,
            attempts: 0,
            branch: None,
            pr_number: None,
            escalated_issue: None,
            created_at: now,
            updated_at: now,
        };
        let id = new_task.id.clone();
        s.tasks.insert(id.clone(), new_task);
        InMemoryTaskStore::push_event(
            &mut s,
            InMemoryTaskStore::make_event(
                EventKind::TaskInserted,
                Some(task.epic_name.clone()),
                Some(id.clone()),
                serde_json::json!({"source": task.source.as_str(), "fingerprint": task.fingerprint}),
                now,
            ),
        );
        Ok(UpsertOutcome::Inserted(id))
    }

    fn claim_next_task(&self, epic: &str, now: DateTime<Utc>) -> Result<Option<Task>> {
        let mut s = self.state.lock().expect("poisoned");
        // Candidates: ready tasks in this epic with all deps Done, ordered by created_at then id
        let mut candidates: Vec<TaskId> = s
            .tasks
            .values()
            .filter(|t| t.epic_name == epic && t.status == TaskStatus::Ready)
            .filter(|t| InMemoryTaskStore::all_deps_done(&s, &t.id))
            .map(|t| (t.created_at, t.id.clone()))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        candidates.sort_by(|a, b| {
            let ta = s.tasks.get(a).unwrap();
            let tb = s.tasks.get(b).unwrap();
            ta.created_at.cmp(&tb.created_at).then_with(|| a.cmp(b))
        });

        let chosen = match candidates.first().cloned() {
            Some(c) => c,
            None => return Ok(None),
        };

        let task = s.tasks.get_mut(&chosen).unwrap();
        task.status = TaskStatus::Wip;
        task.attempts += 1;
        task.updated_at = now;
        let snapshot = task.clone();

        InMemoryTaskStore::push_event(
            &mut s,
            InMemoryTaskStore::make_event(
                EventKind::TaskClaimed,
                Some(epic.to_string()),
                Some(snapshot.id.clone()),
                serde_json::json!({"attempts": snapshot.attempts}),
                now,
            ),
        );

        Ok(Some(snapshot))
    }

    fn complete_task_and_unblock(
        &self,
        id: &TaskId,
        pr_number: u64,
        now: DateTime<Utc>,
    ) -> Result<UnblockReport> {
        let mut s = self.state.lock().expect("poisoned");
        let task = s
            .tasks
            .get_mut(id)
            .ok_or_else(|| TaskStoreError::NotFound(format!("task '{id}'")))?;
        if task.status != TaskStatus::Wip {
            let cur = task.status;
            return Err(DomainError::IllegalTransition(id.clone(), cur, TaskStatus::Done).into());
        }
        task.status = TaskStatus::Done;
        task.pr_number = Some(pr_number);
        task.updated_at = now;
        let epic_name = task.epic_name.clone();

        InMemoryTaskStore::push_event(
            &mut s,
            InMemoryTaskStore::make_event(
                EventKind::TaskCompleted,
                Some(epic_name.clone()),
                Some(id.clone()),
                serde_json::json!({"pr_number": pr_number}),
                now,
            ),
        );

        // Unblock dependents
        let dependents = InMemoryTaskStore::dependents_of(&s, id);
        let mut newly_ready: Vec<TaskId> = Vec::new();
        for dep_task_id in dependents {
            let promote = {
                let dt = match s.tasks.get(&dep_task_id) {
                    Some(t) => t,
                    None => continue,
                };
                matches!(dt.status, TaskStatus::Pending | TaskStatus::Blocked)
                    && InMemoryTaskStore::all_deps_done(&s, &dep_task_id)
            };
            if promote {
                if let Some(dt) = s.tasks.get_mut(&dep_task_id) {
                    dt.status = TaskStatus::Ready;
                    dt.updated_at = now;
                }
                newly_ready.push(dep_task_id.clone());
                InMemoryTaskStore::push_event(
                    &mut s,
                    InMemoryTaskStore::make_event(
                        EventKind::TaskUnblocked,
                        Some(epic_name.clone()),
                        Some(dep_task_id),
                        serde_json::json!({}),
                        now,
                    ),
                );
            }
        }

        Ok(UnblockReport {
            completed: id.clone(),
            newly_ready,
        })
    }

    fn mark_task_failed(
        &self,
        id: &TaskId,
        max_attempts: u32,
        now: DateTime<Utc>,
    ) -> Result<TaskFailureOutcome> {
        let mut s = self.state.lock().expect("poisoned");
        let task = s
            .tasks
            .get_mut(id)
            .ok_or_else(|| TaskStoreError::NotFound(format!("task '{id}'")))?;
        if task.status != TaskStatus::Wip {
            let cur = task.status;
            return Err(DomainError::IllegalTransition(id.clone(), cur, TaskStatus::Ready).into());
        }
        let attempts = task.attempts;
        let epic_name = task.epic_name.clone();

        if attempts >= max_attempts {
            task.status = TaskStatus::Escalated;
            task.updated_at = now;
            InMemoryTaskStore::push_event(
                &mut s,
                InMemoryTaskStore::make_event(
                    EventKind::TaskFailed,
                    Some(epic_name.clone()),
                    Some(id.clone()),
                    serde_json::json!({"final": true, "attempts": attempts}),
                    now,
                ),
            );
            InMemoryTaskStore::push_event(
                &mut s,
                InMemoryTaskStore::make_event(
                    EventKind::TaskEscalated,
                    Some(epic_name.clone()),
                    Some(id.clone()),
                    serde_json::json!({"attempts": attempts}),
                    now,
                ),
            );
            // Block dependents
            let dependents = InMemoryTaskStore::dependents_of(&s, id);
            for dep_id in dependents {
                let should_block = matches!(
                    s.tasks.get(&dep_id).map(|t| t.status),
                    Some(TaskStatus::Pending) | Some(TaskStatus::Ready)
                );
                if should_block {
                    if let Some(dt) = s.tasks.get_mut(&dep_id) {
                        dt.status = TaskStatus::Blocked;
                        dt.updated_at = now;
                    }
                    InMemoryTaskStore::push_event(
                        &mut s,
                        InMemoryTaskStore::make_event(
                            EventKind::TaskBlocked,
                            Some(epic_name.clone()),
                            Some(dep_id),
                            serde_json::json!({"reason": "parent_escalated", "parent": id.as_str()}),
                            now,
                        ),
                    );
                }
            }
            Ok(TaskFailureOutcome::Escalated { attempts })
        } else {
            task.status = TaskStatus::Ready;
            task.updated_at = now;
            InMemoryTaskStore::push_event(
                &mut s,
                InMemoryTaskStore::make_event(
                    EventKind::TaskFailed,
                    Some(epic_name),
                    Some(id.clone()),
                    serde_json::json!({"final": false, "attempts": attempts}),
                    now,
                ),
            );
            Ok(TaskFailureOutcome::Retried { attempts })
        }
    }

    fn escalate_task(&self, id: &TaskId, issue_number: u64, now: DateTime<Utc>) -> Result<()> {
        let mut s = self.state.lock().expect("poisoned");
        let task = s
            .tasks
            .get_mut(id)
            .ok_or_else(|| TaskStoreError::NotFound(format!("task '{id}'")))?;
        task.escalated_issue = Some(issue_number);
        task.updated_at = now;
        let epic_name = task.epic_name.clone();
        InMemoryTaskStore::push_event(
            &mut s,
            InMemoryTaskStore::make_event(
                EventKind::TaskEscalated,
                Some(epic_name),
                Some(id.clone()),
                serde_json::json!({"issue": issue_number}),
                now,
            ),
        );
        Ok(())
    }

    fn revert_to_ready(&self, id: &TaskId, now: DateTime<Utc>) -> Result<()> {
        let mut s = self.state.lock().expect("poisoned");
        let task = s
            .tasks
            .get_mut(id)
            .ok_or_else(|| TaskStoreError::NotFound(format!("task '{id}'")))?;
        let cur = task.status;
        if matches!(cur, TaskStatus::Done | TaskStatus::Escalated) {
            return Err(DomainError::IllegalTransition(id.clone(), cur, TaskStatus::Ready).into());
        }
        task.status = TaskStatus::Ready;
        task.updated_at = now;
        Ok(())
    }

    fn force_status(&self, id: &TaskId, new_status: TaskStatus, now: DateTime<Utc>) -> Result<()> {
        let mut s = self.state.lock().expect("poisoned");
        let task = s
            .tasks
            .get_mut(id)
            .ok_or_else(|| TaskStoreError::NotFound(format!("task '{id}'")))?;
        task.status = new_status;
        task.updated_at = now;
        Ok(())
    }

    fn apply_reconciliation(&self, plan: ReconciliationPlan, now: DateTime<Utc>) -> Result<()> {
        // Validate cycle in incoming plan
        let graph = TaskGraph::build(plan.deps.iter().cloned());
        if let Some(cycle) = graph.detect_cycle() {
            return Err(DomainError::DepCycle(cycle).into());
        }

        let mut s = self.state.lock().expect("poisoned");

        // Upsert epic as active
        let epic_name = plan.epic.name.clone();
        let merged_epic = match s.epics.get(&epic_name) {
            Some(existing) => Epic {
                status: EpicStatus::Active,
                created_at: existing.created_at,
                ..plan.epic.clone()
            },
            None => Epic {
                status: EpicStatus::Active,
                ..plan.epic.clone()
            },
        };
        s.epics.insert(epic_name.clone(), merged_epic);

        // Upsert tasks (preserve attempts, status, branch, pr_number)
        let plan_ids: std::collections::BTreeSet<TaskId> =
            plan.tasks.iter().map(|t| t.id.clone()).collect();
        for nt in &plan.tasks {
            match s.tasks.get_mut(&nt.id) {
                Some(existing) => {
                    existing.title = nt.title.clone();
                    existing.body = nt.body.clone();
                    existing.source = nt.source;
                    if existing.fingerprint.is_none() {
                        existing.fingerprint = nt.fingerprint.clone();
                    }
                    existing.updated_at = now;
                }
                None => {
                    let task = Task {
                        id: nt.id.clone(),
                        epic_name: epic_name.clone(),
                        source: nt.source,
                        fingerprint: nt.fingerprint.clone(),
                        title: nt.title.clone(),
                        body: nt.body.clone(),
                        status: TaskStatus::Pending,
                        attempts: 0,
                        branch: None,
                        pr_number: None,
                        escalated_issue: None,
                        created_at: now,
                        updated_at: now,
                    };
                    s.tasks.insert(task.id.clone(), task);
                }
            }
        }

        // Replace deps for plan tasks: drop existing deps where task_id in plan, insert new
        s.deps.retain(|(a, _)| !plan_ids.contains(a));
        for (a, b) in plan.deps {
            s.deps.push((a, b));
        }

        // Apply remote_state to determine status
        for r in &plan.remote_state {
            let desired = remote_to_status(r, &s);
            let task = match s.tasks.get_mut(&r.task_id) {
                Some(t) => t,
                None => continue,
            };
            task.status = desired;
            if let Some(pr) = &r.pr {
                task.pr_number = Some(pr.number);
            }
            task.updated_at = now;
        }

        // Tasks in plan but not in remote_state: classify by deps
        let in_remote: std::collections::BTreeSet<TaskId> = plan
            .remote_state
            .iter()
            .map(|r| r.task_id.clone())
            .collect();
        for nt in &plan.tasks {
            if in_remote.contains(&nt.id) {
                continue;
            }
            let deps_satisfied = InMemoryTaskStore::all_deps_done(&s, &nt.id);
            let desired = if deps_satisfied {
                TaskStatus::Ready
            } else {
                TaskStatus::Pending
            };
            // Only override if currently Pending — preserve any prior progress.
            if let Some(task) = s.tasks.get_mut(&nt.id) {
                if matches!(task.status, TaskStatus::Pending) {
                    task.status = desired;
                    task.updated_at = now;
                }
            }
        }

        // Orphan branches: emit one reconciled event per orphan
        for branch in &plan.orphan_branches {
            InMemoryTaskStore::push_event(
                &mut s,
                InMemoryTaskStore::make_event(
                    EventKind::Reconciled,
                    Some(epic_name.clone()),
                    None,
                    serde_json::json!({"orphan_branch": branch}),
                    now,
                ),
            );
        }

        InMemoryTaskStore::push_event(
            &mut s,
            InMemoryTaskStore::make_event(
                EventKind::Reconciled,
                Some(epic_name.clone()),
                None,
                serde_json::json!({"tasks": plan.tasks.len()}),
                now,
            ),
        );

        Ok(())
    }

    fn list_deps(&self, task_id: &TaskId) -> Result<Vec<TaskId>> {
        let s = self.state.lock().expect("poisoned");
        Ok(InMemoryTaskStore::deps_of(&s, task_id))
    }
}

fn remote_to_status(r: &crate::ports::task_store::RemoteTaskState, s: &State) -> TaskStatus {
    match (&r.pr, r.branch_exists) {
        (Some(RemotePrState { merged: true, .. }), _) => TaskStatus::Done,
        (Some(_), true) => TaskStatus::Wip,
        (None, true) => TaskStatus::Wip,
        (None, false) => {
            if InMemoryTaskStore::all_deps_done(s, &r.task_id) {
                TaskStatus::Ready
            } else {
                TaskStatus::Pending
            }
        }
        (Some(_), false) => TaskStatus::Wip, // PR open without branch is unusual; treat as wip
    }
}

impl EventLog for InMemoryTaskStore {
    fn append_event(&self, event: &Event) -> Result<()> {
        let mut s = self.state.lock().expect("poisoned");
        s.events.push(event.clone());
        Ok(())
    }

    fn list_events(&self, filter: EventFilter) -> Result<Vec<Event>> {
        let s = self.state.lock().expect("poisoned");
        let mut out: Vec<Event> = s
            .events
            .iter()
            .filter(|e| {
                filter
                    .epic
                    .as_deref()
                    .is_none_or(|n| e.epic_name.as_deref() == Some(n))
            })
            .filter(|e| {
                filter
                    .task
                    .as_ref()
                    .is_none_or(|t| e.task_id.as_ref() == Some(t))
            })
            .filter(|e| filter.kinds.is_empty() || filter.kinds.contains(&e.kind))
            .filter(|e| filter.since.is_none_or(|s| e.at >= s))
            .cloned()
            .collect();
        if let Some(limit) = filter.limit {
            out.truncate(limit as usize);
        }
        Ok(out)
    }
}
