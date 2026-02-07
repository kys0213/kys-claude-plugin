---
description: 자동 커밋 설정을 변경합니다. 프로젝트(.claude/) 또는 사용자(~/.claude/) 범위의 hook을 관리합니다.
allowed-tools:
  - Read
  - Bash
  - Write
  - AskUserQuestion
---

# Auto-Commit Config

이 커맨드는 자동 커밋 hook의 설정을 수정합니다.

## Step 1: 현재 설정 확인

프로젝트와 사용자 범위 모두에서 hook을 탐색합니다:

```bash
# 프로젝트 범위 확인
cat .claude/hooks/auto-commit-hook.sh 2>/dev/null

# 사용자 범위 확인
cat ~/.claude/hooks/auto-commit-hook.sh 2>/dev/null
```

### 설정이 없는 경우

```
auto-commit hook이 설정되지 않았습니다.

먼저 /setup을 실행하여 초기 설정을 완료하세요.
```

### 양쪽 모두 있는 경우

AskUserQuestion으로 어느 범위를 관리할지 선택받습니다:

```
auto-commit hook이 두 범위에서 발견되었습니다.
어느 설정을 변경하시겠습니까?
```

옵션:
1. **프로젝트** - .claude/hooks/auto-commit-hook.sh
2. **사용자** - ~/.claude/hooks/auto-commit-hook.sh

## Step 2: 현재 설정 파싱

선택된 hook 스크립트에서 설정값을 추출합니다:

```bash
# 설정값 추출 (선택된 경로에서)
HOOK_PATH="{selected_hook_path}"
PROJECT_DIR=$(grep '^PROJECT_DIR=' "$HOOK_PATH" | cut -d'"' -f2)
COMMIT_SCRIPT=$(grep '^COMMIT_SCRIPT=' "$HOOK_PATH" | cut -d'"' -f2)

# 범위 판별
if [[ "$HOOK_PATH" == "$HOME"* ]]; then
  SCOPE="사용자 (모든 프로젝트)"
else
  SCOPE="프로젝트 ($PROJECT_DIR)"
fi
```

## Step 3: 변경할 항목 선택

AskUserQuestion으로 변경할 항목을 선택받습니다:

```
현재 설정:
├─ 상태: 활성화
├─ 범위: {SCOPE}
└─ 기본 브랜치 감지: 자동

무엇을 변경하시겠습니까?
```

옵션:
1. **비활성화** - hook을 제거하여 자동 커밋 비활성화
2. **재설정** - hook 스크립트 재생성 (경로 업데이트)
3. **취소** - 변경 없음

### 비활성화 선택 시

hook 스크립트와 settings.json 등록을 모두 제거합니다.

**프로젝트 범위:**

```bash
# settings.json에서 hook 제거
node "${CLAUDE_PLUGIN_ROOT}/scripts/register-hook.js" unregister \
  Stop \
  "bash ./.claude/hooks/auto-commit-hook.sh"

# hook 스크립트 삭제
rm -f .claude/hooks/auto-commit-hook.sh
```

**사용자 범위:**

```bash
# settings.json에서 hook 제거 (사용자 범위)
node "${CLAUDE_PLUGIN_ROOT}/scripts/register-hook.js" unregister \
  Stop \
  "bash $HOME/.claude/hooks/auto-commit-hook.sh" \
  --project-dir="$HOME"

# hook 스크립트 삭제
rm -f ~/.claude/hooks/auto-commit-hook.sh
```

완료 메시지:

```
자동 커밋 hook이 비활성화되었습니다.

제거된 항목:
├─ {hook_path} (삭제됨)
└─ {settings_path} (Stop hook 제거됨)

다시 활성화하려면 /setup을 실행하세요.
```

### 재설정 선택 시

hook 스크립트를 최신 template으로 재생성합니다.

**프로젝트 범위:**

```bash
PROJECT_DIR=$(pwd)
PLUGIN_ROOT="${CLAUDE_PLUGIN_ROOT}"

sed \
  -e "s|{project_dir}|$PROJECT_DIR|g" \
  -e "s|{commit_script_path}|$PLUGIN_ROOT/scripts/commit.sh|g" \
  -e "s|{detect_default_branch_path}|$PLUGIN_ROOT/scripts/detect-default-branch.sh|g" \
  "${PLUGIN_ROOT}/scripts/auto-commit-hook.sh" > .claude/hooks/auto-commit-hook.sh

chmod +x .claude/hooks/auto-commit-hook.sh

# settings.json에 재등록 (idempotent)
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
  -e "s|{detect_default_branch_path}|$PLUGIN_ROOT/scripts/detect-default-branch.sh|g" \
  "${PLUGIN_ROOT}/scripts/auto-commit-hook.sh" > ~/.claude/hooks/auto-commit-hook.sh

chmod +x ~/.claude/hooks/auto-commit-hook.sh

node "${CLAUDE_PLUGIN_ROOT}/scripts/register-hook.js" register \
  Stop "*" \
  "bash $HOME/.claude/hooks/auto-commit-hook.sh" \
  --timeout=10 \
  --project-dir="$HOME"
```

완료 메시지:

```
자동 커밋 hook이 재설정되었습니다!

갱신된 파일:
├─ {hook_path} (재생성됨)
└─ {settings_path} (확인됨)
```

## 예시 실행 흐름

### 프로젝트 범위 비활성화

```
현재 설정:
├─ 상태: 활성화
├─ 범위: 프로젝트 (/Users/user/my-project)
└─ 기본 브랜치 감지: 자동

무엇을 변경하시겠습니까?
[비활성화] [재설정] [취소]

> 비활성화 선택

자동 커밋 hook이 비활성화되었습니다.

제거된 항목:
├─ .claude/hooks/auto-commit-hook.sh (삭제됨)
└─ .claude/settings.json (Stop hook 제거됨)

다시 활성화하려면 /setup을 실행하세요.
```

### 사용자 범위 재설정

```
현재 설정:
├─ 상태: 활성화
├─ 범위: 사용자 (모든 프로젝트)
└─ 기본 브랜치 감지: 자동

무엇을 변경하시겠습니까?
[비활성화] [재설정] [취소]

> 재설정 선택

자동 커밋 hook이 재설정되었습니다!

갱신된 파일:
├─ ~/.claude/hooks/auto-commit-hook.sh (재생성됨)
└─ ~/.claude/settings.json (확인됨)
```
