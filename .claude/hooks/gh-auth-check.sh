#!/bin/bash
# PreToolUse hook: Verify GitHub CLI authentication before Agent/Bash tool use
# - Bash tool: only check if command contains "gh " or "git push"
# - Agent tool: only check if prompt contains GitHub-related keywords
# exit 0 = allow (no feedback)
# exit 2 = block (stderr message shown to Claude)

INPUT=$(cat)
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // empty')

if [[ "$TOOL_NAME" == "Bash" ]]; then
  COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // empty')
  if [[ "$COMMAND" != *"gh "* && "$COMMAND" != *"git push"* ]]; then
    exit 0
  fi
elif [[ "$TOOL_NAME" == "Agent" ]]; then
  PROMPT=$(echo "$INPUT" | jq -r '.tool_input.prompt // empty')
  if ! printf '%s' "$PROMPT" | grep -qiE 'gh |github|git push|autopilot|gap-detect|gap-watch|qa-boost|build-issues|ci-watch|merge-pr'; then
    exit 0
  fi
else
  exit 0
fi

# Verify gh CLI authentication
# Skip the check when origin remote points to a local proxy (cloud autopilot
# environment): the proxy handles auth itself, and `gh` CLI is not used.
if git remote get-url origin 2>/dev/null | grep -qE '127\.0\.0\.1|localhost|local_proxy'; then
  exit 0
fi

if ! gh auth status --hostname github.com &>/dev/null; then
  echo "BLOCK: GitHub CLI is not authenticated." >&2
  echo "Run 'gh auth login' to authenticate before proceeding." >&2
  exit 2
fi

exit 0
