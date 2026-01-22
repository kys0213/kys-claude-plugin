---
name: team-claude:architect
description: 설계 루프 - 인간과 에이전트가 대화형으로 스펙/아키텍처를 협업 설계
argument-hint: "<요구사항> | --resume <session-id> | --list"
allowed-tools: ["Task", "Read", "Write", "Glob", "Grep", "AskUserQuestion", "Bash"]
---

# Architect Loop Command

인간과 에이전트가 **대화형으로** 스펙과 아키텍처를 설계합니다.

## 핵심 원칙

```
┌─────────────────────────────────────────────────────────────────┐
│  ARCHITECT LOOP: 인간은 "무엇을"과 "왜", 에이전트는 "어떻게"    │
│                                                                 │
│  인간의 역할:                                                   │
│  • 비즈니스 요구사항 명확화                                     │
│  • 트레이드오프 결정 (성능 vs 복잡도 등)                        │
│  • Checkpoint 승인                                              │
│                                                                 │
│  에이전트의 역할:                                               │
│  • 기술적 옵션 제안                                             │
│  • 트레이드오프 분석                                            │
│  • Checkpoint 초안 작성                                         │
└─────────────────────────────────────────────────────────────────┘
```

## 사용법

```bash
# 새 설계 시작
/team-claude:architect "결제 시스템에 쿠폰 할인 기능 추가"

# 세션 목록
/team-claude:architect --list

# 기존 설계 재개
/team-claude:architect --resume abc12345

# 특정 세션 상세
/team-claude:architect --show abc12345
```

---

## 실행 절차

```
/team-claude:architect "요구사항"
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│  STEP 1: 세션 초기화                                          │
│                                                               │
│  • session-id 생성 (8자리)                                    │
│  • .team-claude/sessions/{session-id}/ 디렉토리 생성                  │
│  • meta.json 초기화                                           │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│  STEP 2: 요구사항 분석                                        │
│                                                               │
│  • 코드베이스 분석 (기존 구조 파악)                           │
│  • 도메인 키워드 추출                                         │
│  • 초기 질문 생성                                             │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│  STEP 3: 대화형 설계 루프 (핵심)                              │
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  3.1 에이전트: 설계 제안                                │ │
│  │      • 아키텍처 옵션 (2-3개)                            │ │
│  │      • 각 옵션의 트레이드오프                           │ │
│  │      • 추천 옵션 + 이유                                 │ │
│  └─────────────────────────────────────────────────────────┘ │
│                      │                                        │
│                      ▼                                        │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  3.2 AskUserQuestion: 결정 요청                         │ │
│  │                                                         │ │
│  │  질문 유형:                                             │ │
│  │  • 아키텍처 선택 ("옵션 A vs B?")                       │ │
│  │  • 비즈니스 규칙 확인 ("쿠폰 중복 적용 허용?")         │ │
│  │  • 우선순위 결정 ("성능 vs 단순성?")                    │ │
│  │  • 범위 확정 ("MVP 범위는?")                            │ │
│  └─────────────────────────────────────────────────────────┘ │
│                      │                                        │
│                      ▼                                        │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  3.3 인간: 피드백 제공                                  │ │
│  │                                                         │ │
│  │  응답 유형:                                             │ │
│  │  • 선택: "옵션 A로 진행"                                │ │
│  │  • 수정: "A 기반이지만 X는 B처럼"                       │ │
│  │  • 추가 요구: "Y도 고려해줘"                            │ │
│  │  • 승인: "이대로 진행"                                  │ │
│  └─────────────────────────────────────────────────────────┘ │
│                      │                                        │
│                      ▼                                        │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  3.4 에이전트: 피드백 반영                              │ │
│  │                                                         │ │
│  │  • 설계 문서 업데이트                                   │ │
│  │  • 대화 기록 저장                                       │ │
│  │  • 다음 질문 또는 다음 단계 진행                        │ │
│  └─────────────────────────────────────────────────────────┘ │
│                      │                                        │
│                      └────────▶ 모든 핵심 결정 완료까지 반복  │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│  STEP 4: Contract 정의 (Interface + Test Code)               │
│                                                               │
│  각 Task별로:                                                │
│  • Interface 정의 (타입, API 스펙)                           │
│  • Contract Test 작성 (TDD - 구현 전에 테스트 먼저!)         │
│                                                               │
│  산출물:                                                     │
│  .team-claude/sessions/{session-id}/contracts/                       │
│    ├── coupon-model/                                         │
│    │   ├── interface.ts                                      │
│    │   └── contract.test.ts                                  │
│    └── coupon-service/                                       │
│        ├── interface.ts                                      │
│        └── contract.test.ts                                  │
│                                                               │
│  테스트를 프로젝트 테스트 디렉토리에도 복사:                │
│    tests/contracts/test_coupon_model_contract.py             │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│  STEP 5: Checkpoint 정의                                      │
│                                                               │
│  • 각 구현 단위별 검증 기준 정의                             │
│  • validation.command = Contract Test 실행                   │
│  • AskUserQuestion으로 Checkpoint 승인 요청                  │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│  STEP 6: 공유 테스트 환경 확인 (필요시)                      │
│                                                               │
│  여러 Task가 공통 환경을 사용하는 경우:                      │
│  • DB 스키마, fixtures, Docker 등                            │
│  • prerequisite task로 등록                                  │
│  • 병렬 실행 전에 먼저 완료                                  │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│  STEP 7: 산출물 확정                                          │
│                                                               │
│  저장 위치: .team-claude/sessions/{session-id}/                      │
│  • architecture.md - 아키텍처 설계                           │
│  • contracts/ - Interface + Test Code                        │
│  • checkpoints/ - Task 정의 (JSON)                           │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│  STEP 6: 다음 단계 안내                                       │
│                                                               │
│  "설계 완료! 구현을 위임하시겠습니까?"                       │
│  → /team-claude:delegate --session {session-id}                      │
└───────────────────────────────────────────────────────────────┘
```

---

## STEP 3: 대화형 설계 루프 상세

### 3.1 아키텍처 옵션 제안 형식

```markdown
## 🏗️ 아키텍처 옵션

### 옵션 A: 이벤트 기반 (Event-Driven)

```
[Order] ──event──▶ [CouponService] ──event──▶ [NotificationService]
```

**장점:**
- 느슨한 결합
- 확장성 좋음

**단점:**
- 디버깅 어려움
- 복잡도 증가

---

### 옵션 B: 직접 호출 (Direct Call)

```
[Order] ──call──▶ [CouponService]
                        │
                        └──call──▶ [NotificationService]
```

**장점:**
- 단순함
- 디버깅 쉬움

**단점:**
- 강한 결합
- 확장 시 수정 필요

---

### 추천: 옵션 B (직접 호출)

**이유:**
- 현재 코드베이스가 직접 호출 패턴 사용 중
- 쿠폰 기능은 단순한 확장이므로 복잡도 증가 불필요
- 추후 필요시 이벤트 기반으로 마이그레이션 가능
```

### 3.2 AskUserQuestion 사용 패턴

```typescript
// 아키텍처 선택
AskUserQuestion({
  questions: [{
    question: "쿠폰 서비스 아키텍처를 어떻게 구성할까요?",
    header: "Architecture",
    options: [
      { label: "옵션 A: 이벤트 기반 (Recommended)", description: "느슨한 결합, 확장성 좋음" },
      { label: "옵션 B: 직접 호출", description: "단순함, 현재 코드베이스와 일관성" }
    ],
    multiSelect: false
  }]
})

// 비즈니스 규칙 확인
AskUserQuestion({
  questions: [{
    question: "쿠폰 중복 적용을 허용할까요?",
    header: "Business Rule",
    options: [
      { label: "허용 안함 (Recommended)", description: "주문당 1개 쿠폰만" },
      { label: "제한적 허용", description: "카테고리가 다른 쿠폰은 중복 가능" },
      { label: "전체 허용", description: "모든 쿠폰 중복 적용 가능" }
    ],
    multiSelect: false
  }]
})

// Checkpoint 승인
AskUserQuestion({
  questions: [{
    question: "아래 Checkpoint로 구현을 진행할까요?",
    header: "Checkpoints",
    options: [
      { label: "승인", description: "이대로 구현 위임" },
      { label: "수정 필요", description: "Checkpoint 수정 후 재검토" }
    ],
    multiSelect: false
  }]
})
```

---

## STEP 4: Contract 정의 (핵심!)

### Contract = Interface + Test Code + Test Scenarios

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Contract 기반 자동 검증의 핵심                                              │
│                                                                             │
│  Contract 3요소:                                                            │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  1. Interface       - 타입/시그니처 정의                            │   │
│  │  2. Test Code       - TDD 테스트 코드 (실행 가능)                   │   │
│  │  3. Test Scenarios  - 구체적인 입출력 케이스 (자동 검증용)          │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  Test Scenarios가 있어야:                                                   │
│  • Worker가 정확히 무엇을 구현해야 하는지 명확                              │
│  • 서버가 자동으로 검증 가능                                                │
│  • 피드백 루프에서 어떤 케이스가 실패했는지 정확히 파악                      │
│                                                                             │
│  TDD 방식:                                                                  │
│  • 구현 전에 테스트 먼저 작성                                               │
│  • Worker는 이 테스트를 통과시키는 것이 목표                                │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Test Scenarios 정의 (자동 검증 루프의 핵심!)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  왜 Test Scenarios가 필요한가?                                               │
│                                                                             │
│  추상적 Criteria만으로는 자동 검증이 불가능:                                 │
│                                                                             │
│  ❌ BAD: "validate()가 유효한 쿠폰에 대해 true 반환"                        │
│     → Worker: "유효한 쿠폰이 뭔데?"                                         │
│     → Server: "어떤 입력으로 테스트하지?"                                   │
│                                                                             │
│  ✅ GOOD: 구체적인 입출력 시나리오 정의                                     │
│     → Worker: 이 케이스들을 통과시키면 됨                                   │
│     → Server: 이 케이스들로 자동 검증 가능                                  │
└─────────────────────────────────────────────────────────────────────────────┘
```

**각 Criterion에 대한 Test Scenarios 형식:**

```yaml
# .team-claude/sessions/{session-id}/contracts/coupon-service/scenarios.yaml

criteria:
  - id: valid-coupon-returns-true
    description: "validate()가 유효한 쿠폰에 대해 true 반환"
    scenarios:
      - name: "percent discount coupon"
        given:
          coupon:
            code: "SAVE10"
            discount_type: "percent"
            discount_value: 10
            expires_at: "2025-12-31"
          order:
            total: 10000
        when: "coupon.validate(order)"
        then:
          valid: true
          discount_amount: 1000

      - name: "fixed amount discount"
        given:
          coupon:
            code: "FLAT500"
            discount_type: "fixed"
            discount_value: 500
        when: "coupon.validate(order)"
        then:
          valid: true
          discount_amount: 500

  - id: expired-coupon-returns-false
    description: "validate()가 만료된 쿠폰에 대해 false 반환"
    scenarios:
      - name: "expired yesterday"
        given:
          coupon:
            code: "EXPIRED"
            expires_at: "yesterday"  # 상대 날짜 지원
        when: "coupon.validate(order)"
        then:
          valid: false
          error_type: "CouponExpired"

      - name: "expires today but already used"
        given:
          coupon:
            code: "LASTDAY"
            expires_at: "today"
            usage_limit: 1
            used_count: 1
        when: "coupon.validate(order)"
        then:
          valid: false
          error_type: "CouponExhausted"

  - id: duplicate-application-fails
    description: "중복 적용 시 에러 발생"
    scenarios:
      - name: "same coupon applied twice to same order"
        given:
          coupon: { code: "ONCE" }
          order: { id: "order-1" }
          existing_usage: { coupon_code: "ONCE", order_id: "order-1" }
        when: "couponService.apply(coupon, order)"
        then:
          success: false
          error_type: "DuplicateApplication"
          error_message: "이미 적용된 쿠폰입니다"

      - name: "same coupon to different order is OK"
        given:
          coupon: { code: "MULTI" }
          order: { id: "order-2" }
          existing_usage: { coupon_code: "MULTI", order_id: "order-1" }
        when: "couponService.apply(coupon, order)"
        then:
          success: true
```

**Edge Cases 시나리오 (필수!):**

```yaml
edge_cases:
  - id: boundary-discount-percent
    description: "경계값 테스트 - 할인율"
    scenarios:
      - name: "0% discount"
        given: { discount_value: 0 }
        then: { discount_amount: 0 }
      - name: "100% discount"
        given: { discount_value: 100, order_total: 5000 }
        then: { discount_amount: 5000 }
      - name: "negative discount rejected"
        given: { discount_value: -10 }
        then: { error_type: "InvalidDiscount" }
      - name: "over 100% rejected"
        given: { discount_value: 150 }
        then: { error_type: "InvalidDiscount" }

  - id: concurrent-application
    description: "동시성 테스트 - 동시에 같은 쿠폰 적용"
    scenarios:
      - name: "race condition handling"
        given:
          coupon: { code: "RACE", usage_limit: 1 }
          concurrent_requests: 2
        then:
          one_succeeds: true
          one_fails_with: "CouponExhausted"

  - id: null-and-empty
    description: "Null/Empty 처리"
    scenarios:
      - name: "null coupon code"
        given: { code: null }
        then: { error_type: "InvalidInput" }
      - name: "empty coupon code"
        given: { code: "" }
        then: { error_type: "InvalidInput" }
```

### Interface 정의 예시

```typescript
// .team-claude/sessions/{session-id}/contracts/coupon-model/interface.ts

export interface Coupon {
  id: string;
  code: string;
  discountType: 'percent' | 'fixed';
  discountValue: number;
  expiresAt: Date;
  usageLimit: number;
  usedCount: number;
}

export interface CouponRepository {
  save(coupon: Coupon): Promise<Coupon>;
  findByCode(code: string): Promise<Coupon | null>;
  findById(id: string): Promise<Coupon | null>;
  incrementUsedCount(id: string): Promise<void>;
}
```

### Contract Test 작성 예시

```python
# tests/contracts/test_coupon_model_contract.py
# Worker가 이 테스트를 통과해야 Task 완료

import pytest
from datetime import datetime, timedelta

class TestCouponModelContract:
    """
    Coupon Model Contract Tests

    이 테스트는 architect 단계에서 작성됩니다.
    Worker는 이 테스트를 통과시키는 구현을 작성합니다.
    """

    def test_coupon_entity_has_required_fields(self):
        """Coupon 엔티티는 필수 필드를 가져야 함"""
        from src.models.coupon import Coupon

        coupon = Coupon(
            code="SUMMER2024",
            discount_type="percent",
            discount_value=10,
            expires_at=datetime.now() + timedelta(days=30)
        )

        assert coupon.code == "SUMMER2024"
        assert coupon.discount_type == "percent"
        assert coupon.discount_value == 10
        assert coupon.expires_at is not None

    def test_coupon_validates_discount_range(self):
        """percent 타입은 0-100 범위만 허용"""
        from src.models.coupon import Coupon

        with pytest.raises(ValueError):
            Coupon(
                code="INVALID",
                discount_type="percent",
                discount_value=150  # > 100% 는 에러
            )

    def test_repository_save_and_find(self):
        """Repository는 저장 후 조회 가능해야 함"""
        from src.repositories.coupon_repository import CouponRepository
        from src.models.coupon import Coupon

        repo = CouponRepository()
        coupon = Coupon(code="TEST", discount_type="fixed", discount_value=1000)

        repo.save(coupon)
        found = repo.find_by_code("TEST")

        assert found is not None
        assert found.code == "TEST"

    def test_repository_returns_none_for_expired(self):
        """만료된 쿠폰 조회 시 None 반환"""
        from src.repositories.coupon_repository import CouponRepository
        from src.models.coupon import Coupon

        repo = CouponRepository()
        expired_coupon = Coupon(
            code="EXPIRED",
            discount_type="percent",
            discount_value=10,
            expires_at=datetime.now() - timedelta(days=1)  # 어제 만료
        )
        repo.save(expired_coupon)

        found = repo.find_by_code("EXPIRED")
        assert found is None  # 만료된 쿠폰은 조회되지 않음
```

### 공유 테스트 환경 처리

여러 Task가 공통 환경을 사용하는 경우:

```yaml
# 환경 설정 Task (prerequisite)
prerequisites:
  - id: db-setup
    name: "테스트 DB 환경 구축"
    type: environment
    script: |
      docker-compose up -d postgres-test
      alembic upgrade head
      python scripts/seed_test_data.py
```

```
실행 순서:
1. db-setup (환경 구축)
2. coupon-model, user-service (병렬 - Round 1)
3. coupon-service (Round 2)
4. coupon-api (Round 3)
```

---

## STEP 5: Checkpoint 정의 형식

### Checkpoint = Criteria + Test Scenarios + Validation

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Checkpoint 구성 요소                                                        │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  criteria[]       - 충족해야 할 조건 (추상적 설명)                  │   │
│  │       │                                                             │   │
│  │       ▼                                                             │   │
│  │  scenarios[]      - 각 criterion의 구체적 테스트 케이스             │   │
│  │       │             (given → when → then 형식)                      │   │
│  │       ▼                                                             │   │
│  │  validation       - 테스트 실행 방법                                │   │
│  │       │             (command, timeout, success_pattern)             │   │
│  │       ▼                                                             │   │
│  │  auto_verify()    - 서버가 자동으로 검증 루프 실행                  │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  이 구조가 있어야 "자동화된 피드백 루프"가 가능!                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

### checkpoint YAML 구조 (Test Scenarios 포함)

```yaml
# .team-claude/sessions/{session-id}/checkpoints/coupon-service.yaml

id: coupon-service
name: "쿠폰 서비스 로직"
type: implementation
description: "쿠폰 검증 및 적용 로직"
dependencies: [coupon-model]

# 검증 방법
validation:
  # 테스트 실행 명령어
  command: "npm run test -- --grep 'CouponService'"
  # 성공 판정 패턴 (정규식)
  success_pattern: "\\d+ passing"
  # 실패 판정 패턴
  failure_patterns:
    - "failing"
    - "Error:"
    - "AssertionError"
  timeout: 30000

# 충족 조건 + 구체적 시나리오
criteria:
  - id: valid-coupon-validation
    description: "validate()가 유효한 쿠폰에 대해 true 반환"
    # 이 criterion을 검증하는 구체적 시나리오들
    scenarios:
      - name: "10% 할인 쿠폰 적용"
        given:
          coupon:
            code: "SAVE10"
            discount_type: "percent"
            discount_value: 10
            expires_at: "+30days"
          order:
            id: "order-1"
            total: 10000
        when: "couponService.validate(coupon, order)"
        then:
          returns: true
          discount_amount: 1000

      - name: "고정 금액 할인 쿠폰"
        given:
          coupon:
            code: "FLAT1000"
            discount_type: "fixed"
            discount_value: 1000
          order:
            total: 5000
        when: "couponService.validate(coupon, order)"
        then:
          returns: true
          discount_amount: 1000

  - id: expired-coupon-validation
    description: "validate()가 만료된 쿠폰에 대해 false 반환"
    scenarios:
      - name: "어제 만료된 쿠폰"
        given:
          coupon:
            code: "EXPIRED"
            expires_at: "-1days"
        when: "couponService.validate(coupon, order)"
        then:
          returns: false
          error:
            type: "CouponExpired"
            message_contains: "만료"

      - name: "사용 횟수 초과 쿠폰"
        given:
          coupon:
            code: "LIMITED"
            usage_limit: 10
            used_count: 10
        when: "couponService.validate(coupon, order)"
        then:
          returns: false
          error:
            type: "CouponExhausted"

  - id: apply-discount
    description: "apply()가 주문 금액에서 할인 적용"
    scenarios:
      - name: "10% 할인 적용"
        given:
          coupon: { code: "SAVE10", discount_type: "percent", discount_value: 10 }
          order: { id: "order-1", total: 10000 }
        when: "couponService.apply(coupon, order)"
        then:
          success: true
          order:
            discount_amount: 1000
            final_total: 9000
          coupon:
            used_count_increased: true

      - name: "최소 주문금액 미달 시 실패"
        given:
          coupon: { code: "MIN5000", min_order_amount: 5000 }
          order: { total: 3000 }
        when: "couponService.apply(coupon, order)"
        then:
          success: false
          error:
            type: "MinOrderAmountNotMet"
            message_contains: "5000"

  - id: duplicate-prevention
    description: "중복 적용 시 에러 발생"
    scenarios:
      - name: "같은 주문에 같은 쿠폰 재적용 시도"
        given:
          coupon: { code: "ONCE" }
          order: { id: "order-1" }
          # 이미 적용된 상태 (setup 데이터)
          setup:
            - "couponService.apply({ code: 'ONCE' }, { id: 'order-1' })"
        when: "couponService.apply(coupon, order)"
        then:
          success: false
          error:
            type: "DuplicateApplication"
            message: "이미 적용된 쿠폰입니다"

      - name: "다른 주문에는 같은 쿠폰 적용 가능"
        given:
          coupon: { code: "MULTI" }
          order: { id: "order-2" }
          setup:
            - "couponService.apply({ code: 'MULTI' }, { id: 'order-1' })"
        when: "couponService.apply(coupon, order)"
        then:
          success: true

# Edge Cases (필수!)
edge_cases:
  - id: boundary-values
    scenarios:
      - name: "0% 할인"
        given: { discount_value: 0, order_total: 10000 }
        then: { discount_amount: 0, final_total: 10000 }
      - name: "100% 할인"
        given: { discount_value: 100, order_total: 10000 }
        then: { discount_amount: 10000, final_total: 0 }
      - name: "할인액이 주문금액 초과 (fixed)"
        given: { discount_type: "fixed", discount_value: 5000, order_total: 3000 }
        then: { discount_amount: 3000, final_total: 0 }  # 최대 주문금액까지만

  - id: invalid-inputs
    scenarios:
      - name: "존재하지 않는 쿠폰 코드"
        given: { code: "NOTEXIST" }
        then: { error_type: "CouponNotFound" }
      - name: "null 쿠폰 코드"
        given: { code: null }
        then: { error_type: "InvalidInput" }
      - name: "빈 문자열 쿠폰 코드"
        given: { code: "" }
        then: { error_type: "InvalidInput" }

# 테스트 데이터 (Fixture)
test_fixtures:
  coupons:
    - code: "SAVE10"
      discount_type: "percent"
      discount_value: 10
      expires_at: "+30days"
      usage_limit: 100
    - code: "FLAT1000"
      discount_type: "fixed"
      discount_value: 1000
    - code: "EXPIRED"
      expires_at: "-1days"
    - code: "LIMITED"
      usage_limit: 10
      used_count: 10
```

### 전체 Checkpoints 요약

```yaml
# .team-claude/sessions/{session-id}/checkpoints.yaml

session: abc12345
created_at: 2024-01-15T10:00:00Z
approved_at: 2024-01-15T11:30:00Z
approved_by: human

# 요약 (상세는 각 checkpoint 파일에)
checkpoints:
  - id: coupon-model
    name: "쿠폰 도메인 모델"
    dependencies: []
    scenarios_count: 8
    edge_cases_count: 4
    file: "./coupon-model.yaml"

  - id: coupon-service
    name: "쿠폰 서비스 로직"
    dependencies: [coupon-model]
    scenarios_count: 12
    edge_cases_count: 6
    file: "./coupon-service.yaml"

  - id: coupon-api
    name: "쿠폰 API 엔드포인트"
    dependencies: [coupon-service]
    scenarios_count: 8
    edge_cases_count: 4
    file: "./coupon-api.yaml"

  - id: coupon-integration
    name: "통합 테스트"
    dependencies: [coupon-api]
    scenarios_count: 5
    edge_cases_count: 2
    file: "./coupon-integration.yaml"
```

---

## 파일 구조

```
.team-claude/
├── sessions/
│   ├── index.json                    # 전체 세션 목록
│   │
│   └── abc12345/                     # session-id
│       ├── meta.json                 # 세션 메타정보
│       ├── conversation.md           # 대화 기록 (전체)
│       ├── decisions.json            # 결정 사항 목록
│       │
│       ├── contracts/                # Interface + Test Code
│       │   ├── coupon-model/
│       │   │   ├── interface.ts      # 타입/시그니처
│       │   │   └── contract.test.ts  # TDD 테스트 코드
│       │   └── coupon-service/
│       │       ├── interface.ts
│       │       └── contract.test.ts
│       │
│       ├── checkpoints/              # Checkpoint 정의 (시나리오 포함!)
│       │   ├── checkpoints.yaml      # 전체 요약
│       │   ├── coupon-model.yaml     # 개별 checkpoint + scenarios
│       │   ├── coupon-service.yaml
│       │   ├── coupon-api.yaml
│       │   └── coupon-integration.yaml
│       │
│       └── specs/
│           └── architecture.md       # 아키텍처 설계
```

---

## 자동 검증 루프에서 Scenarios 활용

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Delegate → Server → Worker → Validation 흐름                               │
│                                                                             │
│  1. /team-claude:delegate coupon-service                                    │
│     │                                                                       │
│     ▼                                                                       │
│  2. Server가 coupon-service.yaml 로드                                       │
│     │  - criteria[]                                                         │
│     │  - scenarios[]  ← 구체적 테스트 케이스                                │
│     │  - test_fixtures[]                                                    │
│     │                                                                       │
│     ▼                                                                       │
│  3. Worker용 CLAUDE.md 생성                                                 │
│     │  - scenarios를 체크리스트로 변환                                      │
│     │  - "이 케이스들을 통과시켜야 함" 명시                                 │
│     │                                                                       │
│     ▼                                                                       │
│  4. Worker 실행 → 구현 완료                                                 │
│     │                                                                       │
│     ▼                                                                       │
│  5. Server가 validation.command 실행                                        │
│     │  - 테스트 실행 결과 수집                                              │
│     │  - 각 scenario별 pass/fail 판정                                       │
│     │                                                                       │
│     ▼                                                                       │
│  6. 결과 분석                                                               │
│     │  ✅ 모든 scenarios 통과 → 완료!                                       │
│     │  ❌ 일부 실패 → 구체적 피드백 생성                                    │
│     │                                                                       │
│     ▼ (실패 시)                                                             │
│  7. 피드백 생성 (어떤 scenario가 왜 실패했는지)                              │
│     │                                                                       │
│     │  ## Iteration 1 - FAILED                                              │
│     │                                                                       │
│     │  ❌ Scenario: "같은 주문에 같은 쿠폰 재적용 시도"                      │
│     │     Expected: error.type = "DuplicateApplication"                     │
│     │     Actual: success = true (에러 없이 적용됨)                         │
│     │                                                                       │
│     │  💡 Hint: apply() 메서드에서 기존 적용 여부 체크 필요                 │
│     │                                                                       │
│     ▼                                                                       │
│  8. Worker 재실행 (피드백 반영)                                             │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### CLAUDE.md에 Scenarios 포함 예시

Worker에게 전달되는 CLAUDE.md:

```markdown
# Task: coupon-service

## Objective
쿠폰 검증 및 적용 로직 구현

## Test Scenarios (모두 통과해야 함!)

### Criterion 1: validate()가 유효한 쿠폰에 대해 true 반환

| # | Scenario | Input | Expected |
|---|----------|-------|----------|
| 1 | 10% 할인 쿠폰 | code="SAVE10", order.total=10000 | valid=true, discount=1000 |
| 2 | 고정 금액 할인 | code="FLAT1000", order.total=5000 | valid=true, discount=1000 |

### Criterion 2: 만료된 쿠폰 처리

| # | Scenario | Input | Expected |
|---|----------|-------|----------|
| 3 | 어제 만료 | expires_at=yesterday | valid=false, error=CouponExpired |
| 4 | 사용 횟수 초과 | used_count >= usage_limit | valid=false, error=CouponExhausted |

### Criterion 3: 중복 적용 방지

| # | Scenario | Input | Expected |
|---|----------|-------|----------|
| 5 | 같은 주문 재적용 | 이미 적용된 상태 | error=DuplicateApplication |
| 6 | 다른 주문 적용 | 다른 order_id | success=true |

### Edge Cases

| # | Scenario | Input | Expected |
|---|----------|-------|----------|
| 7 | 0% 할인 | discount_value=0 | discount_amount=0 |
| 8 | 100% 할인 | discount_value=100 | discount_amount=order.total |
| 9 | null 쿠폰 코드 | code=null | error=InvalidInput |

## Test Fixtures

테스트에 사용할 데이터가 이미 준비되어 있습니다:
- `SAVE10`: 10% 할인, 30일 후 만료
- `FLAT1000`: 1000원 할인
- `EXPIRED`: 어제 만료
- `LIMITED`: 사용 횟수 초과 (10/10)

## Validation Command

```bash
npm run test -- --grep 'CouponService'
```

모든 scenarios가 통과하면 완료입니다.
```

### meta.json 구조

```json
{
  "sessionId": "abc12345",
  "title": "쿠폰 할인 기능",
  "status": "designing",
  "phase": "checkpoint_review",
  "createdAt": "2024-01-15T10:00:00Z",
  "updatedAt": "2024-01-15T11:30:00Z",
  "decisions": [
    {
      "topic": "architecture",
      "decision": "direct-call",
      "reason": "코드베이스 일관성",
      "decidedAt": "2024-01-15T10:15:00Z"
    },
    {
      "topic": "duplicate_coupon",
      "decision": "not_allowed",
      "reason": "비즈니스 요구사항",
      "decidedAt": "2024-01-15T10:30:00Z"
    }
  ],
  "checkpointsApproved": false
}
```

---

## 출력 예시

### 새 세션 시작

```
🏗️ Architect Loop 시작

  session-id: abc12345
  요구사항: 결제 시스템에 쿠폰 할인 기능 추가

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

📊 코드베이스 분석 중...

  발견된 관련 파일:
  • src/services/payment.service.ts
  • src/models/order.entity.ts
  • src/controllers/order.controller.ts

  현재 아키텍처 패턴: 직접 호출 (Service → Repository)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🤔 몇 가지 질문이 있습니다...
```

### 설계 완료

```
✅ Architect Loop 완료: abc12345

━━━ 산출물 ━━━

  📁 .team-claude/sessions/abc12345/specs/
  ├── architecture.md
  ├── contracts.md
  └── checkpoints.yaml

━━━ Checkpoints ━━━

  1. coupon-model      - 쿠폰 도메인 모델
  2. coupon-service    - 쿠폰 서비스 로직
  3. coupon-api        - 쿠폰 API 엔드포인트
  4. coupon-integration - 통합 테스트

━━━ 다음 단계 ━━━

  구현 위임:
    /team-claude:delegate --session abc12345

  전체 위임 (병렬):
    /team-claude:delegate --session abc12345 --all
```

---

## 에러 처리

### 세션 없음

```
❌ 세션을 찾을 수 없습니다: xyz99999

현재 세션 목록:
  - abc12345: 쿠폰 할인 기능 (설계 중)
  - def67890: 알림 시스템 (완료)

/team-claude:architect --list 로 전체 목록을 확인하세요.
```

### 승인 대기 중

```
⏸️ Checkpoint 승인 대기 중

  세션: abc12345
  상태: checkpoint_review

  승인 후 구현 위임이 가능합니다.
  /team-claude:architect --resume abc12345 로 계속 진행하세요.
```

---

## 설계 원칙

### 1. 대화형 정제 (Conversational Refinement)

```
모호한 요구사항 ───▶ 구체적 스펙
     │                    ▲
     └──── 대화 ──────────┘
```

### 2. 점진적 결정 (Progressive Decision)

```
큰 결정 먼저 ───▶ 세부 결정 나중
  (아키텍처)        (구현 디테일)
```

### 3. 명시적 기준점 (Explicit Checkpoints)

```
암묵적 "완료" ───▶ 명시적 검증 기준
                    (자동 검증 가능)
```
