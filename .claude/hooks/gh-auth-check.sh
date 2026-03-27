#!/bin/bash
# PreToolUse hook: Verify GitHub CLI authentication before Agent/Bash tool use
# - Agent tool: always check gh auth
# - Bash tool: only check if command contains "gh " or "git push"
# exit 0 = allow (no feedback)
# exit 2 = block (stderr message shown to Claude)

INPUT=$(cat)
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // empty')
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // empty')

# For Bash tool: only check commands that need GitHub auth
if [[ "$TOOL_NAME" == "Bash" ]]; then
  if [[ "$COMMAND" != *"gh "* && "$COMMAND" != *"git push"* ]]; then
    exit 0
  fi
fi

# Verify gh CLI authentication
if ! gh auth status --hostname github.com &>/dev/null; then
  echo "BLOCK: GitHub CLI is not authenticated." >&2
  echo "Run 'gh auth login' to authenticate before proceeding." >&2
  exit 2
fi

exit 0
