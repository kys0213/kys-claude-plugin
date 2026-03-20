# Design: Lifecycle Hooks

---

## DataSource가 구현할 인터페이스 (2개)

### 1. ItemSource — "큐에 넣는다"

```rust
#[async_trait]
pub trait ItemSource: Send {
    fn name(&self) -> &str;
    async fn poll(&mut self) -> Vec<QueueItem>;
}
```

구현 예시:

```rust
// GitHub
impl ItemSource for GitHubItemSource {
    fn name(&self) -> &str { "github" }
    async fn poll(&mut self) -> Vec<QueueItem> {
        let issues = self.gh.list_issues(&self.repo, &self.labels).await;
        issues.into_iter()
            .map(|i| QueueItem::from_issue(&self.repo_ref, &i, TaskKind::Analyze))
            .collect()
    }
}

// Jira
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

- 2개 메서드
- QueueItem만 반환하면 끝
- framework이 dedup 처리하므로 중복 반환해도 안전

### 2. LifecycleHook — "전이 시 반영한다"

```rust
#[async_trait]
pub trait LifecycleHook: Send + Sync {
    fn name(&self) -> &str;

    async fn before_transition(&self, t: &Transition) -> HookDecision {
        HookDecision::Allow  // default
    }

    async fn after_transition(&self, t: &Transition) {
        // default: no-op
    }
}
```

- 3개 메서드, 2개는 default
- 실질적으로 `after_transition` 하나만 구현

---

## Framework 컴포넌트 (DataSource 추가자가 모르는 영역)

### TransitionExecutor

hook 실행 + 상태 전이를 조합한다.

```rust
pub struct TransitionExecutor {
    hooks: HookRegistry,
}

impl TransitionExecutor {
    pub async fn transit(
        &self,
        queue: &mut StateQueue<QueueItem>,
        id: &str,
        from: QueuePhase,
        to: QueuePhase,
        context: TransitionContext,
    ) -> Result<bool, HookDenied> {
        let t = Transition { from, to, /* ... */ };

        // 1. before — Deny면 중단
        if let HookDecision::Deny(reason) = self.hooks.run_before(&t).await {
            return Err(HookDenied(reason));
        }

        // 2. 전이
        let ok = queue.transit(id, from, to);

        // 3. after — 전이 성공 시에만
        if ok { self.hooks.run_after(&t).await; }

        Ok(ok)
    }
}
```

### HookRegistry

```rust
pub struct HookRegistry {
    hooks: Vec<Box<dyn LifecycleHook>>,
}

impl HookRegistry {
    pub fn register(&mut self, hook: Box<dyn LifecycleHook>);

    /// before: 순차 실행, 하나라도 Deny → 즉시 중단
    pub async fn run_before(&self, t: &Transition) -> HookDecision;

    /// after: 순차 실행, 개별 실패는 로깅만 하고 계속
    pub async fn run_after(&self, t: &Transition);
}
```

### TaskFactory

```rust
pub struct TaskFactory { /* deps */ }

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

현재 `GitHubTaskSource.drain_ready_tasks()`에 흩어진 매핑을 한 곳으로 모은다.

---

## Supporting Types

```rust
pub struct Transition {
    pub from: QueuePhase,
    pub to: QueuePhase,
    pub work_id: String,
    pub repo_name: String,
    pub task_kind: TaskKind,
    pub queue_type: QueueType,
    pub result: Option<TransitionResult>,  // Running→Done/Failed 시에만
}

pub enum TransitionResult {
    Completed,
    Failed(String),
    Skipped(String),
}

pub enum HookDecision {
    Allow,
    Deny(String),
}
```

---

## Hook 등록 순서

```rust
registry.register(Box::new(LoggingLifecycleHook::new(db)));        // 1. 로깅
registry.register(Box::new(GitHubLifecycleHook::new(gh)));         // 2. DataSource
registry.register(Box::new(EscalationLifecycleHook::new(db)));     // 3. 에스컬레이션
registry.register(Box::new(NotificationLifecycleHook::new(disp))); // 4. 알림
```

- before: 등록 순서대로, Deny 즉시 중단, panic → Deny (fail-safe)
- after: 등록 순서대로, 개별 실패 무시, 나머지 계속

---

## 파일 구조

```
src/core/
  lifecycle.rs            # LifecycleHook, HookDecision, Transition, HookRegistry
  item_source.rs          # ItemSource trait

src/service/daemon/
  task_factory.rs         # TaskFactory
  transition_executor.rs  # TransitionExecutor
  hooks/
    github.rs             # GitHubLifecycleHook
    notification.rs       # NotificationLifecycleHook
    escalation.rs         # EscalationLifecycleHook
    logging.rs            # LoggingLifecycleHook
  sources/
    github.rs             # GitHubItemSource
```
