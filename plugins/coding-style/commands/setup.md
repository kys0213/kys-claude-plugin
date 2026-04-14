---
description: coding-style 플러그인을 설치합니다. ~/.claude/CLAUDE.md에 코딩 원칙을 추가하고 Stop hook을 등록합니다.
allowed-tools:
  - Bash
  - Read
  - Write
  - AskUserQuestion
---

# Coding Style Setup

개인 코딩 원칙(CLAUDE.md)과 Stop hook을 설치합니다.

## Step 1: CLAUDE.md 설치

### 1-1. 템플릿 읽기

플러그인 내장 템플릿을 읽습니다:

```
Read: ${CLAUDE_PLUGIN_ROOT}/templates/CLAUDE.md
```

### 1-2. 기존 CLAUDE.md 확인

```bash
cat ~/.claude/CLAUDE.md 2>/dev/null || echo "(파일 없음)"
```

### 1-3. 워터마크 기반 중복 확인

`~/.claude/CLAUDE.md` 내용에 `[coding-style:begin]`이 이미 존재하는지 확인합니다.

**이미 존재하는 경우:**

기존 `[coding-style:begin]` ~ `[coding-style:end]` 구간을 템플릿 내용으로 **교체**합니다 (업데이트).

```
coding-style 섹션이 이미 존재합니다. 최신 내용으로 업데이트합니다.
```

**존재하지 않는 경우:**

`~/.claude/CLAUDE.md` 끝에 템플릿 내용을 **추가**합니다. 기존 내용은 그대로 유지됩니다.

### 1-4. 쓰기

Write 도구로 `~/.claude/CLAUDE.md`에 결과를 저장합니다.

**주의:** 기존 내용 중 `[coding-style:begin]` ~ `[coding-style:end]` 바깥 영역은 절대 수정하지 않습니다.

## Step 2: Stop Hook 등록

### 2-1. 현재 설정 확인

```bash
cat ~/.claude/settings.json 2>/dev/null || echo "{}"
```

`Stop` hook에 `suggest-simplify.sh`가 이미 등록되어 있으면 이 Step을 건너뜁니다:

```
coding-style Stop hook이 이미 등록되어 있습니다.
```

### 2-2. Hook 등록

`~/.claude/settings.json`의 `hooks` 섹션에 Stop 항목을 추가합니다:

```json
{
  "hooks": {
    "Stop": [
      {
        "matcher": "",
        "hooks": [
          {
            "type": "command",
            "command": "bash ${CLAUDE_PLUGIN_ROOT}/hooks/suggest-simplify.sh",
            "timeout": 10
          }
        ]
      }
    ]
  }
}
```

**주의:** 기존 hooks 설정이 있으면 `Stop`만 추가하고, 다른 hook은 건드리지 않습니다.

## Step 3: 결과 확인

설치 완료 후 요약을 출력합니다:

```
coding-style 설치 완료!

  CLAUDE.md : ~/.claude/CLAUDE.md ({installed|updated})
  Stop hook : ~/.claude/settings.json ({installed|already exists})

포함된 원칙:
  - 설계 최우선 (Design First)
  - SOLID / TDD
  - 코드 품질 게이트
  - /simplify 마무리 검토
```
