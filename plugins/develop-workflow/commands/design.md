---
description: 요구사항 기반 아키텍처 설계 - 3개 LLM으로 설계하고 Contract를 정의합니다
argument-hint: "[설계 요청 사항]"
allowed-tools: ["Task", "Glob", "Read", "Write", "AskUserQuestion"]
---

# 설계 커맨드 (/design)

요구사항을 수집하고, Claude/Codex/Gemini 3개 LLM으로 아키텍처를 설계한 후 Contract(Interface + Test Code)를 정의합니다.

> `/develop` 워크플로우의 Phase 1을 단독 실행합니다.

## 핵심 워크플로우

```
Phase 0: 요구사항 수집 (HITL)
    │
    ▼
Phase 1: 아키텍처 설계 (3개 LLM 병렬)
    ├── Claude ─┐
    ├── Codex  ─┼─→ 취합
    └── Gemini ─┘
    │
    ▼
Phase 2: 통합 + ASCII 다이어그램
    │
    ▼
Phase 3: Contract 정의
    └── Interface + Test Code + Checkpoints
```

---

## Phase 0: 요구사항 수집

사용자 요청을 분석하고, 모호하거나 부정확한 부분이 있으면 `AskUserQuestion`으로 명확화합니다.

### 수집할 항목

1. **기능 요구사항**: 핵심 기능, 사용자 시나리오
2. **비기능 요구사항**: 성능, 확장성, 보안
3. **제약조건**: 기술 스택, 팀 규모, 일정, 기존 시스템 연동
4. **우선순위**: Must-have vs Nice-to-have, MVP 범위

### 모호함 감지 및 질문

- **정량화되지 않은 표현**: "대규모", "빠른" → 구체적 수치 질문
- **기술 스택 미지정**: 선호 스택 또는 제약 확인
- **범위 불명확**: "~같은" → 핵심 기능 범위 확인
- **상충되는 요구사항**: "빠르면서 저렴" → 우선순위 확인

### 질문 원칙

1. 맥락에 맞게: 사용자가 언급한 내용 기반으로 필요한 것만
2. 최소한으로: 설계에 꼭 필요한 정보만 (1-2개 질문)
3. 선택지 제공: 열린 질문보다 구체적 옵션
4. 충분하면 진행: 핵심 정보가 모이면 바로 Phase 1 진행

### 요구사항 정리 형식

```markdown
# 요구사항 정리

## 기능 요구사항
- [FR-1] 사용자 인증 (이메일/소셜 로그인)
- [FR-2] ...

## 비기능 요구사항
- [NFR-1] 동시 사용자 1000명 지원

## 제약조건
- 기술 스택: TypeScript, React, Node.js
- 클라우드: AWS

## 우선순위
- Must: FR-1, FR-2
- Should: FR-3
```

---

## Phase 1: 아키텍처 설계 (3개 LLM 병렬)

### 설계 프롬프트 구성

Phase 0에서 정리한 요구사항을 포함하여 구성:

```
# 아키텍처 설계 요청

## 요구사항
[Phase 0에서 정리한 요구사항]

## 설계 요청
위 요구사항을 만족하는 아키텍처를 설계해주세요.

다음 항목을 포함해주세요:
1. 주요 컴포넌트와 책임
2. 컴포넌트 간 상호작용
3. 데이터 흐름
4. 기술 선택과 근거
5. 잠재적 리스크

구체적인 코드가 아닌 상위 레벨 설계를 제공해주세요.
```

### 3개 Agent 병렬 실행

```
Task(subagent_type="architect-claude", prompt=PROMPT, run_in_background=true)
Task(subagent_type="architect-codex", prompt=PROMPT, run_in_background=true)
Task(subagent_type="architect-gemini", prompt=PROMPT, run_in_background=true)
```

---

## Phase 2: 통합 및 도식화

### 통합 분석

```markdown
# 설계 통합 분석

## 합의 사항 (3개 LLM 공통)
- [공통 컴포넌트]
- [공통 기술 선택]

## 의견 차이
| 항목 | Claude | Codex | Gemini |
|------|--------|-------|--------|
| DB   | PostgreSQL | MongoDB | PostgreSQL |

## 최종 권장사항
[3개 의견을 종합한 추천]
```

### ASCII 다이어그램 생성

컴포넌트 다이어그램과 데이터 흐름도를 ASCII로 표현합니다.

---

## Phase 3: Contract 정의

아키텍처를 병렬 구현 가능한 단위로 분해합니다.

### Contract 구조

각 Checkpoint에 대해:
- **Interface**: 컴포넌트의 공개 인터페이스 (함수 시그니처, 타입)
- **Test Code**: 인터페이스를 검증하는 테스트
- **Validation**: 통과/실패를 판단하는 명령어

```yaml
checkpoints:
  - id: "checkpoint-1"
    description: "사용자 인증 모듈"
    interface:
      - "src/auth/types.ts"
      - "src/auth/auth-service.ts"
    tests:
      - "tests/auth/auth-service.test.ts"
    validation:
      command: "npm test -- --testPathPattern=auth"
      expected: "all tests pass"
    dependencies: []

  - id: "checkpoint-2"
    description: "API 엔드포인트"
    interface:
      - "src/api/routes.ts"
    tests:
      - "tests/api/routes.test.ts"
    validation:
      command: "npm test -- --testPathPattern=api"
      expected: "all tests pass"
    dependencies: ["checkpoint-1"]
```

### 사용자 확인

Contract 정의 완료 후 `AskUserQuestion`으로 확인:
- Checkpoint 구성이 적절한지
- Interface 정의가 맞는지
- 빠진 부분이 없는지

---

## 최종 출력 구조

```markdown
# 설계 결과

## 1. 요구사항 요약

## 2. 아키텍처 개요 (3개 LLM 통합)

## 3. 컴포넌트 다이어그램 (ASCII)

## 4. 데이터 흐름도 (ASCII)

## 5. 기술 스택
| 레이어 | 기술 | 선택 근거 |

## 6. Checkpoints (Contract 정의)

## 7. 리스크 및 고려사항

## 8. 다음 단계
→ `/multi-review`로 스펙 리뷰
→ `/implement`로 구현 시작
```

---

## 사용 예시

```bash
# 새 시스템 설계
/design "실시간 채팅 시스템을 설계해줘"

# 제약조건 포함
/design "React + Node.js로 e-commerce 플랫폼 설계. 동시 사용자 10만명"

# 기존 시스템 확장
/design "현재 모놀리스를 마이크로서비스로 전환하는 설계"
```

## 주의사항

- **요구사항 우선**: 코드가 아닌 요구사항부터 시작
- **도식화 필수**: 모든 설계에 ASCII 다이어그램 포함
- **다양한 관점**: 3개 LLM 의견 차이도 명시
- **API 필요**: Codex, Gemini CLI 설치 필요 (없으면 Claude만 실행)
