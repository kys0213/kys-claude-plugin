---
description: Claude, Codex, Gemini 3개 LLM으로 파일을 다관점 분석합니다
argument-hint: "[파일 패턴] [관점]"
allowed-tools: ["Task", "Glob"]
---

# 분석 커맨드 (/analyze)

Claude, OpenAI Codex, Google Gemini 3개 LLM을 사용하여 파일을 종합적으로 분석합니다.
리뷰와 달리 **패턴, 의존성, 아키텍처 추적**에 초점을 맞춥니다.

## 사용법

```bash
# 파일 패턴 지정
/analyze "src/**/*.rs"
/analyze "plugins/external-llm/**/*"

# 관점 지정
/analyze "src/**/*.rs" architecture
/analyze "src/**/*.rs" dependencies
/analyze "src/**/*.rs" patterns

# 자연어
/analyze "Rust 코드의 의존성 구조를 분석해줘"
/analyze "플러그인 아키텍처를 분석해줘"
```

## 핵심 워크플로우

**토큰 최적화**: MainAgent가 파일 내용을 읽지 않고 경로만 수집

```
1. Glob으로 파일 경로 수집 (내용 안 읽음!)
2. 자연어 프롬프트 구성 (분석 프레이밍)
3. 3개 Agent에 동일한 프롬프트 병렬 전달
4. 결과 취합 및 종합 분석
```

## 작업 프로세스

### Step 1: 사용자 요청 파싱

사용자 요청에서 추출:
- **관점**: architecture, dependencies, patterns, complexity, security (기본: 종합 분석)
- **대상 파일**: glob 패턴 또는 키워드
- **컨텍스트**: 추가 요청사항

**파일 패턴 매핑**:
- "src" / "코드" → `src/**/*.{ts,tsx,js,rs,py}`
- "plans" / "설계" → `plans/*.md`
- "docs" / "문서" → `docs/**/*.md`
- "plugins" → `plugins/**/*.md`
- Glob 패턴이 직접 제공되면 그대로 사용

### Step 2: Glob으로 파일 경로만 수집

**CRITICAL**: 파일 내용을 읽지 않습니다!

```
Glob: plugins/external-llm/**/*
→ ["plugins/external-llm/commands/invoke-codex.md", ...]
```

파일이 없으면 즉시 에러:
```
Error: '[패턴]'에 맞는 파일을 찾을 수 없습니다.
```

### Step 3: 자연어 프롬프트 구성

```
컨텍스트:
- 프로젝트: [프로젝트명]
- 관점: [파악한 관점]

대상 파일:
- path/to/file1.rs
- path/to/file2.rs

사용자 요청:
[원래 사용자 요청]

위 파일들을 분석해주세요. 특히 다음에 초점을 맞춰주세요:
- 코드 패턴과 반복 구조
- 모듈 간 의존성 관계
- 아키텍처 구조와 계층
- 개선 기회와 기술 부채
```

### Step 4: 3개 Agent 병렬 실행

**동일한 프롬프트**를 3개 Agent에 병렬 전달:

```
Task(subagent_type="llm-reviewer-claude", prompt=PROMPT, run_in_background=true)
Task(subagent_type="llm-reviewer-codex", prompt=PROMPT, run_in_background=true)
Task(subagent_type="llm-reviewer-gemini", prompt=PROMPT, run_in_background=true)
```

각 Agent의 역할:
- **llm-reviewer-claude**: 프롬프트 파싱 → Read로 파일 읽기 → 직접 분석
- **llm-reviewer-codex**: 프롬프트 → call-codex.sh (스크립트가 파일 읽기) → Codex CLI
- **llm-reviewer-gemini**: 프롬프트 → call-gemini.sh (스크립트가 파일 읽기) → Gemini CLI

### Step 5: 결과 취합 및 종합 분석

3개 결과를 비교 분석합니다.

## 종합 리포트 구조

```markdown
# 3개 LLM 종합 분석 결과

## 요약

- **Claude 분석**: [핵심 발견]
- **Codex 분석**: [핵심 발견]
- **Gemini 분석**: [핵심 발견]

## 공통 발견 (3개 LLM 모두 동의)

1. [3개 모두 발견한 패턴/구조]
   - **신뢰도**: 높음 (3/3 LLM 동의)

## 아키텍처 분석

### 모듈 구조
### 의존성 관계
### 계층 분석

## 패턴 분석

### 반복 패턴
### 안티패턴
### 개선 기회

## 관점 차이

### Claude의 독특한 발견
### Codex의 독특한 발견
### Gemini의 독특한 발견

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
- **Codex/Gemini 불가 시**: 해당 LLM 결과는 "N/A"로 표시, 가용한 LLM으로만 리포트 생성
