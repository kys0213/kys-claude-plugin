#!/usr/bin/env bash
# SessionStart hook: RALPH 워크플로우 상태 감지
# state.yaml이 있으면 에이전트에게 현재 상태를 알려줍니다.

STATE_FILE=".develop-workflow/state.yaml"

if [ ! -f "$STATE_FILE" ]; then
  exit 0
fi

PHASE=$(grep '^phase:' "$STATE_FILE" | sed 's/phase: *//')
FEATURE=$(grep '^feature:' "$STATE_FILE" | sed 's/feature: *//' | tr -d '"')
STRATEGY=$(grep '^strategy:' "$STATE_FILE" | sed 's/strategy: *//')
UPDATED=$(grep '^updated_at:' "$STATE_FILE" | sed 's/updated_at: *//' | tr -d '"')

# DONE이면 알림만
if [ "$PHASE" = "DONE" ]; then
  echo "[develop-workflow] 이전 워크플로우 완료 상태입니다. 새 /develop 시 초기화됩니다."
  exit 0
fi

# 진행 중인 워크플로우 알림
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[develop-workflow] 진행 중인 워크플로우 감지"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "  Feature : $FEATURE"
echo "  Phase   : $PHASE"
[ -n "$STRATEGY" ] && echo "  Strategy: $STRATEGY"
echo "  Updated : $UPDATED"
echo ""

# Checkpoint 상태 파싱
if grep -q 'checkpoints:' "$STATE_FILE"; then
  echo "  Checkpoints:"
  # YAML의 checkpoint 라인들 추출
  grep -E '^\s+cp-' "$STATE_FILE" | while IFS= read -r line; do
    CP_ID=$(echo "$line" | sed 's/:.*//' | tr -d ' ')
    STATUS=$(echo "$line" | grep -o 'status: [a-z_]*' | sed 's/status: //')
    ITER=$(echo "$line" | grep -o 'iteration: [0-9]*' | sed 's/iteration: //')
    case "$STATUS" in
      passed)      ICON="PASS" ;;
      in_progress) ICON=">>  " ;;
      escalated)   ICON="STOP" ;;
      pending)     ICON="    " ;;
      *)           ICON="????" ;;
    esac
    echo "    [$ICON] $CP_ID (iteration: ${ITER:-0})"
  done
  echo ""
fi

echo "  .develop-workflow/state.yaml 을 Read tool로 읽어 상세 상태를 확인하세요."
echo "  사용자에게 이어서 진행할지 물어보세요."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
