# 03. Detailed Spec

> 02 의 모듈 경계를 구체적인 Rust 시그니처 / SQL 스키마 / 알고리즘 의사코드 / CLI 시그니처로 확정한다. 본 문서는 03 → 코드로 1:1 변환 가능한 수준을 목표로 한다.

## 1. 도메인 타입

`src/domain/` 의 pure 타입. 외부 의존성은 `serde`, `chrono` 만 허용.

```rust
// domain/task_id.rs
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct TaskId(String);    // 12 hex chars, lowercase

impl TaskId {
    pub fn new_deterministic(epic: &str, section_path: &str, requirement: &str) -> Self {
        let canon = format!("{}::{}::{}", epic, normalize_section(section_path), slug(requirement));
        let digest = sha256::digest(canon.as_bytes());
        TaskId(digest[..12].to_string())
    }
    pub fn as_str(&self) -> &str { &self.0 }
}

fn normalize_section(p: &str) -> String { /* trim + lowercase + collapse spaces */ }
fn slug(s: &str) -> String { /* ascii lowercase, [^a-z0-9]+ -> '-', trim '-' */ }
```

```rust
// domain/epic.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Epic {
    pub name: String,
    pub spec_path: PathBuf,
    pub branch: String,
    pub status: EpicStatus,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EpicStatus { Active, Completed, Abandoned }
```

```rust
// domain/task.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub epic_name: String,
    pub source: TaskSource,
    pub fingerprint: Option<String>,
    pub title: String,
    pub body: Option<String>,
    pub status: TaskStatus,
    pub attempts: u32,
    pub branch: Option<String>,
    pub pr_number: Option<u64>,
    pub escalated_issue: Option<u64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus { Pending, Ready, Wip, Blocked, Done, Escalated }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskSource { Decompose, GapWatch, QaBoost, CiWatch, Human }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskFailureOutcome {
    Retried { attempts: u32 },
    Escalated { attempts: u32 },
}
```

```rust
// domain/event.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub task_id: Option<TaskId>,
    pub epic_name: Option<String>,
    pub kind: EventKind,
    pub payload: serde_json::Value,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    EpicStarted, EpicCompleted, EpicAbandoned,
    TaskInserted, TaskClaimed, TaskStarted, TaskCompleted, TaskFailed,
    TaskEscalated, TaskBlocked, TaskUnblocked,
    Reconciled, ClaimLost, MigratedFromIssue, EscalationResolved,
    TaskForceStatus,    // 운영자 force_status (payload: {target, reason})
}
```

```rust
// domain/deps.rs
pub struct TaskGraph { /* adjacency: TaskId -> Vec<TaskId> */ }

impl TaskGraph {
    pub fn build(deps: impl IntoIterator<Item = (TaskId, TaskId)>) -> Self;
    pub fn detect_cycle(&self) -> Option<Vec<TaskId>>;
    pub fn topological_order(&self) -> Result<Vec<TaskId>, CycleError>;
    pub fn dependents_of(&self, id: &TaskId) -> Vec<&TaskId>;
}
```

## 2. 포트 trait 시그니처

`src/ports/` 의 trait. 모든 메서드는 `Send + Sync` 안전을 가정한다.

### 2.1 TaskStore (분할 trait + 슈퍼트레이트)

```rust
// ports/task_store.rs
pub trait EpicRepo {
    fn upsert_epic(&self, epic: &Epic) -> Result<()>;
    fn get_epic(&self, name: &str) -> Result<Option<Epic>>;
    fn list_epics(&self, status: Option<EpicStatus>) -> Result<Vec<Epic>>;
    fn set_epic_status(&self, name: &str, status: EpicStatus, at: DateTime<Utc>) -> Result<()>;

    /// 활성 epic 중 spec_path 가 일치하는 것을 반환. WatchDispatcher 가 갭 발견 시
    /// "이 spec 이 어느 활성 epic 에 속하는지" 매칭에 사용 (UC-6).
    /// 동일 spec_path 의 active epic 은 최대 1개라는 invariant 가정 — 2개 이상 발견 시
    /// `Err(DomainError::Inconsistency)` 반환.
    fn find_active_by_spec_path(&self, spec_path: &Path) -> Result<Option<Epic>>;
}

pub trait TaskRepo {
    fn insert_epic_with_tasks(&self, plan: EpicPlan, now: DateTime<Utc>) -> Result<()>;
    fn get_task(&self, id: &TaskId) -> Result<Option<Task>>;
    fn list_tasks_by_epic(&self, epic: &str, status: Option<TaskStatus>) -> Result<Vec<Task>>;
    fn find_by_fingerprint(&self, epic: &str, fp: &str) -> Result<Option<Task>>;

    /// 머지된 PR 번호로 task 역조회. MergeLoop 가 PR 머지 후 task.status='done' 전이
    /// 대상을 찾기 위해 사용 (UC-2, §5.8).
    fn find_task_by_pr(&self, pr_number: u64) -> Result<Option<Task>>;

    fn upsert_watch_task(&self, task: NewWatchTask, now: DateTime<Utc>) -> Result<UpsertOutcome>;

    fn claim_next_task(&self, epic: &str, now: DateTime<Utc>) -> Result<Option<Task>>;

    fn complete_task_and_unblock(
        &self, id: &TaskId, pr_number: u64, now: DateTime<Utc>
    ) -> Result<UnblockReport>;

    fn mark_task_failed(
        &self, id: &TaskId, max_attempts: u32, now: DateTime<Utc>
    ) -> Result<TaskFailureOutcome>;

    fn escalate_task(
        &self, id: &TaskId, issue_number: u64, now: DateTime<Utc>
    ) -> Result<()>;

    /// 시도조차 실패한 claim 을 되돌린다 (UC-11 의 push reject = claim_lost).
    /// `claim_next_task` 에서 +1 된 attempts 를 차감하여 max_attempts 카운트에 영향이
    /// 없도록 한다. 상태 wip → ready, 변화량 != 1 이면 IllegalTransition.
    /// 주의: push 후 CI 실패 / 구현 실패는 `mark_task_failed` 사용 (attempts 보존).
    fn release_claim(&self, id: &TaskId, now: DateTime<Utc>) -> Result<()>;

    fn apply_reconciliation(&self, plan: ReconciliationPlan, now: DateTime<Utc>) -> Result<()>;

    fn list_deps(&self, task_id: &TaskId) -> Result<Vec<TaskId>>;

    /// 운영자 오버라이드 (CLI `task force-status`). 일반 상태 전이 검증을 우회하지만,
    /// (a) 도메인 enum 에 정의된 상태로만 전이 가능, (b) reason 을 events 에 기록한다.
    /// 의존성 재평가 / 자식 unblock 은 수행하지 않는다 — 필요하면 호출자가 reconcile
    /// 또는 후속 메서드를 명시적으로 호출.
    fn force_status(
        &self, id: &TaskId, target: TaskStatus, reason: &str, now: DateTime<Utc>
    ) -> Result<()>;
}

pub trait EventLog {
    fn append_event(&self, event: &Event) -> Result<()>;
    fn list_events(&self, filter: EventFilter) -> Result<Vec<Event>>;
}

/// HITL escalation 의 fingerprint 기반 중복 발행 억제 (UC-7).
/// 동일 fingerprint + reason 조합이 suppress_until 시점까지 escalate 되지 않게 한다.
pub trait SuppressionRepo {
    fn suppress(
        &self, fingerprint: &str, reason: &str, suppress_until: DateTime<Utc>
    ) -> Result<()>;
    fn is_suppressed(
        &self, fingerprint: &str, reason: &str, now: DateTime<Utc>
    ) -> Result<bool>;
    fn clear(&self, fingerprint: &str, reason: &str) -> Result<()>;
}

pub trait TaskStore: EpicRepo + TaskRepo + EventLog + SuppressionRepo + Send + Sync {}
impl<T: EpicRepo + TaskRepo + EventLog + SuppressionRepo + Send + Sync> TaskStore for T {}
```

보조 타입:

```rust
pub struct EpicPlan {
    pub epic: Epic,
    pub tasks: Vec<NewTask>,           // status는 Pending 으로 들어와도 됨
    pub deps: Vec<(TaskId, TaskId)>,    // (task, depends_on)
}

pub struct NewTask {
    pub id: TaskId,
    pub source: TaskSource,
    pub fingerprint: Option<String>,
    pub title: String,
    pub body: Option<String>,
}

pub struct NewWatchTask {
    pub id: TaskId,                     // 결정적 ID — fingerprint 와 별개
    pub epic_name: String,
    pub source: TaskSource,             // GapWatch / QaBoost / CiWatch / Human
    pub fingerprint: String,
    pub title: String,
    pub body: Option<String>,
}

pub enum UpsertOutcome {
    Inserted(TaskId),
    DuplicateFingerprint(TaskId),
}

pub struct UnblockReport {
    pub completed: TaskId,
    pub newly_ready: Vec<TaskId>,
}

pub struct ReconciliationPlan {
    pub epic: Epic,
    pub tasks: Vec<NewTask>,            // spec 분해 결과
    pub deps: Vec<(TaskId, TaskId)>,
    pub remote_state: Vec<RemoteTaskState>, // 리모트 ↔ task_id 매핑 결과
    pub orphan_branches: Vec<String>,        // 분해 결과에 없는 epic/<name>/* 브랜치
}

pub struct RemoteTaskState {
    pub task_id: TaskId,
    pub branch_exists: bool,
    pub pr: Option<RemotePrState>,
}

pub struct RemotePrState {
    pub number: u64,
    pub merged: bool,
    pub closed: bool,
}

pub struct EventFilter {
    pub epic: Option<String>,
    pub task: Option<TaskId>,
    pub kinds: Vec<EventKind>,
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<u32>,
}
```

### 2.2 GitClient

```rust
// ports/git.rs
pub trait GitClient: Send + Sync {
    fn current_branch(&self) -> Result<String>;
    fn working_tree_clean(&self) -> Result<bool>;

    fn fetch_origin(&self, refspec: Option<&str>) -> Result<()>;
    fn ls_remote_branches(&self, prefix: &str) -> Result<Vec<String>>;
    fn create_branch_from(&self, new_branch: &str, base: &str) -> Result<()>;
    fn push_branch(&self, branch: &str) -> Result<PushOutcome>;
    fn delete_remote_branch(&self, branch: &str) -> Result<()>;

    fn rev_parse(&self, refname: &str) -> Result<Option<String>>;
}

pub enum PushOutcome { Created, FastForward, Rejected(String) }
```

### 2.3 GitHubClient

```rust
// ports/github.rs
pub trait GitHubClient: Send + Sync {
    fn create_issue(&self, req: CreateIssue) -> Result<u64>;
    fn close_issue(&self, number: u64) -> Result<()>;
    fn add_issue_comment(&self, number: u64, body: &str) -> Result<()>;
    fn add_issue_labels(&self, number: u64, labels: &[String]) -> Result<()>;
    fn remove_issue_labels(&self, number: u64, labels: &[String]) -> Result<()>;
    fn list_issues(&self, filter: IssueFilter) -> Result<Vec<IssueRef>>;
    fn get_issue(&self, number: u64) -> Result<Option<IssueRef>>;

    fn create_pr(&self, req: CreatePr) -> Result<u64>;
    fn merge_pr(&self, number: u64, method: MergeMethod) -> Result<()>;
    fn list_prs_targeting(&self, base_branch: &str) -> Result<Vec<PrRef>>;
    fn get_pr(&self, number: u64) -> Result<Option<PrRef>>;
}

pub struct CreateIssue { pub title: String, pub body: String, pub labels: Vec<String> }
pub struct CreatePr { pub head: String, pub base: String, pub title: String, pub body: String, pub labels: Vec<String> }
pub struct PrRef { pub number: u64, pub head: String, pub base: String, pub merged: bool, pub closed: bool, pub labels: Vec<String> }
pub struct IssueRef {
    pub number: u64,
    pub title: String,
    pub closed: bool,
    pub labels: Vec<String>,
    pub body_meta: Option<EscalationMeta>,
}
pub struct IssueFilter { pub labels: Vec<String>, pub state: IssueState }
pub enum IssueState { Open, Closed, All }
pub enum MergeMethod { Squash, Merge, Rebase }
```

### 2.4 SpecDecomposer

```rust
// ports/decompose.rs
pub trait SpecDecomposer: Send + Sync {
    fn decompose(&self, spec_path: &Path, epic_name: &str) -> Result<DecomposeOutput>;
}

pub struct DecomposeOutput {
    pub tasks: Vec<NewTask>,            // id 는 결정적
    pub deps: Vec<(TaskId, TaskId)>,
}
```

### 2.5 Notifier

```rust
// ports/notifier.rs
pub trait Notifier: Send + Sync {
    fn send(&self, event: NotificationEvent) -> Result<()>;
}

pub enum NotificationEvent {
    EpicCompleted { epic: String },
    EscalationOpened { epic: Option<String>, task: Option<TaskId>, issue: u64 },
}
```

### 2.6 Clock

```rust
// ports/clock.rs
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}
```

## 3. 결정적 Task ID 명세

```
TaskId(epic, section_path, requirement) =
    sha256_hex(format!("{}::{}::{}", epic, normalize(section_path), slug(requirement)))[..12]
```

규칙:

- `normalize(section_path)`:
  1. NFKC 정규화
  2. 양옆 공백 제거
  3. 내부 연속 공백을 단일 space 로 압축
  4. 모두 소문자
- `slug(requirement)`:
  1. NFKC 정규화 + 소문자
  2. `[^a-z0-9]+` 를 `-` 로 치환
  3. 양끝 `-` 제거

이 두 함수는 spec 파일이 변경될 때 안정성을 위해 spec-kit 측 출력 직후 본 crate 에서 한 번 더 적용한다 (어댑터 차이 흡수).

테스트 보장: `task_id_stable_across_runs` (같은 입력 → 같은 12자), `task_id_collision_resistance` (현실적인 spec 1000개 입력 시 충돌 0).

## 4. SQLite 스키마 (V1)

`src/store/migrations/V1__initial.sql`:

```sql
PRAGMA foreign_keys = ON;

CREATE TABLE meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
INSERT INTO meta(key, value) VALUES ('schema_version', '1');

CREATE TABLE epics (
  name         TEXT PRIMARY KEY,
  spec_path    TEXT NOT NULL,
  branch       TEXT NOT NULL,
  status       TEXT NOT NULL CHECK (status IN ('active','completed','abandoned')),
  created_at   TEXT NOT NULL,
  completed_at TEXT
);

CREATE TABLE tasks (
  id              TEXT PRIMARY KEY,
  epic_name       TEXT NOT NULL REFERENCES epics(name),
  source          TEXT NOT NULL CHECK (source IN ('decompose','gap-watch','qa-boost','ci-watch','human')),
  fingerprint     TEXT,
  title           TEXT NOT NULL,
  body            TEXT,
  status          TEXT NOT NULL CHECK (status IN ('pending','ready','wip','blocked','done','escalated')),
  attempts        INTEGER NOT NULL DEFAULT 0,
  branch          TEXT,
  pr_number       INTEGER,
  escalated_issue INTEGER,
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL
);

CREATE TABLE task_deps (
  task_id    TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  depends_on TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  PRIMARY KEY (task_id, depends_on)
);

CREATE TABLE events (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,
  epic_name TEXT,
  task_id   TEXT,
  kind      TEXT NOT NULL,
  payload   TEXT NOT NULL DEFAULT '{}',
  at        TEXT NOT NULL
);

CREATE TABLE escalation_suppression (
  fingerprint  TEXT NOT NULL,
  reason       TEXT NOT NULL,
  suppress_until TEXT NOT NULL,
  PRIMARY KEY (fingerprint, reason)
);

CREATE INDEX idx_tasks_epic_status ON tasks(epic_name, status);
CREATE INDEX idx_tasks_fingerprint ON tasks(fingerprint);
CREATE INDEX idx_tasks_pr_number   ON tasks(pr_number) WHERE pr_number IS NOT NULL;
CREATE INDEX idx_epics_active_spec ON epics(spec_path) WHERE status='active';
CREATE INDEX idx_events_epic_at ON events(epic_name, at);
CREATE INDEX idx_events_task_at ON events(task_id, at);
```

신규 인덱스 사유:

- `idx_tasks_pr_number` — `find_task_by_pr` 의 O(1) 조회 (대부분 task 가 PR 미할당이므로 partial index 로 크기 절감).
- `idx_epics_active_spec` — `find_active_by_spec_path` 가 watch 빈번 호출이라 partial index 권장. 동일 spec_path 의 active epic 은 1개라는 invariant 와 일치.

마이그레이션 적용: 시작 시 `meta.schema_version` 을 읽고 누락된 V_n 을 순서대로 실행. 단일 트랜잭션 안에서 적용.

## 5. 알고리즘 의사코드

### 5.1 `insert_epic_with_tasks` (UC-1)

```
fn insert_epic_with_tasks(plan: EpicPlan, now):
  validate:
    - plan.tasks 의 id 가 모두 unique
    - plan.deps 가 모두 plan.tasks.id 안에서만 참조
    - TaskGraph(plan.deps).detect_cycle() == None  -> 아니면 DomainError::DepCycle
  begin tx:
    INSERT INTO epics(...) VALUES(...)             -- ON CONFLICT(name) DO NOTHING; 이후 SELECT 검증
    if epic 이미 존재 AND status != 'abandoned': rollback; Err(EpicAlreadyExists)

    for t in plan.tasks:
      INSERT INTO tasks(id, epic_name, source, fingerprint, title, body,
                        status='pending', attempts=0, created_at=now, updated_at=now)
    for (a, b) in plan.deps:
      INSERT INTO task_deps(task_id=a, depends_on=b)

    -- 진입점 (deps 없음) 을 ready 로 승격
    UPDATE tasks SET status='ready', updated_at=now
     WHERE epic_name = ? AND status='pending'
       AND id NOT IN (SELECT task_id FROM task_deps)

    INSERT INTO events kind='epic_started', epic=plan.epic.name, at=now
    INSERT INTO events kind='task_inserted' (각 task 별 — bulk)
  commit
  Ok(())
```

### 5.2 `claim_next_task` (UC-2, UC-3, UC-11)

```
fn claim_next_task(epic, now) -> Option<Task>:
  begin tx:
    candidate = SELECT t.*
                FROM tasks t
                WHERE t.epic_name = ? AND t.status='ready'
                  AND NOT EXISTS (
                    SELECT 1 FROM task_deps d
                    JOIN tasks dep ON dep.id = d.depends_on
                    WHERE d.task_id = t.id AND dep.status != 'done'
                  )
                ORDER BY t.created_at, t.id
                LIMIT 1
    if candidate is None:
      commit; return None

    n = UPDATE tasks
           SET status='wip', attempts = attempts+1, updated_at=?
         WHERE id = ? AND status='ready'
    if n != 1:
      rollback; return None             -- 동시 변경자가 가져감

    INSERT INTO events kind='task_claimed', task=candidate.id, epic=epic, at=now
  commit
  Ok(Some(candidate.with_status_wip()))
```

핵심: 후보 SELECT → 조건부 UPDATE → 이벤트 INSERT 가 모두 한 트랜잭션. UPDATE 의 `WHERE status='ready'` 로 동시성 race 자연 해소.

### 5.3 `complete_task_and_unblock` (UC-2, UC-3)

```
fn complete_task_and_unblock(id, pr_number, now) -> UnblockReport:
  begin tx:
    n = UPDATE tasks
           SET status='done', pr_number=?, updated_at=?
         WHERE id=? AND status='wip'
    if n != 1: rollback; Err(IllegalTransition)

    INSERT INTO events kind='task_completed', task=id, at=now

    -- 이 task 에 의존하던 (pending|blocked) 중 모든 deps 가 done 인 것을 ready 로
    affected_ids = SELECT d.task_id
                     FROM task_deps d
                    WHERE d.depends_on = ?
                      AND EXISTS (
                        SELECT 1 FROM tasks t
                         WHERE t.id = d.task_id AND t.status IN ('pending','blocked')
                      )
                      AND NOT EXISTS (
                        SELECT 1 FROM task_deps d2
                          JOIN tasks dep ON dep.id = d2.depends_on
                         WHERE d2.task_id = d.task_id AND dep.status != 'done'
                      )

    UPDATE tasks SET status='ready', updated_at=?
     WHERE id IN (affected_ids) AND status IN ('pending','blocked')

    for affected in affected_ids:
      INSERT INTO events kind='task_unblocked', task=affected, at=now
  commit
  Ok(UnblockReport{ completed: id, newly_ready: affected_ids })
```

### 5.4 `mark_task_failed` (UC-8)

```
fn mark_task_failed(id, max_attempts, now) -> TaskFailureOutcome:
  begin tx:
    row = SELECT attempts FROM tasks WHERE id=? AND status='wip'
    if row is None: rollback; Err(IllegalTransition)

    if row.attempts >= max_attempts:
      UPDATE tasks SET status='escalated', updated_at=? WHERE id=?
      INSERT INTO events kind='task_failed' payload={final:true} task=id at=now
      INSERT INTO events kind='task_escalated' task=id at=now

      -- 의존 task 들 blocked 로
      UPDATE tasks SET status='blocked', updated_at=?
       WHERE id IN (SELECT task_id FROM task_deps WHERE depends_on=?)
         AND status IN ('pending','ready')
      commit
      Ok(Escalated{ attempts: row.attempts })
    else:
      UPDATE tasks SET status='ready', updated_at=? WHERE id=?
      INSERT INTO events kind='task_failed' task=id at=now
      commit
      Ok(Retried{ attempts: row.attempts })
```

`escalate_task(id, issue_number, now)` 는 별도 호출로 위 outcome 처리 후 GitHub 이슈 발행을 마치고 `tasks.escalated_issue=?` 를 채운다 (단순 UPDATE + event).

### 5.5 `apply_reconciliation` (UC-4, UC-5)

```
fn apply_reconciliation(plan: ReconciliationPlan, now):
  begin tx:
    upsert epic(plan.epic) with status='active'

    -- 1) 분해 결과를 base 로 task upsert
    for t in plan.tasks:
      INSERT INTO tasks(...) ON CONFLICT(id) DO UPDATE
        SET title=excluded.title, body=excluded.body,
            -- attempts 와 status 는 보존 (단, 아래 2단계에서 덮어씀)
            updated_at=?
    upsert task_deps(plan.deps)
       (DELETE FROM task_deps WHERE task_id IN (plan.tasks.id) AND
                              (task_id, depends_on) NOT IN plan.deps;
        INSERT OR IGNORE for each plan.deps)

    -- 2) 리모트 상태로 status 결정
    for r in plan.remote_state:
      desired = match r:
        PR merged                         -> done
        feature branch + open PR          -> wip
        feature branch + no PR            -> wip   -- stale 후보, observability 로만
        no branch & deps satisfied        -> ready
        no branch & deps unsatisfied      -> pending
      UPDATE tasks SET status=desired, pr_number=r.pr.number?, updated_at=? WHERE id=?

    -- 3) 분해 결과에 없는 orphan 브랜치 기록
    for branch in plan.orphan_branches:
      INSERT INTO events kind='reconciled' payload={orphan_branch: branch} at=now

    INSERT INTO events kind='reconciled' epic=plan.epic.name payload={tasks: count} at=now
  commit
```

idempotency: 동일 plan 으로 N번 호출해도 결과 동일. attempts 카운터는 보존되며 status 는 리모트 기준으로 권위적으로 재설정.

### 5.6 WatchDispatcher (UC-6, UC-7)

```
fn dispatch_finding(finding: WatchFinding):
  match task_store.find_active_by_spec_path(finding.spec_path):
    Some(epic):
      task_id = TaskId::new_deterministic(epic.name, finding.section, finding.requirement)
      outcome = task_store.upsert_watch_task(NewWatchTask{
        id: task_id,
        epic_name: epic.name,
        source: finding.source,
        fingerprint: finding.fingerprint,
        title: finding.title,
        body: finding.body,
      }, now)
      log event accordingly
    None:
      if task_store.is_suppressed(finding.fingerprint, "unmatched_watch", now): return
      issue_number = github.create_issue(escalation_template(finding))
      task_store.append_event(kind='escalated', payload={...})
      task_store.suppress(
        finding.fingerprint,
        reason="unmatched_watch",
        suppress_until = now + Duration::hours(escalation_suppression_window_hours),
      )
```

`upsert_watch_task` 는 fingerprint 충돌 시 `DuplicateFingerprint(existing_id)` 반환, orchestration 에서 이를 보고 신규 발행 skip.

### 5.7 BuildLoop tick (UC-2, UC-11)

```
fn tick():
  for epic in active_epics:
    while parallel_agents < max_parallel_agents:
      task = task_store.claim_next_task(epic.name, now)
      if task is None: break
      branch = format!("{prefix}/{epic.name}/{task.id}", prefix=epic_branch_prefix)
      spawn_implementer(epic, task, branch)
        on_pr_created(pr_number) ->
            -- PR 생성 성공. 머지는 MergeLoop 가 후속 처리.
            task_store.append_event(kind='task_started', task=task.id, payload={pr: pr_number})
        on_push_rejected         ->
            -- UC-11: 다른 머신 / 다른 sub-loop 가 먼저 가져감. 시도 회복.
            task_store.release_claim(task.id, now)
            task_store.append_event(kind='claim_lost', task=task.id)
        on_implementation_failed ->
            -- 코드 작성 실패 / push 후 PR 생성 실패 / CI 실패 등. 시도는 보존.
            outcome = task_store.mark_task_failed(task.id, max_attempts, now)
            if Escalated: escalator.escalate(task)
```

`release_claim` 과 `mark_task_failed` 의 선택 기준: **시도조차 못 했으면** `release_claim`, **시도했으나 실패** 면 `mark_task_failed`.

### 5.8 MergeLoop tick (UC-2, UC-10)

```
fn tick():
  for epic in active_epics:
    prs = github.list_prs_targeting(epic.branch, labels=[":auto"], state=open)
    for pr in prs:
      if pr 의 ci 상태 통과 + 충돌 없음:
        github.merge_pr(pr.number, MergeMethod::Squash)
        task = task_store.find_task_by_pr(pr.number)
        if task is Some: task_store.complete_task_and_unblock(task.id, pr.number, now)

    -- epic 완료 판정
    -- "사람이 close 한 escalated" 의 자동 인식은 §5.9 EscalationWatcher 가 별도로 처리.
    -- 본 루프에서는 task.status ∈ {done} 여부만 본다.
    if all_tasks_done(epic):
      epic_manager.complete(epic.name)
      notifier.send(EpicCompleted { epic: epic.name })
```

### 5.9 EscalationWatcher tick (UC-9, UC-10)

`EscalationWatcher` 는 escalated task 의 GitHub 이슈 close 를 주기 폴링하여 사람의 직접 해소를
자동 인식한다. MergeLoop 와 분리한 이유: SRP — MergeLoop 는 PR 머지 흐름만, 본 루프는
escalation 해소 흐름만 담당. 폴링 주기는 MergeLoop 보다 길게 (예: 5분).

```
fn tick():
  for epic in active_epics:
    escalated = task_store.list_tasks_by_epic(epic.name, status=Escalated)
    for task in escalated:
      if task.escalated_issue is None: continue
      issue = github.get_issue(task.escalated_issue)
      if issue is None or !issue.closed: continue

      -- 사람이 issue 를 close 했음. 두 케이스:
      -- (a) 사람이 직접 코드를 작성하여 epic 브랜치에 PR 머지 → reconcile 이 done 으로 인식
      -- (b) 단순 거부 / 무시 close → suppression 등록 (해당 fingerprint 추가 escalate 차단)
      reconciler.reconcile_epic(epic.name, now)
      task_after = task_store.get_task(task.id)
      if task_after.status != Done:
        -- (b) 케이스. 사람이 코드 작업 없이 close 한 것으로 간주.
        if task.fingerprint is Some:
          task_store.suppress(
            task.fingerprint, reason="rejected_by_human",
            suppress_until = now + Duration::days(30),
          )
        task_store.append_event(kind='escalation_resolved',
                                payload={resolution: 'rejected'}, task=task.id)
      else:
        task_store.append_event(kind='escalation_resolved',
                                payload={resolution: 'human_fixed'}, task=task.id)
```

**중요한 invariant**: 본 루프는 `force_status` 를 직접 호출하지 않는다. 상태 전이는 reconcile
(idempotent) 또는 다른 정상 경로를 통해서만 일어난다.

## 6. CLI 시그니처 (clap)

`autopilot epic <subcommand>`:

```
autopilot epic start   --spec <PATH> --name <NAME> [--from-branch <REF>] [--dry-run]
autopilot epic resume  <NAME> [--reset-attempts]
autopilot epic stop    <NAME> [--purge-branches]
autopilot epic status  [<NAME>] [--json]
autopilot epic list    [--status active|completed|abandoned|all]
```

`autopilot task <subcommand>` (운영자용 진단/오버라이드):

```
autopilot task list    --epic <NAME> [--status <STATUS>] [--json]
autopilot task show    <TASK_ID> [--json]
autopilot task force-status <TASK_ID> --to <STATUS> [--reason <TEXT>]
```

`autopilot migrate <subcommand>`:

```
autopilot migrate import-issue <ISSUE_NUMBER> [--epic <NAME>]
autopilot migrate scan-issues  [--label <LABEL>=":ready"]      # 후보 목록만 출력
```

기존 명령 변화 (04 watch-integration 에서 자세히):

- `autopilot pipeline build-tasks` (build-issues 대체)
- `autopilot pipeline merge-prs` (의미 동일, task store 업데이트 추가)
- `autopilot watch run` (gap/qa/ci 통합 진입, 기존 유지)

출력 포맷: 기본은 사람용 표/요약, `--json` 일 때 stable schema. exit code: 0 정상 / 1 사용자 에러 / 2 일시적 시스템 에러 (재시도 가치 있음) / 3 영구적 시스템 에러.

## 7. 에러 타입

```rust
// domain/error.rs
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("dependency cycle: {0:?}")]
    DepCycle(Vec<TaskId>),
    #[error("illegal status transition for task {0}: {1:?} -> {2:?}")]
    IllegalTransition(TaskId, TaskStatus, TaskStatus),
    #[error("epic '{0}' already exists with status {1:?}")]
    EpicAlreadyExists(String, EpicStatus),
    /// 데이터 불변식 위반 (예: 동일 spec_path 의 active epic 이 2개 이상).
    /// 정상 흐름에서는 발생할 수 없으며, 발생 시 사람 개입 필요.
    #[error("inconsistency: {0}")]
    Inconsistency(String),
}

// ports/task_store.rs
#[derive(Debug, thiserror::Error)]
pub enum TaskStoreError {
    #[error("storage busy")]                Busy,
    #[error("storage backend: {0}")]        Backend(String),
    #[error("schema mismatch: at v{found}, expected v{expected}")]
    SchemaMismatch { found: u32, expected: u32 },
    #[error(transparent)]                   Domain(#[from] DomainError),
}

// 동일 패턴으로:
pub enum GitError      { NotARepo, FetchFailed(String), PushRejected(String), Other(String) }
pub enum GitHubError   { Network(String), NotFound, Unauthorized, RateLimited, Other(String) }
pub enum DecomposeError{ SpecNotFound(PathBuf), Parse(String), Other(String) }

// orchestration/error.rs
#[derive(Debug, thiserror::Error)]
pub enum OrchestrationError {
    #[error(transparent)] Store(#[from] TaskStoreError),
    #[error(transparent)] Git(#[from] GitError),
    #[error(transparent)] GitHub(#[from] GitHubError),
    #[error(transparent)] Decompose(#[from] DecomposeError),
    #[error(transparent)] Domain(#[from] DomainError),
    #[error("inconsistency: {0}")] Inconsistency(String),
}
```

CLI 진입점은 `OrchestrationError` 를 받아 사용자 메시지 + exit code 매핑.

## 8. 설정 스키마

`autopilot.toml` (기본 위치는 기존 설정 컨벤션 유지):

```toml
[storage]
db_path = ".autopilot/state.db"

[epic]
branch_prefix       = "epic/"
max_attempts        = 3
deps_cycle_check    = true

[hitl]
label                                = "autopilot:hitl-needed"
escalation_suppression_window_hours  = 24

[concurrency]
max_parallel_agents = 4
sqlite_busy_timeout_ms = 5000

[migration]
epic_based = true                  # false 면 기존 라벨 기반 동작 유지 (06 spec)
```

설정 우선순위: CLI 플래그 > 환경변수 (`AUTOPILOT_*`) > 파일 > 기본값. 시작 시 유효성 검사 실패면 즉시 종료 (exit 1).

## 9. 본 spec 의 변경 정책

- 인터페이스 (포트 trait, SQL 스키마, CLI 시그니처) 변경은 PR 에서 본 문서 동시 수정 필수
- 알고리즘 의사코드는 구현 PR 에서 실제 코드와 의미가 일치하도록 동시 갱신
- 04 (테스트) 의 시나리오가 본 문서의 시그니처를 사용한다는 사실에 유의 — 시그니처 변경 시 04 도 함께 검토
