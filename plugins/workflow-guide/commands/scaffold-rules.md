---
description: 코드베이스를 분석하여 .claude/rules/ 구조를 자동 제안하고 생성합니다
argument-hint: "[--gap-only]"
allowed-tools:
  - Task
  - Glob
  - Read
  - Write
  - AskUserQuestion
---

# Scaffold Rules Command

> 코드베이스와 문서를 분석하여 프로젝트 가치관(CLAUDE.md)과 레이어별 규칙(.claude/rules/)을 자동 생성합니다.

## Overview

6단계 워크플로우로 진행됩니다:

1. **병렬 분석**: 코드 구조 + 문서 패턴을 동시 분석
2. **가치관 인터뷰**: 자동감지 결과를 기반으로 사용자 확인 (HITL)
3. **CLAUDE.md 생성**: 프로젝트 정체성·가치관·문서 컨벤션을 CLAUDE.md에 추가
4. **규칙 제안**: 레이어별 규칙 파일 구조를 제안하고 사용자 확인 (HITL)
5. **규칙 생성**: 승인된 구조로 `.claude/rules/*.md` 파일 일괄 생성
6. **결과 요약**: 전체 변경 사항 보고

## Execution Steps

### Step 1: 병렬 분석 (Sub-agent 위임)

codebase-analyzer와 document-analyzer를 **동시에** 실행합니다.

```
Task: codebase-analyzer (run_in_background=true)
Prompt: |
  현재 프로젝트의 코드베이스를 분석하여 .claude/rules/ 규칙 파일 구조를 제안해주세요.

  분석 항목:
  1. 언어/프레임워크 감지
  2. LSP 사용 가능 여부 확인
  3. 디렉토리 구조 패턴
  4. 기존 .claude/rules/ 파일 gap 분석
  5. 실제 파일 패턴 샘플링
  6. 프로젝트 맥락 자동감지 (유형, 팀 규모, 엔지니어링 성향)
```

```
Task: document-analyzer (run_in_background=true)
Prompt: |
  현재 프로젝트의 문서를 분석하여 문서화 컨벤션을 추출해주세요.

  분석 항목:
  1. 문체 (종결어미 패턴)
  2. 언어 혼용 패턴
  3. 톤과 독자 수준
  4. 구조 패턴 (헤딩, 리스트, 테이블)
```

`$ARGUMENTS`에 `--gap-only`가 포함된 경우, codebase-analyzer 프롬프트에 다음을 추가합니다:

```
기존 .claude/rules/ 파일이 있는 경우 gap 분석만 수행하세요.
새로운 규칙은 누락된 레이어에 대해서만 제안합니다.
```

두 에이전트의 결과를 모두 수신한 후 Step 2로 진행합니다.

### Step 2: 가치관 인터뷰 (HITL)

분석 결과에서 자동감지된 항목을 프리필하고, 사용자에게 확인·수정을 받습니다. 3회의 AskUserQuestion으로 진행합니다.

**Q1: 프로젝트 맥락** (codebase-analyzer 결과 프리필)

```
AskUserQuestion: |
  자동 감지된 프로젝트 컨텍스트:
    - 유형: {detected_type} ({근거})
    - 팀: {detected_team} ({근거})

  확인 또는 수정해주세요:
    1. 프로젝트 유형: [제품 / 라이브러리 / 내부 도구 / 오픈소스]
    2. 팀 구성: [1인 / 소규모 / 대규모]

  "yes"로 확인하거나 수정할 항목을 알려주세요.
```

**Q2: 엔지니어링 트레이드오프**

```
AskUserQuestion: |
  충돌할 때의 우선순위를 선택해주세요 (기본값은 *표시):

    1. 가독성 ↔ 성능: [*가독성 / 균형 / 성능]
    2. 명시성 ↔ 간결성: [*명시적 / 균형 / 간결]
    3. 안정성 ↔ 속도: [*안정성 / 균형 / 속도]
    4. 추상화 시점: [이른 추상화 / *Rule of 3 / 명시적 중복]

  "yes"로 기본값 선택, 또는 번호와 선택지를 알려주세요.
  (예: "2: 간결, 4: 이른 추상화")
```

**Q3: 문서화 컨벤션** (document-analyzer 결과 프리필)

```
AskUserQuestion: |
  문서 분석 결과:
    - 문체: {detected_style} ({근거})
    - 언어: {detected_language}
    - 독자 수준: {detected_audience}

  확인 또는 수정해주세요:
    1. 문서 독자: [개발자만 / 비기술직 포함 / 외부 사용자]
    2. 톤: [격식 / 캐주얼 / 기술적]
    3. 언어: [한국어 / 영어 / 혼용]

  "yes"로 확인하거나 수정할 항목을 알려주세요.
```

document-analyzer가 "문서 부족" 상태를 반환한 경우, Q3에서 프리필 없이 직접 질문합니다.

### Step 3: CLAUDE.md 섹션 생성 (HITL)

인터뷰 결과를 종합하여 CLAUDE.md에 추가할 섹션을 생성합니다.

1. **기존 CLAUDE.md 확인**: `Read`로 기존 내용을 읽어서 `## 프로젝트 맥락`, `## 엔지니어링 가치`, `## 문서화` 섹션 존재 여부 확인
2. **섹션 생성**: convention-architect Skill Section 9의 템플릿을 기반으로, 인터뷰 답변 + 분석 결과를 채워 섹션을 생성
3. **사용자에게 제시**: 생성된 섹션을 보여주고 승인 요청

```
AskUserQuestion: |
  아래 내용을 CLAUDE.md에 추가합니다:

  {generated_sections}

  "yes"로 추가, "no"로 건너뛰기, 또는 수정 사항을 알려주세요.
```

4. **승인 시**: `Write`로 CLAUDE.md에 섹션 추가
   - CLAUDE.md 없으면 새로 생성
   - 기존 CLAUDE.md에 해당 섹션이 없으면 append
   - 기존 섹션이 있으면 교체 (전체 파일을 다시 Write)
5. **거부 시**: 건너뛰고 Step 4로 진행

### Step 4: 레이어별 규칙 승인 (HITL)

codebase-analyzer의 규칙 구조 제안을 사용자에게 보여주고 확인합니다.

```
AskUserQuestion: |
  위 규칙 파일 구조를 생성하시겠습니까?

  옵션:
  - "yes" 또는 "y": 전체 생성
  - 번호 (예: "1,3,5"): 선택한 파일만 생성
  - "no" 또는 "n": 취소
  - 직접 수정 요청 가능 (예: "service.md의 paths를 수정해주세요")
```

사용자가 거부하면 Step 6(결과 요약)으로 건너뜁니다.

### Step 5: 규칙 파일 생성 (Sub-agent 위임)

사용자가 승인한 구조로 rules-generator 에이전트에게 생성을 위임합니다.

```
Task: rules-generator
Prompt: |
  다음 승인된 규칙 파일을 생성해주세요:

  [사용자가 승인한 규칙 파일 목록]

  각 파일은:
  - paths: frontmatter 필수 포함
  - 코드베이스의 실제 패턴을 반영한 DO/DON'T 예시
  - 50-150줄 이내
```

### Step 6: 결과 요약

전체 변경 사항을 보고합니다:

```
scaffold-rules 완료!

CLAUDE.md 변경:
  ✓ 프로젝트 맥락 섹션 추가
  ✓ 엔지니어링 가치 섹션 추가
  ✓ 문서화 컨벤션 섹션 추가

생성된 규칙 파일:
  .claude/rules/controller.md  (paths: **/*.controller.ts)
  .claude/rules/service.md     (paths: **/*.service.ts)
  .claude/rules/testing.md     (paths: **/*.spec.ts)

CLAUDE.md = 나침반 (항상 로드, 프로젝트 가치관)
.claude/rules/ = 지도 (해당 파일 수정 시 로드, 레이어별 컨벤션)
```
