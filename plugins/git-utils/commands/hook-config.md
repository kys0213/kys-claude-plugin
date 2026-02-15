---
description: hook 설정을 변경합니다. Default Branch Guard(PreToolUse - Write/Edit/Bash)를 프로젝트(.claude/) 또는 사용자(~/.claude/) 범위로 관리합니다.
allowed-tools:
  - Read
  - Bash
  - Write
  - AskUserQuestion
---

# Hook Config

이 커맨드는 Default Branch Guard hook의 설정을 수정합니다.

## Step 1: 현재 설정 확인

`git-utils hook list`로 프로젝트와 사용자 범위의 hook을 탐색합니다:

```bash
# 프로젝트 범위 확인
git-utils hook list PreToolUse

# 사용자 범위 확인
git-utils hook list PreToolUse --project-dir="$HOME"
```

### 설정이 없는 경우

```
hook이 설정되지 않았습니다.

먼저 /setup을 실행하여 초기 설정을 완료하세요.
```

### 양쪽 모두 있는 경우

AskUserQuestion으로 어느 범위를 관리할지 선택받습니다:

```
hook이 두 범위에서 발견되었습니다.
어느 설정을 변경하시겠습니까?
```

옵션:
1. **프로젝트** - .claude/hooks/
2. **사용자** - ~/.claude/hooks/

## Step 2: 현재 설정 파싱

`git-utils hook list` 결과(JSON)에서 설정 상태를 파악합니다.

- `git-utils guard write` 포함 hook → Write/Edit Guard 활성화
- `git-utils guard commit` 포함 hook → Commit Guard 활성화
- 프로젝트 범위 vs 사용자 범위는 `--project-dir` 유무로 판별

## Step 3: 관리할 hook 선택

AskUserQuestion으로 관리할 hook을 선택받습니다:

```
현재 설정:
├─ 범위: {SCOPE}
├─ Write/Edit Guard (PreToolUse): {WRITE_GUARD_EXISTS}
└─ Commit Guard (PreToolUse - Bash): {COMMIT_GUARD_EXISTS}

어떤 hook을 관리하시겠습니까?
```

옵션:
1. **Write/Edit Guard** - Write/Edit 시 기본 브랜치 차단
2. **Commit Guard** - git commit 시 기본 브랜치 차단
3. **모두** - 두 hook 모두 관리
4. **취소** - 변경 없음

## Step 4: 변경할 항목 선택

AskUserQuestion으로 변경할 항목을 선택받습니다:

```
무엇을 변경하시겠습니까?
```

옵션:
1. **비활성화** - hook을 제거
2. **재설정** - hook 스크립트 재생성 (경로 업데이트)
3. **취소** - 변경 없음

### 비활성화 선택 시

hook 스크립트와 settings.json 등록을 모두 제거합니다.

#### Write/Edit Guard 비활성화

**프로젝트 범위:**

```bash
git-utils hook unregister PreToolUse \
  "git-utils guard write --project-dir=$PROJECT_DIR --create-branch-script='git-utils branch' --default-branch=$DEFAULT_BRANCH"
```

**사용자 범위:**

```bash
git-utils hook unregister PreToolUse \
  "git-utils guard write --project-dir=\${CLAUDE_PROJECT_DIR:-.} --create-branch-script='git-utils branch'" \
  --project-dir="$HOME"
```

#### Commit Guard 비활성화

**프로젝트 범위:**

```bash
git-utils hook unregister PreToolUse \
  "git-utils guard commit --project-dir=$PROJECT_DIR --create-branch-script='git-utils branch' --default-branch=$DEFAULT_BRANCH"
```

**사용자 범위:**

```bash
git-utils hook unregister PreToolUse \
  "git-utils guard commit --project-dir=\${CLAUDE_PROJECT_DIR:-.} --create-branch-script='git-utils branch'" \
  --project-dir="$HOME"
```

완료 메시지:

```
hook이 비활성화되었습니다.

제거된 항목:
└─ {settings_path} (hook 제거됨)

다시 활성화하려면 /setup을 실행하세요.
```

### 재설정 선택 시

hook 스크립트를 최신 template으로 재생성합니다.

#### Write/Edit Guard 재설정

**프로젝트 범위:**

```bash
PROJECT_DIR=$(pwd)
DEFAULT_BRANCH=$(git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null | sed 's|refs/remotes/origin/||' || echo 'main')

git-utils hook register PreToolUse "Write|Edit" \
  "git-utils guard write --project-dir=$PROJECT_DIR --create-branch-script='git-utils branch' --default-branch=$DEFAULT_BRANCH" \
  --timeout=5
```

**사용자 범위:**

```bash
git-utils hook register PreToolUse "Write|Edit" \
  "git-utils guard write --project-dir=\${CLAUDE_PROJECT_DIR:-.} --create-branch-script='git-utils branch'" \
  --timeout=5 \
  --project-dir="$HOME"
```

#### Commit Guard 재설정

**프로젝트 범위:**

```bash
git-utils hook register PreToolUse "Bash" \
  "git-utils guard commit --project-dir=$PROJECT_DIR --create-branch-script='git-utils branch' --default-branch=$DEFAULT_BRANCH" \
  --timeout=5
```

**사용자 범위:**

```bash
git-utils hook register PreToolUse "Bash" \
  "git-utils guard commit --project-dir=\${CLAUDE_PROJECT_DIR:-.} --create-branch-script='git-utils branch'" \
  --timeout=5 \
  --project-dir="$HOME"
```

완료 메시지:

```
hook이 재설정되었습니다!

갱신된 항목:
└─ {settings_path} (hook 재등록됨)
```

## 예시 실행 흐름

### Commit Guard 비활성화

```
현재 설정:
├─ 범위: 프로젝트 (/Users/user/my-project)
├─ Write/Edit Guard (PreToolUse): 활성화
└─ Commit Guard (PreToolUse - Bash): 활성화

어떤 hook을 관리하시겠습니까?
[Write/Edit Guard] [Commit Guard] [모두] [취소]

> Commit Guard 선택

무엇을 변경하시겠습니까?
[비활성화] [재설정] [취소]

> 비활성화 선택

hook이 비활성화되었습니다.

제거된 항목:
└─ .claude/settings.json (PreToolUse Bash hook 제거됨)

다시 활성화하려면 /setup을 실행하세요.
```

### 모든 hook 재설정

```
현재 설정:
├─ 범위: 사용자 (모든 프로젝트)
├─ Write/Edit Guard (PreToolUse): 활성화
└─ Commit Guard (PreToolUse - Bash): 활성화

어떤 hook을 관리하시겠습니까?
[Write/Edit Guard] [Commit Guard] [모두] [취소]

> 모두 선택

무엇을 변경하시겠습니까?
[비활성화] [재설정] [취소]

> 재설정 선택

hook이 재설정되었습니다!

갱신된 항목:
└─ ~/.claude/settings.json (hook 재등록됨)
```
