#!/bin/bash
# Worker가 60초 이상 입력 대기 시 실행
# Notification hook (idle_prompt)에서 호출됨

set -e

# stdin에서 이벤트 데이터 읽기
INPUT=$(cat)
WORKTREE=$(basename "$CLAUDE_PROJECT_DIR")

# 타임스탬프
TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# 서버에 idle 보고 (서버가 실행 중인 경우)
if curl -s -o /dev/null -w "%{http_code}" "http://localhost:3847/health" 2>/dev/null | grep -q "200"; then
  curl -s -X POST "http://localhost:3847/status" \
    -H "Content-Type: application/json" \
    -d "{
      \"worktree\": \"$WORKTREE\",
      \"status\": \"idle\",
      \"timestamp\": \"$TIMESTAMP\"
    }" > /dev/null 2>&1 || true
fi

# 상태 파일 업데이트
STATE_FILE="$CLAUDE_PROJECT_DIR/../.team-claude/state/workers.json"
if [ -f "$STATE_FILE" ]; then
  jq --arg worktree "$WORKTREE" \
     --arg timestamp "$TIMESTAMP" \
     '.[$worktree].lastIdle = $timestamp' \
     "$STATE_FILE" > "${STATE_FILE}.tmp" && mv "${STATE_FILE}.tmp" "$STATE_FILE"
fi

# macOS 알림
if [[ "$OSTYPE" == "darwin"* ]]; then
  osascript -e "display notification \"$WORKTREE 응답 대기 중\" with title \"Team Claude\"" 2>/dev/null || true
fi

echo "Worker idle: $WORKTREE"
