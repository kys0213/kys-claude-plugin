#!/bin/bash
# on-worker-question.sh
# Worker가 AskUserQuestion을 호출했을 때 에스컬레이션합니다.

set -e

STATE_FILE=".team-claude/state/current-delegation.json"

# 현재 세션 정보
SESSION_ID=$(jq -r '.sessionId' "$STATE_FILE" 2>/dev/null || echo "unknown")
CHECKPOINT_ID=$(jq -r '.currentCheckpoint' "$STATE_FILE" 2>/dev/null || echo "unknown")

echo "Worker asking question: $CHECKPOINT_ID"

# 상태 업데이트
jq '.status = "waiting_for_human"' "$STATE_FILE" > "${STATE_FILE}.tmp" 2>/dev/null && mv "${STATE_FILE}.tmp" "$STATE_FILE"

# stdin에서 질문 내용 읽기
QUESTION=$(cat)

# 질문 저장
QUESTION_FILE=".team-claude/sessions/${SESSION_ID}/delegations/${CHECKPOINT_ID}/pending-question.json"
mkdir -p "$(dirname "$QUESTION_FILE")"
cat > "$QUESTION_FILE" << EOF
{
  "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "checkpoint": "$CHECKPOINT_ID",
  "question": $(echo "$QUESTION" | jq -Rs .)
}
EOF

# 긴급 알림
if command -v osascript &> /dev/null; then
    osascript -e "display notification \"Worker가 질문을 하고 있습니다\" with title \"Team Claude: 인간 개입 필요\" sound name \"Ping\""
elif command -v notify-send &> /dev/null; then
    notify-send -u critical "Team Claude: 인간 개입 필요" "Worker가 질문 중: $CHECKPOINT_ID"
fi

# 서버 알림
if curl -s http://localhost:3847/health &>/dev/null; then
    curl -X POST http://localhost:3847/worker-question \
        -H "Content-Type: application/json" \
        -d "{\"sessionId\": \"$SESSION_ID\", \"checkpoint\": \"$CHECKPOINT_ID\", \"question\": $(echo "$QUESTION" | jq -Rs .)}"
fi

echo "Question escalated to human"
