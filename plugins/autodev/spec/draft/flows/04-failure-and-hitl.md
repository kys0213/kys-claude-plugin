# Flow 4: 실패 복구와 HITL

> handler 실행 실패 시 DataSource의 escalation 정책에 따라 복구하고, Claw가 HITL로 분류하면 사람의 판단을 요청한다.

---

## 실패 경로

```
handler 실행 실패
    │
    ▼
DataSource escalation 정책 적용 (failure_count 기반):
    │
    ├── Retry → Pending 재진입 (같은 구간, 같은 handlers)
    ├── Retry + Comment → Pending 재진입 + 외부 시스템에 코멘트
    ├── HITL → HITL 이벤트 생성 → 사람 대기
    ├── Skip → Skipped (terminal)
    └── Replan → HITL 이벤트 생성 (replan 옵션 포함)
```

---

## Escalation 정책 (DataSource 소유)

```yaml
# GitHub: 5단계
sources:
  github:
    escalation:
      1: retry
      2: retry_with_comment
      3: hitl
      4: skip
      5: replan
```

DataSource마다 다른 정책이 가능한 구조. v5는 GitHub 정책만 구현.

---

## HITL (Human-in-the-Loop)

### 생성 경로

| 경로 | 트리거 |
|------|--------|
| Escalation | handler 실패 → escalation 정책이 HITL/Replan 결정 |
| 코어 evaluate | handler 성공 → evaluate가 "사람이 봐야 한다" 판단 |
| 스펙 완료 | 모든 linked issues Done → 최종 확인 요청 |
| 충돌 | DependencyGuard가 스펙 충돌 감지 |

### 응답 경로

```
사용자 응답 (TUI / CLI / /claw 세션)
  → autodev hitl respond <id> --choice N
  → 라우팅:
      "done"     → Done 처리 → on_done 액션
      "retry"    → Pending 재진입
      "skip"     → Skipped
      "replan"   → Claw에게 스펙 수정 제안 위임
```

### 타임아웃

```
기본: 24시간
초과 시: hitl-timeout cron이 처리
  → 설정에 따라: remind / skip / pause_spec
```

---

## Graceful Shutdown

```
SIGINT → on_shutdown:
  1. Running 아이템 완료 대기 (timeout: 30초)
     → timeout 초과: Pending으로 롤백
  2. Cron engine 정지
```

---

### 관련 문서

- [DataSource](../concerns/datasource.md) — escalation 정책 정의
- [Claw](../concerns/claw-workspace.md) — 대화형 에이전트 (HITL 응답 경로 포함)
- [이슈 파이프라인](./03-issue-pipeline.md) — 실패가 발생하는 실행 흐름
