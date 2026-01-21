#!/bin/bash
# on-worker-complete.sh
# Worker 완료 시 자동 검증을 트리거합니다.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AFL_ROOT="${SCRIPT_DIR}/../.."
STATE_FILE=".afl/state/current-delegation.json"

# 현재 세션 및 checkpoint 정보 로드
if [ ! -f "$STATE_FILE" ]; then
    echo "No active delegation found"
    exit 0
fi

SESSION_ID=$(jq -r '.sessionId' "$STATE_FILE")
CHECKPOINT_ID=$(jq -r '.currentCheckpoint' "$STATE_FILE")
ITERATION=$(jq -r '.iteration' "$STATE_FILE")

echo "Worker completed: $CHECKPOINT_ID (iteration $ITERATION)"

# 상태 업데이트
jq '.status = "validating"' "$STATE_FILE" > "${STATE_FILE}.tmp" && mv "${STATE_FILE}.tmp" "$STATE_FILE"

# Checkpoint 정의에서 검증 명령어 가져오기
CHECKPOINTS_FILE=".afl/sessions/${SESSION_ID}/specs/checkpoints.yaml"
if [ ! -f "$CHECKPOINTS_FILE" ]; then
    echo "Checkpoints file not found: $CHECKPOINTS_FILE"
    exit 1
fi

# 검증 트리거 (Main Claude에게 알림)
echo "Triggering validation for checkpoint: $CHECKPOINT_ID"

# 서버가 실행 중이면 HTTP 알림
if curl -s -o /dev/null -w "%{http_code}" http://localhost:3847/health 2>/dev/null | grep -q "200"; then
    curl -X POST http://localhost:3847/validate \
        -H "Content-Type: application/json" \
        -d "{\"sessionId\": \"$SESSION_ID\", \"checkpoint\": \"$CHECKPOINT_ID\", \"iteration\": $ITERATION}"
fi

# OS 알림 (macOS)
if command -v osascript &> /dev/null; then
    osascript -e "display notification \"Checkpoint $CHECKPOINT_ID 검증 시작\" with title \"AFL: Worker 완료\""
fi

# OS 알림 (Linux)
if command -v notify-send &> /dev/null; then
    notify-send "AFL: Worker 완료" "Checkpoint $CHECKPOINT_ID 검증 시작"
fi

echo "Validation triggered successfully"
