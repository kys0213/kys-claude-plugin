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

# --- frontmatter에서 설정 파싱 ---
parse_frontmatter_value() {
  local key="$1"
  sed -n '/^---$/,/^---$/p' "$CONFIG_FILE" \
    | grep "^${key}:" \
    | head -1 \
    | sed "s/^${key}:[[:space:]]*//" \
    | tr -d '"' \
    | tr -d "'" \
    | xargs  # trim whitespace
}

WORK_BRANCH=$(parse_frontmatter_value "work_branch")
BRANCH_STRATEGY=$(parse_frontmatter_value "branch_strategy")

# --- 기대하는 base branch 결정 ---
if [[ -n "$WORK_BRANCH" ]]; then
  EXPECTED_BASE="$WORK_BRANCH"
elif [[ "$BRANCH_STRATEGY" == "draft-develop-main" ]]; then
  EXPECTED_BASE="develop"
else
  # draft-main (기본값)
  EXPECTED_BASE="main"
fi

# --- tool input에서 실제 base branch 추출 ---
TOOL_INPUT=$(cat)
TOOL_NAME="${CLAUDE_TOOL_USE_NAME:-}"

extract_actual_base() {
  case "$TOOL_NAME" in
    mcp__github__create_pull_request)
      # MCP tool: JSON input에서 base 필드 추출
      echo "$TOOL_INPUT" | grep -o '"base"[[:space:]]*:[[:space:]]*"[^"]*"' \
        | head -1 \
        | sed 's/.*"base"[[:space:]]*:[[:space:]]*"//' \
        | tr -d '"'
      ;;
    Bash)
      # Bash tool: command에서 gh pr create --base 값 추출
      local cmd
      cmd=$(echo "$TOOL_INPUT" | grep -o '"command"[[:space:]]*:[[:space:]]*"[^"]*"' \
        | head -1 \
        | sed 's/.*"command"[[:space:]]*:[[:space:]]*"//' \
        | sed 's/"$//')

      # gh pr create가 아니면 관심 없음
      if ! echo "$cmd" | grep -q 'gh pr create'; then
        echo ""
        return
      fi

      # --base 값 추출
      echo "$cmd" | grep -oP '(?<=--base[[:space:]])\S+' | head -1 | tr -d '"' \
        || echo "$cmd" | grep -oP '(?<=--base=)\S+' | head -1 | tr -d '"'
      ;;
    *)
      echo ""
      ;;
  esac
}

ACTUAL_BASE=$(extract_actual_base)

# --- base를 지정하지 않은 경우 (관심 없는 tool call) → skip ---
if [[ -z "$ACTUAL_BASE" ]]; then
  exit 0
fi

# --- 검증 ---
if [[ "$ACTUAL_BASE" != "$EXPECTED_BASE" ]]; then
  echo "BLOCKED: PR base branch mismatch" >&2
  echo "  expected: $EXPECTED_BASE (from github-autopilot.local.md)" >&2
  echo "  actual:   $ACTUAL_BASE" >&2
  echo "" >&2
  echo "github-autopilot.local.md의 work_branch 또는 branch_strategy 설정을 확인하세요." >&2
  exit 2
fi

exit 0
