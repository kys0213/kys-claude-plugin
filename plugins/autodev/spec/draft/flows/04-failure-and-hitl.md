# Flow 4: 실패 복구와 HITL

> handler 실행 실패 시 escalation 정책에 따라 복구하고, evaluate가 HITL로 분류하면 사람의 판단을 요청한다.

---

## 실패 경로

```
handler 실행 실패
    │
    ▼
escalation 정책 적용 (failure_count 기반, history에서 계산):
    │
    ├── retry             → 조용히 재시도 (on_fail 실행 안 함, worktree 보존)
    ├── retry_with_comment → on_fail script 실행 + 재시도 (worktree 보존)
    ├── hitl              → on_fail script 실행 + HITL 이벤트 생성 (worktree 보존)
    ├── skip              → on_fail script 실행 + Skipped (worktree 정리)
    └── replan            → on_fail script 실행 + HITL(replan) (worktree 보존)
```

`retry`만 on_fail을 실행하지 않는다. "조용한 재시도"로 외부 시스템에 노이즈를 주지 않는다.

---

## Escalation 정책 (workspace yaml 소유)

```yaml
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

### on_fail script 예시

```yaml
on_fail:
  - script: |
      CTX=$(autodev context $WORK_ID --json)
      ISSUE=$(echo $CTX | jq -r '.issue.number')
      REPO=$(echo $CTX | jq -r '.source.url')
      FAILURES=$(echo $CTX | jq '[.history[] | select(.status=="failed")] | length')
      gh issue comment $ISSUE --body "실패 (시도 횟수: $FAILURES)" -R $REPO
```

failure_count는 별도 컬럼이 아니라 history의 append-only 이벤트에서 계산.

---

## HITL (Human-in-the-Loop)

### 생성 경로

| 경로 | 트리거 |
|------|--------|
| Escalation | handler 실패 → escalation 정책이 HITL/Replan 결정 |
| evaluate | handler 성공 → evaluate가 "사람이 봐야 한다" 판단 |
| 스펙 완료 | 모든 linked issues Done → 최종 확인 요청 |
| 충돌 | DependencyGuard가 스펙 충돌 감지 |

### 응답 경로

```
사용자 응답 (TUI / CLI / /claw 세션)
  → autodev hitl respond <id> --choice N
  → 라우팅:
      "done"     → on_done script 실행
                     ├── script 성공 → Done (worktree 정리)
                     └── script 실패 → Failed (worktree 보존, 로그 기록)
      "retry"    → 새 아이템 생성 → Pending (worktree 보존)
      "skip"     → Skipped (worktree 정리)
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

- [DataSource](../concerns/datasource.md) — escalation 정책 + on_fail script
- [Claw](../concerns/claw-workspace.md) — 대화형 에이전트 (HITL 응답 경로 포함)
- [이슈 파이프라인](./03-issue-pipeline.md) — 실패가 발생하는 실행 흐름
