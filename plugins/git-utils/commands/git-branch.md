---
name: git-branch
description: default 브랜치 또는 지정한 base 브랜치를 최신화 후 신규 브랜치 생성
argument-hint: "[new-branch] [base-branch]"
allowed-tools:
  - Bash
  - AskUserQuestion
---

# Git Branch

default 브랜치 또는 지정한 base 브랜치를 최신화한 후 신규 브랜치를 생성합니다.

## Context

- Current branch: !`git branch --show-current`
- Default branch: !`${CLAUDE_PLUGIN_ROOT}/scripts/detect-default-branch.sh`
- Has uncommitted changes: !`git status --short`

## Usage

- `/git-branch` - 브랜치 이름을 대화형으로 입력받음
- `/git-branch feature/new-feature` - default 브랜치 기반으로 생성
- `/git-branch feature/new-feature develop` - develop 브랜치 기반으로 생성
- `/git-branch fix/hotfix release/1.0` - release/1.0 브랜치 기반으로 생성

## Execution

### Step 1: 인자 확인 및 브랜치 이름 요청

인자가 없으면 대화 맥락을 분석하여 브랜치 이름을 추천합니다.

**맥락이 있는 경우** → AskUserQuestion으로 추천 제공:

```
question: "생성할 브랜치 이름을 선택하거나 입력해주세요."
header: "Branch"
options:
  - label: "feature/user-auth"
    description: "사용자 인증 기능 구현 (Recommended)"
  - label: "feature/add-user-authentication"
    description: "대안 이름"
multiSelect: false
```

**맥락이 없는 경우** → 텍스트로 직접 질문:

"생성할 브랜치 이름을 알려주세요. (예: `feature/user-auth`, `fix/login-bug`, `WAD-0212`)"

**브랜치 이름 추론 규칙:**
- kebab-case 사용 (예: `user-auth`, `add-cache`)
- 간결하고 명확하게 (2-4 단어)
- 영문 소문자만 사용
- 작업 유형에 맞는 prefix 자동 선택:
  - 새 기능 → `feature/`
  - 버그 수정 → `fix/`
  - 리팩토링 → `refactor/`
  - 문서 → `docs/`

### Step 2: 스크립트 실행

브랜치 이름이 확보되면 스크립트를 실행합니다:

```bash
${CLAUDE_PLUGIN_ROOT}/scripts/create-branch.sh <new-branch> [base-branch]
```

### 인자 처리

| 인자 | 필수 | 설명 |
|------|------|------|
| `new-branch` | Yes (또는 대화형 입력) | 생성할 브랜치 이름 |
| `base-branch` | No | 기반 브랜치 (미지정 시 default 브랜치) |

### 에러 처리

**uncommitted changes가 있는 경우:**
- 스크립트가 에러로 중단됨
- 사용자에게 먼저 커밋하거나 stash할 것을 안내

**base 브랜치가 존재하지 않는 경우:**
- 스크립트가 에러로 중단됨
- 사용자에게 올바른 브랜치 이름을 입력하도록 안내

## Output Examples

### default 브랜치에서 생성

```
Base branch: main
Creating branch: feature/user-auth
✓ Branch 'feature/user-auth' created successfully from 'main'
```

### 특정 브랜치에서 생성

```
Base branch: develop
Creating branch: feature/user-auth
✓ Branch 'feature/user-auth' created successfully from 'develop'
```

### uncommitted changes 에러

```
Error: Uncommitted changes detected.
Please commit or stash your changes before creating a new branch.

Changed files:
 M src/index.ts
?? src/new-file.ts
```

## Notes

- base 브랜치 미지정 시 자동으로 default 브랜치 감지 (detect-default-branch.sh 사용)
- base 브랜치가 원격에만 존재하면 자동으로 tracking 브랜치 생성
- uncommitted changes가 있으면 작업 중단 (안전 모드)
- 대화 맥락이 있으면 적절한 브랜치 이름을 추천
