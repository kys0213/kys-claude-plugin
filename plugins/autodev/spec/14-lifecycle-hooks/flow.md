# Flow 14: Lifecycle Hooks

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

### before hook의 Deny 처리

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

## 관련 문서

- [design.md](./design.md) — trait/struct 정의, HookRegistry, TransitionExecutor
- [hooks.md](./hooks.md) — DataSource별 hook 구현체 상세
- [migration.md](./migration.md) — 점진적 마이그레이션 전략, 사이드이펙트 분석
