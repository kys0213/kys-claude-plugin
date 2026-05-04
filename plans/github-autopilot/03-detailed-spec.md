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

### 2.2 Clock

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

### 5.6 Agent 측 흐름 (의사코드, 참고용)

본 §5 의 알고리즘은 모두 autopilot 내부 (TaskStore 메서드) 의 것이다. Agent 는 이들을 CLI 로
호출하며 자기 흐름은 자유롭게 설계한다. 권장 패턴:

```
# Watch dispatch (UC-6, UC-7)
fn agent_dispatch_finding(finding):
  let epic = `autopilot epic find-by-spec-path <finding.spec_path>` (JSON)
  if epic is Some:
    `autopilot task add --epic <epic.name> --id <det_id>
                       --section <finding.section> --requirement <finding.req>
                       --fingerprint <finding.fp> --source gap-watch`
    # autopilot 이 fingerprint 중복 시 DuplicateFingerprint 로 응답 → no-op
  else:
    if `autopilot suppress check --fingerprint <fp> --reason unmatched_watch`:
      return
    issue_n = `gh issue create --label autopilot:hitl-needed ...`
    `autopilot suppress add --fingerprint <fp> --reason unmatched_watch --until <now+24h>`

# Build cycle (UC-2, UC-11)
fn agent_build_tick():
  for epic in `autopilot epic list --status active` (JSON):
    while parallel_count < max_parallel:
      let task = `autopilot task claim --epic <epic.name>` or break
      let branch = format!("epic/{}/{}", epic.name, task.id)
      result = implement_and_push(branch, task)
      match result:
        PrCreated(n)   -> `autopilot task complete --id <task.id> --pr <n>`
        PushRejected   -> `autopilot task release --id <task.id>`   # UC-11
        OtherFailure   ->
          let outcome = `autopilot task fail --id <task.id>`
          if outcome == Escalated:
            issue_n = `gh issue create --label autopilot:hitl-needed ...`
            `autopilot task escalate --id <task.id> --issue <issue_n>`

# Escalation resolution polling (UC-9)
fn agent_escalation_tick():
  for epic in active_epics:
    for task in `autopilot task list --epic <epic.name> --status escalated`:
      if task.escalated_issue is None: continue
      let issue = gh.get_issue(task.escalated_issue)
      if not issue.closed: continue
      # 사람이 close 했음. reconcile 시도 → done 이면 자연 흡수, 아니면 suppress 등록
      let plan = build_reconcile_plan(epic, monitor, git)
      `autopilot epic reconcile --name <epic.name> --plan <plan.jsonl>`
      let task_after = `autopilot task get <task.id>`
      if task_after.status != "done":
        `autopilot suppress add --fingerprint <task.fingerprint>
                                --reason rejected_by_human --until <now+30d>`
```

이 흐름은 단지 권장 구현일 뿐 — agent 는 자유롭게 구성할 수 있다. autopilot 의 트레이트 메서드
계약 (§2, §5.1-§5.5) 만 만족하면 된다.

## 6. CLI 시그니처 (clap)

전 명령은 `--json` 출력 옵션을 지원하며, 기본은 사람용 요약. exit code: 0 정상 / 1 사용자 에러 / 2 일시적 시스템 에러 (재시도 가치 있음) / 3 영구적 시스템 에러.

### 6.1 `autopilot epic` (Ledger CLI)

```
autopilot epic create     --name <NAME> --spec <PATH> [--branch <REF>]
                          [--from-branch <REF>]                              # branch 기본값: epic/<NAME>
autopilot epic list       [--status active|completed|abandoned|all] [--json]
autopilot epic get        <NAME> [--json]
autopilot epic status     [<NAME>] [--json]                                  # task 상태 카운트
autopilot epic complete   <NAME>                                             # status='completed' + 알림은 agent 책임
autopilot epic abandon    <NAME>
autopilot epic reconcile  --name <NAME> --plan <FILE>                        # JSONL: tasks + deps + remote_state + orphan_branches

autopilot epic find-by-spec-path <PATH> [--json]                             # invariant 위반 시 exit 3 + Inconsistency 메시지
```

### 6.2 `autopilot task` (Ledger CLI)

```
autopilot task add        --epic <NAME> --id <TASK_ID>
                          [--section <PATH>] [--requirement <TEXT>]
                          [--title <TEXT>] [--body <TEXT>]
                          [--fingerprint <FP>]
                          [--source decompose|gap-watch|qa-boost|ci-watch|human]
autopilot task add-batch  --epic <NAME> --from <FILE>                        # JSONL: NewTask 한 줄에 1건
autopilot task list       --epic <NAME> [--status <STATUS>] [--json]
autopilot task get        <TASK_ID> [--json]
autopilot task find-by-pr <PR_NUMBER> [--json]

autopilot task claim      --epic <NAME> [--json]                             # 다음 ready task 원자적 wip 전환
autopilot task release    <TASK_ID>                                          # UC-11 claim_lost (attempts -1)
autopilot task complete   <TASK_ID> --pr <NUMBER>
autopilot task fail       <TASK_ID>                                          # exit 0 + Retried | Escalated 출력
autopilot task escalate   <TASK_ID> --issue <NUMBER>
autopilot task force-status <TASK_ID> --to <STATUS> [--reason <TEXT>]        # 운영자 override
```

### 6.3 `autopilot suppress` (Ledger CLI)

```
autopilot suppress add    --fingerprint <FP> --reason <TEXT> --until <ISO8601>
autopilot suppress check  --fingerprint <FP> --reason <TEXT>                 # exit 0=억제 중 / exit 1=아님
autopilot suppress clear  --fingerprint <FP> --reason <TEXT>
```

### 6.4 `autopilot events` (Ledger CLI)

```
autopilot events list     [--epic <NAME>] [--task <TASK_ID>] [--kind <KIND>...]
                          [--since <ISO8601>] [--limit <N>] [--json]
```

### 6.5 기존 헬퍼 명령

`watch run`, `pipeline idle`, `worktree`, `issue`, `issue list`, `labels`, `preflight`, `simhash`, `stats`, `check` 는 기존 시그니처 그대로. **ledger 와 결합되지 않음** — agent 가 자기 흐름에서 자유롭게 호출.

### 6.6 입출력 형식 (JSONL)

`epic reconcile --plan` 의 JSONL 한 줄 예시:

```json
{"kind":"task","id":"a1b2c3d4e5f6","title":"...","section":"## 인증","requirement":"토큰 갱신","fingerprint":null}
{"kind":"dep","task":"b2c3d4e5f6a1","depends_on":"a1b2c3d4e5f6"}
{"kind":"remote_state","task_id":"a1b2c3d4e5f6","branch_exists":true,"pr":{"number":42,"merged":true,"closed":true}}
{"kind":"orphan_branch","ref":"epic/auth/old-task-id"}
```

`task add-batch --from` 의 JSONL: 각 줄이 NewTask (`id`, `title`, `body?`, `section?`, `requirement?`, `fingerprint?`, `source`).

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
    #[error("dep references unknown task: {0}")]
    UnknownDepTarget(TaskId),
    #[error("duplicate task id in plan: {0}")]
    DuplicateTaskId(TaskId),
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
    #[error("not found: {0}")]              NotFound(String),
    #[error(transparent)]                   Domain(#[from] DomainError),
}
```

git / GitHub / 분해 실패 / 알림 실패 등 외부 도구 에러는 **agent 의 영역** 으로 autopilot 의 트레이트 시스템에 들어가지 않는다. CLI 진입점은 `TaskStoreError` 를 사용자 메시지 + exit code 로 매핑한다.

## 8. 설정 스키마

`autopilot.toml` (기본 위치는 기존 설정 컨벤션 유지). Ledger 관련 항목만 정의:

```toml
[storage]
db_path = ".autopilot/state.db"

[epic]
max_attempts = 3                   # task fail → outcome=Escalated 임계
hitl_label   = "autopilot:hitl-needed"   # 정보용; agent 측이 사용

[suppression]
default_window_hours = 24          # suppress add 의 --until 기본 (선택)
```

다른 헬퍼 명령 (gh / git / watch / pipeline ...) 의 설정은 별도 섹션으로 유지되며 ledger 와 독립.

설정 우선순위: CLI 플래그 > 환경변수 (`AUTOPILOT_*`) > 파일 > 기본값. 시작 시 유효성 검사 실패면 즉시 종료 (exit 1).

## 9. 본 spec 의 변경 정책

- 인터페이스 (포트 trait, SQL 스키마, CLI 시그니처) 변경은 PR 에서 본 문서 동시 수정 필수
- 알고리즘 의사코드는 구현 PR 에서 실제 코드와 의미가 일치하도록 동시 갱신
- 04 (테스트) 의 시나리오가 본 문서의 시그니처를 사용한다는 사실에 유의 — 시그니처 변경 시 04 도 함께 검토
