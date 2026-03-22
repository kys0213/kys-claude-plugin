---
description: 코드베이스를 분석하여 .claude/rules/ 구조를 자동 제안하고 생성합니다
argument-hint: "[--gap-only]"
allowed-tools:
  - Task
  - Glob
  - AskUserQuestion
---

# Scaffold Rules Command

> 코드베이스의 언어, 프레임워크, 디렉토리 구조를 분석하여 최적의 `.claude/rules/` 규칙 파일을 자동 생성합니다.

## Overview

3단계 워크플로우로 진행됩니다:

1. **분석**: 코드베이스의 기술 스택과 아키텍처 패턴을 감지
2. **제안**: 규칙 파일 구조를 제안하고 사용자 확인 (HITL)
3. **생성**: 승인된 구조로 `.claude/rules/*.md` 파일 일괄 생성

## Execution Steps

### Step 1: 코드베이스 분석 (Sub-agent 위임)

codebase-analyzer 에이전트에게 분석을 위임합니다.

```
Task: codebase-analyzer
Prompt: |
  현재 프로젝트의 코드베이스를 분석하여 .claude/rules/ 규칙 파일 구조를 제안해주세요.

  분석 항목:
  1. 언어/프레임워크 감지
  2. 디렉토리 구조 패턴
  3. 기존 .claude/rules/ 파일 gap 분석
  4. 실제 파일 패턴 샘플링
```

`$ARGUMENTS`에 `--gap-only`가 포함된 경우, 프롬프트에 다음을 추가합니다:

```
기존 .claude/rules/ 파일이 있는 경우 gap 분석만 수행하세요.
새로운 규칙은 누락된 레이어에 대해서만 제안합니다.
```

### Step 2: 사용자 확인 (HITL)

codebase-analyzer의 제안 결과를 사용자에게 보여주고 확인을 받습니다.

```
AskUserQuestion: |
  위 규칙 파일 구조를 생성하시겠습니까?

  옵션:
  - "yes" 또는 "y": 전체 생성
  - 번호 (예: "1,3,5"): 선택한 파일만 생성
  - "no" 또는 "n": 취소
  - 직접 수정 요청 가능 (예: "service.md의 paths를 수정해주세요")
```

사용자가 거부하거나 수정을 요청하면 적절히 대응합니다.

### Step 3: 규칙 파일 생성 (Sub-agent 위임)

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

### Step 4: 결과 요약

생성 결과를 사용자에게 보고합니다:

```
scaffold-rules 완료!

생성된 규칙 파일:
  .claude/rules/controller.md  (paths: **/*.controller.ts)
  .claude/rules/service.md     (paths: **/*.service.ts)
  .claude/rules/testing.md     (paths: **/*.spec.ts)

모든 규칙 파일에 paths: frontmatter가 설정되어 있습니다.
해당 파일 수정 시에만 Claude 컨텍스트에 자동 로드됩니다.

규칙을 세부 조정하려면 해당 파일을 직접 편집하세요.
```
