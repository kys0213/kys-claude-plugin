#!/usr/bin/env bash
# SessionStart hook: 워크플로우 상태 감지
# state.json이 있으면 에이전트에게 현재 상태를 알려줍니다.

STATE_FILE=".develop-workflow/state.json"

if [ ! -f "$STATE_FILE" ]; then
  exit 0
fi

# node가 있으면 JSON 파싱, 없으면 grep 폴백
if command -v node >/dev/null 2>&1; then
  PARSED=$(node -e "
    const s = JSON.parse(require('fs').readFileSync('$STATE_FILE','utf8'));
    console.log('PHASE=' + (s.phase || ''));
    console.log('FEATURE=' + (s.feature || ''));
    console.log('STRATEGY=' + (s.strategy || ''));
    console.log('UPDATED=' + (s.updated_at || ''));
    console.log('GATE_REVIEW=' + (s.gates && s.gates.review_clean_pass ? 'true' : 'false'));
    console.log('GATE_ARCHITECT=' + (s.gates && s.gates.architect_verified ? 'true' : 'false'));
    console.log('GATE_RE_REVIEW=' + (s.gates && s.gates.re_review_clean ? 'true' : 'false'));
    if (s.checkpoints) {
      Object.entries(s.checkpoints).forEach(([k,v]) => {
        console.log('CP=' + k + '|' + (v.status||'pending') + '|' + (v.iteration||0));
      });
    }
  " 2>/dev/null)

  eval "$(echo "$PARSED" | grep -E '^(PHASE|FEATURE|STRATEGY|UPDATED|GATE_)=')"
  CPS=$(echo "$PARSED" | grep '^CP=')
else
  # node 없을 때 grep 폴백 (제한적 파싱)
  PHASE=$(grep -o '"phase"[[:space:]]*:[[:space:]]*"[^"]*"' "$STATE_FILE" | head -1 | sed 's/.*: *"//;s/"//')
  FEATURE=$(grep -o '"feature"[[:space:]]*:[[:space:]]*"[^"]*"' "$STATE_FILE" | head -1 | sed 's/.*: *"//;s/"//')
  STRATEGY=$(grep -o '"strategy"[[:space:]]*:[[:space:]]*"[^"]*"' "$STATE_FILE" | head -1 | sed 's/.*: *"//;s/"//')
  UPDATED=$(grep -o '"updated_at"[[:space:]]*:[[:space:]]*"[^"]*"' "$STATE_FILE" | head -1 | sed 's/.*: *"//;s/"//')
  GATE_REVIEW="unknown"
  GATE_ARCHITECT="unknown"
  GATE_RE_REVIEW="unknown"
  CPS=""
fi

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
echo "  Gates:"
gate_icon() { [ "$1" = "true" ] && echo "PASS" || echo "    "; }
echo "    [$(gate_icon "$GATE_REVIEW")] review_clean_pass"
echo "    [$(gate_icon "$GATE_ARCHITECT")] architect_verified"
echo "    [$(gate_icon "$GATE_RE_REVIEW")] re_review_clean"
echo ""

# Checkpoint 상태 표시
if [ -n "$CPS" ]; then
  echo "  Checkpoints:"
  echo "$CPS" | while IFS= read -r line; do
    CP_ID=$(echo "$line" | cut -d'|' -f1 | sed 's/^CP=//')
    STATUS=$(echo "$line" | cut -d'|' -f2)
    ITER=$(echo "$line" | cut -d'|' -f3)
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

echo "  .develop-workflow/state.json 을 Read tool로 읽어 상세 상태를 확인하세요."
echo "  사용자에게 이어서 진행할지 물어보세요."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
