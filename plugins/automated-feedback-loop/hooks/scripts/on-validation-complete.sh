#!/bin/bash
# on-validation-complete.sh
# 검증 완료 후 결과에 따라 피드백 라우팅을 수행합니다.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STATE_FILE=".afl/state/current-delegation.json"

# 현재 세션 정보
SESSION_ID=$(jq -r '.sessionId' "$STATE_FILE" 2>/dev/null || echo "")
CHECKPOINT_ID=$(jq -r '.currentCheckpoint' "$STATE_FILE" 2>/dev/null || echo "")
ITERATION=$(jq -r '.iteration' "$STATE_FILE" 2>/dev/null || echo "1")
MAX_ITERATIONS=$(jq -r '.maxIterations' "$STATE_FILE" 2>/dev/null || echo "5")

# stdin으로 검증 결과 받기 (Claude hooks에서 전달)
VALIDATION_OUTPUT=$(cat)
EXIT_CODE=$?

echo "Validation completed for: $CHECKPOINT_ID"

# 결과 저장 디렉토리
RESULT_DIR=".afl/sessions/${SESSION_ID}/delegations/${CHECKPOINT_ID}/iterations/${ITERATION}"
mkdir -p "$RESULT_DIR"

# 검증 결과 저장
cat > "${RESULT_DIR}/result.json" << EOF
{
  "iteration": $ITERATION,
  "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "exitCode": $EXIT_CODE,
  "output": $(echo "$VALIDATION_OUTPUT" | jq -Rs .)
}
EOF

# 결과 분석 및 라우팅
if [ $EXIT_CODE -eq 0 ]; then
    echo "✅ Validation PASSED"

    # 상태 업데이트: 완료
    jq '.status = "completed" | .result = "pass"' "$STATE_FILE" > "${STATE_FILE}.tmp" && mv "${STATE_FILE}.tmp" "$STATE_FILE"

    # 성공 알림
    if command -v osascript &> /dev/null; then
        osascript -e "display notification \"Checkpoint $CHECKPOINT_ID 통과!\" with title \"AFL: 검증 성공\" sound name \"Glass\""
    elif command -v notify-send &> /dev/null; then
        notify-send "AFL: 검증 성공" "Checkpoint $CHECKPOINT_ID 통과!"
    fi

    # 서버 알림
    if curl -s http://localhost:3847/health &>/dev/null; then
        curl -X POST http://localhost:3847/checkpoint-passed \
            -H "Content-Type: application/json" \
            -d "{\"sessionId\": \"$SESSION_ID\", \"checkpoint\": \"$CHECKPOINT_ID\"}"
    fi

else
    echo "❌ Validation FAILED"

    if [ "$ITERATION" -lt "$MAX_ITERATIONS" ]; then
        echo "Triggering auto-retry (iteration $((ITERATION + 1))/$MAX_ITERATIONS)"

        # 상태 업데이트: 재시도
        jq ".status = \"retrying\" | .iteration = $((ITERATION + 1))" "$STATE_FILE" > "${STATE_FILE}.tmp" && mv "${STATE_FILE}.tmp" "$STATE_FILE"

        # 피드백 생성 요청 (Main Claude에게)
        if curl -s http://localhost:3847/health &>/dev/null; then
            curl -X POST http://localhost:3847/generate-feedback \
                -H "Content-Type: application/json" \
                -d "{\"sessionId\": \"$SESSION_ID\", \"checkpoint\": \"$CHECKPOINT_ID\", \"iteration\": $ITERATION, \"output\": $(echo "$VALIDATION_OUTPUT" | jq -Rs .)}"
        fi

    else
        echo "⚠️ Max iterations reached, escalating"

        # 상태 업데이트: 에스컬레이션
        jq '.status = "escalated" | .result = "max_retry_exceeded"' "$STATE_FILE" > "${STATE_FILE}.tmp" && mv "${STATE_FILE}.tmp" "$STATE_FILE"

        # 에스컬레이션 알림
        if command -v osascript &> /dev/null; then
            osascript -e "display notification \"Checkpoint $CHECKPOINT_ID 에스컬레이션 필요\" with title \"AFL: 개입 필요\" sound name \"Sosumi\""
        elif command -v notify-send &> /dev/null; then
            notify-send -u critical "AFL: 개입 필요" "Checkpoint $CHECKPOINT_ID 에스컬레이션"
        fi
    fi
fi

echo "Routing complete"
