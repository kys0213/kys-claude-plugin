# Flow 7: 피드백 루프

### 시나리오

사용자가 구현 결과를 확인하고 수정이 필요하다고 판단한다.

### 3가지 경로

#### Case 1: PR review comment

```
GitHub에서 changes-requested
  → DataSource.collect(): 큐에 Review/Improve 아이템 추가
  → 표준 파이프라인 (DataSource hook + AgentRuntime 실행)
```

기존 v3 흐름과 동일. DataSource hook이 자동 적용.

#### Case 2: /spec update

```
/spec update <id>
  → 현재 스펙 + 진행 상태 로드
  → 대화형 impact analysis (어떤 이슈가 영향받는지)
  → 사용자가 변경 승인
  → autodev spec update CLI 실행
  → 코어 on_spec_active 이벤트 재발행
  → ForceClawEvaluate → Claw 재평가
```

#### Case 3: Replan (HITL 응답)

```
on_failed Level 5 → DataSource.on_failed() → EscalationAction::Replan
  → 코어: HITL 생성
  → DataSource.after_hitl_created(): 알림
  → 사용자 "replan" 응답
  → 코어 on_hitl_responded → Claw에게 스펙 수정 제안 위임
  → 사용자 승인 → spec update → on_spec_active
```

### 핵심 원칙

**스펙 = 계약**. 계약이 바뀌어야 하면 `/spec update`. 계약 범위 내 작업이면 이슈 등록.

---

### 관련 플로우

- [Flow 3: 스펙 등록](../03-spec-registration/flow.md) — spec lifecycle
- [Flow 5: HITL 알림](../05-hitl-notification/flow.md) — replan 경로
- [Flow 9: 실패 복구](../09-failure-recovery/flow.md) — Level 5 replan
