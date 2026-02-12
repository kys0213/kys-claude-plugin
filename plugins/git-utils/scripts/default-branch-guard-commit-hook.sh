#!/bin/bash
# Default Branch Guard - Commit Hook (PreToolUse - Bash)
# 기본 브랜치에서 git commit 명령 실행을 차단합니다.
# Re-run /setup or /hook-config to modify

set -euo pipefail
INPUT=$(cat)

# Bash 명령어에서 git commit 패턴 확인
COMMAND=$(echo "$INPUT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('tool_input',{}).get('command',''))" 2>/dev/null || true)
if [ -z "$COMMAND" ]; then
  exit 0
fi

# git commit 명령이 아니면 패스
if ! echo "$COMMAND" | grep -qE '\bgit\b.*\bcommit\b'; then
  exit 0
fi

# ===== Settings (baked in at setup time) =====
PROJECT_DIR="{project_dir}"
CREATE_BRANCH_SCRIPT="{create_branch_script_path}"
DEFAULT_BRANCH="{default_branch}"
# =============================================

cd "$PROJECT_DIR" 2>/dev/null || exit 0

# Guard 1: git repo 확인
git rev-parse --is-inside-work-tree >/dev/null 2>&1 || exit 0

# DEFAULT_BRANCH가 비어있으면 런타임 감지 (사용자 범위: 프로젝트마다 다를 수 있음)
if [ -z "$DEFAULT_BRANCH" ]; then
  DEFAULT_BRANCH=$(git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null | sed 's@^refs/remotes/origin/@@')
  if [ -z "$DEFAULT_BRANCH" ]; then
    if git show-ref --verify --quiet refs/remotes/origin/main 2>/dev/null; then
      DEFAULT_BRANCH="main"
    elif git show-ref --verify --quiet refs/remotes/origin/develop 2>/dev/null; then
      DEFAULT_BRANCH="develop"
    elif git show-ref --verify --quiet refs/remotes/origin/master 2>/dev/null; then
      DEFAULT_BRANCH="master"
    fi
  fi
  # 런타임 감지 실패 시 현재 브랜치가 기본 브랜치 후보면 차단
  if [ -z "$DEFAULT_BRANCH" ]; then
    CURRENT=$(git branch --show-current 2>/dev/null || true)
    case "$CURRENT" in
      main|master|develop) DEFAULT_BRANCH="$CURRENT" ;;
      *) exit 0 ;;
    esac
  fi
fi

# Guard 2: 특수 상태 확인 (rebase/merge) → 패스
GIT_DIR=$(git rev-parse --git-dir 2>/dev/null) || exit 0
[ -d "$GIT_DIR/rebase-merge" ] || [ -d "$GIT_DIR/rebase-apply" ] && exit 0
[ -f "$GIT_DIR/MERGE_HEAD" ] && exit 0

# Guard 3: detached HEAD → 패스
CURRENT_BRANCH=$(git branch --show-current 2>/dev/null || true)
[ -z "$CURRENT_BRANCH" ] && exit 0

# 기본 브랜치가 아니면 패스
[ "$CURRENT_BRANCH" != "$DEFAULT_BRANCH" ] && exit 0

# 기본 브랜치에서 git commit 시도 → 블로킹
echo "[Branch Guard] 기본 브랜치($DEFAULT_BRANCH)에서 커밋할 수 없습니다." >&2
echo "기본 브랜치에 직접 커밋하는 것은 권장하지 않습니다." >&2
echo "" >&2
echo "먼저 새 브랜치를 생성해주세요:" >&2
echo "  $CREATE_BRANCH_SCRIPT <branch-name>" >&2
echo "" >&2
echo "예시:" >&2
echo "  $CREATE_BRANCH_SCRIPT feat/my-feature" >&2
echo "  $CREATE_BRANCH_SCRIPT fix/bug-fix" >&2
exit 2
