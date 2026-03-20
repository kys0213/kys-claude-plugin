# Design: Lifecycle Hooks

> trait/struct 정의, 실행 순서, 에러 처리 규칙.

---

## Core Types

### LifecycleHook trait

```rust
/// 상태 전이 시점에 실행되는 hook.
/// DataSource별로 구현하여, 전이 시 부수효과를 정의한다.
#[async_trait]
pub trait LifecycleHook: Send + Sync {
    /// Hook 이름 (로깅/디버깅용)
    fn name(&self) -> &str;

    /// 전이 전 실행. Deny 반환 시 전이를 중단한다.
    async fn before_transition(
        &self,
        transition: &Transition,
    ) -> HookDecision {
        // 기본: 허용
        HookDecision::Allow
    }

    /// 전이 후 실행. 부수효과만 수행한다.
    async fn after_transition(
        &self,
        transition: &Transition,
    ) {
        // 기본: no-op
    }
}
```

### Transition (전이 컨텍스트)

```rust
/// 상태 전이에 대한 컨텍스트 정보.
/// hook이 전이의 세부 사항을 알 수 있도록 한다.
pub struct Transition {
    pub from: QueuePhase,
    pub to: QueuePhase,
    pub work_id: String,
    pub repo_name: String,
    pub task_kind: TaskKind,
    pub queue_type: QueueType,
    /// Task 실행 결과 (Running→Done/Failed 전이 시에만 존재)
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
    /// 전이 허용
    Allow,
    /// 전이 거부 (사유 포함). before_transition에서만 의미 있음.
    Deny(String),
}
```

---

## HookRegistry

```rust
/// LifecycleHook 목록을 관리하고 순서대로 실행한다.
pub struct HookRegistry {
    hooks: Vec<Box<dyn LifecycleHook>>,
}

impl HookRegistry {
    pub fn new() -> Self { ... }

    pub fn register(&mut self, hook: Box<dyn LifecycleHook>) { ... }

    /// 모든 hook의 before_transition을 순서대로 실행한다.
    /// 하나라도 Deny를 반환하면 즉시 중단하고 Deny를 반환한다.
    pub async fn run_before(&self, transition: &Transition) -> HookDecision { ... }

    /// 모든 hook의 after_transition을 순서대로 실행한다.
    /// 개별 hook의 실패는 로깅만 하고 나머지를 계속 실행한다.
    pub async fn run_after(&self, transition: &Transition) { ... }
}
```

---

## TransitionExecutor

```rust
/// hook 실행 + 상태 전이를 조합하는 오케스트레이터.
pub struct TransitionExecutor<T: HasWorkId> {
    queue: StateQueue<T>,
    hooks: HookRegistry,
}

impl<T: HasWorkId + Clone> TransitionExecutor<T> {
    /// hook-aware 상태 전이.
    ///
    /// 1. before hook 실행 → Deny면 전이 중단
    /// 2. StateQueue.transit() 실행
    /// 3. after hook 실행
    pub async fn transit(
        &mut self,
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
        match self.hooks.run_before(&transition).await {
            HookDecision::Allow => {}
            HookDecision::Deny(reason) => return Err(HookDenied(reason)),
        }

        // 2. 실제 전이
        let ok = self.queue.transit(id, from, to);

        // 3. after hooks (전이 성공 시에만)
        if ok {
            self.hooks.run_after(&transition).await;
        }

        Ok(ok)
    }
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
// 권장 등록 순서
registry.register(Box::new(LoggingLifecycleHook::new(db)));        // 1. 로깅 (항상 먼저)
registry.register(Box::new(GitHubLifecycleHook::new(gh)));         // 2. DataSource 부수효과
registry.register(Box::new(EscalationLifecycleHook::new(db)));     // 3. 에스컬레이션
registry.register(Box::new(NotificationLifecycleHook::new(disp))); // 4. 알림 (항상 마지막)
```

---

## 파일 구조

```
src/core/
  lifecycle.rs          # LifecycleHook trait, HookDecision, Transition, HookRegistry
  mod.rs                # pub mod lifecycle 추가

src/service/daemon/
  hooks/
    mod.rs              # pub mod github, notification, escalation, logging
    github.rs           # GitHubLifecycleHook
    notification.rs     # NotificationLifecycleHook
    escalation.rs       # EscalationLifecycleHook
    logging.rs          # LoggingLifecycleHook
  transition_executor.rs  # TransitionExecutor
  mod.rs                # pub mod hooks, transition_executor 추가
```

---

## 테스트 전략

- `HookRegistry`: 인메모리 mock hook으로 before/after 실행 순서, Deny 전파 검증
- `TransitionExecutor`: mock hook + StateQueue 조합으로 전이+hook 통합 검증
- 각 Hook 구현체: mock Gh/Notifier로 부수효과 호출 검증
