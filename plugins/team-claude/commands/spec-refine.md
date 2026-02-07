---
name: team-claude:spec-refine
description: 멀티 리뷰 + RALPH 루프로 스펙을 반복 개선 - 3개 LLM 관점에서 스펙을 검토하고 자동 정제
argument-hint: "--session <id> [--max-iterations 5] [--pass-threshold 80]"
allowed-tools: ["Task", "Bash", "Read", "Write", "Glob", "Grep", "AskUserQuestion"]
---

# Spec Refine Command - 멀티 리뷰 RALPH 루프

> **먼저 읽기**: `${CLAUDE_PLUGIN_ROOT}/INFRASTRUCTURE.md`

3개 LLM(Claude, Codex, Gemini)의 관점으로 스펙을 병렬 리뷰하고, RALPH 루프로 자동 정제합니다.

---

## 핵심 원칙

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  SPEC REFINE: Multi-Review RALPH Loop                                       │
│                                                                             │
│  기존 spec-reviewer (단일 Claude) 대비 강화점:                              │
│                                                                             │
│  1. 3관점 병렬 리뷰: Claude(깊이) + Codex(구현) + Gemini(대안)              │
│  2. 합의/분기 분석: 3개 LLM이 동의하는 이슈 = Critical                      │
│  3. RALPH 자동 정제: 피드백 → 스펙 수정 → 재리뷰 반복                       │
│  4. 정량 평가: 각 관점별 점수 → 가중 평균 → 통과/실패 결정                  │
│                                                                             │
│  Contract 품질이 병렬 실행의 안전성을 결정하므로,                            │
│  스펙 단계에서 최대한 높은 품질을 확보하는 것이 핵심입니다.                  │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 사용법

```bash
# 세션의 스펙 정제 (기본 설정)
/team-claude:spec-refine --session abc12345

# 최대 반복 횟수 지정
/team-claude:spec-refine --session abc12345 --max-iterations 3

# 통과 임계값 조정 (기본 80)
/team-claude:spec-refine --session abc12345 --pass-threshold 90

# 특정 리뷰어만 사용
/team-claude:spec-refine --session abc12345 --reviewers claude,codex
```

---

## 실행 절차

```
/team-claude:spec-refine --session {session-id}
        │
        ▼
┌───────────────────────────────────────────────────────────────────┐
│  STEP 0: 스펙 파일 로드                                          │
│                                                                   │
│  세션 디렉토리에서 스펙 파일 확인:                                │
│  SESSION_DIR=".team-claude/sessions/{session-id}"                 │
│                                                                   │
│  필수 파일:                                                       │
│  • ${SESSION_DIR}/specs/architecture.md                           │
│  • ${SESSION_DIR}/specs/contracts.md                              │
│  • ${SESSION_DIR}/specs/checkpoints.yaml                          │
│                                                                   │
│  파일 없으면:                                                     │
│  → "스펙이 없습니다. /team-claude:architect 를 먼저 실행하세요."  │
└───────────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────────┐
│  STEP 1: RALPH 루프 시작                                         │
│                                                                   │
│  iteration = 0                                                    │
│  max_iterations = 5 (기본값)                                      │
│  pass_threshold = 80 (기본값)                                     │
│                                                                   │
│  WHILE iteration < max_iterations:                                │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                                                             │ │
│  │  PHASE A: 3-LLM 병렬 리뷰                                  │ │
│  │  PHASE B: 리뷰 통합 및 합의 분석                            │ │
│  │  PHASE C: 통과 여부 판정                                    │ │
│  │  PHASE D: 스펙 자동 정제 (FAIL인 경우)                      │ │
│  │                                                             │ │
│  └─────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────┘
```

### PHASE A: 3-LLM 병렬 리뷰

```
┌───────────────────────────────────────────────────────────────────┐
│  PHASE A: 3개 LLM 병렬 스펙 리뷰                                 │
│                                                                   │
│  Task 도구로 3개 에이전트를 동시에 실행:                          │
│                                                                   │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │
│  │   Claude     │  │   Codex     │  │   Gemini    │              │
│  │             │  │             │  │             │              │
│  │  관점:      │  │  관점:      │  │  관점:      │              │
│  │  아키텍처   │  │  구현 가능성│  │  대안 관점  │              │
│  │  완전성     │  │  코드 품질  │  │  리스크     │              │
│  │  일관성     │  │  테스트 충분│  │  확장성     │              │
│  │  의존성     │  │  Contract   │  │  트레이드오프│             │
│  │             │  │  실행 가능성│  │  누락 패턴  │              │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘              │
│         │               │               │                        │
│         └───────────────┼───────────────┘                        │
│                         ▼                                        │
│                  리뷰 결과 3개                                    │
└───────────────────────────────────────────────────────────────────┘
```

**Claude 리뷰 에이전트 (spec-reviewer 사용):**

```
Task 에이전트 호출:
  에이전트: spec-reviewer (기존)
  입력: 세션의 스펙 파일들
  관점: 아키텍처 깊이, 완전성, 일관성, 의존성
```

**Codex 리뷰 에이전트 (spec-reviewer-codex 사용):**

```
Task 에이전트 호출:
  에이전트: spec-reviewer-codex
  입력: 스크립트를 통해 스펙 파일 전달
  관점: 구현 가능성, Contract 실행 가능성, 코드 품질
```

**Gemini 리뷰 에이전트 (spec-reviewer-gemini 사용):**

```
Task 에이전트 호출:
  에이전트: spec-reviewer-gemini
  입력: 스크립트를 통해 스펙 파일 전달
  관점: 대안 설계, 리스크, 확장성, 누락 패턴
```

### PHASE B: 리뷰 통합 및 합의 분석

```
┌───────────────────────────────────────────────────────────────────┐
│  PHASE B: 합의 분석 (Consensus Analysis)                         │
│                                                                   │
│  3개 리뷰 결과를 분석하여 통합 피드백 생성:                       │
│                                                                   │
│  1. 이슈 추출                                                     │
│     각 LLM의 리뷰에서 이슈 항목 추출 (Critical/Important/Nice)    │
│                                                                   │
│  2. 합의도 분석                                                    │
│     ┌─────────────────────────────────────────────────────┐      │
│     │  3/3 동의 → CONSENSUS (반드시 반영)                  │      │
│     │  2/3 동의 → MAJORITY  (강력 권장)                   │      │
│     │  1/3 제기 → MINORITY  (검토 필요)                   │      │
│     └─────────────────────────────────────────────────────┘      │
│                                                                   │
│  3. 점수 산출                                                     │
│     ┌─────────────────────────────────────────────────────┐      │
│     │  가중 평균 점수:                                     │      │
│     │    Claude  × 0.4 (아키텍처 깊이 가중)               │      │
│     │  + Codex   × 0.35 (구현 현실성 가중)                │      │
│     │  + Gemini  × 0.25 (대안 관점 가중)                  │      │
│     │  = 통합 점수 (0-100)                                 │      │
│     └─────────────────────────────────────────────────────┘      │
│                                                                   │
│  4. 판정                                                          │
│     점수 ≥ pass_threshold → PASS                                 │
│     CONSENSUS 이슈 0개 + 점수 ≥ warn_threshold → WARN           │
│     그 외 → FAIL                                                  │
│                                                                   │
│  산출물:                                                          │
│  ${SESSION_DIR}/reviews/iteration-{n}/                            │
│  ├── claude-review.md                                             │
│  ├── codex-review.md                                              │
│  ├── gemini-review.md                                             │
│  └── consensus-report.md   ← 통합 분석 결과                      │
└───────────────────────────────────────────────────────────────────┘
```

**통합 리포트 형식:**

```markdown
## Spec Review - Iteration {n} 통합 리포트

### 점수
| LLM | 점수 | 가중치 | 가중 점수 |
|-----|------|--------|-----------|
| Claude | 75 | 0.40 | 30.0 |
| Codex | 80 | 0.35 | 28.0 |
| Gemini | 70 | 0.25 | 17.5 |
| **통합** | | | **75.5** |

### 판정: ❌ FAIL (threshold: 80)

### CONSENSUS 이슈 (3/3 동의 - 반드시 수정)
1. **Contract Test 불완전**: CouponService.apply()의 에러 케이스 누락
   - Claude: "에러 케이스 테스트 없음"
   - Codex: "apply() 예외 경로 미검증"
   - Gemini: "실패 시나리오 커버리지 부족"

### MAJORITY 이슈 (2/3 동의 - 강력 권장)
1. **의존성 그래프 단순화 가능**: coupon-api가 coupon-service에 직접 의존할 필요 없음
   - Claude: "중간 레이어 불필요"
   - Gemini: "직접 호출 패턴으로 단순화 가능"

### MINORITY 이슈 (1/3 제기 - 검토)
1. **캐싱 전략 부재** (Gemini)
   - "자주 사용되는 쿠폰 조회에 캐싱 고려 필요"

### 개선 방향
1. [CONSENSUS] Contract Test에 에러 케이스 추가
2. [MAJORITY] 의존성 그래프 재검토
3. [MINORITY] 캐싱 전략은 다음 단계에서 검토
```

### PHASE C: 통과 여부 판정

```
┌───────────────────────────────────────────────────────────────────┐
│  PHASE C: 판정                                                    │
│                                                                   │
│  PASS (통합 점수 ≥ pass_threshold):                               │
│  → 리뷰 완료, 다음 단계 진행 가능                                │
│  → 최종 리포트 출력                                               │
│                                                                   │
│  WARN (CONSENSUS 이슈 0개 + 점수 ≥ 60):                          │
│  → 경고 표시, 진행 가능                                           │
│  → AskUserQuestion: "경고 사항이 있습니다. 진행할까요?"           │
│                                                                   │
│  FAIL:                                                             │
│  → PHASE D로 이동 (자동 정제)                                     │
│                                                                   │
│  MAX_ITERATIONS 도달:                                              │
│  → 에스컬레이션                                                    │
│  → AskUserQuestion: "최대 반복에 도달했습니다. 수동 수정?"        │
└───────────────────────────────────────────────────────────────────┘
```

### PHASE D: 스펙 자동 정제 (RALPH)

```
┌───────────────────────────────────────────────────────────────────┐
│  PHASE D: 자동 정제 (RALPH Feedback Loop)                         │
│                                                                   │
│  consensus-report.md를 기반으로 스펙 자동 수정:                   │
│                                                                   │
│  1. CONSENSUS 이슈 자동 반영 (반드시)                              │
│     • Contract Test 보강                                          │
│     • 의존성 그래프 수정                                          │
│     • 누락 엣지 케이스 추가                                       │
│                                                                   │
│  2. MAJORITY 이슈 자동 반영 (기본 반영)                            │
│     • 아키텍처 단순화                                              │
│     • 네이밍 일관성 교정                                          │
│                                                                   │
│  3. MINORITY 이슈 기록 (반영하지 않음)                             │
│     • 다음 단계 고려사항으로 기록                                  │
│                                                                   │
│  수정 완료 후 → PHASE A로 돌아감 (iteration++)                    │
│                                                                   │
│  산출물:                                                          │
│  ${SESSION_DIR}/reviews/iteration-{n}/                            │
│  └── refinement-log.md   ← 무엇을 왜 수정했는지 기록             │
└───────────────────────────────────────────────────────────────────┘
```

---

## 리뷰 프롬프트 구성

### Claude (spec-reviewer)

기존 spec-reviewer 에이전트를 그대로 사용합니다. 입력:

```yaml
sessionId: "{session-id}"
specs:
  architecture: ".team-claude/sessions/{session-id}/specs/architecture.md"
  contracts: ".team-claude/sessions/{session-id}/specs/contracts.md"
  checkpoints: ".team-claude/sessions/{session-id}/specs/checkpoints.yaml"
```

### Codex (spec-reviewer-codex)

```
리뷰 종류: spec

컨텍스트:
- 목적: Contract 기반 병렬 실행을 위한 스펙 리뷰
- 관점: 구현 가능성 중심 리뷰어 (Senior Engineer)
- 반복: {iteration}/{max_iterations}
- 이전 이슈: {previous_consensus_issues}

대상 파일:
- {architecture.md 경로}
- {contracts.md 경로}
- {checkpoints.yaml 경로}

평가 기준:
1. Contract 실행 가능성 - Interface가 실제 구현 가능한가? Test가 실행되는가?
2. 코드 품질 - Contract Test의 코드 품질, 가독성, 유지보수성
3. 테스트 충분성 - 엣지 케이스, 에러 경로가 커버되는가?
4. Checkpoint 분할 - 각 단위가 독립적으로 구현/테스트 가능한가?
5. 검증 명령어 - validation.command가 정확하고 결정적인가?

위 파일들을 구현 가능성 관점에서 리뷰하고,
점수(0-100)와 이슈 목록(Critical/Important/Nice-to-have)을 출력해주세요.
```

### Gemini (spec-reviewer-gemini)

```
리뷰 종류: spec

컨텍스트:
- 목적: Contract 기반 병렬 실행을 위한 스펙 리뷰
- 관점: 대안 설계 및 리스크 분석 (Architect)
- 반복: {iteration}/{max_iterations}
- 이전 이슈: {previous_consensus_issues}

대상 파일:
- {architecture.md 경로}
- {contracts.md 경로}
- {checkpoints.yaml 경로}

평가 기준:
1. 대안 설계 - 더 나은 아키텍처 옵션이 있는가?
2. 리스크 분석 - 구현 과정에서 발생할 수 있는 리스크는?
3. 확장성 - 향후 요구사항 변경에 유연한가?
4. 누락 패턴 - 업계 표준 또는 모범 사례에서 빠진 것은?
5. 트레이드오프 - 현재 설계의 트레이드오프가 명시적인가?

위 파일들을 대안 관점에서 리뷰하고,
점수(0-100)와 이슈 목록(Critical/Important/Nice-to-have)을 출력해주세요.
```

---

## 출력 예시

### 시작

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Spec Refine - Multi-Review RALPH Loop

  세션: abc12345
  스펙 파일: 3개
  리뷰어: Claude, Codex, Gemini
  최대 반복: 5
  통과 기준: 80점

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

--- Iteration 1/5 ---

  [A] 3-LLM 병렬 리뷰 실행 중...
      Claude:  완료 (75/100)
      Codex:   완료 (80/100)
      Gemini:  완료 (70/100)

  [B] 합의 분석 중...
      CONSENSUS 이슈: 1개
      MAJORITY 이슈:  1개
      MINORITY 이슈:  1개

  [C] 판정: FAIL (통합 점수: 75.5 < 80)

  [D] 자동 정제 중...
      [CONSENSUS] Contract Test 에러 케이스 추가... 완료
      [MAJORITY]  의존성 그래프 단순화... 완료
      [MINORITY]  캐싱 전략 → 기록만 (반영 안함)

--- Iteration 2/5 ---

  [A] 3-LLM 병렬 리뷰 실행 중...
      Claude:  완료 (90/100)
      Codex:   완료 (88/100)
      Gemini:  완료 (82/100)

  [B] 합의 분석 중...
      CONSENSUS 이슈: 0개
      MAJORITY 이슈:  0개
      MINORITY 이슈:  1개

  [C] 판정: PASS (통합 점수: 87.1 >= 80)
```

### 완료

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Spec Refine 완료

  결과: PASS
  반복 횟수: 2/5
  최종 점수: 87.1/100

━━━ 반복별 점수 추이 ━━━

  Iteration 1: 75.5 (FAIL) → 자동 정제
  Iteration 2: 87.1 (PASS)

━━━ 반영된 개선 사항 ━━━

  1. [CONSENSUS] Contract Test 에러 케이스 3개 추가
     - CouponService.apply() 만료 쿠폰 에러
     - CouponService.apply() 사용 한도 초과 에러
     - CouponRepository.save() 중복 코드 에러

  2. [MAJORITY] 의존성 그래프 단순화
     - coupon-api → coupon-core (중간 레이어 제거)

━━━ 미반영 사항 (향후 고려) ━━━

  1. [MINORITY] 캐싱 전략 (Gemini 제안)

━━━ 산출물 ━━━

  리뷰 이력: .team-claude/sessions/abc12345/reviews/
  정제 로그: .team-claude/sessions/abc12345/reviews/refinement-summary.md

━━━ 다음 단계 ━━━

  스펙이 승인되었습니다. 구현을 시작할 수 있습니다.
  → /team-claude:delegate --session abc12345

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### 에스컬레이션

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Spec Refine - 에스컬레이션

  결과: MAX_ITERATIONS 도달 (5/5)
  최종 점수: 72.3/100

━━━ 미해결 이슈 ━━━

  1. [CONSENSUS] 순환 의존성 해소 불가
     - 3개 LLM 모두 동의하지만 자동 해결 실패
     - 수동 아키텍처 재설계 필요

  2. [MAJORITY] Integration Test 전략 불명확
     - 환경 의존성이 높아 Contract만으로 검증 어려움

━━━ 권장 조치 ━━━

  1. 수동 재설계: /team-claude:architect --resume abc12345
  2. 특정 부분만 수정 후 재실행: /team-claude:spec-refine --session abc12345

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

---

## 설정

```yaml
# .claude/team-claude.yaml
specRefine:
  enabled: true
  maxIterations: 5
  passThreshold: 80
  warnThreshold: 60

  reviewers:
    claude:
      enabled: true
      weight: 0.40
      agent: spec-reviewer
    codex:
      enabled: true
      weight: 0.35
      agent: spec-reviewer-codex
    gemini:
      enabled: true
      weight: 0.25
      agent: spec-reviewer-gemini

  consensus:
    autoApplyConsensus: true    # 3/3 동의 이슈 자동 반영
    autoApplyMajority: true     # 2/3 동의 이슈 자동 반영
    recordMinority: true        # 1/3 이슈는 기록만

  refinement:
    autoRefine: true            # FAIL 시 자동 정제
    humanApprovalOnWarn: true   # WARN 시 사용자 확인
```

---

## 파일 구조

```
.team-claude/sessions/{session-id}/
├── specs/
│   ├── architecture.md        # 정제 대상
│   ├── contracts.md           # 정제 대상
│   └── checkpoints.yaml       # 정제 대상
│
└── reviews/
    ├── iteration-1/
    │   ├── claude-review.md   # Claude 리뷰 결과
    │   ├── codex-review.md    # Codex 리뷰 결과
    │   ├── gemini-review.md   # Gemini 리뷰 결과
    │   ├── consensus-report.md # 합의 분석 리포트
    │   └── refinement-log.md  # 정제 기록
    ├── iteration-2/
    │   └── ...
    └── refinement-summary.md  # 전체 정제 요약
```
