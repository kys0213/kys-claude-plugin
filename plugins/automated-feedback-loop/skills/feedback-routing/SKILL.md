---
name: feedback-routing
description: 자동 피드백 라우팅 스킬 - 검증 결과에 따른 자동 분기
---

# Feedback Routing Skill

검증 결과에 따라 자동으로 다음 액션을 결정합니다.

## 핵심 개념

```
┌─────────────────────────────────────────────────────────────────┐
│  FEEDBACK ROUTING: 검증 결과 → 자동 액션 결정                   │
│                                                                 │
│  ┌──────────────┐                                               │
│  │ 검증 결과    │                                               │
│  └──────┬───────┘                                               │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────────────────────────────────────────┐      │
│  │                    라우팅 결정                        │      │
│  │                                                      │      │
│  │   ✅ 통과 → 다음 Checkpoint 또는 완료                │      │
│  │   ❌ 실패 (재시도 가능) → 자동 피드백 + 재시도       │      │
│  │   ❌ 실패 (설계 문제) → 에스컬레이션                 │      │
│  │   ⚠️ 타임아웃 → 재시도 또는 에스컬레이션            │      │
│  │   ❓ 불명확 → 에스컬레이션                           │      │
│  └──────────────────────────────────────────────────────┘      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## 라우팅 규칙

### 규칙 1: 통과 (PASS)

```yaml
condition:
  - validation_result == "pass"
  - all_criteria_met == true

action:
  - mark_checkpoint_complete
  - trigger_impl_reviewer (optional)
  - proceed_to_next_checkpoint OR complete_session
```

### 규칙 2: 재시도 가능 실패 (RETRY)

```yaml
condition:
  - validation_result == "fail"
  - failure_type in [NOT_IMPLEMENTED, LOGIC_ERROR, TYPE_ERROR]
  - iteration < max_iterations

action:
  - invoke_test_oracle (분석)
  - generate_feedback
  - send_feedback_to_worker
  - increment_iteration
  - retry_implementation
```

### 규칙 3: 설계 문제 (ESCALATE_DESIGN)

```yaml
condition:
  - validation_result == "fail"
  - failure_type == DESIGN_MISMATCH

action:
  - generate_escalation_report
  - notify_human
  - suggest_architect_resume
  - pause_delegation
```

### 규칙 4: 최대 재시도 초과 (ESCALATE_MAX_RETRY)

```yaml
condition:
  - validation_result == "fail"
  - iteration >= max_iterations

action:
  - analyze_failure_pattern
  - generate_escalation_report
  - notify_human
  - pause_delegation
```

### 규칙 5: 환경 문제 (ESCALATE_ENV)

```yaml
condition:
  - validation_result == "fail"
  - failure_type == ENV_ISSUE

action:
  - notify_human
  - suggest_env_fix
  - pause_delegation
```

## 라우팅 흐름도

```
                    ┌─────────────┐
                    │ 검증 실행   │
                    └──────┬──────┘
                           │
                    ┌──────▼──────┐
                    │ 결과 분석   │
                    └──────┬──────┘
                           │
          ┌────────────────┼────────────────┐
          │                │                │
          ▼                ▼                ▼
    ┌───────────┐   ┌───────────┐   ┌───────────┐
    │   통과    │   │   실패    │   │  타임아웃  │
    └─────┬─────┘   └─────┬─────┘   └─────┬─────┘
          │               │               │
          │         ┌─────▼─────┐         │
          │         │ 원인 분류 │         │
          │         └─────┬─────┘         │
          │               │               │
          │    ┌──────────┼──────────┐    │
          │    │          │          │    │
          ▼    ▼          ▼          ▼    ▼
    ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐
    │ PASS │ │RETRY │ │DESIGN│ │ MAX  │ │ ENV  │
    └──┬───┘ └──┬───┘ └──┬───┘ └──┬───┘ └──┬───┘
       │        │        │        │        │
       ▼        ▼        ▼        ▼        ▼
    다음CP   피드백    에스컬   에스컬   에스컬
             + 재시도   레이션   레이션   레이션
```

## 피드백 생성 파이프라인

```
실패 결과
    │
    ▼
┌─────────────────────────────────────────┐
│  1. Test Oracle Agent 호출              │
│                                         │
│  • 테스트 출력 파싱                     │
│  • 실패 원인 분류                       │
│  • 관련 코드 분석                       │
└─────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────┐
│  2. 피드백 문서 생성                    │
│                                         │
│  • 실패 요약                            │
│  • 구체적 수정 제안                     │
│  • 코드 예시                            │
└─────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────┐
│  3. Worker에게 전달                     │
│                                         │
│  • .afl/sessions/{id}/delegations/      │
│    {checkpoint}/iterations/{n}/         │
│    feedback.md 생성                     │
│                                         │
│  • Worker 프롬프트에 피드백 포함        │
└─────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────┐
│  4. Worker 재실행                       │
│                                         │
│  • Task 도구로 Worker 재생성            │
│  • 기존 컨텍스트 + 피드백 전달          │
└─────────────────────────────────────────┘
```

## 에스컬레이션 리포트 구조

```markdown
## ⚠️ 에스컬레이션 리포트

### 기본 정보

| 항목 | 값 |
|------|-----|
| 세션 | {session_id} |
| Checkpoint | {checkpoint_id} |
| 시도 횟수 | {iterations}/{max_iterations} |
| 에스컬레이션 사유 | {reason} |

### 실패 이력

{failure_history_table}

### 패턴 분석

{pattern_analysis}

### 추정 원인

1. {cause_1}
2. {cause_2}

### 권장 조치

**설계 재검토가 필요한 경우:**
```bash
/afl:architect --resume {session_id}
```

**환경 문제인 경우:**
{env_fix_instructions}

**수동 개입이 필요한 경우:**
{manual_intervention_guide}
```

## 상태 전이

```
┌──────────┐
│ pending  │ ────────▶ 의존성 충족
└──────────┘
      │
      ▼
┌──────────────┐
│ in_progress  │ ◀──────────────────┐
└──────────────┘                    │
      │                             │
      ├──────▶ 통과 ────▶ completed │
      │                             │
      └──────▶ 실패 ───┬──▶ retry ──┘
                       │
                       └──▶ escalated ──▶ 인간 개입
```

## 설정

```json
{
  "feedbackRouting": {
    "maxIterations": 5,
    "autoRetryDelay": 5000,
    "escalationOnDesignMismatch": true,
    "runImplReviewerOnPass": true,
    "notifyOnEscalation": true
  }
}
```

## 메트릭 수집

라우팅 결과를 기록하여 시스템 개선에 활용:

```json
{
  "sessionId": "abc12345",
  "checkpoint": "coupon-service",
  "routingHistory": [
    { "iteration": 1, "result": "RETRY", "reason": "NOT_IMPLEMENTED" },
    { "iteration": 2, "result": "RETRY", "reason": "LOGIC_ERROR" },
    { "iteration": 3, "result": "PASS" }
  ],
  "totalTime": "15m",
  "feedbackQuality": "effective"
}
```
