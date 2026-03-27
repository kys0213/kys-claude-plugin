#!/bin/bash
# PreToolUse hook: Check branch freshness before autopilot Agent tool use
# Only applies to Agent tool with autopilot-related prompts
# exit 0 = allow (no feedback)
# exit 2 = block (stderr message shown to Claude)

INPUT=$(cat)
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // empty')

# Only applies to Agent tool
if [[ "$TOOL_NAME" != "Agent" ]]; then
  exit 0
fi

PROMPT=$(echo "$INPUT" | jq -r '.tool_input.prompt // empty')

# Check if prompt contains autopilot keywords
AUTOPILOT_KEYWORDS="gap-detect|gap-watch|qa-boost|build-issues|ci-watch|autopilot"
if ! printf '%s' "$PROMPT" | grep -qiE "$AUTOPILOT_KEYWORDS"; then
  exit 0
fi

# Fetch latest from origin quietly
git fetch origin --quiet 2>/dev/null

# Count commits behind origin/main
BEHIND=$(git rev-list --count HEAD..origin/main 2>/dev/null || echo "0")

if [[ "$BEHIND" -gt 10 ]]; then
  echo "BLOCK: Current branch is $BEHIND commits behind origin/main." >&2
  echo "Run 'git pull origin main' to update before running autopilot tasks." >&2
  exit 2
fi

exit 0
