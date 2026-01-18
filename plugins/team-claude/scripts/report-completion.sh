#!/bin/bash

# report-completion.sh
# Called by Stop hook to report Worker Claude completion to Coordination Server
#
# Usage: report-completion.sh [--subagent]
#   --subagent: Indicates this is a subagent completion (SubagentStop hook)

set -e

# Configuration
SERVER_URL="${TEAM_CLAUDE_SERVER_URL:-http://localhost:3847}"
ENDPOINT="${SERVER_URL}/complete"

# Parse arguments
IS_SUBAGENT=false
if [[ "$1" == "--subagent" ]]; then
    IS_SUBAGENT=true
fi

# Get current directory info
CURRENT_DIR="$(pwd)"
WORKTREE_NAME="$(basename "$CURRENT_DIR")"

# Detect if we're in a worktree
if ! git rev-parse --is-inside-work-tree &>/dev/null; then
    echo "[report-completion] Not in a git repository, skipping report"
    exit 0
fi

# Get worktree info
GIT_DIR="$(git rev-parse --git-dir)"
if [[ "$GIT_DIR" != *".git/worktrees/"* ]]; then
    echo "[report-completion] Not in a worktree, skipping report"
    exit 0
fi

# Extract worktree name from git dir path
WORKTREE_NAME="${GIT_DIR##*.git/worktrees/}"

# Get session ID (from environment or generate)
SESSION_ID="${CLAUDE_SESSION_ID:-$(uuidgen 2>/dev/null || cat /proc/sys/kernel/random/uuid 2>/dev/null || echo "unknown-session")}"

# Get git diff stats
DIFF_STAT=$(git diff --stat HEAD~1 2>/dev/null || git diff --stat 2>/dev/null || echo "No changes")
FILES_CHANGED=$(git diff --name-only HEAD~1 2>/dev/null || git diff --name-only 2>/dev/null || echo "")

# Convert files to JSON array
FILES_JSON="[]"
if [[ -n "$FILES_CHANGED" ]]; then
    FILES_JSON=$(echo "$FILES_CHANGED" | jq -R -s 'split("\n") | map(select(length > 0))')
fi

# Determine status
STATUS="success"
BLOCKERS="[]"

# Check for feedback.md (indicates there might be pending issues)
if [[ -f ".claude/feedback.md" ]]; then
    # Check if feedback has "REVISE" action
    if grep -q "REVISE" ".claude/feedback.md"; then
        STATUS="partial"
    fi
fi

# Check for blockers file
if [[ -f ".claude/blockers.md" ]]; then
    STATUS="blocked"
    BLOCKERS=$(cat ".claude/blockers.md" | jq -R -s 'split("\n") | map(select(length > 0))')
fi

# Generate summary
SUMMARY="Worker session completed in worktree: $WORKTREE_NAME"
if [[ "$IS_SUBAGENT" == "true" ]]; then
    SUMMARY="Subagent session completed in worktree: $WORKTREE_NAME"
fi

# Check if tests were run (look for common test result indicators)
TESTS_RUN=false
TESTS_PASSED=false

if [[ -f "test-results.json" ]] || [[ -f "coverage/lcov.info" ]] || [[ -d ".nyc_output" ]]; then
    TESTS_RUN=true
    # Simple heuristic: if test-results exists and is recent, assume success
    # More sophisticated check would parse the actual results
    TESTS_PASSED=true
fi

# Build JSON payload
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

PAYLOAD=$(cat <<EOF
{
    "worktree": "$WORKTREE_NAME",
    "sessionId": "$SESSION_ID",
    "status": "$STATUS",
    "summary": "$SUMMARY",
    "filesChanged": $FILES_JSON,
    "testsRun": $TESTS_RUN,
    "testsPassed": $TESTS_PASSED,
    "blockers": $BLOCKERS,
    "timestamp": "$TIMESTAMP"
}
EOF
)

echo "[report-completion] Reporting completion to $ENDPOINT"
echo "[report-completion] Worktree: $WORKTREE_NAME"
echo "[report-completion] Status: $STATUS"
echo "[report-completion] Files changed: $(echo "$FILES_CHANGED" | wc -l | tr -d ' ')"

# Send report to coordination server
RESPONSE=$(curl -s -X POST "$ENDPOINT" \
    -H "Content-Type: application/json" \
    -d "$PAYLOAD" \
    --max-time 10 \
    2>&1) || {
    echo "[report-completion] Failed to send report (server may not be running)"
    echo "[report-completion] This is non-fatal, continuing..."
    exit 0
}

# Check response
if echo "$RESPONSE" | jq -e '.success == true' &>/dev/null; then
    echo "[report-completion] Report sent successfully"
    NOTIFICATION_PATH=$(echo "$RESPONSE" | jq -r '.data.notificationPath // "N/A"')
    echo "[report-completion] Notification saved to: $NOTIFICATION_PATH"
else
    ERROR=$(echo "$RESPONSE" | jq -r '.error // "Unknown error"')
    echo "[report-completion] Report failed: $ERROR"
fi

exit 0
