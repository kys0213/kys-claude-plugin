# Flow 14: Lifecycle Hooks

> DataSource는 큐에 넣고, 전이 시 before/after만 수행한다.
> 나머지는 framework이 처리한다.

---

## 현재 문제

```
현재: DataSource(Collector)가 너무 많은 것을 안다

  Collector
    ├── 외부 API 호출           ← DataSource 관심사 ✅
    ├── QueueItem 생성          ← DataSource 관심사 ✅
    ├── Task 구현체 선택/생성    ← framework 관심사 ❌
    ├── StateQueue 직접 조작    ← framework 관심사 ❌
    ├── concurrency 관리        ← framework 관심사 ❌
    └── apply() 시 라벨 변경    ← hook 관심사 ❌

  Task 구현체
    └── after_invoke()에서 라벨/코멘트 직접 호출  ← hook 관심사 ❌

  Daemon main loop
    ├── dispatch_notification()  ← hook 관심사 ❌
    ├── escalation::escalate()   ← hook 관심사 ❌
    └── log_insert()             ← hook 관심사 ❌
```

**결과**: 새 DataSource 추가 시 Task 생성 로직, 큐 조작, 부수효과를 모두 알아야 함.

---

## 목표

```
DataSource 추가자가 하는 일 = 2개

  1. 큐에 넣는다     (ItemSource.poll → QueueItem)
  2. 전이 시 반영한다  (LifecycleHook.before/after)

  나머지는 전부 framework.
```

---

## 전체 구조

```
  외부 시스템
  GitHub        Jira          Slack         ...
    │             │             │
    ▼             ▼             ▼
  ┌─────────────────────────────────────────┐
  │         ItemSource.poll()                │  ← 구현 1: 큐에 넣는다
  │         → Vec<QueueItem>                 │
  └─────────────────┬───────────────────────┘
                    │
                    ▼
  ┌─────────────────────────────────────────┐
  │              Framework                   │
  │                                          │
  │  StateQueue    TaskFactory   Concurrency │  ← 몰라도 됨
  │  (상태 관리)   (Kind→Task)   (슬롯 관리)  │
  │                                          │
  │         ┌──────────────────┐             │
  │         │ TransitionExecutor│             │
  │         │                  │             │
  │         │  before hooks ──┐│             │
  │         │  transit()      ││             │
  │         │  after hooks  ──┘│             │
  │         └──────────────────┘             │
  └─────────────────┬───────────────────────┘
                    │
                    ▼
  ┌─────────────────────────────────────────┐
  │      LifecycleHook.before/after          │  ← 구현 2: 전이 시 반영한다
  │                                          │
  │  GitHub: 라벨 + 코멘트                    │
  │  Jira:   상태 전이                        │
  │  Slack:  이모지 변경                      │
  └─────────────────────────────────────────┘
```

---

## 상태 전이 모델

```
  Pending ──► Ready ──► Running ──► Done
                                 ──► Failed
                                 ──► Skipped

  각 전이마다:
    before(from, to) → Allow / Deny
    [전이 실행]
    after(from, to)  → 부수효과
```

| 전이 | 트리거 | before | after |
|------|--------|--------|-------|
| Pending → Ready | Claw evaluate | 선행 조건 검사 | 라벨 변경 |
| Ready → Running | Daemon spawn | 가용성 검사 | 라벨(wip), 코멘트 |
| Running → Done | Task 완료 | - | 라벨(done), 코멘트, 알림 |
| Running → Failed | Task 실패 | - | 라벨(failed), 에스컬레이션, 알림 |
| Running → Skipped | Preflight skip | - | 라벨 제거 |

**before가 Deny 반환 시**: 전이 중단 → HITL 생성 → 다음 틱에서 재시도.

---

## 관련 문서

- [design.md](./design.md) — 인터페이스 정의, framework 컴포넌트
- [hooks.md](./hooks.md) — DataSource별 hook 구현체 상세
- [migration.md](./migration.md) — 점진적 마이그레이션 전략
