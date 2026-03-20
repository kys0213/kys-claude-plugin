# Design: Lifecycle Hooks

> DataSource 추가 시 구현해야 할 인터페이스를 최소화하는 설계.

---

## 설계 목표: DataSource 추가자의 인지 부하 최소화

```
새 DataSource 추가자가 알아야 할 것:

  현재 (Collector)                      목표 (DataSource + LifecycleHook)
  ┌──────────────────────────┐          ┌──────────────────────────┐
  │ ✅ 외부 API 호출          │          │ ✅ 외부 API 호출          │
  │ ✅ QueueItem 생성         │          │ ✅ QueueItem 생성         │
  │ ❌ Task 구현체 선택 로직   │          │ ✅ 전이별 부수효과        │
  │ ❌ TaskKind ↔ Task 매핑   │          └──────────────────────────┘
  │ ❌ StateQueue 직접 조작   │
  │ ❌ apply() 시 라벨 변경   │          framework이 처리:
  └──────────────────────────┘          ┌──────────────────────────┐
                                        │ TaskFactory: Kind→Task   │
                                        │ StateQueue 조작          │
                                        │ concurrency 관리         │
                                        └──────────────────────────┘
```

---

## DataSource 추가 시 구현할 인터페이스 (2개)

### 1. ItemSource trait — "외부에서 QueueItem을 가져온다"

```rust
/// 외부 시스템에서 작업 아이템을 수집하는 인터페이스.
///
/// Collector에서 Task 생성/큐 조작 책임을 분리한 순수 수집 인터페이스.
/// QueueItem만 반환하면 되고, Task 생성은 framework(TaskFactory)가 담당한다.
#[async_trait]
pub trait ItemSource: Send {
    /// 소스 이름 (로깅용)
    fn name(&self) -> &str;

    /// 외부 시스템을 스캔하여 새 QueueItem을 반환한다.
    /// framework이 dedup 처리하므로 중복 반환해도 안전하다.
    async fn poll(&mut self) -> Vec<QueueItem>;
}
```

**GitHub 구현 예시:**
```rust
impl ItemSource for GitHubItemSource {
    fn name(&self) -> &str { "github" }

    async fn poll(&mut self) -> Vec<QueueItem> {
        let issues = self.gh.list_issues(&self.repo, &self.labels).await;
        issues.into_iter()
            .map(|i| QueueItem::from_issue(&self.repo_ref, &i, TaskKind::Analyze))
            .collect()
    }
}
```

**Jira 구현 예시:**
```rust
impl ItemSource for JiraItemSource {
    fn name(&self) -> &str { "jira" }

    async fn poll(&mut self) -> Vec<QueueItem> {
        let issues = self.client.search("project = FOO AND status = Open").await;
        issues.into_iter()
            .map(|i| QueueItem::from_external(/* ... */))
            .collect()
    }
}
```

**비교:**
```
현재 Collector trait (4개 메서드, Task 생성 필요):
  poll()         → Vec<Box<dyn Task>>   ← Task 구현체를 직접 생성해야 함
  drain_tasks()  → Vec<Box<dyn Task>>   ← 큐 조작 + Task 생성
  apply()        → 결과 반영             ← 큐 조작 + 부수효과 혼재
  active_items() → 상태 리포트

새 ItemSource trait (2개 메서드, QueueItem만):
  name()  → &str
  poll()  → Vec<QueueItem>              ← DTO만 반환하면 끝
```

### 2. LifecycleHook trait — "전이 시 뭘 할지 정의한다"

```rust
/// 상태 전이 시점에 실행되는 hook.
/// DataSource별로 구현하여, 전이 시 부수효과를 정의한다.
#[async_trait]
pub trait LifecycleHook: Send + Sync {
    /// Hook 이름 (로깅/디버깅용)
    fn name(&self) -> &str;

    /// 전이 전 실행. Deny 반환 시 전이를 중단한다.
    async fn before_transition(&self, t: &Transition) -> HookDecision {
        HookDecision::Allow  // 기본: 허용
    }

    /// 전이 후 실행. 부수효과만 수행한다.
    async fn after_transition(&self, t: &Transition) {
        // 기본: no-op
    }
}
```

**3개 메서드, 2개는 default 있음. 실질적으로 구현할 것은 `after_transition` 하나.**

---

## DataSource 추가 비교표

```
                    GitHub          Jira            Slack
                    ──────          ────            ─────
ItemSource
  poll()            API 스캔         JQL 쿼리        이모지 스캔
  반환값             QueueItem       QueueItem       QueueItem

LifecycleHook
  before             충돌 검사        Block 검사      -
  after              라벨+코멘트      상태 전이        이모지 변경

Task 구현체          재사용 ←─────── 재사용 ←──────── 재사용
(AnalyzeTask 등)    (변경 없음)     (변경 없음)     (변경 없음)

core 변경            없음            없음            없음
```

---

## Framework 컴포넌트 (DataSource 추가자가 모르는 영역)

### TaskFactory — TaskKind → Task 매핑

```rust
/// QueueItem의 TaskKind를 보고 적절한 Task 구현체를 생성한다.
/// DataSource 추가자는 이 로직을 알 필요 없다.
pub struct TaskFactory { /* 의존성 주입 */ }

impl TaskFactory {
    pub fn create(&self, item: QueueItem) -> Box<dyn Task> {
        match item.task_kind {
            TaskKind::Analyze   => Box::new(AnalyzeTask::new(/* ... */, item)),
            TaskKind::Implement => Box::new(ImplementTask::new(/* ... */, item)),
            TaskKind::Review    => Box::new(ReviewTask::new(/* ... */, item)),
            TaskKind::Improve   => Box::new(ImproveTask::new(/* ... */, item)),
            TaskKind::Extract   => Box::new(ExtractTask::new(/* ... */, item)),
        }
    }
}
```

현재 `GitHubTaskSource.drain_ready_tasks()`에 흩어진 매핑 로직을 한 곳으로 모은다.

### TransitionExecutor — hook + 전이 조합

```rust
/// hook 실행 + 상태 전이를 조합하는 오케스트레이터.
pub struct TransitionExecutor {
    hooks: HookRegistry,
}

impl TransitionExecutor {
    /// hook-aware 상태 전이.
    ///
    /// 1. before hook 실행 → Deny면 전이 중단
    /// 2. StateQueue.transit() 실행
    /// 3. after hook 실행
    pub async fn transit(
        &self,
        queue: &mut StateQueue<QueueItem>,
        id: &str,
        from: QueuePhase,
        to: QueuePhase,
        context: TransitionContext,
    ) -> Result<bool, HookDenied> {
        let transition = Transition {
            from, to,
            work_id: id.to_string(),
            repo_name: context.repo_name,
            task_kind: context.task_kind,
            queue_type: context.queue_type,
            result: context.result,
        };

        // 1. before hooks
        if let HookDecision::Deny(reason) = self.hooks.run_before(&transition).await {
            return Err(HookDenied(reason));
        }

        // 2. 실제 전이
        let ok = queue.transit(id, from, to);

        // 3. after hooks (전이 성공 시에만)
        if ok {
            self.hooks.run_after(&transition).await;
        }

        Ok(ok)
    }
}
```

### HookRegistry

```rust
/// LifecycleHook 목록을 관리하고 순서대로 실행한다.
pub struct HookRegistry {
    hooks: Vec<Box<dyn LifecycleHook>>,
}

impl HookRegistry {
    pub fn new() -> Self { ... }
    pub fn register(&mut self, hook: Box<dyn LifecycleHook>) { ... }

    /// before hooks 순차 실행. 하나라도 Deny → 즉시 중단.
    pub async fn run_before(&self, t: &Transition) -> HookDecision { ... }

    /// after hooks 순차 실행. 개별 실패는 로깅만, 나머지 계속.
    pub async fn run_after(&self, t: &Transition) { ... }
}
```

---

## Supporting Types

### Transition

```rust
pub struct Transition {
    pub from: QueuePhase,
    pub to: QueuePhase,
    pub work_id: String,
    pub repo_name: String,
    pub task_kind: TaskKind,
    pub queue_type: QueueType,
    /// Running→Done/Failed 전이 시에만 존재
    pub result: Option<TransitionResult>,
}

pub enum TransitionResult {
    Completed,
    Failed(String),
    Skipped(String),
}
```

### HookDecision

```rust
pub enum HookDecision {
    Allow,
    Deny(String),
}
```

---

## Hook 실행 순서 및 에러 처리

### before hooks

- **등록 순서대로** 실행
- **하나라도 Deny** → 즉시 중단, 이후 hook은 실행하지 않음
- **panic/에러** → Deny로 처리 (fail-safe)

### after hooks

- **등록 순서대로** 실행
- **개별 실패는 로깅만** 하고 나머지 hook 계속 실행
- **하나의 hook 실패가 다른 hook에 영향주지 않음**

### 등록 순서 가이드

```rust
// 권장 등록 순서 (cross-cutting 먼저, DataSource 다음, 알림 마지막)
registry.register(Box::new(LoggingLifecycleHook::new(db)));        // 1. 로깅
registry.register(Box::new(GitHubLifecycleHook::new(gh)));         // 2. DataSource
registry.register(Box::new(EscalationLifecycleHook::new(db)));     // 3. 에스컬레이션
registry.register(Box::new(NotificationLifecycleHook::new(disp))); // 4. 알림
```

---

## 파일 구조

```
src/core/
  lifecycle.rs            # LifecycleHook, HookDecision, Transition, HookRegistry
  item_source.rs          # ItemSource trait
  mod.rs                  # pub mod lifecycle, item_source

src/service/daemon/
  task_factory.rs         # TaskFactory (TaskKind → Task 매핑)
  transition_executor.rs  # TransitionExecutor (hook + 전이 조합)
  hooks/
    mod.rs                # pub mod github, notification, escalation, logging
    github.rs             # GitHubLifecycleHook
    notification.rs       # NotificationLifecycleHook
    escalation.rs         # EscalationLifecycleHook
    logging.rs            # LoggingLifecycleHook
  sources/
    mod.rs                # pub mod github, (jira, slack, ...)
    github.rs             # GitHubItemSource (기존 GitHubTaskSource에서 분리)
  mod.rs
```

---

## 테스트 전략

- `HookRegistry`: 인메모리 mock hook으로 실행 순서, Deny 전파 검증
- `TransitionExecutor`: mock hook + StateQueue 조합 통합 검증
- `TaskFactory`: TaskKind 전수 매핑 검증
- `ItemSource` 구현체: mock API로 QueueItem 생성 검증
- 각 Hook 구현체: mock Gh/Notifier로 부수효과 호출 검증
