---
name: review-code
description: 3개 LLM(Claude, Codex, Gemini)으로 코드를 리뷰합니다
argument-hint: "[파일 패턴 또는 리뷰 요청]"
allowed-tools: ["Task", "Glob"]
---

# Code 리뷰 커맨드

Claude, OpenAI Codex, Google Gemini 3개 LLM을 모두 사용하여 코드를 종합적으로 리뷰합니다.

## 사용법

```bash
# 기본 코드 리뷰 (src/**/*.ts)
/review-code

# 특정 경로 지정
/review-code "src/components/*.tsx"

# 관점 지정
/review-code "보안 관점에서 api/*.ts를 리뷰해줘"

# 특정 이슈 중점
/review-code "성능 최적화 관점에서 리뷰해줘"
```

## 기본 대상 파일

- `src/**/*.ts` 또는 `src/**/*.tsx` (프로젝트에 따라)

## 파일 패턴 매핑

사용자 요청에서 파일 패턴 추출:
- "components" → `src/components/**/*.{ts,tsx}`
- "api" → `src/api/**/*.ts`
- "hooks" → `src/hooks/**/*.ts`
- "utils" → `src/utils/**/*.ts`

## 워크플로우

### Step 1: 사용자 요청 파싱

사용자 요청에서 추출:
- **파일 패턴**: 특정 경로 또는 기본값
- **관점**: "보안", "성능", "가독성" 등 (기본: 코드 품질)
- **컨텍스트**: 프로젝트 특성, 팀 규모 등

### Step 2: Glob으로 파일 경로만 수집

**CRITICAL**: 파일 내용을 읽지 않습니다!

```
Glob: src/**/*.ts
→ ["src/index.ts", "src/utils/helper.ts", ...]
```

파일이 없으면 즉시 에러:
```
Error: src/**/*.ts에 맞는 파일을 찾을 수 없습니다.
```

### Step 3: 자연어 프롬프트 구성

```
리뷰 종류: code

컨텍스트:
- 프로젝트: [프로젝트명]
- 관점: [파악한 관점 - 보안/성능/가독성 등]

대상 파일:
- src/index.ts
- src/utils/helper.ts

사용자 요청:
[원래 사용자 요청]

위 파일들을 코드 리뷰해주세요.
```

### Step 4: 3개 Agent 병렬 실행

**동일한 프롬프트**를 3개 Agent에 병렬 전달:

```
Task(subagent_type="claude", prompt=PROMPT, run_in_background=true)
Task(subagent_type="codex", prompt=PROMPT, run_in_background=true)
Task(subagent_type="gemini", prompt=PROMPT, run_in_background=true)
```

### Step 5: 결과 취합 및 종합 분석

3개 결과를 비교 분석:
- 공통 이슈 / 권장사항
- 관점 차이
- 종합 개선 방향

## 종합 리포트 구조

```markdown
# Code 리뷰 결과 (3개 LLM 종합)

## 요약

- **Claude 점수**: XX/100
- **Codex 점수**: XX/100
- **Gemini 점수**: XX/100
- **평균 점수**: XX/100

## 공통 이슈 (3개 LLM 모두 지적)

1. [3개 모두 지적한 이슈]
   - **위치**: 파일:라인
   - **심각도**: Critical/Important/Minor
   - **신뢰도**: 높음 (3/3 LLM 동의)

## 공통 강점

1. [3개 모두 긍정적으로 평가한 부분]

## 관점 차이

### Claude의 독특한 지적
### Codex의 독특한 지적
### Gemini의 독특한 지적

## 종합 권장사항

### Critical (즉시 수정 필요)
### Important (조기 수정 권장)
### Nice-to-have (여유 시 개선)
```

## 코드 리뷰 관점 예시

- **보안**: SQL injection, XSS, 인증/인가 취약점
- **성능**: N+1 쿼리, 불필요한 렌더링, 메모리 누수
- **가독성**: 네이밍, 함수 크기, 주석
- **유지보수성**: 결합도, 응집도, 테스트 용이성
- **Best Practice**: 언어/프레임워크 컨벤션 준수

## 주의사항

- **API 필요**: Codex, Gemini CLI 설치 필요
- **파일 수 제한**: 너무 많은 파일은 토큰 한도 초과 가능
- **중요한 리뷰에 사용**: 일상적 리뷰는 단일 LLM 사용 권장
