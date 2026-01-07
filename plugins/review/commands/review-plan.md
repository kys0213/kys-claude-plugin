---
name: review-plan
description: 3개 LLM(Claude, Codex, Gemini)으로 plans 문서를 리뷰합니다
argument-hint: "[리뷰 요청 사항]"
allowed-tools: ["Task", "Glob"]
---

# Plan 리뷰 커맨드

Claude, OpenAI Codex, Google Gemini 3개 LLM을 모두 사용하여 plans 문서를 종합적으로 리뷰합니다.

## 사용법

```bash
# 기본 plan 리뷰
/review-plan

# 관점 지정
/review-plan "staff+ 엔지니어 관점으로 리뷰해줘"

# 컨텍스트 추가
/review-plan "3명 스타트업 팀 관점에서 기술적 타당성 검토해줘"
```

## 기본 대상 파일

- `plans/*.md`

## 워크플로우

### Step 1: 사용자 요청 파싱

사용자 요청에서 추출:
- **관점**: "엔지니어 관점", "편집자 관점" 등 (기본: 기술 리뷰어)
- **컨텍스트**: "스타트업", "3명 팀" 등

### Step 2: Glob으로 파일 경로만 수집

**CRITICAL**: 파일 내용을 읽지 않습니다!

```
Glob: plans/*.md
→ ["plans/file1.md", "plans/file2.md", ...]
```

파일이 없으면 즉시 에러:
```
Error: plans/*.md에 맞는 파일을 찾을 수 없습니다.
```

### Step 3: 자연어 프롬프트 구성

```
리뷰 종류: plan

컨텍스트:
- 프로젝트: [프로젝트명]
- 관점: [파악한 관점]

대상 파일:
- plans/file1.md
- plans/file2.md
- plans/file3.md

사용자 요청:
[원래 사용자 요청]

위 파일들을 리뷰해주세요.
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
- 공통 강점 / 약점
- 관점 차이
- 종합 권장사항

## 종합 리포트 구조

```markdown
# Plan 리뷰 결과 (3개 LLM 종합)

## 요약

- **Claude 점수**: XX/100
- **Codex 점수**: XX/100
- **Gemini 점수**: XX/100
- **평균 점수**: XX/100

## 공통 강점 (3개 LLM 모두 동의)

1. [3개 모두 언급한 강점]

## 공통 약점 (Critical - 3개 LLM 모두 지적)

1. [3개 모두 지적한 문제]
   - **신뢰도**: 높음 (3/3 LLM 동의)

## 관점 차이

### Claude의 독특한 지적
### Codex의 독특한 지적
### Gemini의 독특한 지적

## 종합 권장사항

### Critical (3개 LLM 합의)
### Important (2개 이상 LLM 언급)
### 참고사항 (1개 LLM만 언급)
```

## 토큰 최적화

- MainAgent: Glob만 (파일 내용 안 읽음)
- Claude Agent: 파일 읽기 + 직접 리뷰
- Codex/Gemini Agent: 프롬프트만 전달 (스크립트가 파일 읽기)

## 주의사항

- **API 필요**: Codex, Gemini CLI 설치 필요
- **중요한 리뷰에 사용**: 일상적 리뷰는 단일 LLM 사용 권장
