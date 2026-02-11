#!/bin/bash
# Default Branch Commit Protection Hook (PreToolUse - Bash)
# 기본 브랜치에서 git commit 명령 실행을 차단합니다.

set -euo pipefail
INPUT=$(cat)

# Bash 명령어에서 git commit 패턴 확인
COMMAND=$(echo "$INPUT" | grep -oP '"command"\s*:\s*"[^"]*"' | head -1 | sed 's/"command"\s*:\s*"//;s/"$//')
if [ -z "$COMMAND" ]; then
  COMMAND=$(echo "$INPUT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('tool_input',{}).get('command',''))" 2>/dev/null || true)
fi

# git commit 명령이 아니면 패스
if ! echo "$COMMAND" | grep -qE '\bgit\b.*\bcommit\b'; then
  exit 0
fi

# Guard 1: git repo 확인
git rev-parse --is-inside-work-tree >/dev/null 2>&1 || exit 0

# 기본 브랜치 감지
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
[ -z "$DEFAULT_BRANCH" ] && exit 0

# 특수 상태 확인 (rebase/merge) → 패스
GIT_DIR=$(git rev-parse --git-dir 2>/dev/null) || exit 0
[ -d "$GIT_DIR/rebase-merge" ] || [ -d "$GIT_DIR/rebase-apply" ] && exit 0
[ -f "$GIT_DIR/MERGE_HEAD" ] && exit 0

# detached HEAD → 패스
CURRENT_BRANCH=$(git branch --show-current 2>/dev/null || true)
[ -z "$CURRENT_BRANCH" ] && exit 0

# 기본 브랜치가 아니면 패스
[ "$CURRENT_BRANCH" != "$DEFAULT_BRANCH" ] && exit 0

# 기본 브랜치에서 git commit 시도 → 차단
echo "[Branch Guard] 기본 브랜치($DEFAULT_BRANCH)에서 커밋할 수 없습니다." >&2
echo "먼저 새 브랜치를 생성해주세요:" >&2
echo "" >&2
echo "  git checkout -b <branch-name>" >&2
echo "" >&2
echo "예시:" >&2
echo "  git checkout -b feat/my-feature" >&2
echo "  git checkout -b fix/bug-fix" >&2
exit 2
