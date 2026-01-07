---
name: review-all
description: Claude, Codex, Gemini 3개 LLM으로 동시에 문서를 리뷰합니다
argument-hint: "[리뷰 요청 사항]"
allowed-tools: ["Task", "Glob"]
---

# 종합 리뷰 커맨드

Claude, OpenAI Codex, Google Gemini 3개 LLM을 모두 사용하여 문서를 종합적으로 리뷰합니다.

## 핵심 워크플로우

**토큰 최적화**: MainAgent가 파일 내용을 읽지 않고 경로만 수집하여 자연어 프롬프트 구성

```
1. Glob으로 파일 경로 수집 (내용 안 읽음!)
2. 자연어 프롬프트 구성
3. 3개 Agent에 동일한 프롬프트 병렬 전달
4. 결과 취합 및 종합 분석
```

## 작업 프로세스

### Step 1: 사용자 요청 파싱

사용자 요청에서 추출:
- **관점**: "엔지니어 관점", "편집자 관점" 등 (기본: 기술 리뷰어)
- **대상 파일**: "plans", "시놉시스" 등 (기본: plans/*.md)
- **컨텍스트**: "스타트업", "3명 팀" 등

**파일 패턴 매핑**:
- "plans" → plans/*.md
- "1화" → novels/*/1화/*.md
- "시놉시스" → novels/*/전체 시놉시스.md

### Step 2: Glob으로 파일 경로만 수집

**CRITICAL**: 파일 내용을 읽지 않습니다!

```
Glob: plans/*.md
→ ["plans/file1.md", "plans/file2.md", ...]
```

파일이 없으면 즉시 에러:
```
Error: 'pattern'에 맞는 파일을 찾을 수 없습니다.
```

### Step 3: 자연어 프롬프트 구성

```
컨텍스트:
- 프로젝트: 소설 집필 시스템
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

각 Agent가 처리:
- **Claude**: 프롬프트 파싱 → Read로 파일 읽기 → 직접 리뷰
- **Codex**: 프롬프트 → 스크립트 (스크립트가 파일 읽기) → Codex CLI
- **Gemini**: 프롬프트 → 스크립트 (스크립트가 파일 읽기) → Gemini CLI

### Step 5: 결과 취합 및 종합 분석

3개 결과를 비교 분석:
- 공통 강점 / 약점
- 관점 차이
- 종합 권장사항

## 사용법

```bash
# 기본 종합 리뷰
/review-all

# 관점 지정
/review-all "staff+ 엔지니어 관점으로 plans를 리뷰해줘"

# 복합 요청
/review-all "웹소설 편집자 관점에서 시놉시스와 1-3화를 평가해줘"
```

## 종합 리포트 구조

```markdown
# 3개 LLM 종합 리뷰 결과

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

## 토큰 절감 효과

**Before (기존)**:
- MainAgent: 파일 읽기 (50K 토큰)
- Claude Agent: 파일 읽기 (50K 토큰)
- Codex Agent: 파일 읽기 (50K 토큰)
- Gemini Agent: 파일 읽기 (50K 토큰)
- Total: 200K 토큰

**After (최적화)**:
- MainAgent: Glob만 (2K 토큰)
- Claude Agent: 파일 읽기 (50K 토큰)
- Codex Agent: 프롬프트만 (0.5K 토큰, 스크립트가 파일 읽기)
- Gemini Agent: 프롬프트만 (0.5K 토큰, 스크립트가 파일 읽기)
- Total: 53K 토큰

**절감률**: ~74%

## 주의사항

- **API 필요**: Codex, Gemini CLI 설치 필요
- **시간 소요**: 약 2-3분
- **중요한 리뷰에 사용**: 범용 종합 리뷰용 (특정 대상은 /review-plan, /review-code 사용)
