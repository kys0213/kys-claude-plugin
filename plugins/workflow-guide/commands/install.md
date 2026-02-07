---
description: 프로젝트에 에이전트 설계 원칙 룰을 설치합니다
argument-hint: "[--force]"
allowed-tools:
  - Read
  - Write
  - Bash
  - Glob
  - AskUserQuestion
---

# Workflow Guide Install Command

플러그인 내부의 룰 원본 파일을 읽어서 프로젝트의 `.claude/rules/` 디렉토리에 설치합니다.

> 참고: [소프트웨어 3.0 시대를 맞이하며](https://toss.tech/article/software-3-0-era) 블로그의 원칙을 기반으로 합니다.

## 룰 원본 파일

```
${CLAUDE_PLUGIN_ROOT}/rules/agent-design-principles.md
```

## Execution Steps

### Step 1: 현재 프로젝트 상태 확인

```bash
ls .claude/rules/ 2>/dev/null
```

- `.claude/rules/` 디렉토리가 없으면 `mkdir -p .claude/rules`로 생성
- 이미 `.claude/rules/agent-design-principles.md` 파일이 존재하면:
  - `$ARGUMENTS`에 `--force`가 있으면 덮어쓰기
  - 없으면 `AskUserQuestion`으로 덮어쓸지 확인

### Step 2: 기존 워크플로우 파일 탐색

프로젝트에 이미 존재하는 에이전트 관련 파일을 탐색합니다:

```
Glob: .claude/commands/**/*.md
Glob: .claude/agents/**/*.md
Glob: .claude/skills/**/SKILL.md
```

탐색 결과가 있으면 설치 완료 시 안내에 포함합니다.

### Step 3: 룰 원본 파일 읽기

플러그인 내부의 룰 원본 파일을 Read 도구로 읽습니다:

```
Read: ${CLAUDE_PLUGIN_ROOT}/rules/agent-design-principles.md
```

### Step 4: 프로젝트에 룰 파일 설치

Step 3에서 읽어온 내용을 **그대로** 프로젝트의 `.claude/rules/agent-design-principles.md`에 Write 도구로 작성합니다.

**중요**: 내용을 수정하거나 요약하지 않고 원본 그대로 복사합니다.

### Step 5: 결과 확인

설치 완료 후 안내 메시지를 출력합니다:

```
설치 완료: .claude/rules/agent-design-principles.md

이 룰은 Claude가 새로운 Skill, Sub-agent, Slash Command를 만들 때 자동으로 참조합니다.

주요 원칙:
  Slash Command = Controller (진입점만, 로직 위임)
  Sub-agent = Service Layer (Skill 조합, 독립 Context)
  Skill = SRP (단일 책임, 폭발 주의)
  토큰 = 메모리 (Glob 경로만, 스크립트 분리, CLAUDE.md 정적만)

ref: https://toss.tech/article/software-3-0-era
```

기존 워크플로우 파일이 탐색된 경우 추가 안내:

```
기존 워크플로우 파일이 감지되었습니다:
  - .claude/commands/xxx.md
  - .claude/agents/xxx.md

workflow-reviewer 에이전트로 기존 파일의 설계 원칙 준수 여부를 리뷰할 수 있습니다.
```
