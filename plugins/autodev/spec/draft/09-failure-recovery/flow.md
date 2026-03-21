# Flow 9: 실패 복구

### 시나리오

자동 처리 중 실패가 발생하여 복구 또는 사용자 개입이 필요하다.

### Escalation 정책 (DataSource 소유)

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
| 5+ | Replan | HitlEvent (replan 옵션, Flow 7 연계) |

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

### Worktree 보존

```
DataSource.after_task(Implement, Failed):
  → worktree 보존 (삭제하지 않음)
  → HITL 알림에 경로 포함
  → 보존 기간: 7일 (설정 가능)
  → autodev worktree list/clean으로 관리
  → log-cleanup cron에서 만료 시 자동 삭제
```

### Graceful Shutdown

```
SIGINT → on_shutdown:
  1. Running 아이템 완료 대기 (timeout: 30초, 설정 가능)
     → timeout 초과: Pending으로 롤백
  2. DataSource.on_phase_exit(Running) 호출 (best-effort)
  3. Cron engine 정지
```

---

### 관련 플로우

- [Flow 0: DataSource](../00-datasource/flow.md) — on_failed, EscalationAction
- [Flow 5: HITL 알림](../05-hitl-notification/flow.md) — Level 3, 5
- [Flow 7: 피드백 루프](../07-feedback-loop/flow.md) — replan 경로
