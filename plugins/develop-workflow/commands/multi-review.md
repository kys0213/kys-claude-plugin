---
description: Claude, Codex, Gemini 3개 LLM으로 동시에 문서를 리뷰합니다
argument-hint: "[리뷰 요청 사항]"
allowed-tools: ["Task", "Glob"]
---

# 리뷰 커맨드 (/multi-review)

Claude, OpenAI Codex, Google Gemini 3개 LLM을 모두 사용하여 문서를 종합적으로 리뷰합니다.

> `/develop` 워크플로우의 Phase 2를 단독 실행합니다.

## 사용법

```bash
# 기본 리뷰 (plans/*.md)
/multi-review

# 대상 지정
/multi-review "plans 리뷰해줘"
/multi-review "src 코드 리뷰해줘"

# 관점 지정
/multi-review "staff+ 엔지니어 관점으로 plans를 리뷰해줘"
/multi-review "보안 관점에서 api 코드 리뷰해줘"

# 복합 요청
/multi-review "코드와 테스트를 종합적으로 평가해줘"
```

## 핵심 워크플로우

**토큰 최적화**: MainAgent가 파일 내용을 읽지 않고 경로만 수집

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
- **대상 파일**: "plans", "src", "시놉시스" 등 (기본: plans/*.md)
- **컨텍스트**: "스타트업", "3명 팀" 등

**파일 패턴 매핑**:
- "plans" → plans/*.md
- "src" / "코드" → src/**/*.{ts,tsx,js}
- "설계" → docs/design/*.md

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
- 프로젝트: [프로젝트명]
- 관점: [파악한 관점]

대상 파일:
- path/to/file1.md
- path/to/file2.md

사용자 요청:
[원래 사용자 요청]

위 파일들을 리뷰해주세요.
```

### Step 4: 3개 Agent 병렬 실행

**동일한 프롬프트**를 3개 Agent에 병렬 전달:

```
Task(subagent_type="reviewer-claude", prompt=PROMPT, run_in_background=true)
Task(subagent_type="reviewer-codex", prompt=PROMPT, run_in_background=true)
Task(subagent_type="reviewer-gemini", prompt=PROMPT, run_in_background=true)
```

각 Agent의 역할:
- **reviewer-claude**: 프롬프트 파싱 → Read로 파일 읽기 → 직접 리뷰
- **reviewer-codex**: 프롬프트 → call-codex.sh (스크립트가 파일 읽기) → Codex CLI
- **reviewer-gemini**: 프롬프트 → call-gemini.sh (스크립트가 파일 읽기) → Gemini CLI

### Step 5: 결과 취합 및 종합 분석

3개 결과를 비교 분석합니다.

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

## 컨센서스 분석 기준

- **3/3 합의**: 높은 신뢰도 → 반드시 반영
- **2/3 동의**: 중간 신뢰도 → 사용자에게 제시
- **1/3 지적**: 참고 사항 → 정보 제공

## 주의사항

- **API 필요**: Codex, Gemini CLI 설치 필요
- **토큰 최적화**: MainAgent는 파일 경로만 수집, SubAgent가 실제 내용 읽기
- **Codex/Gemini 불가 시**: Claude만으로 리뷰 실행 (3개 LLM 비교 불가 안내)
