#!/bin/bash
# Worker가 권한 승인을 요청할 때 실행
# Notification hook (permission_prompt)에서 호출됨

set -e

# stdin에서 이벤트 데이터 읽기
INPUT=$(cat)
WORKTREE=$(basename "$CLAUDE_PROJECT_DIR")

# 타임스탬프
TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# 서버에 permission 요청 보고
if curl -s -o /dev/null -w "%{http_code}" "http://localhost:3847/health" 2>/dev/null | grep -q "200"; then
  curl -s -X POST "http://localhost:3847/status" \
    -H "Content-Type: application/json" \
    -d "{
      \"worktree\": \"$WORKTREE\",
      \"status\": \"permission\",
      \"timestamp\": \"$TIMESTAMP\"
    }" > /dev/null 2>&1 || true
fi

# 상태 파일 업데이트
STATE_FILE="$CLAUDE_PROJECT_DIR/../.team-claude/state/workers.json"
if [ -f "$STATE_FILE" ]; then
  jq --arg worktree "$WORKTREE" \
     --arg timestamp "$TIMESTAMP" \
     '.[$worktree].status = "permission" | .[$worktree].permissionRequestedAt = $timestamp' \
     "$STATE_FILE" > "${STATE_FILE}.tmp" && mv "${STATE_FILE}.tmp" "$STATE_FILE"
fi

# macOS 알림 (긴급)
if [[ "$OSTYPE" == "darwin"* ]]; then
  osascript -e "display notification \"$WORKTREE: 권한 승인 필요\" with title \"Team Claude ⚠️\" sound name \"Ping\"" 2>/dev/null || true
fi

# Linux 알림
if command -v notify-send &> /dev/null; then
  notify-send -u critical "Team Claude ⚠️" "$WORKTREE: 권한 승인 필요" 2>/dev/null || true
fi

echo "Worker permission: $WORKTREE"
