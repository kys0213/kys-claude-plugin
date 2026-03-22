# QueuePhase 상태 머신

> 큐 아이템의 전체 생명주기를 정의한다.
> 상위 설계는 [DESIGN-v5](../DESIGN-v5.md) 참조.

---

## Phase 정의

| Phase | 설명 |
|-------|------|
| **Pending** | DataSource.collect()가 감지, 큐 대기 |
| **Ready** | 실행 준비 완료 (자동 전이) |
| **Running** | worktree 생성 + handler 실행 중 |
| **Completed** | handler 전부 성공, evaluate 대기 |
| **Done** | evaluate 완료 판정 + on_done script 성공 |
| **HITL** | evaluate가 사람 판단 필요로 분류 |
| **Skipped** | escalation skip 또는 preflight 실패 |
| **Failed** | on_done script 실패, 인프라 오류 등 |

---

## 전체 상태 전이

```
                            DataSource.collect()
                                    │
                                    ▼
                    ┌───────────────────────────────┐
                    │           Pending              │
                    │   (큐 대기, 수집됨)              │
                    └───────────────┬───────────────┘
                                    │ 자동 전이
                                    ▼
                    ┌───────────────────────────────┐
                    │            Ready               │
                    │   (실행 준비 완료)               │
                    └───────────────┬───────────────┘
                                    │ 자동 전이 (concurrency 제한)
                                    ▼
                    ┌───────────────────────────────┐
                    │           Running              │
                    │                                │
                    │  ① worktree 생성 (or 재사용)    │
                    │  ② on_enter script             │
                    │  ③ handlers 순차 실행           │
                    │     prompt → LLM (worktree)    │
                    │     script → bash              │
                    └──────┬────────────┬───────────┘
                           │            │
                    전부 성공        handler 실패
                           │            │
                           ▼            ▼
          ┌─────────────────┐    ┌─────────────────────────────┐
          │    Completed     │    │     Escalation 정책 적용      │
          │                  │    │     (history 기반 count)      │
          │  handler 완료    │    │                               │
          │  evaluate 대기   │    │  1: retry                    │
          │                  │    │     → 새 아이템 → Pending     │
          │  force_trigger   │    │     → worktree 보존          │
          │  ("evaluate")    │    │     → on_fail 실행 안 함      │
          └────────┬────────┘    │                               │
                   │              │  2: retry_with_comment        │
                   │              │     → on_fail script 실행     │
                   │              │     → 새 아이템 → Pending     │
                   │              │     → worktree 보존          │
                   │              │                               │
                   │              │  3: hitl                      │
                   │              │     → on_fail script 실행     │
                   │              │     → HITL 이벤트 생성 ───────┐│
                   │              │     → worktree 보존          ││
                   │              │                               ││
                   │              │  4: skip                      ││
                   │              │     → on_fail script 실행     ││
                   │              │     → Skipped ────────────────┼┼──┐
                   │              │     → worktree 정리          ││  │
                   │              │                               ││  │
                   │              │  5: replan                    ││  │
                   │              │     → on_fail script 실행     ││  │
                   │              │     → HITL(replan) ───────────┤│  │
                   │              │     → worktree 보존          ││  │
                   │              └───────────────────────────────┘│  │
                   │                                               │  │
                   │  evaluate cron                                │  │
                   │  (LLM이 autodev queue done/hitl CLI 호출)     │  │
                   │                                               │  │
              ┌────┴────┐                                          │  │
              │         │                                          │  │
          완료 판정   사람 필요                                      │  │
              │         │                                          │  │
              ▼         ▼                                          │  │
    ┌──────────┐    ┌──────────────────────────────────────┐       │  │
    │ on_done  │    │                HITL                   │◄──────┘  │
    │ script   │    │                                      │          │
    │ 실행     │    │  사람 대기 (worktree 보존)             │          │
    └──┬───┬──┘    │                                      │          │
       │   │       │  응답 경로:                            │          │
    성공  실패     │    "done"  → on_done → Done           │          │
       │   │       │    "retry" → 새 아이템 → Pending      │          │
       ▼   ▼       │    "skip"  → Skipped                 │          │
  ┌──────┐┌─────┐  │    "replan"→ 스펙 수정 제안           │          │
  │ Done ││ Fail│  └──────────────────────────────────────┘          │
  │      ││ ed  │                                                    │
  │ wt   ││     │  ┌──────────────────────────────────────┐          │
  │ 정리  ││ wt  │  │              Skipped                 │◄─────────┘
  │      ││ 보존 │  │                                      │
  └──────┘│ 로그 │  │  terminal (worktree 정리)             │
          │ 기록 │  └──────────────────────────────────────┘
          └─────┘
```

---

## Worktree 생명주기

| Phase / 이벤트 | Worktree |
|----------------|----------|
| Running | 생성 (또는 retry 시 기존 보존분 재사용) |
| Completed | 유지 (evaluate 대기) |
| Done | **정리** |
| HITL | 보존 (사람 확인 후 결정) |
| Failed | 보존 (디버깅용) |
| Skipped | 정리 |
| Retry | 보존 (이전 작업 위에서 재시도) |
| Graceful shutdown 롤백 (Running→Pending) | **정리** |
| hitl-timeout (HITL 만료) | **정리** |
| log-cleanup cron | 보존된 worktree 중 TTL 초과분 정리 |

**정리 원칙**: worktree는 **Done이 되어야만** 정리한다. 단, shutdown 롤백과 HITL 만료 시에도 정리하여 좀비 worktree를 방지한다. 나머지 보존분은 `log-cleanup` cron이 TTL(기본 7일) 기준으로 주기 정리한다.

---

## on_fail 실행 조건

| Escalation | on_fail 실행 | 동작 |
|------------|-------------|------|
| retry | 안 함 | 조용한 재시도 |
| retry_with_comment | 실행 | 외부 알림 + 재시도 |
| hitl | 실행 | 외부 알림 + 사람 대기 |
| skip | 실행 | 외부 알림 + 종료 |
| replan | 실행 | 외부 알림 + 스펙 수정 제안 |

`retry`만 on_fail을 실행하지 않는다. "조용한 재시도"로 외부 시스템에 노이즈를 주지 않는다.

failure_count는 append-only history에서 계산한다: `history | filter(state, failed) | count`.

---

### 관련 문서

- [DESIGN-v5](../DESIGN-v5.md) — 설계 철학
- [DataSource](./datasource.md) — escalation 정책 + on_fail script
- [Cron 엔진](./cron-engine.md) — evaluate cron + force_trigger
- [실패 복구와 HITL](../flows/04-failure-and-hitl.md) — 실패/HITL 시나리오
