# Flow 5: HITL 알림

### 시나리오

자동 처리 중 사람의 판단이 필요한 상황이 발생한다.

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

### 관련 플로우

- [Flow 0: DataSource](../00-datasource/flow.md) — on_failed, after_hitl_created
- [Flow 7: 피드백 루프](../07-feedback-loop/flow.md) — replan 경로
- [Flow 8: 스펙 완료 판정](../08-spec-completion/flow.md) — 최종 확인
- [Flow 9: 실패 복구](../09-failure-recovery/flow.md) — escalation
