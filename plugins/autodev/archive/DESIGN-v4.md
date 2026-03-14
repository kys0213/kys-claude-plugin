# DESIGN v4: Claw Layer — LLM 기반 자율 스케줄러

> **Date**: 2026-03-14
> **Revision**: v4.0 — Claw Layer (LLM 스케줄러), TaskSource/Queue 분리, Spec-driven 모드, Gap Detection
> **Base**: [DESIGN-v3.md](./DESIGN-v3.md) — Auto-Approve, Label-Positive, add-first 라벨 전이

---

## 1. 변경 동기

### v3의 한계

```
v3: TaskSource.poll() = 수집 + 판단(drain) 결합
                              ↑
                    기계적 상태 전이 (슬롯 기반)
                    판단이 TaskSource 내부에 갇힘
```

- `drain_queue_items()`가 GitHubTaskSource 내부에 있어 **소스 확장 시 판단 로직을 복제해야 함**
- 이슈 간 의존성/충돌을 감지하지 못함 → 같은 파일을 수정하는 이슈가 병렬 실행
- 설계-구현 간 구조적 gap을 탐지하지 못함 → 사람이 직접 발견해야 함
- HITL 필요성을 판단하지 못함 → 고정된 게이트만 존재 (analyzed → approved)
- 리뷰 3회 실패 같은 패턴을 감지하지 못함 → 기계적으로 계속 반복
- 디자인 스펙 기반 자율 구현 불가 → 이슈를 사람이 하나씩 등록해야 함

### v4 목표

1. **수집과 판단 분리**: TaskSource는 수집만, 판단은 소스 무관 Claw Layer에서
2. **Claw Layer**: `drain_queue_items()`를 LLM 판단으로 대체하는 스케줄러
3. **Dual Mode**: Issue-driven (v3 호환) + Spec-driven (자율 루프) 공존
4. **Spec-driven 모드**: 디자인 스펙 등록 → 이슈 분해 → 구현 → gap 탐지 → 스펙 충족까지 반복
5. **Gap Detection**: 설계-구현 간 구조적 불일치를 자동 탐지하여 이슈 생성
6. **지능적 HITL**: 고정 게이트 대신 Claw가 맥락 기반으로 HITL 필요성 판단

### v3 → v4 주요 차이

| | v3 | v4 |
|---|---|---|
| TaskSource 역할 | 수집 + 판단 (poll → drain → Task) | **수집만** (scan → WorkItem → Queue) |
| 판단 위치 | GitHubTaskSource 내부 (`drain_queue_items`) | **ClawManager** (소스 무관 통합 판단) |
| TaskManager | DefaultTaskManager (단순 수집/분배) | **ClawManager** (Claw 스케줄러 내장) |
| 큐 소유자 | 각 TaskSource 내부 (per-repo) | **ClawManager** (통합 TaskQueue) |
| 상태 전이 판단 | 기계적, 슬롯 기반 | LLM 판단, 맥락 기반 |
| 소스 확장 | 판단 로직 복제 필요 | **Claw가 소스 무관 통합 처리** |
| 이슈 생성 | 사람이 수동 등록 | 사람 수동 + Claw가 스펙에서 분해 |
| Gap 탐지 | 없음 | Claw가 스펙-코드 대조 → 이슈 생성 |
| Claw 비활성 시 | N/A | 기계적 drain fallback (v3 호환) |

---

## 2. 아키텍처

### 핵심 원칙: 수집과 판단의 분리

v3에서 TaskSource.poll()은 **수집 + 판단**을 동시에 수행했다:

```
v3: GitHubTaskSource.poll()
      ├─ sync_repos, recovery, scan  (수집)
      └─ drain_queue_items()          (판단 + Task 생성)
```

v4에서는 **수집**과 **판단**을 완전히 분리한다:

```
v4: TaskSource.collect()  → WorkItem을 TaskQueue에 push (수집만)
    ClawManager.schedule() → TaskQueue를 보고 판단 → Task 생성
```

### 전체 구조

```
┌──────────────────────────────────────────────────────────┐
│  Daemon (변경 최소)                                       │
│  select! { completion, tick, heartbeat, shutdown }       │
│  manager: ClawManager (was: DefaultTaskManager)          │
└──────────────┬───────────────────────────────────────────┘
               │ tick → manager.tick()
               ▼
┌──────────────────────────────────────────────────────────┐
│  ClawManager (NEW — DefaultTaskManager 대체)              │
│                                                          │
│  ┌──────────────────┐  ┌──────────────────┐             │
│  │ GitHubCollector   │  │ (SlackCollector) │  ...        │
│  │ scan → WorkItem   │  │ poll → WorkItem  │             │
│  └────────┬─────────┘  └────────┬─────────┘             │
│           │                      │                        │
│           └──────────┬───────────┘                        │
│                      ▼                                    │
│  ┌──────────────────────────────────────────────────┐    │
│  │  TaskQueue (통합, 소스 무관)                       │    │
│  │  Issue: Pending → Analyzing → Ready → Implementing │   │
│  │  PR: Pending → Reviewing → ReviewDone → ...        │   │
│  └──────────────────────────┬───────────────────────┘    │
│                              │                            │
│  ┌──────────────────────────▼───────────────────────┐    │
│  │  ClawScheduler (LLM 판단)                         │    │
│  │  OR                                               │    │
│  │  MechanicalScheduler (v3 호환 fallback)           │    │
│  │                                                   │    │
│  │  큐 전체를 보고 → Vec<ClawDecision> 반환           │    │
│  └──────────────────────────┬───────────────────────┘    │
│                              │                            │
│  ┌──────────────────────────▼───────────────────────┐    │
│  │  TaskFactory                                      │    │
│  │  ClawDecision::Advance → AnalyzeTask 등 생성      │    │
│  │  ClawDecision::Decompose → gh.create_issue()     │    │
│  │  ClawDecision::DetectGap → gh.create_issue()     │    │
│  └──────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────┘
               │
               ▼
┌──────────────────────────────────────────────────────────┐
│  기존 Task Pipeline (변경 없음)                           │
│  Analyze → Implement → Review → Improve → Extract        │
└──────────────────────────────────────────────────────────┘
```

### 구성 요소 역할

| 구성 요소 | v3 | v4 |
|-----------|----|----|
| **TaskSource** (→ Collector) | 수집 + 판단 + Task 생성 + 큐 소유 | **수집만** (WorkItem 반환) |
| **TaskQueue** | 각 Source 내부에 분산 | **통합 큐** (ClawManager 소유) |
| **DefaultTaskManager** | 단순 수집/분배 | **제거** → ClawManager로 대체 |
| **ClawManager** | 없음 | **NEW**: 수집 → 큐 → 판단 → Task 생성 통합 |
| **Scheduler** | `drain_queue_items()` (기계적) | **ClawScheduler** (LLM) or **MechanicalScheduler** (fallback) |
| **TaskFactory** | Source 내부에 분산 | **NEW**: Decision → Task 변환 전담 |

---

## 3. TaskSource 재정의 (Collector)

### v3 TaskSource trait

```rust
// v3: 수집 + 판단 결합
pub trait TaskSource: Send {
    async fn poll(&mut self) -> Vec<Box<dyn Task>>;  // Task를 직접 반환
    fn apply(&mut self, result: &TaskResult);         // 큐 상태 전이
    fn active_items(&self) -> Vec<StatusItem>;
}
```

### v4 Collector trait

```rust
/// v4: 수집만 수행. 외부 소스에서 WorkItem을 감지하고 반환한다.
/// 판단(어떤 아이템을 다음 phase로 보낼지)은 Scheduler가 담당.
#[async_trait(?Send)]
pub trait Collector: Send {
    /// 외부 소스를 스캔하여 새로운 WorkItem을 수집한다.
    /// recovery, scan 등을 수행하고 새로 감지된 아이템만 반환.
    async fn collect(&mut self) -> Vec<WorkItem>;

    /// Task 완료 후 외부 소스에 결과를 반영한다.
    /// (라벨 변경, 코멘트 게시 등은 Task가 직접 수행하므로,
    ///  여기서는 내부 상태 정리만 수행)
    fn apply(&mut self, result: &TaskResult);

    /// 현재 활성 아이템 목록 (status heartbeat용).
    fn active_items(&self) -> Vec<StatusItem>;

    /// 이 Collector의 소스 이름 (로깅/디버깅용).
    fn source_name(&self) -> &str;
}
```

### WorkItem (소스 무관 통합 아이템)

```rust
/// 소스에 관계없이 통합 큐에서 관리되는 작업 아이템.
/// GitHub 이슈, Slack 메시지, Jira 티켓 등 어떤 소스에서 왔든
/// 동일한 구조로 큐에 적재된다.
#[derive(Debug, Clone)]
pub struct WorkItem {
    pub work_id: String,          // "issue:org/repo:42", "slack:channel:ts"
    pub source: String,           // "github", "slack", "jira"
    pub repo_name: String,
    pub item_type: WorkItemType,  // Issue or PR
    pub phase: String,            // 초기 phase (Pending, Ready 등)
    pub metadata: WorkItemMetadata,
}

#[derive(Debug, Clone)]
pub enum WorkItemType {
    Issue(IssueItem),
    Pr(PrItem),
}

/// 소스별 추가 메타데이터 (Claw 판단에 참고).
#[derive(Debug, Clone)]
pub struct WorkItemMetadata {
    pub github_number: Option<i64>,
    pub title: String,
    pub labels: Vec<String>,
    pub review_iteration: Option<u32>,
    pub source_issue_number: Option<i64>,
}
```

### GitHubCollector (기존 GitHubTaskSource 리팩토링)

```rust
/// GitHub 이슈/PR 스캔 전용 Collector.
/// v3 GitHubTaskSource에서 drain_queue_items()와 큐를 제거한 버전.
///
/// 수행하는 작업:
/// - sync_repos (DB → in-memory)
/// - run_recovery (orphan 정리)
/// - run_scans (라벨 기반 감지)
///
/// 수행하지 않는 작업:
/// - 큐 상태 전이 판단 (ClawManager가 담당)
/// - Task 생성 (TaskFactory가 담당)
pub struct GitHubCollector<DB> {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
    env: Arc<dyn Env>,
    git: Arc<dyn Git>,
    sw: Arc<dyn SuggestWorkflow>,
    db: DB,
    repos: HashMap<String, GitRepository>,
}
```

`collect()` 구현:
- `sync_repos()`, `run_recovery()`, `run_scans()` 수행 (기존과 동일)
- 감지된 이슈/PR을 `WorkItem`으로 변환하여 반환
- **큐 drain 로직 없음** — 그건 ClawManager가 함

---

## 4. TaskQueue (통합 큐)

### 정의

소스 무관 통합 큐. 기존 `StateQueue<IssueItem>` + `StateQueue<PrItem>`을 **ClawManager가 소유**.

```rust
/// 소스 무관 통합 작업 큐.
/// 모든 Collector에서 수집된 WorkItem이 여기로 모인다.
/// ClawManager가 소유하고, Scheduler가 이 큐를 참조하여 판단.
pub struct TaskQueue {
    issue_queue: StateQueue<IssueItem>,
    pr_queue: StateQueue<PrItem>,
}

impl TaskQueue {
    /// Collector에서 수집된 WorkItem을 적절한 큐에 push.
    pub fn ingest(&mut self, item: WorkItem) { ... }

    /// 큐 전체 상태의 읽기 전용 스냅샷 생성 (Scheduler 입력용).
    pub fn snapshot(&self) -> QueueSnapshot { ... }

    /// Decision에 따른 phase 전이.
    pub fn advance(&mut self, work_id: &str, from: &str, to: &str) -> Option<WorkItem> { ... }

    /// 아이템 제거.
    pub fn remove(&mut self, work_id: &str) { ... }

    /// 슬롯 계산.
    pub fn available_issue_slots(&self, concurrency: usize) -> usize { ... }
    pub fn available_pr_slots(&self, concurrency: usize) -> usize { ... }
}
```

### 큐 소유권 이동

```
v3: GitHubTaskSource가 per-repo 큐 소유
    repos: HashMap<String, GitRepository>
    각 GitRepository가 issue_queue, pr_queue 소유

v4: ClawManager가 통합 큐 소유
    queue: TaskQueue
    Collector는 큐를 모름 — WorkItem을 반환만 함
```

---

## 5. ClawManager (DefaultTaskManager 대체)

### 역할

Daemon과 Collector 사이의 **중재자 + 스케줄러**:
1. Collector들에게서 WorkItem 수집 → TaskQueue에 ingest
2. Scheduler에게 큐 스냅샷 전달 → Decision 수신
3. Decision을 실행 (Task 생성, 이슈 생성, HITL 알림 등)

```rust
/// Claw 기반 TaskManager.
/// Collector → TaskQueue → Scheduler → Task 생성의 전체 흐름을 관리.
pub struct ClawManager {
    collectors: Vec<Box<dyn Collector>>,
    queue: TaskQueue,
    scheduler: Box<dyn Scheduler>,
    task_factory: TaskFactory,
    spec_db: Database,                // Spec CRUD
    ready_tasks: Vec<Box<dyn Task>>,  // Daemon이 가져갈 Task
    last_claw_invocation: Option<std::time::Instant>,
    claw_interval_secs: u64,
    force_claw_next_tick: bool,
}
```

### TaskManager trait 구현

ClawManager는 기존 TaskManager trait을 구현하여 **Daemon 코드 변경 최소화**:

```rust
#[async_trait(?Send)]
impl TaskManager for ClawManager {
    async fn tick(&mut self) {
        // 1. 모든 Collector에서 WorkItem 수집
        for collector in &mut self.collectors {
            let items = collector.collect().await;
            for item in items {
                self.queue.ingest(item);
            }
        }

        // 2. Scheduler 호출 (주기 확인)
        let decisions = if self.should_invoke_scheduler() {
            self.scheduler.schedule(&self.build_context()).await
        } else {
            Vec::new()  // 호출 사이에는 빈 결정
        };

        // 3. Decision 적용 → Task 생성
        for decision in decisions {
            self.log_decision(&decision);
            self.ready_tasks.extend(
                self.task_factory.apply(decision, &mut self.queue)
            );
        }
    }

    fn drain_ready(&mut self) -> Vec<Box<dyn Task>> {
        std::mem::take(&mut self.ready_tasks)
    }

    fn apply(&mut self, result: &TaskResult) {
        // Collector들에게 결과 반영
        for collector in &mut self.collectors {
            collector.apply(result);
        }
        // 큐에서 아이템 처리
        self.queue.apply_result(result);
    }

    fn active_items(&self) -> Vec<StatusItem> { ... }
}
```

---

## 6. Scheduler trait

### 정의

큐 전체 상태를 보고 판단하는 스케줄러. LLM 기반과 기계적 fallback 두 가지 구현체.

```rust
/// 큐 전체 상태를 평가하고 스케줄링 결정을 반환한다.
#[async_trait(?Send)]
pub trait Scheduler: Send {
    async fn schedule(&self, context: &ScheduleContext) -> Vec<ClawDecision>;
}
```

### ScheduleContext (Scheduler 입력)

```
ScheduleContext
├── specs: Vec<Spec>                  활성 스펙 (Mode 2)
├── queue_snapshot: QueueSnapshot     현재 큐 전체 상태 (소스 무관)
├── completed_history: Vec<CompletedItem>  최근 완료 이력
├── repos: Vec<RepoContext>           레포별 메타데이터
│     ├── repo_name, repo_url
│     ├── available_issue_slots
│     └── available_pr_slots
└── code_tree: Option<String>         파일 트리 (gap detection 시)
```

### ClawScheduler (LLM 구현체)

```rust
/// LLM 기반 스케줄러. Claude를 호출하여 판단.
pub struct ClawScheduler {
    agent: Arc<dyn Agent>,
    config: ClawConfig,
}

#[async_trait(?Send)]
impl Scheduler for ClawScheduler {
    async fn schedule(&self, context: &ScheduleContext) -> Vec<ClawDecision> {
        let prompt = self.build_prompt(context);
        let response = self.agent.invoke(AgentRequest {
            working_dir: PathBuf::from("."),
            prompt,
            session_opts: SessionOptions {
                json_schema: Some(CLAW_DECISION_SCHEMA.to_string()),
                ..Default::default()
            },
        }).await;
        self.parse_decisions(&response)
    }
}
```

### MechanicalScheduler (v3 호환 fallback)

```rust
/// v3의 drain_queue_items() 로직을 Scheduler trait으로 래핑.
/// Claw 비활성 시 사용. LLM 호출 없이 슬롯 기반 기계적 전이.
pub struct MechanicalScheduler;

#[async_trait(?Send)]
impl Scheduler for MechanicalScheduler {
    async fn schedule(&self, context: &ScheduleContext) -> Vec<ClawDecision> {
        let mut decisions = Vec::new();

        for repo in &context.repos {
            let mut issue_slots = repo.available_issue_slots;

            // Issue: Pending → Analyzing (FIFO, 슬롯 한도)
            for item in context.queue_snapshot.items_in_phase(&repo.repo_name, "issue", "Pending") {
                if issue_slots == 0 { break; }
                decisions.push(ClawDecision::advance(&item.work_id, "Pending", "Analyzing"));
                issue_slots -= 1;
            }

            // Issue: Ready → Implementing
            for item in context.queue_snapshot.items_in_phase(&repo.repo_name, "issue", "Ready") {
                if issue_slots == 0 { break; }
                decisions.push(ClawDecision::advance(&item.work_id, "Ready", "Implementing"));
                issue_slots -= 1;
            }

            // PR: 동일 패턴...
        }

        decisions
    }
}
```

**핵심**: MechanicalScheduler는 v3의 `drain_queue_items()`와 **동일한 판단 로직**을 Scheduler trait으로 표현한 것.
Claw 비활성 시 이 구현체가 사용되므로 v3 동작이 100% 보존됨.

---

## 7. TaskFactory

Decision을 Task로 변환하는 전담 구성 요소.

```rust
/// ClawDecision → Task 변환.
/// 기존 AnalyzeTask, ImplementTask 등의 생성 로직을 중앙 집중화.
pub struct TaskFactory {
    workspace: Arc<dyn WorkspaceOps>,
    gh: Arc<dyn Gh>,
    config: Arc<dyn ConfigLoader>,
    env: Arc<dyn Env>,
    git: Arc<dyn Git>,
    sw: Arc<dyn SuggestWorkflow>,
}

impl TaskFactory {
    /// Decision에 따라 Task를 생성하거나 사이드 이펙트를 실행.
    pub fn apply(
        &self,
        decision: ClawDecision,
        queue: &mut TaskQueue,
    ) -> Vec<Box<dyn Task>> {
        match decision.decision_type {
            ClawDecisionType::Advance { work_id, from_phase, to_phase } => {
                // 큐에서 아이템을 전이하고 적절한 Task 생성
                if let Some(item) = queue.advance(&work_id, &from_phase, &to_phase) {
                    vec![self.create_task(item, &to_phase)]
                } else {
                    vec![]
                }
            }
            ClawDecisionType::Decompose { spec_id, issues } => {
                // GitHub 이슈 생성 (다음 scan 사이클에서 자동 감지됨)
                self.create_issues(&spec_id, &issues);
                vec![]  // Task는 이슈 생성 후 다음 틱에서 생김
            }
            ClawDecisionType::DetectGap { spec_id, gap } => {
                self.create_gap_issue(&spec_id, &gap);
                vec![]
            }
            ClawDecisionType::Hitl { work_id, message, .. } => {
                self.post_hitl_comment(&work_id, &message);
                vec![]
            }
            ClawDecisionType::Complete { spec_id } => {
                self.complete_spec(&spec_id);
                vec![]
            }
            ClawDecisionType::Skip { .. } => vec![],
            ClawDecisionType::Replan { .. } => vec![],  // 큐 조정 (future)
        }
    }

    fn create_task(&self, item: WorkItem, phase: &str) -> Box<dyn Task> {
        match (item.item_type, phase) {
            (WorkItemType::Issue(i), "Analyzing") => Box::new(AnalyzeTask::new(..., i)),
            (WorkItemType::Issue(i), "Implementing") => Box::new(ImplementTask::new(..., i)),
            (WorkItemType::Pr(p), "Reviewing") => Box::new(ReviewTask::new(..., p)),
            (WorkItemType::Pr(p), "Improving") => Box::new(ImproveTask::new(..., p)),
            _ => unreachable!(),
        }
    }
}
```

---

## 8. Dual Mode

### Mode 1: Issue-driven (v3 호환)

```
claw.enabled: false → MechanicalScheduler 사용
claw.enabled: true, 스펙 없음 → ClawScheduler (Advance/Skip/Hitl만)

사람이 이슈 등록 → Collector 감지 → Queue → Scheduler 판단 → Task 실행
```

### Mode 2: Spec-driven (자율 루프)

```
claw.enabled: true, 활성 스펙 있음

스펙 등록 → ClawScheduler가 Decompose → 이슈 생성
→ Collector가 다음 틱에서 감지 → Queue
→ ClawScheduler가 Advance → Task 실행
→ 완료 후 ClawScheduler가 DetectGap → 추가 이슈
→ 모든 이슈 완료 + gap 없음 → Complete
```

### 하위호환

| 시나리오 | Scheduler | 동작 |
|---------|-----------|------|
| `claw.enabled: false` (기본값) | MechanicalScheduler | v3와 100% 동일 |
| `claw.enabled: true`, 스펙 없음 | ClawScheduler | Mode 1: 지능적 drain |
| `claw.enabled: true`, 스펙 있음 | ClawScheduler | Mode 2: 자율 루프 |

---

## 9. Spec (1급 엔티티)

### 정의

Spec은 **디자인 문서**를 데이터베이스에 저장한 엔티티다. 마크다운 형식의 설계 문서를 그대로 담으며, 해당 스펙에서 분해된 이슈들과 링크된다.

```
Spec
├── id: UUID
├── repo_id: 레포 FK
├── title: 스펙 제목
├── body: 마크다운 디자인 문서 전문
├── status: Active | Completed | Paused | Archived
├── source_path: 레포 내 원본 파일 경로 (옵션)
├── linked_issues: 이 스펙에서 생성된 GitHub 이슈 번호들
├── created_at, updated_at
```

### 상태 전이

```
                ┌─────────────┐
         등록   │   Active     │ ◄─── resume
                └──────┬──────┘
                       │
           ┌───────────┼───────────┐
           │           │           │
     Claw 판단    사람 일시정지   사람 보관
           │           │           │
           ▼           ▼           ▼
     ┌──────────┐ ┌────────┐ ┌──────────┐
     │Completed │ │ Paused │ │ Archived │
     └──────────┘ └────────┘ └──────────┘
```

### DB 스키마

```sql
CREATE TABLE IF NOT EXISTS specs (
    id          TEXT PRIMARY KEY,
    repo_id     TEXT NOT NULL REFERENCES repositories(id),
    title       TEXT NOT NULL,
    body        TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'active',
    source_path TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_specs_repo_status ON specs(repo_id, status);

CREATE TABLE IF NOT EXISTS spec_issues (
    spec_id      TEXT NOT NULL REFERENCES specs(id),
    issue_number INTEGER NOT NULL,
    created_at   TEXT NOT NULL,
    PRIMARY KEY (spec_id, issue_number)
);
```

### CLI

```bash
autodev spec add --title "Auth Module v2" --file ./DESIGN-auth.md --repo org/repo
autodev spec list [--repo org/repo] [--status active]
autodev spec show <spec-id>
autodev spec pause / resume <spec-id>
autodev spec evaluate <spec-id>    # Scheduler 즉시 실행
autodev spec decisions [--spec <spec-id>] [-n 20]
```

---

## 10. ClawDecision

### 구조

```rust
pub struct ClawDecision {
    pub decision_type: ClawDecisionType,
    pub reasoning: String,
    pub confidence: f64,
}

pub enum ClawDecisionType {
    Advance { work_id: String, from_phase: String, to_phase: String },
    Decompose { spec_id: String, issues: Vec<NewIssueRequest> },
    DetectGap { spec_id: String, gap: DetectedGap },
    Hitl { work_id: String, reason: HitlReason, message: String },
    Replan { spec_id: String, adjustments: Vec<ReplanAction> },
    Complete { spec_id: String },
    Skip { work_id: String },
}
```

### Decision Types

| Type | 설명 | Mode 1 | Mode 2 |
|------|------|--------|--------|
| **Advance** | 아이템을 다음 phase로 전이 | ✅ | ✅ |
| **Skip** | 이번 틱에서 건너뜀 | ✅ | ✅ |
| **Hitl** | HITL 요청 + GitHub 코멘트 | ✅ | ✅ |
| **Decompose** | 스펙 → 이슈 분해 | ❌ | ✅ |
| **DetectGap** | 설계-구현 gap → 이슈 생성 | ❌ | ✅ |
| **Replan** | 실행 계획 수정 | ❌ | ✅ |
| **Complete** | 스펙 완료 판정 | ❌ | ✅ |

HITL 사유:

| 사유 | 트리거 |
|------|--------|
| `ReviewFailingRepeatedly` | review_iteration ≥ 3 |
| `LowConfidence` | confidence < hitl_confidence_threshold |
| `ConflictDetected` | 같은 파일을 수정하는 이슈 동시 진행 |
| `AmbiguousRequirement` | 스펙의 특정 섹션이 모호 |

---

## 11. Claw 호출 전략

### 호출 주기

```
daemon tick: 10초 (기존)
scheduler 호출: 60초 (기본, 설정 가능)
gap detection: 3600초 (기본, code_tree 포함 여부로 제어)
```

### 호출 흐름

```
매 tick (10s):
  1. collectors.collect() → queue.ingest()  // 수집
  2. IF should_invoke_scheduler():
       context = build_context(queue, specs)
       decisions = scheduler.schedule(context)  // LLM or 기계적
       task_factory.apply(decisions)             // Task 생성
     ELSE:
       // 호출 사이에는 대기 (수집만 계속)
```

MechanicalScheduler는 매 틱마다 호출해도 비용 없음 (LLM 호출 없음).
ClawScheduler만 주기를 따르고, 사이에는 MechanicalScheduler로 fallback하거나 대기.

### 강제 트리거 (force_claw_next_tick)

| 이벤트 | 이유 |
|--------|------|
| 스펙 신규 등록 | 즉시 Decompose 시작 |
| 태스크 Failed 완료 | Replan 또는 HITL 판단 필요 |
| 스펙 연관 태스크 완료 | 다음 단계 판단 필요 |
| `autodev spec evaluate` 실행 | 사용자 수동 트리거 |

---

## 12. Decision 감사 로그

```sql
CREATE TABLE IF NOT EXISTS claw_decisions (
    id            TEXT PRIMARY KEY,
    repo_id       TEXT NOT NULL REFERENCES repositories(id),
    spec_id       TEXT REFERENCES specs(id),
    decision_json TEXT NOT NULL,
    created_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_claw_decisions_repo ON claw_decisions(repo_id, created_at);
```

---

## 13. Configuration

```yaml
claw:
  enabled: false                    # Claw 활성화 (기본: false → MechanicalScheduler)
  schedule_interval_secs: 60        # ClawScheduler 호출 주기 (초)
  model: "sonnet"                   # Claw용 LLM 모델
  max_context_tokens: 8000          # 컨텍스트 최대 토큰
  history_depth: 20                 # 완료 이력 포함 개수
  hitl_confidence_threshold: 0.5    # 이 confidence 이하면 HITL 요청
  gap_detection: false              # Gap Detection 활성화
  gap_detection_interval_secs: 3600 # Gap Detection 주기 (초)
```

---

## 14. Claw Prompt 구조

### System Prompt

```
You are Claw, an autonomous development scheduler.
You receive a snapshot of the current development state and return structured
scheduling decisions as a JSON array.

Rules:
1. Return valid JSON matching the ClawDecision schema.
2. Respect concurrency limits per repository.
3. Do not advance more items than available slots.
4. For items with review_iteration >= 3, consider HITL instead of another cycle.
5. When a spec has active gaps, prioritize gap issues before new decomposition.
6. Provide reasoning for every decision.
7. If no action is needed, return an empty array.
```

### User Prompt

```
## Repositories:
{for each repo: name, available_issue_slots, available_pr_slots}

## Active Specs:
{for each spec: title, body (truncated), linked issues + phases}

## Queue State:
### Issues:
| # | Repo | Title | Phase | Labels | Source |
{for each issue item — source-agnostic view}

### PRs:
| # | Repo | Title | Phase | Iteration | Source Issue |
{for each PR item}

## Completed History (last {N}):
| Work ID | Task | Status | Duration |

## Code Tree (if gap detection due):
{file listing}

Return a JSON array of ClawDecision objects.
```

---

## 15. Label-Positive 모델과의 관계

Claw는 Label-Positive 모델을 **확장하지만 위배하지 않는다**.

- Decompose/DetectGap으로 생성된 이슈에 `autodev:analyze` 라벨 추가 → 기존 scan에서 감지
- Advance는 기존 phase 전이와 동일
- **새로운 라벨 없음**. Claw는 기존 라벨 체계만 사용.

---

## 16. End-to-End Flow

```
┌──────────────────────────────────────────────────────────────────────┐
│                      DAEMON LOOP                                      │
│                                                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ 1. COLLECT (Collector들이 소스별 수집)                          │  │
│  │    GitHubCollector: recovery → scan → Vec<WorkItem>           │  │
│  │    (SlackCollector: poll → Vec<WorkItem>)                     │  │
│  │    → ClawManager.queue.ingest()                               │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                            │                                        │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ 2. SCHEDULE (Scheduler가 큐 전체를 보고 판단)                   │  │
│  │    ClawScheduler (LLM) or MechanicalScheduler (fallback)      │  │
│  │    → Vec<ClawDecision>                                        │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                            │                                        │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ 3. EXECUTE (TaskFactory가 Decision → Task 변환)                │  │
│  │    Advance  → Task 생성 (AnalyzeTask, ImplementTask 등)       │  │
│  │    Decompose → gh.create_issue + spec_link_issue              │  │
│  │    DetectGap → gh.create_issue (autodev:analyze 라벨)         │  │
│  │    Hitl      → gh.post_comment                                │  │
│  │    Complete  → spec.status = Completed                        │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                            │                                        │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ 4. TASK EXECUTION (v3와 동일)                                 │  │
│  │    AnalyzeTask, ImplementTask, ReviewTask, ImproveTask,       │  │
│  │    ExtractTask                                                │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                            │                                        │
│                      sleep(tick) → loop                              │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 17. Trait 정의 요약

```rust
// Collector: 소스별 수집기
#[async_trait(?Send)]
pub trait Collector: Send {
    async fn collect(&mut self) -> Vec<WorkItem>;
    fn apply(&mut self, result: &TaskResult);
    fn active_items(&self) -> Vec<StatusItem>;
    fn source_name(&self) -> &str;
}

// Scheduler: 큐 전체를 보고 판단
#[async_trait(?Send)]
pub trait Scheduler: Send {
    async fn schedule(&self, context: &ScheduleContext) -> Vec<ClawDecision>;
}

// SpecRepository: Spec persistence
pub trait SpecRepository {
    fn spec_create(&self, ...) -> Result<String>;
    fn spec_update_status(&self, ...) -> Result<()>;
    fn spec_find_active(&self, repo_id: &str) -> Result<Vec<Spec>>;
    fn spec_link_issue(&self, ...) -> Result<()>;
    fn claw_log_decision(&self, ...) -> Result<()>;
}

// TaskManager: 기존 trait 유지 (Daemon 변경 최소화)
#[async_trait(?Send)]
pub trait TaskManager: Send {
    async fn tick(&mut self);
    fn drain_ready(&mut self) -> Vec<Box<dyn Task>>;
    fn pop_ready(&mut self) -> Option<Box<dyn Task>>;
    fn apply(&mut self, result: &TaskResult);
    fn active_items(&self) -> Vec<StatusItem>;
}
```

---

## 18. Module 구조

```
cli/src/
├── claw/                           # NEW 모듈
│   ├── mod.rs
│   ├── models.rs                   # WorkItem, ClawDecision, Spec, ScheduleContext
│   ├── manager.rs                  # ClawManager (TaskManager impl)
│   ├── scheduler.rs                # Scheduler trait
│   ├── scheduler_claw.rs           # ClawScheduler (LLM 구현체)
│   ├── scheduler_mechanical.rs     # MechanicalScheduler (v3 fallback)
│   ├── task_factory.rs             # Decision → Task 변환
│   ├── task_queue.rs               # TaskQueue (통합 큐)
│   ├── repository.rs               # SpecRepository trait
│   └── context_builder.rs          # ScheduleContext 빌드
├── collectors/                     # RENAME: sources/ → collectors/
│   ├── mod.rs
│   ├── collector.rs                # Collector trait
│   └── github.rs                   # GitHubCollector (was: GitHubTaskSource)
├── queue/
│   ├── schema.rs                   # ADD: specs, spec_issues, claw_decisions DDL
│   ├── repository.rs               # ADD: SpecRepository impl
│   └── state_queue.rs              # 기존 유지
├── config/
│   └── models.rs                   # ADD: ClawConfig
├── daemon/
│   ├── mod.rs                      # MODIFY: ClawManager 사용
│   ├── task.rs                     # 기존 유지
│   ├── task_manager.rs             # 기존 trait 유지
│   ├── task_runner.rs              # 기존 유지
│   └── task_runner_impl.rs         # 기존 유지
├── tasks/                          # 기존 유지 (변경 없음)
│   ├── analyze.rs
│   ├── implement.rs
│   ├── review.rs
│   ├── improve.rs
│   └── extract.rs
├── client/
│   └── mod.rs                      # ADD: spec_* CLI 핸들러
└── main.rs                         # ADD: Spec 서브커맨드
```

---

## 19. 구현 Phase

### Phase 1: 모델 + 트레이트 (additive only)

- `claw/models.rs` — WorkItem, Spec, ScheduleContext, ClawDecision 등 DTO
- `claw/scheduler.rs` — Scheduler trait
- `claw/repository.rs` — SpecRepository trait
- `collectors/collector.rs` — Collector trait
- `config/models.rs` — ClawConfig 추가
- `queue/schema.rs` — 3개 테이블 DDL

### Phase 2: MechanicalScheduler + TaskQueue (v3 동작 보존)

- `claw/task_queue.rs` — TaskQueue 구현 + 테스트
- `claw/scheduler_mechanical.rs` — v3 drain 로직 래핑 + 테스트
- `claw/task_factory.rs` — Decision → Task 변환 + 테스트
- **검증**: 기존 drain_queue_items()와 동일한 결과

### Phase 3: GitHubCollector 리팩토링

- `collectors/github.rs` — GitHubTaskSource → GitHubCollector
- drain_queue_items() 제거, WorkItem 반환으로 변경
- `claw/manager.rs` — ClawManager 구현 (MechanicalScheduler 사용)
- **검증**: 기존 테스트 전부 통과

### Phase 4: Persistence + CLI

- `queue/repository.rs` — SpecRepository impl
- `client/mod.rs` + `main.rs` — Spec 서브커맨드

### Phase 5: ClawScheduler (LLM)

- `claw/scheduler_claw.rs` — LLM 기반 구현체
- 프롬프트 설계 + JSON Schema
- `claw/manager.rs` — claw.enabled에 따라 Scheduler 선택

### Phase 6: Gap Detection

- Gap detection 프롬프트 + code_tree 빌드
- ScheduleContext에 code_tree 포함 주기 제어

---

## 20. Scope 외

- **병렬 에이전트 실행**: worktree 격리 기반 (#235)
- **외부 알림**: Slack/이메일
- **Jira 연동**: Jira 티켓 자동 생성
- **테스트 환경 트리거**: Docker 기반 e2e/unit
- **스펙 버전 관리**: diff 기반 재분해
- **SlackCollector, JiraCollector**: Collector trait 구현 (v4 아키텍처가 지원하지만 구현은 후속)
