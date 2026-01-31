#!/bin/bash
# on-worker-idle.sh
# Worker가 60초 이상 대기 상태일 때 알림을 보냅니다.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEAM_CLAUDE_ROOT="${SCRIPT_DIR}/../.."
STATE_FILE=".team-claude/state/current-delegation.json"

# common.sh에서 서버 URL 함수 로드
# shellcheck source=../../scripts/lib/common.sh
source "${TEAM_CLAUDE_ROOT}/scripts/lib/common.sh" 2>/dev/null || {
  # common.sh 로드 실패 시 기본값 사용
  get_server_url() { echo "http://localhost:7890"; }
}
SERVER_URL=$(get_server_url)

# 현재 세션 정보
SESSION_ID=$(jq -r '.sessionId' "$STATE_FILE" 2>/dev/null || echo "unknown")
CHECKPOINT_ID=$(jq -r '.currentCheckpoint' "$STATE_FILE" 2>/dev/null || echo "unknown")

echo "Worker idle detected: $CHECKPOINT_ID"

# 마지막 idle 시간 업데이트
jq '.lastIdleAt = "'"$(date -u +"%Y-%m-%dT%H:%M:%SZ")"'"' "$STATE_FILE" > "${STATE_FILE}.tmp" 2>/dev/null && mv "${STATE_FILE}.tmp" "$STATE_FILE"

# 알림 (너무 자주 보내지 않도록 체크 필요)
LAST_NOTIFIED=$(jq -r '.lastIdleNotified // ""' "$STATE_FILE" 2>/dev/null || echo "")
CURRENT_TIME=$(date +%s)

# 5분에 한 번만 알림
if [ -z "$LAST_NOTIFIED" ] || [ $((CURRENT_TIME - LAST_NOTIFIED)) -gt 300 ]; then
    if command -v osascript &> /dev/null; then
        osascript -e "display notification \"Worker가 대기 중입니다\" with title \"Team Claude: Worker Idle\""
    elif command -v notify-send &> /dev/null; then
        notify-send "Team Claude: Worker Idle" "Worker가 대기 중: $CHECKPOINT_ID"
    fi

    # 알림 시간 기록
    jq ".lastIdleNotified = $CURRENT_TIME" "$STATE_FILE" > "${STATE_FILE}.tmp" 2>/dev/null && mv "${STATE_FILE}.tmp" "$STATE_FILE"
fi

# 서버 상태 업데이트
if curl -s "${SERVER_URL}/health" &>/dev/null; then
    curl -X POST "${SERVER_URL}/worker-idle" \
        -H "Content-Type: application/json" \
        -d "{\"sessionId\": \"$SESSION_ID\", \"checkpoint\": \"$CHECKPOINT_ID\"}"
fi

echo "Idle notification sent"
