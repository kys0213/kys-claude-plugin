# Flow 4: 실패 복구와 HITL — 장애 대응 → 사람 개입 → 복구

> 자동 처리 중 실패가 발생하면 DataSource별 escalation 정책에 따라 복구하고, 필요 시 사람의 판단을 요청한다.

---

## 1. Escalation 정책 (DataSource 소유)

각 DataSource가 자신의 escalation 전략을 결정하고, 코어가 실행한다.

```
Task 실패 → Daemon on_task_complete(Failed):
  → DataSource.after_task(kind, item, Failed): 실패 코멘트
  → DataSource.on_failed(item, failure_count):
    → return EscalationAction (DataSource가 결정)
  → 코어.apply_escalation(action):
    Retry           → db.queue_transit(Running, Pending)
    CommentAndRetry → db.queue_transit(Running, Pending)
    Hitl { event }  → db.hitl_create + DataSource.after_hitl_created()
    Skip { reason } → db.queue_skip + DataSource.on_skip()
    Replan { event }→ db.hitl_create + DataSource.after_hitl_created()
  → 코어.force_claw_evaluate()
```

### GitHubDataSource 5단계

| failure_count | Level | 동작 |
|---------------|-------|------|
| 1 | Retry | Pending 롤백 (재시도) |
| 2 | Comment+Retry | GitHub 코멘트 + Pending 롤백 |
| 3 | HITL | HitlEvent 생성 (High severity) |
| 4 | Skip | Skipped 전이 |
| 5+ | Replan | HitlEvent (replan 옵션, 피드백 루프 연계) |

### OCP 확장

```rust
// GitHub: 5단계
impl DataSource for GitHubDataSource {
    async fn on_failed(&self, item, count, _ctx) -> Result<EscalationAction> {
        match count { 1 => Retry, 2 => CommentAndRetry, 3 => Hitl, 4 => Skip, _ => Replan }
    }
}

// Slack: 다른 정책 가능
impl DataSource for SlackDataSource {
    async fn on_failed(&self, item, count, _ctx) -> Result<EscalationAction> {
        match count { 1 => Retry, _ => Hitl }  // 바로 사용자에게 DM
    }
}
```

새 DataSource 추가 시 escalation 정책만 구현. 코어 변경 0.

---

## 2. HITL (Human-in-the-Loop)

### HITL 생성 경로

| 생성자 | 트리거 |
|--------|--------|
| DataSource.on_failed() | EscalationAction::Hitl/Replan 반환 시 |
| 코어 on_spec_completing | 최종 확인 요청 시 |
| 코어 DependencyGuard | 스펙 충돌 감지 시 |

### 이벤트 유형별 선택지 (고정)

```rust
pub struct HitlOption {
    pub key: String,      // "retry", "skip" 등
    pub label: String,    // 사용자 표시용
    pub action: String,   // 라우팅 대상: "advance", "skip", "replan"
}
```

| 유형 | 선택지 |
|------|--------|
| Escalation Level 3 | retry→advance, skip→skip, reassign→replan |
| Escalation Level 5 | replan→replan, force-retry→advance, abandon→skip |
| Spec Completion | approve→spec_completed, request-changes→spec_active |
| Conflict | prioritize-A→advance, prioritize-B→advance, pause-both→pause |

### HITL 생성 흐름

```
1. DataSource.on_failed(item, count=3)
   → return EscalationAction::Hitl { event }
2. 코어: db.hitl_create(event)
3. DataSource.after_hitl_created(event)
   → GitHub: 이슈에 HITL 코멘트 (선택지 포함)
   → Slack: 채널에 알림 메시지
   → Webhook: payload 전송
```

### 응답 경로

```
사용자 응답 (TUI / CLI / GitHub 코멘트)
  → autodev hitl respond <id> --choice N
  → DB: hitl_response 저장
  → 코어 on_hitl_responded:
    1. HitlResponseRouter:
       "advance"        → queue advance
       "skip"           → queue skip
       "replan"         → Claw에게 스펙 수정 제안 위임
       "spec_completed" → on_spec_completed
       "spec_active"    → spec Active 복귀
    2. ForceClawEvaluate
```

### 타임아웃

```
기본: 24시간
초과 시: hitl-timeout cron job이 처리
  → 설정에 따라: remind / skip / pause_spec
```

### /claw 세션에서 처리

```
사용자: "HITL 대기 목록 보여줘"
Claw: autodev hitl list --json → 포맷팅

사용자: "첫 번째 항목 proceed"
Claw: autodev hitl respond <id> --choice 1
  → on_hitl_responded 이벤트 자동 발행
```

---

## 3. Worktree 보존

```
DataSource.after_task(Implement, Failed):
  → worktree 보존 (삭제하지 않음)
  → HITL 알림에 경로 포함
  → 보존 기간: 7일 (설정 가능)
  → autodev worktree list/clean으로 관리
  → log-cleanup cron에서 만료 시 자동 삭제
```

---

## 4. Graceful Shutdown

```
SIGINT → on_shutdown:
  1. Running 아이템 완료 대기 (timeout: 30초, 설정 가능)
     → timeout 초과: Pending으로 롤백
  2. DataSource.on_phase_exit(Running) 호출 (best-effort)
  3. Cron engine 정지
```

---

### 관련 문서

- [이슈 파이프라인](./03-issue-pipeline.md) — 실패가 발생하는 실행 흐름
- [스펙 생명주기](./02-spec-lifecycle.md) — 완료 확인 HITL, 충돌 HITL
- [DataSource](../concerns/datasource.md) — on_failed, EscalationAction, after_hitl_created
- [Cron 엔진](../concerns/cron-engine.md) — hitl-timeout cron
