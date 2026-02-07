---
description: Checkpoint 관리 - 검증 기준점 조회, 추가, 검증
argument-hint: "--list | validate <checkpoint-id> | add <description>"
allowed-tools: ["Bash", "Read", "Write", "Glob", "AskUserQuestion"]
---

# Checkpoint Command

> **먼저 읽기**: `${CLAUDE_PLUGIN_ROOT}/INFRASTRUCTURE.md`

Checkpoint(검증 기준점)를 관리합니다.

---

## PREREQUISITES CHECK

```bash
# 1. 설정 파일 존재 확인
if ! tc config show &>/dev/null; then
  echo "❌ 설정 파일이 없습니다."
  echo "'/team-claude:setup'을 먼저 실행하세요."
  exit 1
fi

# 2. 세션 지정 시 세션 존재 확인
if [[ -n "${SESSION_ID}" ]]; then
  if ! tc session show ${SESSION_ID} &>/dev/null; then
    echo "❌ 세션을 찾을 수 없습니다: ${SESSION_ID}"
    exit 1
  fi
fi
```

---

## Checkpoint란?

```
┌─────────────────────────────────────────────────────────────────┐
│  CHECKPOINT: 구현 성공/실패의 명확한 기준                        │
│                                                                 │
│  구성 요소:                                                     │
│  • criteria: 충족해야 할 조건 목록                              │
│  • validation: 자동 검증 방법 (명령어 + 예상 결과)              │
│  • dependencies: 선행 Checkpoint                                │
│                                                                 │
│  목적:                                                          │
│  • 모호한 "완료"를 명확한 기준으로 변환                         │
│  • 자동 검증 가능하게 함                                        │
│  • 에이전트가 자율적으로 성공/실패 판단 가능                    │
└─────────────────────────────────────────────────────────────────┘
```

## 사용법

```bash
# Checkpoint 목록 조회
/team-claude:checkpoint --list

# 특정 세션의 Checkpoint
/team-claude:checkpoint --list --session abc12345

# 특정 Checkpoint 검증
/team-claude:checkpoint validate coupon-api

# Checkpoint 추가 (대화형)
/team-claude:checkpoint add "새 API 엔드포인트 검증"

# Checkpoint 수정 (대화형)
/team-claude:checkpoint edit coupon-api

# Checkpoint 삭제
/team-claude:checkpoint delete coupon-api
```

---

## 명령어 상세

### --list: Checkpoint 목록

```
📋 Checkpoints: abc12345

┌────────────────────┬───────────────┬────────────┬───────────────┐
│ ID                 │ 이름          │ 상태       │ 의존성        │
├────────────────────┼───────────────┼────────────┼───────────────┤
│ coupon-model       │ 쿠폰 도메인   │ ✅ 통과    │ -             │
│ coupon-service     │ 쿠폰 서비스   │ 🔄 2/5    │ coupon-model  │
│ coupon-api         │ 쿠폰 API      │ ⏸️ 대기   │ coupon-service│
│ coupon-integration │ 통합 테스트   │ ⏸️ 대기   │ coupon-api    │
└────────────────────┴───────────────┴────────────┴───────────────┘

상세: /team-claude:checkpoint show <id>
검증: /team-claude:checkpoint validate <id>
```

### validate: Checkpoint 검증

```bash
/team-claude:checkpoint validate coupon-api
```

```
🔍 Checkpoint 검증: coupon-api

━━━ 기준 (Criteria) ━━━

  1. ❌ POST /coupons/validate - 200 with valid coupon
  2. ❌ POST /coupons/validate - 400 with expired coupon
  3. ❌ POST /coupons/apply - 200 and discount applied
  4. ❌ POST /coupons/apply - 409 on duplicate application

━━━ 검증 실행 ━━━

  명령어: npm run test:e2e -- --grep 'coupon'
  예상: 4 passing

━━━ 결과 ━━━

  ❌ 실패

  실제 출력:
    0 passing (해당 테스트 파일 없음)

━━━ 분석 ━━━

  테스트 파일이 아직 생성되지 않았습니다.
  구현 위임 시 테스트와 함께 생성됩니다.
```

### add: Checkpoint 추가 (대화형)

```bash
/team-claude:checkpoint add "rate limiting 검증"
```

대화형으로 Checkpoint 상세를 정의합니다:

```
➕ Checkpoint 추가

━━━ 기본 정보 ━━━

  ID: rate-limiting (자동 생성)
  설명: rate limiting 검증

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

AskUserQuestion으로 상세 정보 수집:

```typescript
AskUserQuestion({
  questions: [
    {
      question: "어떤 유형의 Checkpoint인가요?",
      header: "Type",
      options: [
        { label: "implementation", description: "코드 구현" },
        { label: "api", description: "API 엔드포인트" },
        { label: "integration", description: "통합 테스트" },
        { label: "performance", description: "성능 테스트" }
      ],
      multiSelect: false
    },
    {
      question: "어떤 Checkpoint에 의존하나요?",
      header: "Dependencies",
      options: [
        { label: "없음", description: "독립적으로 실행 가능" },
        { label: "coupon-service", description: "쿠폰 서비스 완료 후" },
        { label: "coupon-api", description: "쿠폰 API 완료 후" }
      ],
      multiSelect: true
    }
  ]
})
```

그 후 criteria와 validation 정의:

```
━━━ 기준 정의 ━━━

성공 기준을 입력하세요 (한 줄에 하나씩, 빈 줄로 종료):

> 1분에 10회 초과 요청 시 429 응답
> 1분 후 요청 제한 해제
> 헤더에 X-RateLimit-Remaining 포함
>

━━━ 검증 방법 ━━━

검증 명령어: npm run test:e2e -- --grep 'rate limit'
예상 결과: 3 passing

━━━ 결과 ━━━

✅ Checkpoint 추가됨: rate-limiting

  저장: .team-claude/sessions/abc12345/specs/checkpoints.yaml
```

---

## Checkpoint YAML 구조

```yaml
checkpoints:
  - id: rate-limiting
    name: "Rate Limiting"
    type: api
    description: "API 요청 제한 검증"
    dependencies: [coupon-api]
    criteria:
      - "1분에 10회 초과 요청 시 429 응답"
      - "1분 후 요청 제한 해제"
      - "헤더에 X-RateLimit-Remaining 포함"
    validation:
      command: "npm run test:e2e -- --grep 'rate limit'"
      expected: "3 passing"
      timeout: 60000
    created_at: "2024-01-15T14:00:00Z"
    created_by: human
```

---

## Checkpoint 유형별 템플릿

### implementation (코드 구현)

```yaml
- id: user-service
  type: implementation
  criteria:
    - "UserService 클래스가 존재"
    - "findById() 메서드가 User 반환"
    - "create() 메서드가 새 User 생성"
  validation:
    command: "npm run test -- --grep 'UserService'"
    expected: "passing"
```

### api (API 엔드포인트)

```yaml
- id: user-api
  type: api
  criteria:
    - "GET /users/:id - 200 with user data"
    - "GET /users/:id - 404 for non-existent user"
    - "POST /users - 201 with created user"
    - "POST /users - 400 for invalid data"
  validation:
    command: "npm run test:e2e -- --grep 'users'"
    expected: "4 passing"
```

### integration (통합 테스트)

```yaml
- id: user-flow
  type: integration
  criteria:
    - "회원가입 → 로그인 → 프로필 조회 플로우 동작"
    - "잘못된 비밀번호로 로그인 시 401"
    - "로그아웃 후 인증 필요 API 접근 시 401"
  validation:
    command: "npm run test:integration"
    expected: "passing"
```

### performance (성능 테스트)

```yaml
- id: api-performance
  type: performance
  criteria:
    - "GET /users/:id 응답 시간 < 100ms (p95)"
    - "POST /users 응답 시간 < 200ms (p95)"
    - "동시 100 요청 처리 가능"
  validation:
    command: "npm run test:perf"
    expected: "all benchmarks passed"
```

---

## 좋은 Checkpoint 작성법

### ✅ 좋은 예

```yaml
criteria:
  - "POST /coupons/apply가 유효한 쿠폰에 대해 200 반환"
  - "응답 body에 discountedAmount 필드 포함"
  - "Order 테이블의 discount_amount 컬럼이 업데이트됨"
```

- 구체적이고 검증 가능
- 입력과 예상 출력이 명확
- 부수 효과도 명시

### ❌ 나쁜 예

```yaml
criteria:
  - "쿠폰 기능이 잘 동작함"
  - "에러 처리가 적절함"
  - "성능이 좋음"
```

- 모호하고 주관적
- 자동 검증 불가능
- 에이전트가 판단할 수 없음

---

## 검증 명령어 예시

### Jest/Vitest

```yaml
validation:
  command: "npm run test -- --grep 'CouponService'"
  expected: "passing"
```

### Playwright/Cypress (E2E)

```yaml
validation:
  command: "npm run test:e2e -- --spec 'coupon.spec.ts'"
  expected: "All specs passed"
```

### cURL (API 직접 테스트)

```yaml
validation:
  command: |
    curl -s -o /dev/null -w '%{http_code}' \
      -X POST http://localhost:3000/coupons/validate \
      -H 'Content-Type: application/json' \
      -d '{"code": "VALID123"}'
  expected: "200"
```

### 커스텀 스크립트

```yaml
validation:
  command: "node scripts/verify-checkpoint.js coupon-api"
  expected: "PASSED"
```

---

## 에러 처리

### Checkpoint 없음

```
❌ Checkpoint를 찾을 수 없습니다: unknown-checkpoint

사용 가능한 Checkpoints:
  - coupon-model
  - coupon-service
  - coupon-api

/team-claude:checkpoint --list 로 전체 목록을 확인하세요.
```

### 검증 실패

```
❌ 검증 실패: coupon-api

  명령어: npm run test:e2e -- --grep 'coupon'
  예상: 4 passing
  실제: 2 passing, 2 failing

  실패한 테스트:
    1. POST /coupons/apply - 409 on duplicate
    2. POST /coupons/validate - 400 with expired

  로그: /tmp/checkpoint-validation-abc123.log
```

### 의존성 미충족

```
⚠️ 의존성 미충족: coupon-api

  필요: coupon-service (현재: 진행 중)

  coupon-service 완료 후 검증 가능합니다.
```
