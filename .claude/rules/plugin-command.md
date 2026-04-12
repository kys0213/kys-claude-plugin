---
paths:
  - "**/commands/*.md"
---

# Plugin Command 명세 컨벤션

> Command 명세 파일의 작성 형식 규칙. 설계 원칙(진입점 역할, 위임 구조, 멱등성)은 `agent-design-principles.md` 참조.

## 원칙

1. **Frontmatter 필수**: `description`, `allowed-tools`는 반드시 설정한다
2. **위임 대상 명시**: 어떤 agent/script에 위임하는지, 커맨드와 출력 형식을 포함한다
3. **에러 처리 명시**: 사전 조건 실패, 외부 도구 에러 케이스를 명세에 포함한다
4. **Output Examples**: 성공/실패 케이스의 예시 출력을 포함한다

## DO

frontmatter에 역할을 선언하고, 단계별 실행 흐름과 에러 케이스를 명시한다:

```markdown
---
description: default 브랜치 또는 지정한 base 브랜치를 최신화 후 신규 브랜치 생성
argument-hint: "[new-branch] [base-branch]"
allowed-tools:
  - Bash
  - AskUserQuestion
---

# Git Branch

## Context

- Current branch: !`git branch --show-current`

## Usage

- `/git-branch` - 브랜치 이름을 대화형으로 입력받음
- `/git-branch feature/new-feature` - default 브랜치 기반으로 생성

## Execution

### Step 1: 인자 확인 및 브랜치 이름 요청
...

### Step 2: git-utils CLI 실행

\`\`\`bash
git-utils branch <new-branch> [--base=<base-branch>]
\`\`\`

**출력 (JSON):** `{ "branchName": "...", "baseBranch": "..." }`

## 에러 처리

**uncommitted changes가 있는 경우:**
- 사용자에게 먼저 커밋하거나 stash할 것을 안내

## Output Examples
...
```

## DON'T

frontmatter를 누락하거나, 에러 케이스 없이 실행 흐름만 나열하지 않는다:

```markdown
# Branch

git checkout -b $BRANCH
git push -u origin $BRANCH
# ↑ description 없음, allowed-tools 없음, 에러 처리 없음, Output Examples 없음
```

## 체크리스트

- [ ] `description`이 한 줄로 목적을 명확히 설명하는가?
- [ ] `allowed-tools`에 실제로 필요한 도구만 나열했는가?
- [ ] 실행 단계가 명확한 Step으로 구분되어 있는가?
- [ ] CLI/agent에 위임하는 경우 커맨드와 출력 형식을 명시했는가?
- [ ] 에러 케이스와 사용자 안내를 포함했는가?
