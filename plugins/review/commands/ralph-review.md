---
name: ralph-review
description: RALPH 피드백 루프용 Multi-LLM 코드 리뷰 - 구현 후 테스트 전에 3개 LLM으로 코드 품질 검토
argument-hint: "[변경된 파일 패턴 또는 경로]"
allowed-tools: ["Task", "Glob", "Bash"]
---

# RALPH Multi-LLM 코드 리뷰

Worker가 RALPH 루프에서 코드 구현 후, 테스트 전에 3개 LLM(Claude, Codex, Gemini)으로 코드 품질을 검토합니다.

## 사용법

```bash
# 변경된 파일 자동 감지 (git diff)
/ralph-review

# 특정 파일/패턴 지정
/ralph-review "src/services/*.ts"
/ralph-review "coupon-service 구현 리뷰해줘"

# 관점 지정
/ralph-review "보안 관점에서 리뷰해줘"
```

## 핵심 워크플로우

```
RALPH 루프 내 위치:

구현 ──▶ /ralph-review ──▶ 피드백 반영 ──▶ 테스트
                │
        ┌───────┴───────┐
        ▼       ▼       ▼
     Claude   Codex   Gemini
        │       │       │
        └───────┴───────┘
                │
                ▼
        종합 피드백 (실행 가능한 개선사항)
```

**토큰 최적화**: MainAgent가 파일 경로만 수집, 각 Agent가 직접 읽음

## 작업 프로세스

### Step 1: 리뷰 대상 파일 결정

사용자 인자가 없으면 **git diff로 변경된 파일 자동 감지**:

```bash
# staged + unstaged 변경 파일
git diff --name-only HEAD
```

패턴이 지정되면 Glob으로 파일 수집:

```
Glob: src/services/*.ts
→ ["src/services/coupon.ts", "src/services/discount.ts", ...]
```

### Step 2: 프로젝트 컨텍스트 수집

```bash
# 언어/프레임워크 감지
ls package.json pyproject.toml go.mod Cargo.toml 2>/dev/null
```

컨텍스트 정보:
- 프로젝트 언어
- 테스트 프레임워크
- 코딩 컨벤션 (있으면)

### Step 3: RALPH 리뷰 프롬프트 구성

```
# RALPH 코드 리뷰 요청

## 컨텍스트
- 프로젝트 언어: TypeScript
- RALPH 루프 단계: 구현 완료, 테스트 전
- 목적: 테스트 전 코드 품질 검토

## 대상 파일
- src/services/coupon.ts
- src/services/discount.ts

## 리뷰 요청 사항
{사용자 요청 또는 기본 리뷰}

## 리뷰 관점

**RALPH 특화**: 이 코드는 곧 테스트됩니다. 다음에 집중:

1. **버그 가능성**: 테스트에서 실패할 가능성이 있는 코드
2. **엣지 케이스**: 놓친 경계 조건
3. **타입 안전성**: 런타임 에러 가능성
4. **계약 준수**: 인터페이스 스펙과의 일치성

일반적인 코드 스타일보다 **테스트 통과에 영향을 줄 수 있는 이슈**를 우선 지적해주세요.
```

### Step 4: 3개 Agent 병렬 실행

```
Task(subagent_type="ralph-claude", prompt=PROMPT, run_in_background=true)
Task(subagent_type="ralph-codex", prompt=PROMPT, run_in_background=true)
Task(subagent_type="ralph-gemini", prompt=PROMPT, run_in_background=true)
```

### Step 5: 결과 취합 및 실행 가능한 피드백 생성

3개 결과를 분석하여 **실행 가능한 개선사항** 생성:

```markdown
# RALPH Multi-LLM 리뷰 결과

## 즉시 수정 권장 (3개 LLM 합의)

### 1. [파일:라인] 이슈 설명
**현재 코드**:
```typescript
const result = data.items.map(...)
```

**수정 제안**:
```typescript
const result = data?.items?.map(...) ?? []
```

**이유**: null 체크 누락 - 테스트 실패 가능성 높음

---

## 고려 권장 (2개 이상 LLM 언급)

### 2. [파일:라인] 이슈 설명
...

---

## 참고사항 (1개 LLM만 언급)

- Claude: ...
- Codex: ...
- Gemini: ...

---

## 요약

- **Critical (즉시 수정)**: N개
- **Important (고려 권장)**: N개
- **테스트 통과 예상**: [높음/중간/낮음]
```

## RALPH 특화 리뷰 기준

### Critical (테스트 실패 가능)

- null/undefined 미처리
- 타입 불일치
- 인터페이스 계약 위반
- 경계 조건 미처리
- 예외 미처리

### Important (품질 이슈)

- 중복 코드
- 비효율적 알고리즘
- 하드코딩된 값
- 누락된 에러 핸들링

### Nice-to-have (스타일)

- 네이밍 개선
- 코드 정리
- 문서화

## 에러 처리

### 변경된 파일 없음

```
Warning: git diff에서 변경된 파일이 없습니다.

다음 중 하나를 시도하세요:
1. 파일 패턴 직접 지정: /ralph-review "src/**/*.ts"
2. 작업 중인 파일 확인 후 다시 시도
```

### API 실패

```
Warning: {LLM} 리뷰를 가져올 수 없습니다.
나머지 {N}개 LLM 결과로 진행합니다.
```

## 주의사항

- **RALPH 루프 전용**: 구현 → 리뷰 → 수정 → 테스트 흐름에 최적화
- **테스트 중심**: 일반 코드 리뷰보다 테스트 통과에 집중
- **실행 가능한 피드백**: 모호한 조언 대신 구체적인 수정 제안
- **API 필요**: Codex, Gemini CLI 설치 필요
