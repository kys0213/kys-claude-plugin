---
description: hook 설정을 변경합니다. Auto-Commit(Stop), Default Branch Guard(PreToolUse)를 프로젝트(.claude/) 또는 사용자(~/.claude/) 범위로 관리합니다.
allowed-tools:
  - Read
  - Bash
  - Write
  - AskUserQuestion
---

# Hook Config

이 커맨드는 Auto-Commit hook과 Default Branch Guard hook의 설정을 수정합니다.

## Step 1: 현재 설정 확인

프로젝트와 사용자 범위 모두에서 hook을 탐색합니다:

```bash
# 프로젝트 범위 확인
cat .claude/hooks/auto-commit-hook.sh 2>/dev/null
cat .claude/hooks/default-branch-guard-hook.sh 2>/dev/null

# 사용자 범위 확인
cat ~/.claude/hooks/auto-commit-hook.sh 2>/dev/null
cat ~/.claude/hooks/default-branch-guard-hook.sh 2>/dev/null
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

선택된 범위에서 설정 상태를 확인합니다:

```bash
# 범위 판별
HOOKS_DIR="{selected_hooks_dir}"

# 각 hook 존재 여부
AUTO_COMMIT_EXISTS=$([ -f "$HOOKS_DIR/auto-commit-hook.sh" ] && echo "활성화" || echo "없음")
BRANCH_GUARD_EXISTS=$([ -f "$HOOKS_DIR/default-branch-guard-hook.sh" ] && echo "활성화" || echo "없음")

if [[ "$HOOKS_DIR" == "$HOME"* ]]; then
  SCOPE="사용자 (모든 프로젝트)"
else
  PROJECT_DIR=$(grep '^PROJECT_DIR=' "$HOOKS_DIR/auto-commit-hook.sh" 2>/dev/null | cut -d'"' -f2 || grep '^PROJECT_DIR=' "$HOOKS_DIR/default-branch-guard-hook.sh" 2>/dev/null | cut -d'"' -f2)
  SCOPE="프로젝트 ($PROJECT_DIR)"
fi
```

## Step 3: 관리할 hook 선택

AskUserQuestion으로 관리할 hook을 선택받습니다:

```
현재 설정:
├─ 범위: {SCOPE}
├─ Auto-Commit Hook (Stop): {AUTO_COMMIT_EXISTS}
└─ Default Branch Guard (PreToolUse): {BRANCH_GUARD_EXISTS}

어떤 hook을 관리하시겠습니까?
```

옵션:
1. **Auto-Commit Hook** - 세션 종료 시 미커밋 감지
2. **Default Branch Guard** - Write/Edit 시 기본 브랜치 차단
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

#### Auto-Commit Hook 비활성화

**프로젝트 범위:**

```bash
node "${CLAUDE_PLUGIN_ROOT}/scripts/register-hook.js" unregister \
  Stop \
  "bash ./.claude/hooks/auto-commit-hook.sh"

rm -f .claude/hooks/auto-commit-hook.sh
```

**사용자 범위:**

```bash
node "${CLAUDE_PLUGIN_ROOT}/scripts/register-hook.js" unregister \
  Stop \
  "bash $HOME/.claude/hooks/auto-commit-hook.sh" \
  --project-dir="$HOME"

rm -f ~/.claude/hooks/auto-commit-hook.sh
```

#### Default Branch Guard 비활성화

**프로젝트 범위:**

```bash
node "${CLAUDE_PLUGIN_ROOT}/scripts/register-hook.js" unregister \
  PreToolUse \
  "bash ./.claude/hooks/default-branch-guard-hook.sh"

rm -f .claude/hooks/default-branch-guard-hook.sh
```

**사용자 범위:**

```bash
node "${CLAUDE_PLUGIN_ROOT}/scripts/register-hook.js" unregister \
  PreToolUse \
  "bash $HOME/.claude/hooks/default-branch-guard-hook.sh" \
  --project-dir="$HOME"

rm -f ~/.claude/hooks/default-branch-guard-hook.sh
```

완료 메시지:

```
hook이 비활성화되었습니다.

제거된 항목:
├─ {hook_path} (삭제됨)
└─ {settings_path} (hook 제거됨)

다시 활성화하려면 /setup을 실행하세요.
```

### 재설정 선택 시

hook 스크립트를 최신 template으로 재생성합니다.

#### Auto-Commit Hook 재설정

**프로젝트 범위:**

```bash
PROJECT_DIR=$(pwd)
PLUGIN_ROOT="${CLAUDE_PLUGIN_ROOT}"
DEFAULT_BRANCH=$("${PLUGIN_ROOT}/scripts/detect-default-branch.sh")

sed \
  -e "s|{project_dir}|$PROJECT_DIR|g" \
  -e "s|{commit_script_path}|$PLUGIN_ROOT/scripts/commit.sh|g" \
  -e "s|{create_branch_script_path}|$PLUGIN_ROOT/scripts/create-branch.sh|g" \
  -e "s|{default_branch}|$DEFAULT_BRANCH|g" \
  "${PLUGIN_ROOT}/scripts/auto-commit-hook.sh" > .claude/hooks/auto-commit-hook.sh

chmod +x .claude/hooks/auto-commit-hook.sh

node "${CLAUDE_PLUGIN_ROOT}/scripts/register-hook.js" register \
  Stop "*" \
  "bash ./.claude/hooks/auto-commit-hook.sh" \
  --timeout=10
```

**사용자 범위:**

```bash
PLUGIN_ROOT="${CLAUDE_PLUGIN_ROOT}"

sed \
  -e 's|{project_dir}|${CLAUDE_PROJECT_DIR:-.}|g' \
  -e "s|{commit_script_path}|$PLUGIN_ROOT/scripts/commit.sh|g" \
  -e "s|{create_branch_script_path}|$PLUGIN_ROOT/scripts/create-branch.sh|g" \
  -e "s|{default_branch}||g" \
  "${PLUGIN_ROOT}/scripts/auto-commit-hook.sh" > ~/.claude/hooks/auto-commit-hook.sh

chmod +x ~/.claude/hooks/auto-commit-hook.sh

node "${CLAUDE_PLUGIN_ROOT}/scripts/register-hook.js" register \
  Stop "*" \
  "bash $HOME/.claude/hooks/auto-commit-hook.sh" \
  --timeout=10 \
  --project-dir="$HOME"
```

#### Default Branch Guard 재설정

**프로젝트 범위:**

```bash
sed \
  -e "s|{project_dir}|$PROJECT_DIR|g" \
  -e "s|{create_branch_script_path}|$PLUGIN_ROOT/scripts/create-branch.sh|g" \
  -e "s|{default_branch}|$DEFAULT_BRANCH|g" \
  "${PLUGIN_ROOT}/scripts/default-branch-guard-hook.sh" > .claude/hooks/default-branch-guard-hook.sh

chmod +x .claude/hooks/default-branch-guard-hook.sh

node "${CLAUDE_PLUGIN_ROOT}/scripts/register-hook.js" register \
  PreToolUse "Write|Edit" \
  "bash ./.claude/hooks/default-branch-guard-hook.sh" \
  --timeout=5
```

**사용자 범위:**

```bash
sed \
  -e 's|{project_dir}|${CLAUDE_PROJECT_DIR:-.}|g' \
  -e "s|{create_branch_script_path}|$PLUGIN_ROOT/scripts/create-branch.sh|g" \
  -e "s|{default_branch}||g" \
  "${PLUGIN_ROOT}/scripts/default-branch-guard-hook.sh" > ~/.claude/hooks/default-branch-guard-hook.sh

chmod +x ~/.claude/hooks/default-branch-guard-hook.sh

node "${CLAUDE_PLUGIN_ROOT}/scripts/register-hook.js" register \
  PreToolUse "Write|Edit" \
  "bash $HOME/.claude/hooks/default-branch-guard-hook.sh" \
  --timeout=5 \
  --project-dir="$HOME"
```

완료 메시지:

```
hook이 재설정되었습니다!

갱신된 파일:
├─ {hook_path} (재생성됨)
└─ {settings_path} (확인됨)
```

## 예시 실행 흐름

### Default Branch Guard 비활성화

```
현재 설정:
├─ 범위: 프로젝트 (/Users/user/my-project)
├─ Auto-Commit Hook (Stop): 활성화
└─ Default Branch Guard (PreToolUse): 활성화

어떤 hook을 관리하시겠습니까?
[Auto-Commit] [Branch Guard] [모두] [취소]

> Branch Guard 선택

무엇을 변경하시겠습니까?
[비활성화] [재설정] [취소]

> 비활성화 선택

hook이 비활성화되었습니다.

제거된 항목:
├─ .claude/hooks/default-branch-guard-hook.sh (삭제됨)
└─ .claude/settings.json (PreToolUse hook 제거됨)

다시 활성화하려면 /setup을 실행하세요.
```

### 모든 hook 재설정

```
현재 설정:
├─ 범위: 사용자 (모든 프로젝트)
├─ Auto-Commit Hook (Stop): 활성화
└─ Default Branch Guard (PreToolUse): 활성화

어떤 hook을 관리하시겠습니까?
[Auto-Commit] [Branch Guard] [모두] [취소]

> 모두 선택

무엇을 변경하시겠습니까?
[비활성화] [재설정] [취소]

> 재설정 선택

hook이 재설정되었습니다!

갱신된 파일:
├─ ~/.claude/hooks/auto-commit-hook.sh (재생성됨)
├─ ~/.claude/hooks/default-branch-guard-hook.sh (재생성됨)
└─ ~/.claude/settings.json (확인됨)
```
