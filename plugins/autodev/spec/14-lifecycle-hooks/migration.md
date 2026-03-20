# Migration: 점진적 마이그레이션 전략

> 기존 부수효과를 hook으로 이동하는 단계별 계획과 사이드이펙트 분석.

---

## 마이그레이션 원칙

한번에 모든 부수효과를 hook으로 옮기지 않는다. 각 Phase마다:

1. hook 구현체 추가
2. 기존 코드와 hook이 **공존**하는 상태에서 동작 검증
3. 기존 코드 제거

```
Phase 1 (Infrastructure) → Phase 2 (Notification) → Phase 3 (GitHub) → Phase 4 (Escalation)
```

---

## Phase 1: Infrastructure

**목표**: trait + registry + executor 정의. 기존 동작 변경 없음.

1. `core/lifecycle.rs` 생성
   - `LifecycleHook` trait
   - `HookDecision`, `Transition`, `TransitionResult`
   - `HookRegistry`
2. `service/daemon/transition_executor.rs` 생성
   - `TransitionExecutor`
3. `HookRegistry`를 빈 상태로 Daemon에 주입
4. **기존 동작 변경 없음** (hook이 없으면 기존과 동일)

### 검증

- `HookRegistry` 단위 테스트: mock hook 실행 순서, Deny 전파
- `TransitionExecutor` 단위 테스트: mock hook + StateQueue 조합

---

## Phase 2: Notification Hook 분리

**목표**: Daemon main loop의 `dispatch_notification()` 호출을 hook으로 대체.

1. `NotificationLifecycleHook` 구현
2. Daemon 생성 시 registry에 등록
3. Daemon main loop에서 `dispatch_notification()` 직접 호출 제거
4. hook이 동일한 시점에 동일한 알림 발송

### 검증

- mock NotificationDispatcher로 hook 경유 알림 발송 확인
- 기존 알림 테스트가 동일하게 통과하는지 확인

---

## Phase 3: GitHub Label/Comment Hook 분리

**목표**: Task 구현체에서 라벨/코멘트 직접 호출을 hook으로 이동.

1. `GitHubLifecycleHook` 구현
2. Daemon 생성 시 registry에 등록
3. Task 구현체에서 라벨/코멘트 로직 제거:
   - `AnalyzeTask.after_invoke()` → 라벨 변경 제거
   - `ImplementTask.after_invoke()` → 라벨/코멘트 제거
   - `ReviewTask.after_invoke()` → 라벨/코멘트 제거
   - `ImproveTask.after_invoke()` → 라벨/코멘트 제거
4. `Collector.apply()`에서 라벨 로직 일부 제거

### 검증

- mock Gh로 hook 경유 라벨/코멘트 호출 확인
- 기존 Task 테스트가 라벨/코멘트 없이도 core 동작에 영향 없는지 확인

---

## Phase 4: Escalation Hook 분리

**목표**: Daemon main loop의 에스컬레이션 로직을 hook으로 이동.

1. `EscalationLifecycleHook` 구현
2. Daemon 생성 시 registry에 등록
3. Daemon main loop에서 `escalation::escalate()` 직접 호출 제거
4. hook이 동일한 로직 수행

### 검증

- mock DB로 failure_count 증가, HITL 생성 확인
- 에스컬레이션 시나리오 테스트 (retry → remove → HITL)

---

## 사이드이펙트 분석

### 영향받는 기존 코드

| 파일 | Phase | 변경 내용 |
|------|-------|----------|
| `core/state_queue.rs` | - | **변경 없음** (순수 데이터 구조 유지) |
| `core/task.rs` | - | **변경 없음** (Task trait 유지) |
| `core/collector.rs` | - | **변경 없음** (Collector trait 유지) |
| `core/notifier.rs` | - | **변경 없음** (NotificationLifecycleHook이 래핑) |
| `service/daemon/mod.rs` | 1-4 | TransitionExecutor 사용, 직접 부수효과 코드 점진적 제거 |
| `service/daemon/task_runner_impl.rs` | - | **변경 없음** (Task 생명주기는 그대로) |
| `service/daemon/collectors/github.rs` | 3 | apply()에서 라벨 로직 일부를 hook으로 이동 |
| `service/tasks/analyze.rs` | 3 | GitHub 라벨/코멘트 직접 호출 제거 |
| `service/tasks/implement.rs` | 3 | GitHub 라벨/코멘트 직접 호출 제거 |
| `service/tasks/review.rs` | 3 | GitHub 라벨/코멘트 직접 호출 제거 |
| `service/tasks/improve.rs` | 3 | GitHub 라벨/코멘트 직접 호출 제거 |

### 호환성 보장

- **기존 API 유지**: StateQueue, Task trait, Collector trait 모두 변경 없음
- **hook 미등록 시**: 기존과 동일하게 동작 (빈 registry = no-op)
- **점진적 마이그레이션**: hook 추가와 기존 코드 제거를 독립적으로 진행 가능
- **Phase 간 독립**: 각 Phase를 별도 PR로 진행 가능

### Daemon main loop 변경 전후 비교

```
현재 (Daemon main loop):
  task 완료
    → manager.apply()
    → log_insert() + usage_insert()
    → escalation::escalate()
    → dispatch_notification()
    → cron force_trigger
    → spec completion check

Phase 4 완료 후 (Daemon main loop):
  task 완료
    → transit(Running → Done/Failed)     ← hook이 모든 부수효과 수행
    → cron force_trigger                 ← Daemon 고유 관심사 (hook 대상 아님)
    → spec completion check              ← Daemon 고유 관심사 (hook 대상 아님)
```

> **cron trigger, spec completion**은 상태 전이의 부수효과가 아닌
> Daemon 고유 관심사이므로 hook으로 이동하지 않는다.
