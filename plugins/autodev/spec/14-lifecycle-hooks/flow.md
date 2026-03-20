# 14. Lifecycle Hooks

> 상태 전이 시점마다 DataSource별 before/after hook을 실행하여,
> core 로직 수정 없이 부수효과를 정의할 수 있는 확장 포인트.

---

## 배경 및 동기

### 현재 문제

1. **부수효과가 Task 구현체에 하드코딩됨**
   - `AnalyzeTask.after_invoke()` 안에서 GitHub 라벨 변경, 코멘트 작성 등을 직접 수행
   - DataSource가 바뀌면 (GitHub → Jira, Slack 등) Task 코드를 수정해야 함 → OCP 위반

2. **상태 전이 시점에 대한 공통 관심사가 흩어져 있음**
   - Notification: Daemon main loop에서 직접 호출
   - 라벨 변경: Task 내부 + Collector.apply()
   - 로깅: Daemon main loop에서 직접 수행
   - 에스컬레이션: Daemon main loop에서 직접 수행

3. **Collector.apply()는 완료 시점 한 곳에서만 동작**
   - Pending→Ready, Ready→Running 전이 시점에는 hook이 없음
   - 예: "Running 진입 시 라벨을 autodev:wip으로 변경" 같은 동작을 넣을 곳이 없음

### 목표

```
상태 전이(Pending→Ready→Running→Done/Failed)의 각 시점에
DataSource별 before/after hook을 실행할 수 있는 확장 포인트를 제공한다.
```

- **before hook**: 전이 전 실행. 실패 시 전이를 중단(가드)할 수 있음
- **after hook**: 전이 후 실행. 부수효과만 수행 (전이 결과에 영향 없음)

---

## 상태 전이 모델

```
         ┌──────────────────────────────────────────────────┐
         │                Queue Phase 전이                    │
         │                                                    │
         │  Pending ──► Ready ──► Running ──► Done            │
         │                                    ──► Failed      │
         │                                    ──► Skipped     │
         │                                                    │
         │  각 전이마다:                                       │
         │    before(from, to, item) → Allow / Deny(reason)   │
         │    [전이 실행]                                      │
         │    after(from, to, item)  → 부수효과                │
         └──────────────────────────────────────────────────┘
```

### 전이 시점 (Transition)

| 전이 | 트리거 | before 용도 | after 용도 |
|------|--------|-------------|-----------|
| `Pending → Ready` | Claw evaluate (advance) | 선행 조건 검사 (PR 충돌 여부 등) | 라벨 변경, 알림 |
| `Ready → Running` | Daemon task spawn | 리소스 가용성 검사 | 라벨 변경 (wip), 코멘트 |
| `Running → Done` | Task 완료 | - | 라벨 변경 (done), 코멘트, 알림 |
| `Running → Failed` | Task 실패 | - | 라벨 변경 (failed), 에스컬레이션, 알림 |
| `Running → Skipped` | Preflight skip | - | 라벨 제거, 로깅 |

---

## 핵심 설계

### 1. LifecycleHook trait

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

### 2. Transition (전이 컨텍스트)

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

### 3. HookDecision

```rust
pub enum HookDecision {
    /// 전이 허용
    Allow,
    /// 전이 거부 (사유 포함). before_transition에서만 의미 있음.
    Deny(String),
}
```

### 4. HookRegistry

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

## DataSource별 Hook 구현 예시

### GitHubLifecycleHook

```rust
pub struct GitHubLifecycleHook {
    gh: Arc<dyn Gh>,
}

#[async_trait]
impl LifecycleHook for GitHubLifecycleHook {
    fn name(&self) -> &str { "github" }

    async fn before_transition(&self, t: &Transition) -> HookDecision {
        match (t.from, t.to) {
            // Ready→Running 전이 시: PR 충돌 검사
            (QueuePhase::Ready, QueuePhase::Running) => {
                if self.has_merge_conflict(t).await {
                    HookDecision::Deny("PR has merge conflicts".into())
                } else {
                    HookDecision::Allow
                }
            }
            _ => HookDecision::Allow,
        }
    }

    async fn after_transition(&self, t: &Transition) {
        match (t.from, t.to) {
            (QueuePhase::Pending, QueuePhase::Ready) => {
                self.set_label(t, "autodev:ready").await;
            }
            (QueuePhase::Ready, QueuePhase::Running) => {
                self.set_label(t, "autodev:wip").await;
                self.add_comment(t, "작업을 시작합니다.").await;
            }
            (QueuePhase::Running, QueuePhase::Done) => {
                self.set_label(t, "autodev:done").await;
                self.add_comment(t, "작업이 완료되었습니다.").await;
            }
            (QueuePhase::Running, QueuePhase::Failed) => {
                self.set_label(t, "autodev:failed").await;
            }
            _ => {}
        }
    }
}
```

### NotificationLifecycleHook

```rust
/// 기존 Notifier를 hook으로 래핑하여,
/// 상태 전이 시 자동으로 알림을 발송한다.
pub struct NotificationLifecycleHook {
    dispatcher: NotificationDispatcher,
}

#[async_trait]
impl LifecycleHook for NotificationLifecycleHook {
    fn name(&self) -> &str { "notification" }

    async fn after_transition(&self, t: &Transition) {
        match (t.from, t.to) {
            (QueuePhase::Running, QueuePhase::Failed) => {
                if let Some(TransitionResult::Failed(ref msg)) = t.result {
                    let event = NotificationEvent::from_task_failed(
                        &t.work_id, &t.repo_name, msg,
                    );
                    self.dispatcher.dispatch(&event).await;
                }
            }
            _ => {}
        }
    }
}
```

---

## 통합 지점

### StateQueue 확장

`StateQueue.transit()` 자체는 변경하지 않는다. hook 실행은 **StateQueue를 호출하는 상위 계층**에서 수행한다.

```
현재:  StateQueue.transit(id, from, to)
변경:  TransitionExecutor가 hook 실행 + StateQueue.transit() 조합
```

### TransitionExecutor

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

### Daemon main loop 변경

기존 Daemon main loop에 흩어진 부수효과를 hook으로 이동:

```
현재 (Daemon main loop):
  task 완료 → manager.apply() → log_insert() → escalation → notification

변경 후 (Daemon main loop):
  task 완료 → transit(Running→Done/Failed)  ← hook이 모든 부수효과 수행
```

| 현재 위치 | hook 이동 후 |
|-----------|-------------|
| Daemon: `dispatch_notification()` | `NotificationLifecycleHook.after_transition()` |
| Daemon: `escalation::escalate()` | `EscalationLifecycleHook.after_transition()` |
| Daemon: `log_insert()` + `usage_insert()` | `LoggingLifecycleHook.after_transition()` |
| Task: GitHub 라벨 변경 | `GitHubLifecycleHook.after_transition()` |
| Task: GitHub 코멘트 작성 | `GitHubLifecycleHook.after_transition()` |

---

## before hook의 Deny 처리

before hook이 Deny를 반환하면:

1. **전이를 중단**한다 (StateQueue.transit() 호출하지 않음)
2. **HITL 이벤트를 생성**한다 (사람의 개입이 필요한 상황)
3. **아이템은 현재 phase에 그대로 남는다**

```
예시 흐름:

  Claw: "issue:org/repo:42를 Ready→Running으로 advance"
    → before_transition() 호출
    → GitHubLifecycleHook: "PR에 충돌 있음" → Deny
    → 전이 중단, HITL 생성
    → 아이템은 Ready에 그대로 남음
    → 다음 Claw evaluate에서 재시도 가능
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

## 점진적 마이그레이션 전략

한번에 모든 부수효과를 hook으로 옮기지 않는다. 단계적으로 진행한다.

### Phase 1: Infrastructure

1. `LifecycleHook` trait + `HookRegistry` + `TransitionExecutor` 정의
2. `HookRegistry`에 빈 상태로 Daemon에 주입
3. 기존 동작 변경 없음 (hook이 없으면 기존과 동일하게 동작)

### Phase 2: Notification Hook 분리

1. `NotificationLifecycleHook` 구현
2. Daemon main loop에서 `dispatch_notification()` 호출 제거
3. hook으로 대체

### Phase 3: GitHub Label/Comment Hook 분리

1. `GitHubLifecycleHook` 구현
2. Task 구현체 (AnalyzeTask, ImplementTask 등)에서 라벨/코멘트 로직 제거
3. hook으로 대체

### Phase 4: Escalation Hook 분리

1. `EscalationLifecycleHook` 구현
2. Daemon main loop에서 에스컬레이션 로직 제거
3. hook으로 대체

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

## 사이드이펙트 분석

### 영향받는 기존 코드

| 파일 | 변경 내용 |
|------|----------|
| `core/state_queue.rs` | 변경 없음 (순수 데이터 구조 유지) |
| `service/daemon/mod.rs` | TransitionExecutor 사용, 직접 부수효과 코드 제거 |
| `service/daemon/task_runner_impl.rs` | 변경 없음 (Task 생명주기는 그대로) |
| `service/daemon/collectors/github.rs` | apply()에서 라벨 로직 일부를 hook으로 이동 |
| `service/tasks/*.rs` | GitHub 라벨/코멘트 직접 호출 제거 (점진적) |
| `core/notifier.rs` | 변경 없음 (NotificationLifecycleHook이 래핑) |

### 호환성

- **기존 API 유지**: StateQueue, Task trait, Collector trait 모두 변경 없음
- **hook 미등록 시**: 기존과 동일하게 동작 (빈 registry = no-op)
- **점진적 마이그레이션**: hook 추가와 기존 코드 제거를 독립적으로 진행 가능

### 테스트 전략

- `HookRegistry`: 인메모리 mock hook으로 before/after 실행 순서, Deny 전파 검증
- `TransitionExecutor`: mock hook + StateQueue 조합으로 전이+hook 통합 검증
- 각 Hook 구현체: mock Gh/Notifier로 부수효과 호출 검증
