#!/bin/bash
# Worker 작업 완료 시 실행
# Stop hook에서 호출됨

set -e

# stdin에서 이벤트 데이터 읽기
INPUT=$(cat)
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
WORKTREE=$(basename "$CLAUDE_PROJECT_DIR")

# 타임스탬프
TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# 서버에 완료 보고 (서버가 실행 중인 경우)
if curl -s -o /dev/null -w "%{http_code}" "http://localhost:3847/health" 2>/dev/null | grep -q "200"; then
  curl -s -X POST "http://localhost:3847/complete" \
    -H "Content-Type: application/json" \
    -d "{
      \"worktree\": \"$WORKTREE\",
      \"sessionId\": \"$SESSION_ID\",
      \"status\": \"completed\",
      \"timestamp\": \"$TIMESTAMP\"
    }" > /dev/null 2>&1 || true
fi

# 상태 파일 업데이트
STATE_FILE="$CLAUDE_PROJECT_DIR/../.team-claude/state/workers.json"
if [ -f "$STATE_FILE" ]; then
  # jq로 상태 업데이트
  jq --arg worktree "$WORKTREE" \
     --arg timestamp "$TIMESTAMP" \
     '.[$worktree].status = "completed" | .[$worktree].completedAt = $timestamp' \
     "$STATE_FILE" > "${STATE_FILE}.tmp" && mv "${STATE_FILE}.tmp" "$STATE_FILE"
fi

# macOS 알림 (macOS인 경우)
if [[ "$OSTYPE" == "darwin"* ]]; then
  osascript -e "display notification \"$WORKTREE 작업 완료\" with title \"Team Claude\" sound name \"Glass\"" 2>/dev/null || true
fi

# Linux 알림 (notify-send 있는 경우)
if command -v notify-send &> /dev/null; then
  notify-send "Team Claude" "$WORKTREE 작업 완료" 2>/dev/null || true
fi

echo "Worker complete: $WORKTREE"
