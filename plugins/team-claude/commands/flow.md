---
name: team-claude:flow
description: 통합 워크플로우 - 스펙 설계부터 구현, 머지까지 자동화된 워크플로우 실행
argument-hint: "<요구사항> | --session <id> [--mode autopilot|assisted|manual]"
allowed-tools: ["Task", "Bash", "Read", "Write", "Glob", "Grep", "AskUserQuestion"]
---

# Flow Command - 통합 워크플로우

> **먼저 읽기**: `${CLAUDE_PLUGIN_ROOT}/INFRASTRUCTURE.md`

자동화된 워크플로우로 스펙 설계부터 구현, 머지까지 한 번에 진행합니다.

---

## 핵심 원칙

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  AUTOMATED WORKFLOW                                                          │
│                                                                              │
│  모드별 자동화 수준:                                                        │
│                                                                              │
│  autopilot  : 전체 자동화 (에스컬레이션 시에만 HITL)                        │
│  assisted   : 단계별 확인 (각 단계 완료 시 HITL)                           │
│  manual     : 기존 방식 (모든 결정에 HITL)                                  │
│                                                                              │
│  공통 원칙:                                                                 │
│  • 자동 리뷰 루프: 스펙과 코드 모두 자동 리뷰 → 피드백 → 수정 반복          │
│  • 병렬 실행: 독립적인 태스크는 자동으로 병렬 처리                          │
│  • 에스컬레이션: 해결 불가 시 자동으로 사용자에게 요청                      │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## PREREQUISITES CHECK

```bash
SCRIPTS="${CLAUDE_PLUGIN_ROOT}/scripts"

# 1. 설정 확인
if ! ${SCRIPTS}/tc-config.sh show &>/dev/null; then
  echo "❌ 설정 파일이 없습니다."
  echo "'/team-claude:setup'을 먼저 실행하세요."
  exit 1
fi

# 2. 상태 초기화 확인
if ! ${SCRIPTS}/tc-state.sh check &>/dev/null; then
  echo "❌ 상태 파일이 없습니다."
  echo "'/team-claude:setup'을 먼저 실행하세요."
  exit 1
fi
```

---

## 사용법

```bash
# 전체 자동화 (autopilot)
/team-claude:flow "결제 시스템에 쿠폰 기능 추가" --mode autopilot

# 단계별 확인 (assisted, 기본값)
/team-claude:flow "알림 시스템 리팩토링"

# 기존 세션 재개
/team-claude:flow --session abc12345

# 특정 단계만 실행
/team-claude:flow --session abc12345 --phase impl

# 시뮬레이션 (dry-run)
/team-claude:flow "새 기능" --dry-run
```

---

## 실행 절차

```
/team-claude:flow "요구사항" --mode <mode>
        │
        ▼
┌───────────────────────────────────────────────────────────────────┐
│  STEP 0: 파라미터 파싱 및 모드 결정                               │
│                                                                   │
│  입력 분석:                                                       │
│  • Magic Keyword 감지 (autopilot:, spec:, impl:, etc.)           │
│  • --mode 옵션 확인                                               │
│  • --session 옵션으로 기존 세션 재개 여부                         │
│                                                                   │
│  모드 결정 우선순위:                                              │
│  1. Magic Keyword (메시지 시작)                                   │
│  2. --mode 옵션                                                   │
│  3. 설정 파일 기본값                                              │
│  4. assisted (fallback)                                           │
└───────────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────────┐
│  STEP 1: 세션 초기화                                              │
│                                                                   │
│  새 세션:                                                         │
│  SESSION_ID=$(${SCRIPTS}/tc-session.sh create "요구사항 제목")    │
│  ${SCRIPTS}/tc-state.sh transition flow_started                   │
│  ${SCRIPTS}/tc-state.sh set-session ${SESSION_ID}                 │
│                                                                   │
│  기존 세션 재개:                                                  │
│  ${SCRIPTS}/tc-session.sh show ${SESSION_ID}                      │
│  현재 상태에서 계속 진행                                          │
└───────────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────────┐
│  PHASE 1: SPEC (스펙 설계)                                        │
│                                                                   │
│  1.1 요구사항 분석                                                │
│      • 코드베이스 스캔                                            │
│      • 관련 파일 식별                                             │
│      • 도메인 키워드 추출                                         │
│                                                                   │
│  1.2 아키텍처 설계                                                │
│      • 옵션 제안 (2-3개)                                          │
│      • 트레이드오프 분석                                          │
│      • 추천안 선정                                                │
│                                                                   │
│  1.3 Contract 정의                                                │
│      • Interface 정의                                             │
│      • Contract Test 작성                                         │
│                                                                   │
│  1.4 Checkpoint 정의                                              │
│      • 구현 단위 분할                                             │
│      • 의존성 그래프 생성                                         │
│      • 검증 기준 정의                                             │
│                                                                   │
│  1.5 Auto-Review Loop (autopilot/assisted)                        │
│      ┌────────────────────────────────────────────────────────┐  │
│      │  SPEC_REVIEW_LOOP:                                      │  │
│      │    1. spec-reviewer 에이전트 호출                       │  │
│      │    2. 피드백 분석                                       │  │
│      │    3. 피드백 없음 → 통과                                │  │
│      │    4. 피드백 있음 → 수정 후 1로 돌아감                  │  │
│      │    5. 최대 반복(5) 도달 → 에스컬레이션                  │  │
│      └────────────────────────────────────────────────────────┘  │
│                                                                   │
│  1.6 HITL 확인 (assisted/manual)                                  │
│      AskUserQuestion: "스펙을 승인하시겠습니까?"                  │
│      → 승인: 다음 단계                                            │
│      → 수정: 피드백 반영 후 1.5로                                │
└───────────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────────┐
│  PHASE 2: IMPL (구현)                                             │
│                                                                   │
│  2.1 Worktree 준비                                                │
│      의존성 순서로 Round 구성:                                    │
│      Round 1: 의존성 없는 checkpoint들 (병렬)                     │
│      Round 2: Round 1에 의존하는 것들                             │
│      Round N: Round N-1에 의존하는 것들                           │
│                                                                   │
│  2.2 Worker 실행 (RALPH Loop)                                     │
│      각 checkpoint에 대해:                                        │
│      ┌────────────────────────────────────────────────────────┐  │
│      │  RALPH_LOOP:                                            │  │
│      │    1. Worktree 생성                                     │  │
│      │    2. CLAUDE.md 작성                                    │  │
│      │    3. Worker 실행                                       │  │
│      │    4. Validation 실행                                   │  │
│      │    5. 통과 → PR 생성                                    │  │
│      │    6. 실패 → 피드백 생성 → 3으로 (최대 5회)            │  │
│      │    7. 최대 도달 → 에스컬레이션                          │  │
│      └────────────────────────────────────────────────────────┘  │
│                                                                   │
│  2.3 Auto-Review Loop (코드 리뷰)                                 │
│      각 완료된 checkpoint에 대해:                                 │
│      ┌────────────────────────────────────────────────────────┐  │
│      │  CODE_REVIEW_LOOP:                                      │  │
│      │    1. code-reviewer 에이전트 호출                       │  │
│      │    2. 피드백 분석                                       │  │
│      │    3. 피드백 없음 → 통과                                │  │
│      │    4. 피드백 있음 → 수정 커밋 → 1로                    │  │
│      │    5. 최대 반복 도달 → 에스컬레이션                    │  │
│      └────────────────────────────────────────────────────────┘  │
│                                                                   │
│  2.4 HITL 확인 (assisted/manual)                                  │
│      AskUserQuestion: "구현을 승인하시겠습니까?"                  │
│      → 승인: 다음 단계                                            │
│      → 수정 필요: 피드백 반영 후 2.2로                           │
└───────────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────────┐
│  PHASE 3: MERGE (머지)                                            │
│                                                                   │
│  3.1 PR 수집                                                      │
│      완료된 모든 checkpoint의 PR 목록                             │
│                                                                   │
│  3.2 순차 머지                                                    │
│      Round 순서대로 epic 브랜치에 머지:                           │
│      • 충돌 없음 → 자동 머지                                      │
│      • 충돌 발생 → conflict-analyzer 호출                         │
│        → 자동 해결 가능 → 해결 후 머지                           │
│        → 자동 해결 불가 → HITL                                    │
│                                                                   │
│  3.3 최종 PR 생성                                                 │
│      epic → main PR 생성                                          │
│                                                                   │
│  3.4 정리                                                         │
│      • Worktree 삭제                                              │
│      • 임시 브랜치 삭제                                           │
│      • 상태 업데이트                                              │
└───────────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────────┐
│  COMPLETE                                                         │
│                                                                   │
│  결과 요약:                                                       │
│  • 세션 ID                                                        │
│  • 생성된 PR URL                                                  │
│  • 소요 시간                                                      │
│  • 반복 횟수 통계                                                 │
└───────────────────────────────────────────────────────────────────┘
```

---

## Auto-Review Loop 상세

### Spec Review

```markdown
## 🔍 Spec Review (자동)

### 검토 항목

1. **완전성 (Completeness)**
   - 모든 요구사항이 스펙에 반영되었는가?
   - 누락된 엣지 케이스가 있는가?

2. **일관성 (Consistency)**
   - 기존 아키텍처와 일관되는가?
   - 용어 사용이 일관되는가?

3. **테스트 가능성 (Testability)**
   - 각 기준이 검증 가능한가?
   - Contract Test가 충분한가?

4. **의존성 (Dependencies)**
   - 의존성 그래프가 올바른가?
   - 순환 의존성이 없는가?

### 피드백 형식

- ✅ PASS: 검토 통과
- ⚠️ WARN: 개선 권장 (자동 진행)
- ❌ FAIL: 수정 필요 (수정 후 재검토)

### 예시

❌ FAIL: Checkpoint `coupon-api`의 의존성에 `coupon-service`가 누락됨
   → 제안: dependencies에 "coupon-service" 추가
```

### Code Review

```markdown
## 🔍 Code Review (자동)

### 검토 항목

1. **Contract 준수**
   - Interface 구현이 정확한가?
   - Test가 모두 통과하는가?

2. **코드 품질**
   - 기존 코드 스타일 준수
   - 불필요한 복잡도 없음
   - 적절한 에러 처리

3. **보안**
   - 입력 검증
   - SQL Injection, XSS 등 OWASP Top 10

4. **성능**
   - N+1 쿼리
   - 불필요한 반복

### 피드백 형식

- ✅ PASS: 검토 통과, 머지 가능
- ⚠️ WARN: 개선 권장 (자동 진행)
- ❌ FAIL: 수정 필요 (수정 후 재검토)

### 예시

❌ FAIL: CouponService.apply()에서 중복 적용 검사 누락
   → 파일: src/services/coupon.service.ts:45
   → 제안: 적용 전 usedCount 확인 로직 추가
```

---

## Magic Keywords 처리

```bash
# 메시지에서 Magic Keyword 감지
parse_magic_keyword() {
  local message="$1"

  case "$message" in
    autopilot:*|auto:*|ap:*)
      echo "autopilot"
      ;;
    spec:*|sp:*)
      echo "spec"
      ;;
    impl:*|im:*)
      echo "impl"
      ;;
    review:*|rv:*)
      echo "review"
      ;;
    parallel:*|pl:*)
      echo "parallel"
      ;;
    ralph:*|rl:*)
      echo "ralph"
      ;;
    *)
      echo ""
      ;;
  esac
}

# Keyword 제거 후 실제 메시지 추출
extract_message() {
  local message="$1"
  echo "$message" | sed 's/^[a-z]*://'
}
```

### 사용 예시

```bash
# 입력: "autopilot: 쿠폰 기능 추가"
# → 모드: autopilot
# → 메시지: "쿠폰 기능 추가"

# 입력: "spec: 결제 시스템 설계해줘"
# → 모드: spec
# → 메시지: "결제 시스템 설계해줘"

# 입력: "parallel: coupon-model, coupon-service"
# → 모드: parallel
# → 태스크: ["coupon-model", "coupon-service"]
```

---

## 스크립트 도구

```bash
SCRIPTS="${CLAUDE_PLUGIN_ROOT}/scripts"

# Flow 실행
${SCRIPTS}/tc-flow.sh start "요구사항" --mode autopilot
${SCRIPTS}/tc-flow.sh resume ${SESSION_ID}
${SCRIPTS}/tc-flow.sh status ${SESSION_ID}

# PSM (Parallel Session Manager)
${SCRIPTS}/tc-psm.sh new "feature-name"
${SCRIPTS}/tc-psm.sh list
${SCRIPTS}/tc-psm.sh parallel session1 session2
${SCRIPTS}/tc-psm.sh status

# 리뷰
${SCRIPTS}/tc-review.sh spec ${SESSION_ID}
${SCRIPTS}/tc-review.sh code ${CHECKPOINT_ID}
```

---

## 출력 예시

### 시작

```
🚀 Automated Workflow 시작

  모드: assisted
  세션: abc12345
  요구사항: 결제 시스템에 쿠폰 기능 추가

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

📋 PHASE 1: SPEC (스펙 설계)

  ⏳ 요구사항 분석 중...
  ⏳ 아키텍처 설계 중...
  ⏳ Contract 정의 중...
  ⏳ Checkpoint 정의 중...

━━━ Auto-Review ━━━

  Iteration 1: 🔍 검토 중...
  Iteration 1: ❌ 2개 이슈 발견
    • coupon-api 의존성 누락
    • Contract Test 불완전

  Iteration 2: 🔄 수정 중...
  Iteration 2: 🔍 검토 중...
  Iteration 2: ✅ 통과

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### 완료

```
✅ Automated Workflow 완료

━━━ 결과 요약 ━━━

  세션: abc12345
  모드: assisted
  소요 시간: 45분

━━━ 단계별 결과 ━━━

  📋 SPEC
    • 리뷰 반복: 2회
    • 결과: ✅ 승인됨

  🔧 IMPL
    • Checkpoints: 3개
    • 병렬 실행: Round 1 (1개), Round 2 (1개), Round 3 (1개)
    • 총 반복: 5회
    • 결과: ✅ 모두 통과

  🔀 MERGE
    • PRs: 3개
    • 충돌: 0건
    • 결과: ✅ 머지 완료

━━━ 산출물 ━━━

  Final PR: https://github.com/user/repo/pull/123

━━━ 다음 단계 ━━━

  PR 리뷰 후 머지하세요.
```

---

## 에러 처리

### 에스컬레이션

```
⚠️ 에스컬레이션 필요

  단계: IMPL / coupon-service
  이유: 최대 반복 횟수 도달 (5/5)

  마지막 에러:
    AssertionError: expected 200 to equal 409

  권장 조치:
    1. 수동 검토: cd .team-claude/worktrees/coupon-service
    2. 스펙 수정: /team-claude:architect --resume abc12345
    3. 건너뛰기: /team-claude:flow --session abc12345 --skip coupon-service
```

### 충돌

```
⚠️ 머지 충돌 발생

  PR: #45 (coupon-service)
  충돌 파일:
    • src/services/payment.service.ts

  conflict-analyzer 결과:
    자동 해결 불가 - 비즈니스 로직 충돌

  선택지:
    1. [A] PR #45의 변경 사용
    2. [B] 기존 코드 유지
    3. [C] 수동 해결
```

---

## 설정

```yaml
# .claude/team-claude.yaml
flow:
  defaultMode: assisted

  autoReview:
    enabled: true
    maxIterations: 5
    specReviewer: spec-reviewer
    codeReviewer: code-reviewer

  escalation:
    onMaxIterations: true
    onConflict: true
    onError: true

  parallel:
    enabled: true
    maxWorkers: 4
```
