---
name: team-claude:spec-refine
description: 동적 관점 멀티 리뷰 + RALPH 루프로 스펙을 반복 개선 - 스펙 내용에 따라 최적의 리뷰 관점을 자동 선택
argument-hint: "--session <id> [--max-iterations 5] [--pass-threshold 80]"
allowed-tools: ["Task", "Bash", "Read", "Write", "Glob", "Grep", "AskUserQuestion"]
---

# Spec Refine Command - 동적 관점 멀티 리뷰 RALPH 루프

> **먼저 읽기**: `${CLAUDE_PLUGIN_ROOT}/INFRASTRUCTURE.md`

스펙 내용을 분석하여 최적의 리뷰 관점(디자이너, PM, CTO, 보안 전문가 등)을 동적으로 결정하고, 다수의 LLM에 분배하여 병렬 리뷰한 뒤, RALPH 루프로 자동 정제합니다.

---

## 핵심 원칙

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  SPEC REFINE: Dynamic Perspective Multi-Review RALPH Loop                   │
│                                                                             │
│  고정 관점 (AS-IS):                                                         │
│    Claude → 아키텍처 / Codex → 구현 / Gemini → 대안                        │
│    → 매번 같은 관점, 스펙 성격과 무관                                       │
│                                                                             │
│  동적 관점 (TO-BE):                                                         │
│    스펙 분석 → Perspective Planner → [PM, 보안, DBA, QA, ...]              │
│    → 스펙마다 다른 관점, 도메인에 최적화                                    │
│                                                                             │
│  흐름:                                                                      │
│    1. Perspective Planner가 스펙을 읽고 관점 3-4개 생성                     │
│    2. 각 관점을 LLM 엔진(Claude/Codex/Gemini)에 분배                       │
│    3. 병렬 리뷰 실행                                                        │
│    4. 합의 분석 + 점수 산출                                                 │
│    5. FAIL → 자동 정제 → 관점 재선정 → 재리뷰                              │
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

# 관점 수 지정 (기본 3-4)
/team-claude:spec-refine --session abc12345 --max-perspectives 5
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
│  │  PHASE A: Perspective Planner (관점 동적 결정)              │ │
│  │  PHASE B: 병렬 리뷰 실행                                   │ │
│  │  PHASE C: 리뷰 통합 및 합의 분석                            │ │
│  │  PHASE D: 통과 여부 판정                                    │ │
│  │  PHASE E: 스펙 자동 정제 (FAIL인 경우)                      │ │
│  │                                                             │ │
│  └─────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────┘
```

### PHASE A: Perspective Planner (관점 동적 결정)

```
┌───────────────────────────────────────────────────────────────────┐
│  PHASE A: 동적 관점 결정                                          │
│                                                                   │
│  perspective-planner 에이전트를 호출하여 관점 생성:               │
│                                                                   │
│  입력:                                                            │
│  • 스펙 파일 (architecture.md, contracts.md, checkpoints.yaml)    │
│  • iteration 번호                                                 │
│  • 이전 반복의 미해결 이슈 목록                                   │
│  • maxPerspectives (기본 4)                                       │
│                                                                   │
│  Planner 분석:                                                    │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  1. 도메인 파악 (결제? UI? 인프라? 데이터?)                 │ │
│  │  2. 리스크 영역 식별 (보안? 성능? 데이터 무결성?)           │ │
│  │  3. 이해관계자 파악 (누가 영향받는가?)                      │ │
│  │  4. 이전 이슈 반영 (미해결 영역 재검증)                     │ │
│  │  5. 최적 관점 선택 + LLM 엔진 할당 + 가중치 설정            │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                   │
│  출력 예시 (쿠폰-결제 시스템):                                    │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  perspectives:                                              │ │
│  │    1. 보안 전문가  (codex,  w=0.30) "PCI DSS 검증"         │ │
│  │    2. PM           (gemini, w=0.25) "비즈니스 규칙 정합성"  │ │
│  │    3. DBA          (claude, w=0.25) "동시성, 트랜잭션"      │ │
│  │    4. QA 엔지니어  (claude, w=0.20) "Contract Test 충분성"  │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                   │
│  출력 예시 (디자인 시스템 컴포넌트):                               │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  perspectives:                                              │ │
│  │    1. 디자이너     (claude, w=0.30) "디자인 시스템 일관성"  │ │
│  │    2. 프론트엔드   (codex,  w=0.30) "컴포넌트 API, 접근성" │ │
│  │    3. 접근성 전문가(gemini, w=0.25) "WCAG 2.1 AA 준수"     │ │
│  │    4. 주니어 개발자(claude, w=0.15) "사용 난이도, 문서"     │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                   │
│  2차 반복 (보안 이슈 미해결 시):                                   │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  perspectives:                                              │ │
│  │    1. 보안 전문가  (codex,  w=0.40) "[미해결] 재검증"      │ │
│  │    2. 백엔드 엔지니어(claude,w=0.35) "보안 수정 영향도"    │ │
│  │    3. QA 엔지니어  (claude, w=0.25) "회귀 테스트 충분성"   │ │
│  └─────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────┘
```

### PHASE B: 병렬 리뷰 실행

```
┌───────────────────────────────────────────────────────────────────┐
│  PHASE B: 동적 관점 기반 병렬 리뷰                                │
│                                                                   │
│  Planner가 생성한 관점별로 리뷰 에이전트 실행:                    │
│                                                                   │
│  각 관점에 대해:                                                   │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  engine == "claude"인 관점:                                  │ │
│  │    → Task 도구로 spec-reviewer 에이전트 호출                │ │
│  │    → 관점(role)과 focus를 프롬프트에 포함                   │ │
│  │                                                             │ │
│  │  engine == "codex"인 관점:                                   │ │
│  │    → Task 도구로 리뷰 프롬프트를 call-codex.sh에 전달      │ │
│  │    → 관점(role)과 focus를 프롬프트에 포함                   │ │
│  │                                                             │ │
│  │  engine == "gemini"인 관점:                                  │ │
│  │    → Task 도구로 리뷰 프롬프트를 call-gemini.sh에 전달     │ │
│  │    → 관점(role)과 focus를 프롬프트에 포함                   │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                   │
│  모든 관점을 병렬(Task 동시 호출)로 실행                          │
│                                                                   │
│  예시 (쿠폰-결제):                                                │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌──────────┐│
│  │  보안 전문가  │ │     PM       │ │     DBA      │ │  QA      ││
│  │  (Codex)     │ │  (Gemini)    │ │  (Claude)    │ │ (Claude) ││
│  │              │ │              │ │              │ │          ││
│  │  PCI DSS    │ │  비즈니스    │ │  동시성      │ │ Contract ││
│  │  토큰화     │ │  시나리오    │ │  트랜잭션    │ │ 충분성   ││
│  │  암호화     │ │  엣지케이스  │ │  인덱스      │ │ 에러경로 ││
│  └──────┬───────┘ └──────┬───────┘ └──────┬───────┘ └────┬─────┘│
│         └────────────────┼────────────────┼──────────────┘      │
│                          ▼                                       │
│                   리뷰 결과 N개                                   │
└───────────────────────────────────────────────────────────────────┘
```

**리뷰 프롬프트 템플릿 (공통):**

모든 관점에 대해 동일한 구조의 프롬프트를 사용하되, role과 focus만 변경:

```
리뷰 종류: spec (Contract 기반 병렬 실행을 위한 스펙 리뷰)

당신의 역할: {perspective.role}
{perspective.reason}

집중 영역:
{perspective.focus (각 항목 bullet)}

반복: {iteration}/{max_iterations}
이전 미해결 이슈: {previous_issues}

대상 파일:
- {architecture.md 경로}
- {contracts.md 경로}
- {checkpoints.yaml 경로}

위 파일들을 "{perspective.role}" 관점에서 리뷰해주세요.

출력 형식:
## {perspective.role} 리뷰 결과

### 점수: [0-100]

### 이슈 목록
#### Critical
1. [이슈 - 구체적 위치와 수정 제안]

#### Important
1. [이슈]

#### Nice-to-have
1. [이슈]

### 구체적 개선 제안
[코드 예시 또는 구조 변경 제안]
```

### PHASE C: 리뷰 통합 및 합의 분석

```
┌───────────────────────────────────────────────────────────────────┐
│  PHASE C: 합의 분석 (Consensus Analysis)                         │
│                                                                   │
│  N개 리뷰 결과를 분석하여 통합 피드백 생성:                       │
│                                                                   │
│  1. 이슈 추출                                                     │
│     각 관점의 리뷰에서 이슈 항목 추출 (Critical/Important/Nice)   │
│                                                                   │
│  2. 합의도 분석 (N개 관점 기준)                                    │
│     ┌─────────────────────────────────────────────────────┐      │
│     │  N/N 동의    → CONSENSUS (반드시 반영)               │      │
│     │  ≥N/2 동의  → MAJORITY  (강력 권장)                 │      │
│     │  <N/2 제기   → MINORITY  (검토 필요)                │      │
│     │                                                     │      │
│     │  "동의" 판정: 이슈의 핵심 영역이 동일한 경우        │      │
│     │  (정확히 같은 표현이 아니어도 같은 문제를 지적하면   │      │
│     │   동의로 간주)                                       │      │
│     └─────────────────────────────────────────────────────┘      │
│                                                                   │
│  3. 점수 산출                                                     │
│     ┌─────────────────────────────────────────────────────┐      │
│     │  가중 평균 점수:                                     │      │
│     │  Σ (관점별 점수 × 관점별 가중치) = 통합 점수 (0-100) │      │
│     │                                                     │      │
│     │  가중치는 Perspective Planner가 관점별로 설정         │      │
│     │  (도메인 중요도에 따라 유동적)                        │      │
│     └─────────────────────────────────────────────────────┘      │
│                                                                   │
│  4. 판정                                                          │
│     점수 ≥ pass_threshold → PASS                                 │
│     CONSENSUS 이슈 0개 + 점수 ≥ warn_threshold → WARN           │
│     그 외 → FAIL                                                  │
│                                                                   │
│  산출물:                                                          │
│  ${SESSION_DIR}/reviews/iteration-{n}/                            │
│  ├── perspectives.yaml        ← Planner가 결정한 관점 목록       │
│  ├── {role-1}-review.md       ← 각 관점별 리뷰 결과              │
│  ├── {role-2}-review.md                                           │
│  ├── ...                                                          │
│  └── consensus-report.md      ← 통합 분석 결과                   │
└───────────────────────────────────────────────────────────────────┘
```

**통합 리포트 형식:**

```markdown
## Spec Review - Iteration {n} 통합 리포트

### 선택된 관점
| # | 관점 | 엔진 | 가중치 | 선택 이유 |
|---|------|------|--------|-----------|
| 1 | 보안 전문가 | Codex | 0.30 | 결제 시스템 PCI DSS 검증 |
| 2 | PM | Gemini | 0.25 | 쿠폰 비즈니스 규칙 정합성 |
| 3 | DBA | Claude | 0.25 | 동시성, 트랜잭션 무결성 |
| 4 | QA 엔지니어 | Claude | 0.20 | Contract Test 충분성 |

### 점수
| 관점 | 점수 | 가중치 | 가중 점수 |
|------|------|--------|-----------|
| 보안 전문가 | 65 | 0.30 | 19.5 |
| PM | 80 | 0.25 | 20.0 |
| DBA | 75 | 0.25 | 18.75 |
| QA 엔지니어 | 70 | 0.20 | 14.0 |
| **통합** | | | **72.25** |

### 판정: FAIL (threshold: 80)

### CONSENSUS 이슈 (4/4 동의 - 반드시 수정)
1. **Contract Test 에러 경로 누락**
   - 보안 전문가: "인증 실패 시 토큰 무효화 테스트 없음"
   - PM: "만료 쿠폰 적용 시나리오 미커버"
   - DBA: "동시 적용 시 race condition 테스트 없음"
   - QA: "에러 경로 커버리지 40% 미만"

### MAJORITY 이슈 (3/4 동의 - 강력 권장)
1. **데이터 모델 정규화 필요**
   - 보안 전문가: "카드 정보와 쿠폰 데이터 분리 필요"
   - DBA: "쿠폰-주문 관계 테이블 설계 개선"
   - QA: "테스트 데이터 셋업 복잡도 높음"

### MINORITY 이슈 (1/4 제기 - 검토)
1. **캐싱 전략** (PM): "자주 조회되는 쿠폰 목록 캐싱"
2. **모니터링** (보안): "결제 실패율 알림 설정"

### 개선 방향
1. [CONSENSUS] 에러 경로 Contract Test 대폭 보강
2. [MAJORITY] 데이터 모델 재설계 검토
3. [MINORITY] 캐싱, 모니터링은 구현 단계에서 고려
```

### PHASE D: 통과 여부 판정

```
┌───────────────────────────────────────────────────────────────────┐
│  PHASE D: 판정                                                    │
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
│  → PHASE E로 이동 (자동 정제)                                     │
│                                                                   │
│  MAX_ITERATIONS 도달:                                              │
│  → 에스컬레이션                                                    │
│  → AskUserQuestion: "최대 반복에 도달했습니다. 수동 수정?"        │
└───────────────────────────────────────────────────────────────────┘
```

### PHASE E: 스펙 자동 정제 (RALPH)

```
┌───────────────────────────────────────────────────────────────────┐
│  PHASE E: 자동 정제 (RALPH Feedback Loop)                         │
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
│  수정 완료 후:                                                     │
│  → PHASE A로 돌아감 (iteration++)                                 │
│  → Perspective Planner가 미해결 이슈 기반으로 관점 재선정          │
│    (이전 반복의 문제 영역에 집중하는 관점으로 변경)                │
│                                                                   │
│  산출물:                                                          │
│  ${SESSION_DIR}/reviews/iteration-{n}/                            │
│  └── refinement-log.md   ← 무엇을 왜 수정했는지 기록             │
└───────────────────────────────────────────────────────────────────┘
```

---

## 출력 예시

### 시작

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Spec Refine - Dynamic Perspective Multi-Review RALPH Loop

  세션: abc12345
  스펙 파일: 3개
  최대 반복: 5
  통과 기준: 80점

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

--- Iteration 1/5 ---

  [A] Perspective Planner 실행 중...
      도메인 분석: 쿠폰-결제 시스템
      선택된 관점:
        1. 보안 전문가  (Codex,  w=0.30)
        2. PM           (Gemini, w=0.25)
        3. DBA          (Claude, w=0.25)
        4. QA 엔지니어  (Claude, w=0.20)

  [B] 병렬 리뷰 실행 중...
      보안 전문가:  완료 (65/100)
      PM:           완료 (80/100)
      DBA:          완료 (75/100)
      QA 엔지니어:  완료 (70/100)

  [C] 합의 분석 중...
      CONSENSUS 이슈: 1개
      MAJORITY 이슈:  1개
      MINORITY 이슈:  2개

  [D] 판정: FAIL (통합 점수: 72.25 < 80)

  [E] 자동 정제 중...
      [CONSENSUS] 에러 경로 Contract Test 보강... 완료
      [MAJORITY]  데이터 모델 정규화... 완료
      [MINORITY]  캐싱, 모니터링 → 기록만

--- Iteration 2/5 ---

  [A] Perspective Planner 실행 중...
      미해결 이슈 반영: 보안 검증 강화 필요
      선택된 관점:
        1. 보안 전문가  (Codex,  w=0.40)  ← 가중치 상향
        2. 백엔드 엔지니어(Claude, w=0.35)  ← 보안 수정 영향도
        3. QA 엔지니어  (Claude, w=0.25)  ← 회귀 테스트

  [B] 병렬 리뷰 실행 중...
      보안 전문가:    완료 (85/100)
      백엔드 엔지니어: 완료 (88/100)
      QA 엔지니어:    완료 (82/100)

  [C] 합의 분석 중...
      CONSENSUS 이슈: 0개
      MAJORITY 이슈:  0개
      MINORITY 이슈:  1개

  [D] 판정: PASS (통합 점수: 85.5 >= 80)
```

### 완료

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Spec Refine 완료

  결과: PASS
  반복 횟수: 2/5
  최종 점수: 85.5/100

━━━ 관점 변화 추이 ━━━

  Iteration 1: 보안 전문가, PM, DBA, QA 엔지니어
  Iteration 2: 보안 전문가(강화), 백엔드 엔지니어, QA 엔지니어

━━━ 반복별 점수 추이 ━━━

  Iteration 1: 72.25 (FAIL) → 자동 정제
  Iteration 2: 85.5  (PASS)

━━━ 반영된 개선 사항 ━━━

  1. [CONSENSUS] 에러 경로 Contract Test 5개 추가
  2. [MAJORITY] 쿠폰-주문 데이터 모델 정규화

━━━ 미반영 사항 (향후 고려) ━━━

  1. [MINORITY] 캐싱 전략 (PM 제안)
  2. [MINORITY] 결제 실패율 모니터링 (보안 전문가 제안)

━━━ 산출물 ━━━

  리뷰 이력:  .team-claude/sessions/abc12345/reviews/
  정제 로그:  .team-claude/sessions/abc12345/reviews/refinement-summary.md

━━━ 다음 단계 ━━━

  스펙이 승인되었습니다. 구현을 시작할 수 있습니다.
  → /team-claude:delegate --session abc12345

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

---

## 상태 모델 (SpecRefineState)

> 타입 정의: `cli/src/lib/common.ts`

spec-refine의 핵심은 **iteration 간 전달되는 상태(`carry`)**입니다.
Planner가 다음 관점을 결정하려면, 이전에 뭐가 문제였고 뭐가 해결됐는지 알아야 합니다.

### 상태 파일 위치

```
${SESSION_DIR}/refine-state.json
```

### 상태 전이 다이어그램

```
┌────────┐
│  idle  │ ── spec-refine 시작 ──▶ ┌─────────┐
└────────┘                          │ running │
                                    └────┬────┘
                                         │
          ┌──────────────────────────────┘
          │
          ▼
    ┌─────────────┐
    │  iteration  │◀─────────────────────────────┐
    │  시작       │                               │
    └──────┬──────┘                               │
           │                                      │
    ┌──────▼──────┐                               │
    │  Planner    │ ◀── carry.unresolvedIssues    │
    │  관점 결정  │ ◀── carry.perspectiveHistory  │
    └──────┬──────┘                               │
           │                                      │
    ┌──────▼──────┐                               │
    │  병렬 리뷰  │                               │
    └──────┬──────┘                               │
           │                                      │
    ┌──────▼──────┐                               │
    │  합의 분석  │ ──▶ consensusIssues           │
    │  점수 산출  │ ──▶ weightedScore             │
    └──────┬──────┘                               │
           │                                      │
     ┌─────┼─────┐                                │
     │     │     │                                │
     ▼     ▼     ▼                                │
   PASS  WARN  FAIL                               │
     │     │     │                                │
     │     │     ▼                                │
     │     │  ┌──────────┐                        │
     │     │  │  정제    │                        │
     │     │  │  carry   │ ──── update ──────────▶│
     │     │  │  업데이트│                        │
     │     │  └──────────┘                        │
     │     │                                      │
     ▼     ▼                                      │
  ┌────────────┐         ┌─────────────┐          │
  │  passed /  │         │  escalated  │◀── max   │
  │  warned    │         │             │   iter ──┘
  └────────────┘         └─────────────┘
```

### 상태 구조

```typescript
interface SpecRefineState {
  sessionId: string;
  status: "idle" | "running" | "passed" | "warned" | "failed" | "escalated";

  // 설정
  config: {
    maxIterations: number;    // 최대 반복 횟수 (기본 5)
    passThreshold: number;    // 통과 점수 (기본 80)
    warnThreshold: number;    // 경고 점수 (기본 60)
    maxPerspectives: number;  // 최대 관점 수 (기본 4)
  };

  // 현재 반복
  currentIteration: number;
  iterations: RefineIteration[];  // 모든 반복의 상세 기록

  // ━━━ 핵심: 반복 간 전달 상태 ━━━
  carry: {
    unresolvedIssues: ConsensusIssue[];  // → Planner 입력
    resolvedIssues: ConsensusIssue[];    // → 관점 제외 근거
    scoreHistory: number[];              // → 개선 추세 판단
    perspectiveHistory: string[][];      // → 중복 관점 방지
  };

  // 타임스탬프
  startedAt: string;
  updatedAt: string;
  completedAt: string | null;
}
```

### carry 필드 상세

| 필드 | 누가 쓰는가 | 누가 읽는가 | 용도 |
|------|-------------|-------------|------|
| `unresolvedIssues` | 합의 분석 (PHASE C) | Perspective Planner (PHASE A) | 미해결 이슈 영역의 관점 선택, 가중치 상향 |
| `resolvedIssues` | 정제 (PHASE E) | Perspective Planner (PHASE A) | 해결된 영역의 관점 제외/교체 |
| `scoreHistory` | 합의 분석 (PHASE C) | 판정 (PHASE D) | 점수 개선 추세로 에스컬레이션 판단 |
| `perspectiveHistory` | Planner (PHASE A) | Planner (PHASE A) | 이전과 동일한 관점 조합 방지 |

### 상태 라이프사이클

#### 1. 초기화 (spec-refine 시작)

```json
{
  "sessionId": "abc12345",
  "status": "running",
  "config": { "maxIterations": 5, "passThreshold": 80, "warnThreshold": 60, "maxPerspectives": 4 },
  "currentIteration": 0,
  "iterations": [],
  "carry": {
    "unresolvedIssues": [],
    "resolvedIssues": [],
    "scoreHistory": [],
    "perspectiveHistory": []
  },
  "startedAt": "2026-02-07T10:00:00Z",
  "updatedAt": "2026-02-07T10:00:00Z",
  "completedAt": null
}
```

#### 2. Iteration 1 완료 후 (FAIL)

```json
{
  "status": "running",
  "currentIteration": 1,
  "iterations": [{
    "iteration": 1,
    "perspectives": [
      { "role": "보안 전문가", "engine": "codex", "weight": 0.30, "..." : "..." },
      { "role": "PM", "engine": "gemini", "weight": 0.25, "..." : "..." },
      { "role": "DBA", "engine": "claude", "weight": 0.25, "..." : "..." },
      { "role": "QA 엔지니어", "engine": "claude", "weight": 0.20, "..." : "..." }
    ],
    "reviews": [
      { "perspective": "보안 전문가", "score": 65, "..." : "..." },
      { "perspective": "PM", "score": 80, "..." : "..." },
      { "perspective": "DBA", "score": 75, "..." : "..." },
      { "perspective": "QA 엔지니어", "score": 70, "..." : "..." }
    ],
    "consensusIssues": [
      {
        "summary": "Contract Test 에러 경로 누락",
        "level": "consensus",
        "agreedBy": ["보안 전문가", "PM", "DBA", "QA 엔지니어"],
        "resolved": false
      },
      {
        "summary": "데이터 모델 정규화 필요",
        "level": "majority",
        "agreedBy": ["보안 전문가", "DBA", "QA 엔지니어"],
        "resolved": false
      }
    ],
    "weightedScore": 72.25,
    "verdict": "fail",
    "refinementActions": [
      "contracts.md: 에러 경로 테스트 5개 추가",
      "architecture.md: 쿠폰-주문 데이터 모델 정규화"
    ]
  }],
  "carry": {
    "unresolvedIssues": [
      { "summary": "Contract Test 에러 경로 누락", "level": "consensus", "resolved": false }
    ],
    "resolvedIssues": [
      { "summary": "데이터 모델 정규화 필요", "level": "majority", "resolved": true, "resolvedAt": "iteration-1" }
    ],
    "scoreHistory": [72.25],
    "perspectiveHistory": [["보안 전문가", "PM", "DBA", "QA 엔지니어"]]
  }
}
```

#### 3. Iteration 2 - Planner가 carry를 읽음

```
Planner 입력:
  unresolvedIssues: ["Contract Test 에러 경로 누락" (consensus)]
  resolvedIssues: ["데이터 모델 정규화" (majority, iteration-1에서 해결)]
  scoreHistory: [72.25]
  perspectiveHistory: [["보안 전문가", "PM", "DBA", "QA 엔지니어"]]

Planner 판단:
  - 에러 경로 누락이 미해결 → 보안 전문가 유지 + 가중치 상향 (0.30→0.40)
  - 데이터 모델은 해결됨 → DBA 제외
  - PM 관점은 80점으로 양호 → 제외
  - 보안 수정의 영향도를 볼 새 관점 필요 → 백엔드 엔지니어 추가
  - QA는 회귀 테스트 검증 위해 유지

Planner 출력:
  1. 보안 전문가    (codex,  w=0.40)
  2. 백엔드 엔지니어 (claude, w=0.35)
  3. QA 엔지니어    (claude, w=0.25)
```

#### 4. PASS 시 최종 상태

```json
{
  "status": "passed",
  "currentIteration": 2,
  "carry": {
    "unresolvedIssues": [],
    "resolvedIssues": [
      { "summary": "데이터 모델 정규화", "resolved": true, "resolvedAt": "iteration-1" },
      { "summary": "Contract Test 에러 경로", "resolved": true, "resolvedAt": "iteration-2" }
    ],
    "scoreHistory": [72.25, 85.5],
    "perspectiveHistory": [
      ["보안 전문가", "PM", "DBA", "QA 엔지니어"],
      ["보안 전문가", "백엔드 엔지니어", "QA 엔지니어"]
    ]
  },
  "completedAt": "2026-02-07T10:15:00Z"
}
```

### 에스컬레이션 판단 기준

```
scoreHistory를 기반으로 판단:

1. 점수 정체: 최근 2회 점수 차이 < 3점 → "자동 정제로 개선 불가"
2. 점수 하락: 이전보다 낮아짐 → "정제가 역효과"
3. 동일 이슈 반복: unresolvedIssues에 3회 이상 동일 이슈 → "구조적 문제"
4. 최대 반복 도달: currentIteration >= maxIterations

→ 위 조건 충족 시 status = "escalated"
```

---

## Hook 연동 (결정적 실행 보장)

> 구현: `cli/src/commands/hook.ts`, 설정: `hooks/hooks.json`

LLM이 상태 전이를 자발적으로 하길 기대하면 비결정적입니다.
Hook이 이벤트를 감지하고 상태를 **자동으로** 업데이트합니다.

### Hook 목록

| Hook | 트리거 | 명령어 | 역할 |
|------|--------|--------|------|
| `PostToolUse(Task)` | 리뷰 에이전트 완료 | `tc hook refine-review-complete` | 리뷰 수집 추적, 전체 완료 알림 |
| `PostToolUse(Bash)` | call-codex/gemini 완료 | `tc hook refine-review-complete` | 외부 LLM 리뷰 수집 추적 |
| `PostToolUse(Write)` | specs/ 파일 수정 | `tc hook refine-spec-modified` | 정제 액션 자동 기록 |
| `Stop` | iteration 종료 | `tc hook refine-iteration-end` | carry 업데이트, 에스컬레이션 판단 |

### Hook이 강제하는 전이

```
LLM이 하는 것           Hook이 하는 것
─────────────────────   ──────────────────────────────────
Planner 호출            (LLM)
리뷰 에이전트 호출      (LLM)
                        → refine-review-complete:
                          리뷰 카운트 추적
                          전체 완료 시 알림
합의 분석               (LLM)
판정                    (LLM, 상태에 verdict 기록)
스펙 파일 수정          (LLM)
                        → refine-spec-modified:
                          정제 액션 자동 기록
iteration 종료          → refine-iteration-end:
                          carry.scoreHistory 업데이트
                          carry.perspectiveHistory 업데이트
                          carry.unresolvedIssues 분류
                          carry.resolvedIssues 분류
                          에스컬레이션 판단 (자동)
                          status 전이 (passed/escalated/fail)
                          OS 알림
```

### 핵심: LLM은 실행만, Hook은 상태 관리

```
┌──────────────────────────────────────────────────────────────┐
│  LLM의 책임 (실행):                                          │
│  • Planner 호출하여 관점 생성                                │
│  • 리뷰 에이전트/스크립트 호출                               │
│  • 리뷰 결과를 합의 분석                                     │
│  • verdict를 refine-state.json에 기록                        │
│  • FAIL 시 스펙 파일 수정 (정제)                             │
│                                                              │
│  Hook의 책임 (상태):                                         │
│  • 리뷰 완료 카운트 추적 (refine-review-complete)            │
│  • 스펙 수정 액션 기록 (refine-spec-modified)                │
│  • carry 업데이트 - 이슈 분류, 점수/관점 이력                │
│  • 에스컬레이션 판단 - 정체/하락/반복 감지                   │
│  • status 전이 - passed/warned/escalated                     │
│  • OS 알림                                                   │
│                                                              │
│  분리 이유:                                                   │
│  • LLM이 carry를 직접 조작하면 실수할 수 있음                │
│  • 에스컬레이션은 규칙 기반이므로 코드가 더 정확             │
│  • Hook은 매번 확실하게 실행됨 (LLM의 "깜빡함" 없음)        │
└──────────────────────────────────────────────────────────────┘
```

### 에스컬레이션 자동 판단 (refine-iteration-end)

```typescript
// Hook이 자동으로 판단하는 에스컬레이션 조건:

// 1. 점수 정체 (최근 2회 차이 < 3점)
if (Math.abs(scoreHistory[-1] - scoreHistory[-2]) < 3) → escalate

// 2. 점수 하락
if (scoreHistory[-1] < scoreHistory[-2]) → escalate

// 3. 동일 이슈 3회 이상 반복
if (unresolvedIssues.some(issue => appearances >= 3)) → escalate

// 4. 최대 반복 도달
if (currentIteration >= maxIterations) → escalate
```

LLM이 "한 번 더 해볼까?" 하고 고민할 필요 없이,
Hook이 `status = "escalated"`로 바꿔버리면 LLM은 그냥 에스컬레이션 리포트를 출력합니다.

---

## 설정

```yaml
# .claude/team-claude.yaml
specRefine:
  enabled: true
  maxIterations: 5
  passThreshold: 80
  warnThreshold: 60
  maxPerspectives: 4          # Planner가 생성할 최대 관점 수

  engines:                     # 사용 가능한 LLM 엔진
    claude:
      enabled: true
      agent: spec-reviewer     # Claude는 에이전트로 실행
    codex:
      enabled: true
      script: "common/scripts/call-codex.sh"
    gemini:
      enabled: true
      script: "common/scripts/call-gemini.sh"

  planner:
    agent: perspective-planner  # 관점 결정 에이전트
    adaptOnRetry: true          # 재시도 시 관점 재선정

  consensus:
    autoApplyConsensus: true    # 전원 동의 이슈 자동 반영
    autoApplyMajority: true     # 과반 동의 이슈 자동 반영
    recordMinority: true        # 소수 이슈는 기록만

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
    │   ├── perspectives.yaml          # Planner가 결정한 관점 목록
    │   ├── security-expert-review.md  # 관점별 리뷰 결과 (동적 파일명)
    │   ├── pm-review.md
    │   ├── dba-review.md
    │   ├── qa-engineer-review.md
    │   ├── consensus-report.md        # 합의 분석 리포트
    │   └── refinement-log.md          # 정제 기록
    ├── iteration-2/
    │   ├── perspectives.yaml          # 2차는 관점이 다를 수 있음
    │   ├── security-expert-review.md
    │   ├── backend-engineer-review.md
    │   ├── qa-engineer-review.md
    │   └── consensus-report.md
    └── refinement-summary.md          # 전체 정제 요약
```
