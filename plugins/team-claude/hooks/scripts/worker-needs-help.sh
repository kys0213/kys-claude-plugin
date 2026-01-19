#!/bin/bash
# Worker가 AskUserQuestion 호출 시 실행 (질문 발생)
# PreToolUse hook에서 호출됨

set -e

# stdin에서 이벤트 데이터 읽기
INPUT=$(cat)
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
QUESTION=$(echo "$INPUT" | jq -r '.tool_input.question // "질문이 있습니다"')
WORKTREE=$(basename "$CLAUDE_PROJECT_DIR")

# 타임스탬프
TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# 서버에 질문 보고 (서버가 실행 중인 경우)
if curl -s -o /dev/null -w "%{http_code}" "http://localhost:3847/health" 2>/dev/null | grep -q "200"; then
  curl -s -X POST "http://localhost:3847/question" \
    -H "Content-Type: application/json" \
    -d "{
      \"worktree\": \"$WORKTREE\",
      \"sessionId\": \"$SESSION_ID\",
      \"question\": \"$QUESTION\",
      \"timestamp\": \"$TIMESTAMP\"
    }" > /dev/null 2>&1 || true
fi

# 상태 파일 업데이트
STATE_FILE="$CLAUDE_PROJECT_DIR/../.team-claude/state/workers.json"
if [ -f "$STATE_FILE" ]; then
  jq --arg worktree "$WORKTREE" \
     --arg timestamp "$TIMESTAMP" \
     --arg question "$QUESTION" \
     '.[$worktree].status = "waiting" | .[$worktree].pendingQuestion = {"question": $question, "timestamp": $timestamp}' \
     "$STATE_FILE" > "${STATE_FILE}.tmp" && mv "${STATE_FILE}.tmp" "$STATE_FILE"
fi

# macOS 알림 (긴급)
if [[ "$OSTYPE" == "darwin"* ]]; then
  osascript -e "display notification \"$WORKTREE: 질문이 있습니다\" with title \"Team Claude ⚠️\" sound name \"Ping\"" 2>/dev/null || true
fi

# Linux 알림
if command -v notify-send &> /dev/null; then
  notify-send -u critical "Team Claude ⚠️" "$WORKTREE: 질문이 있습니다" 2>/dev/null || true
fi

echo "Worker needs help: $WORKTREE"
