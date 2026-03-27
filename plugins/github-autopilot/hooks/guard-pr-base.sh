#!/usr/bin/env bash
# guard-pr-base.sh — PreToolUse hook
# autopilot 설정에 지정된 base branch 외의 PR 생성을 차단합니다.
#
# 트리거: mcp__github__create_pull_request, Bash (gh pr create)
# 동작:
#   - github-autopilot.local.md 없음 → exit 0 (비 autopilot 프로젝트, skip)
#   - PR base branch가 설정과 일치 → exit 0 (허용)
#   - PR base branch가 설정과 불일치 → exit 2 (차단)

set -euo pipefail

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-.}"
CONFIG_FILE="${PROJECT_DIR}/github-autopilot.local.md"

# --- config 파일 없으면 autopilot 프로젝트가 아님 → skip ---
if [[ ! -f "$CONFIG_FILE" ]]; then
  exit 0
fi

# frontmatter에서 work_branch, branch_strategy를 한 번에 파싱
eval "$(awk '
  /^---$/ { fm++; next }
  fm == 1 && /^work_branch:/ { sub(/^work_branch:[[:space:]]*/, ""); gsub(/["'"'"']/, ""); printf "WORK_BRANCH=%s\n", $0 }
  fm == 1 && /^branch_strategy:/ { sub(/^branch_strategy:[[:space:]]*/, ""); gsub(/["'"'"']/, ""); printf "BRANCH_STRATEGY=%s\n", $0 }
  fm >= 2 { exit }
' "$CONFIG_FILE")"
WORK_BRANCH="${WORK_BRANCH:-}"
BRANCH_STRATEGY="${BRANCH_STRATEGY:-}"

if [[ -n "$WORK_BRANCH" ]]; then
  EXPECTED_BASE="$WORK_BRANCH"
elif [[ "$BRANCH_STRATEGY" == "draft-develop-main" ]]; then
  EXPECTED_BASE="develop"
else
  # draft-main (기본값)
  EXPECTED_BASE="main"
fi

TOOL_INPUT=$(cat)
TOOL_NAME="${CLAUDE_TOOL_USE_NAME:-}"

extract_actual_base() {
  case "$TOOL_NAME" in
    mcp__github__create_pull_request)
      echo "$TOOL_INPUT" | grep -o '"base"[[:space:]]*:[[:space:]]*"[^"]*"' \
        | head -1 \
        | sed 's/.*"base"[[:space:]]*:[[:space:]]*"//' \
        | tr -d '"'
      ;;
    Bash)
      local cmd
      cmd=$(echo "$TOOL_INPUT" | grep -o '"command"[[:space:]]*:[[:space:]]*"[^"]*"' \
        | head -1 \
        | sed 's/.*"command"[[:space:]]*:[[:space:]]*"//' \
        | sed 's/"$//')

      if ! echo "$cmd" | grep -q 'gh pr create'; then
        return
      fi

      # --base <val> 또는 --base=<val> 모두 처리 (grep -P 대신 sed 사용 — macOS 호환)
      echo "$cmd" | sed -n 's/.*--base[= ][[:space:]]*\([^ "]*\).*/\1/p' | head -1
      ;;
    *)
      ;;
  esac
}

ACTUAL_BASE=$(extract_actual_base)

if [[ -z "$ACTUAL_BASE" ]]; then
  exit 0
fi

if [[ "$ACTUAL_BASE" != "$EXPECTED_BASE" ]]; then
  echo "BLOCKED: PR base branch mismatch" >&2
  echo "  expected: $EXPECTED_BASE (from github-autopilot.local.md)" >&2
  echo "  actual:   $ACTUAL_BASE" >&2
  echo >&2
  echo "github-autopilot.local.md의 work_branch 또는 branch_strategy 설정을 확인하세요." >&2
  exit 2
fi

exit 0
