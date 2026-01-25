# 에이전트 관리

HITL 기반 대화형 인터페이스로 전체 에이전트 라이프사이클을 관리합니다.

## 메인 메뉴

```typescript
AskUserQuestion({
  questions: [{
    question: "에이전트 관리 - 무엇을 하시겠습니까?",
    header: "Agents",
    options: [
      { label: "에이전트 목록 보기", description: "활성화/비활성화 상태 포함" },
      { label: "새 에이전트 생성", description: "커스텀 에이전트 대화형 생성" },
      { label: "에이전트 상세 보기", description: "설정, 역할, 체크리스트 확인" },
      { label: "에이전트 수정", description: "기존 에이전트 설정 변경" },
      { label: "에이전트 삭제", description: "커스텀 에이전트 제거" }
    ],
    multiSelect: false
  }]
})
```

---

## 에이전트 목록 보기

플러그인 기본 + 커스텀 에이전트를 계층별로 표시:

```
📋 Team Claude 에이전트 목록

━━━ 플러그인 기본 에이전트 ━━━

  ✅ spec_validator (활성화)
     설계 문서 일관성 검증, Contract 정합성 확인
     위치: plugins/team-claude/agents/spec-validator.md

  ✅ test_oracle (활성화)
     테스트 실패 분석, 피드백 생성, 재시도 전략 제안
     위치: plugins/team-claude/agents/test-oracle.md

  ✅ impl_reviewer (활성화)
     구현 품질 검토, 코드 리뷰, 개선점 제안
     위치: plugins/team-claude/agents/impl-reviewer.md

  ○ conflict_analyzer (비활성화)
     머지 충돌 분석, 해결 방안 제시
     위치: plugins/team-claude/agents/conflict-analyzer.md

━━━ 커스텀 에이전트 ━━━

  ✅ payment_expert (활성화)
     결제 도메인 전문가 - PG 연동, 금액 계산 검증
     위치: .team-claude/agents/payment-expert.md

  ✅ security_auditor (활성화)
     보안 취약점 검토, OWASP 체크리스트
     위치: .team-claude/agents/security-auditor.md
```

목록 조회 후:

```typescript
AskUserQuestion({
  questions: [{
    question: "다음 작업을 선택하세요",
    header: "Next",
    options: [
      { label: "에이전트 활성화/비활성화 토글", description: "선택한 에이전트 상태 변경" },
      { label: "새 에이전트 생성", description: "커스텀 에이전트 추가" },
      { label: "뒤로", description: "에이전트 메뉴로 돌아가기" }
    ],
    multiSelect: false
  }]
})
```

---

## 새 에이전트 생성 (HITL 대화형)

7단계 대화형 위자드로 에이전트를 생성합니다.

### Step 1: 역할 영역

```typescript
AskUserQuestion({
  questions: [{
    question: "새 에이전트의 역할 영역은 무엇인가요?",
    header: "Domain",
    options: [
      { label: "도메인 전문가", description: "특정 비즈니스 도메인 (결제, 금융, 물류 등)" },
      { label: "기술 전문가", description: "특정 기술 스택 (DB, 캐시, 메시징 등)" },
      { label: "품질 검토", description: "코드 품질, 성능, 보안 등" },
      { label: "프로세스 전문가", description: "CI/CD, 테스트, 배포 등" }
    ],
    multiSelect: false
  }]
})
```

### Step 2: 세부 전문 분야

선택에 따라 동적 질문:

**도메인 전문가 선택 시:**

```typescript
AskUserQuestion({
  questions: [{
    question: "어떤 도메인을 전문으로 하나요?",
    header: "Specialty",
    options: [
      { label: "결제/PG", description: "PG 연동, 금액 계산, 환불 로직" },
      { label: "인증/보안", description: "OAuth, JWT, 권한 관리" },
      { label: "주문/재고", description: "주문 플로우, 재고 관리, 배송" },
      { label: "직접 입력", description: "커스텀 도메인 설명" }
    ],
    multiSelect: false
  }]
})
```

**기술 전문가 선택 시:**

```typescript
AskUserQuestion({
  questions: [{
    question: "어떤 기술 영역을 전문으로 하나요?",
    header: "Tech",
    options: [
      { label: "데이터베이스", description: "쿼리 최적화, 스키마 설계, 트랜잭션" },
      { label: "캐싱", description: "Redis, Memcached, 캐시 전략" },
      { label: "메시징", description: "Kafka, RabbitMQ, 이벤트 처리" },
      { label: "직접 입력", description: "커스텀 기술 영역" }
    ],
    multiSelect: false
  }]
})
```

### Step 3: 이름 및 설명

사용자에게 직접 텍스트 입력을 요청합니다. (AskUserQuestion의 "Other" 옵션 활용)

```typescript
AskUserQuestion({
  questions: [{
    question: "에이전트 이름을 입력하세요",
    header: "Name",
    options: [
      { label: "payment-expert", description: "결제 도메인 전문가 (예시)" },
      { label: "security-auditor", description: "보안 검토 전문가 (예시)" },
      { label: "db-optimizer", description: "데이터베이스 최적화 전문가 (예시)" }
      // 사용자가 "Other" 선택 시 직접 입력
    ],
    multiSelect: false
  }, {
    question: "에이전트 설명을 입력하세요 (한 줄)",
    header: "Description",
    options: [
      { label: "직접 입력", description: "에이전트의 역할과 전문 분야를 설명" }
      // "Other"로 직접 입력 유도
    ],
    multiSelect: false
  }]
})
```

**이름 규칙:**
- 형식: kebab-case (소문자 + 하이픈)
- 길이: 3-50자
- 예: `payment-expert`, `security-auditor`, `api-reviewer`

**설명 예시:**
```
결제 도메인 전문가 - PG사 연동 로직, 금액 계산 정확성, 환불/취소 플로우 검증
```

### Step 4: 모델 선택

```typescript
AskUserQuestion({
  questions: [{
    question: "이 에이전트가 사용할 모델을 선택하세요",
    header: "Model",
    options: [
      { label: "sonnet (권장)", description: "균형잡힌 성능과 비용" },
      { label: "opus", description: "복잡한 분석이 필요한 경우" },
      { label: "haiku", description: "빠른 응답이 중요한 경우" }
    ],
    multiSelect: false
  }]
})
```

### Step 5: 도구 선택

```typescript
AskUserQuestion({
  questions: [{
    question: "에이전트가 사용할 도구를 선택하세요",
    header: "Tools",
    options: [
      { label: "Read", description: "파일 읽기 (필수)" },
      { label: "Glob", description: "파일 패턴 검색" },
      { label: "Grep", description: "내용 검색" },
      { label: "Bash", description: "명령어 실행 (주의 필요)" }
    ],
    multiSelect: true
  }]
})
```

### Step 6: 체크리스트 정의

선택한 도메인/전문 분야에 따라 추천 체크리스트를 제시하고, 사용자가 선택/추가합니다.

```typescript
// 도메인에 따른 추천 체크리스트 제시 (결제 도메인 예시)
AskUserQuestion({
  questions: [{
    question: "이 에이전트의 리뷰 체크리스트를 선택하세요",
    header: "Checklist",
    options: [
      { label: "금액 계산 정확성", description: "반올림/버림, 할인, 세금 계산" },
      { label: "PG 응답 에러 핸들링", description: "응답 코드별 처리 로직" },
      { label: "트랜잭션 롤백", description: "실패 시 롤백 처리" },
      { label: "동시성 이슈", description: "동시 요청 처리" }
    ],
    multiSelect: true  // 복수 선택 가능
  }]
})
```

선택 후 추가 항목 여부 확인:

```typescript
AskUserQuestion({
  questions: [{
    question: "체크리스트에 추가할 항목이 있나요?",
    header: "Add More",
    options: [
      { label: "항목 추가", description: "커스텀 체크 항목 입력" },
      { label: "완료", description: "현재 체크리스트로 진행" }
    ],
    multiSelect: false
  }]
})
```

**"항목 추가" 선택 시:** 사용자가 "Other"로 직접 입력 → 반복

**최종 체크리스트 예시:**
```
  • 금액 계산 시 반올림/버림 처리 정확성
  • PG 응답 코드별 에러 핸들링
  • 트랜잭션 실패 시 롤백 처리
  • 동시 결제 요청 시 동시성 이슈
  • 환불 금액이 원거래 금액 초과하지 않음 (사용자 추가)
```

### Step 7: 추가 컨텍스트 (선택)

```typescript
AskUserQuestion({
  questions: [{
    question: "에이전트에 추가 컨텍스트를 제공하시겠습니까?",
    header: "Context",
    options: [
      { label: "프로젝트 특화 지침 추가", description: "이 프로젝트만의 규칙이나 가이드" },
      { label: "참고 문서 연결", description: "API 문서, 스펙 문서 경로" },
      { label: "건너뛰기", description: "기본 설정으로 생성" }
    ],
    multiSelect: false
  }]
})
```

### 생성 완료

```
✅ 에이전트 생성 완료

📄 파일: .team-claude/agents/payment-expert.md

━━━ 생성된 에이전트 요약 ━━━

  이름: payment-expert
  설명: 결제 도메인 전문가 - PG사 연동 로직, 금액 계산 정확성, 환불/취소 플로우 검증
  모델: sonnet
  도구: Read, Glob, Grep

  체크리스트:
    • 금액 계산 시 반올림/버림 처리 정확성
    • PG 응답 코드별 에러 핸들링
    • 트랜잭션 실패 시 롤백 처리
    • 동시 결제 요청 시 동시성 이슈
    • 환불 금액이 원거래 금액 초과하지 않음

  상태: ✅ 활성화됨
```

---

## 에이전트 상세 보기

```typescript
AskUserQuestion({
  questions: [{
    question: "상세 정보를 볼 에이전트를 선택하세요",
    header: "Select",
    options: [
      // 동적으로 현재 에이전트 목록 표시
      { label: "spec_validator", description: "플러그인 기본" },
      { label: "test_oracle", description: "플러그인 기본" },
      { label: "payment_expert", description: "커스텀" },
      { label: "security_auditor", description: "커스텀" }
    ],
    multiSelect: false
  }]
})
```

### 상세 출력

```
📋 에이전트 상세: payment-expert

━━━ 기본 정보 ━━━
  타입: 커스텀
  상태: ✅ 활성화됨
  위치: .team-claude/agents/payment-expert.md
  생성일: 2024-01-23

━━━ 설정 ━━━
  모델: sonnet
  도구: Read, Glob, Grep

━━━ 역할 ━━━
  결제 도메인 전문가
  - PG사 연동 로직 검토
  - 금액 계산 정확성 확인
  - 환불/취소 플로우 검증

━━━ 체크리스트 (5개 항목) ━━━
  1. 금액 계산 시 반올림/버림 처리 정확성
  2. PG 응답 코드별 에러 핸들링
  3. 트랜잭션 실패 시 롤백 처리
  4. 동시 결제 요청 시 동시성 이슈
  5. 환불 금액이 원거래 금액 초과하지 않음
```

후속 액션:

```typescript
AskUserQuestion({
  questions: [{
    question: "다음 작업을 선택하세요",
    header: "Action",
    options: [
      { label: "이 에이전트 수정", description: "설정, 체크리스트 등 변경" },
      { label: "활성화/비활성화 토글", description: "상태 변경" },
      { label: "에이전트 파일 열기", description: "직접 편집" },
      { label: "뒤로", description: "에이전트 목록으로" }
    ],
    multiSelect: false
  }]
})
```

---

## 에이전트 수정 (HITL)

```typescript
AskUserQuestion({
  questions: [{
    question: "payment-expert 에이전트의 어떤 부분을 수정하시겠습니까?",
    header: "Modify",
    options: [
      { label: "기본 정보", description: "이름, 설명 변경" },
      { label: "모델 변경", description: "sonnet → opus 등" },
      { label: "도구 변경", description: "사용 가능한 도구 수정" },
      { label: "체크리스트 수정", description: "항목 추가/수정/삭제" },
      { label: "프로젝트 컨텍스트", description: "추가 지침 수정" },
      { label: "전체 재설정", description: "처음부터 다시 설정" }
    ],
    multiSelect: false
  }]
})
```

### 체크리스트 수정

```typescript
AskUserQuestion({
  questions: [{
    question: "체크리스트를 어떻게 수정하시겠습니까?",
    header: "Checklist",
    options: [
      { label: "항목 추가", description: "새 체크 항목 추가" },
      { label: "항목 수정", description: "기존 항목 내용 변경" },
      { label: "항목 삭제", description: "불필요한 항목 제거" },
      { label: "순서 변경", description: "우선순위 조정" },
      { label: "전체 재작성", description: "체크리스트 새로 작성" }
    ],
    multiSelect: false
  }]
})
```

### 수정 완료

```
✅ 에이전트 수정 완료

변경 사항:
  model: sonnet → opus
  체크리스트: 5개 → 7개 항목 (+2)
    + 부분 환불 금액 계산 정확성
    + PG사별 응답 포맷 호환성

저장됨: .team-claude/agents/payment-expert.md
```

---

## 에이전트 삭제

```typescript
AskUserQuestion({
  questions: [{
    question: "삭제할 에이전트를 선택하세요 (커스텀만 삭제 가능)",
    header: "Delete",
    options: [
      // 커스텀 에이전트만 표시
      { label: "payment_expert", description: "결제 도메인 전문가" },
      { label: "security_auditor", description: "보안 취약점 검토" }
    ],
    multiSelect: false
  }]
})
```

### 삭제 확인

```typescript
AskUserQuestion({
  questions: [{
    question: "⚠️ payment-expert 에이전트를 정말 삭제하시겠습니까?",
    header: "Confirm",
    options: [
      { label: "예, 삭제", description: "에이전트 파일 삭제 및 설정에서 제거" },
      { label: "아니오, 취소", description: "삭제하지 않음" },
      { label: "비활성화만", description: "파일은 유지하고 비활성화만" }
    ],
    multiSelect: false
  }]
})
```

### 삭제 완료

```
✅ 에이전트 삭제 완료

  삭제됨: .team-claude/agents/payment-expert.md
  설정에서 제거됨: agents.custom, agents.enabled

현재 활성 에이전트: 4개
  - spec_validator
  - test_oracle
  - impl_reviewer
  - security_auditor
```

### 플러그인 기본 에이전트 삭제 시도

```
❌ 플러그인 기본 에이전트는 삭제할 수 없습니다: spec_validator

다음 작업을 선택하세요:
  • 비활성화: 에이전트 목록 > 활성화/비활성화 토글
  • 커스터마이징: 로컬에 복사 후 수정
```

---

## 플러그인 에이전트 커스터마이징

기본 에이전트를 로컬에 복사하여 수정:

```typescript
AskUserQuestion({
  questions: [{
    question: "spec_validator를 커스터마이징하시겠습니까?",
    header: "Customize",
    options: [
      { label: "로컬에 복사 후 수정", description: "플러그인 원본은 유지, 로컬 버전 생성" },
      { label: "취소", description: "커스터마이징하지 않음" }
    ],
    multiSelect: false
  }]
})
```

```
📝 에이전트 커스터마이징: spec_validator

복사됨:
  원본: plugins/team-claude/agents/spec-validator.md
  대상: .team-claude/agents/spec-validator.md

이제 로컬 버전이 우선 적용됩니다.
수정하시겠습니까?
```

---

## 에이전트 계층 구조

```
┌─────────────────────────────────────────────────────────────┐
│                    에이전트 해석 순서                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  1. 프로젝트 로컬 (최우선)                                   │
│     .team-claude/agents/{name}.md                          │
│                                                             │
│  2. 플러그인 기본                                            │
│     plugins/team-claude/agents/{name}.md                   │
│                                                             │
│  동일 이름 → 로컬이 플러그인 기본을 오버라이드               │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 에이전트 파일 스키마

```yaml
---
name: {agent-name}
description: {한 줄 설명}
model: sonnet | opus | haiku
tools: ["Read", "Glob", "Grep", ...]
---

# {Agent Name}

{에이전트 역할 설명}

## 역할

- 역할 1
- 역할 2

## 리뷰 체크리스트

- [ ] 체크 항목 1
- [ ] 체크 항목 2

## 프로젝트 컨텍스트

(선택적 - 프로젝트 특화 지침)

## 리뷰 출력 형식

(선택적 - 커스텀 출력 형식)
```

### 예시: payment-expert.md

```markdown
---
name: payment-expert
description: 결제 도메인 전문가 - PG사 연동, 금액 계산, 환불/취소 검증
model: sonnet
tools: ["Read", "Glob", "Grep"]
---

# Payment Expert Agent

결제 도메인을 전문적으로 검토하는 에이전트입니다.

## 역할

- PG사 연동 로직의 정확성 검토
- 금액 계산 로직 검증 (반올림, 할인, 세금 등)
- 환불/취소 플로우의 완전성 확인
- 결제 보안 관련 취약점 검토

## 리뷰 체크리스트

- [ ] 금액 계산 시 반올림/버림 처리 정확성
- [ ] PG 응답 코드별 에러 핸들링
- [ ] 트랜잭션 실패 시 롤백 처리
- [ ] 동시 결제 요청 시 동시성 이슈
- [ ] 환불 금액이 원거래 금액 초과하지 않음

## 리뷰 출력 형식

### ✅ 통과 항목
### ⚠️ 주의 필요
### ❌ 수정 필요
### 💡 개선 제안
```
