#!/usr/bin/env bash
# check-idle.sh — autopilot 파이프라인 idle 상태 검사
#
# Usage:
#   check-idle.sh [label_prefix]
#
# Arguments:
#   label_prefix — 라벨 접두사 (기본값: "autopilot:")
#
# Exit codes:
#   0 — idle (활성 이슈/PR 없음 → cycle 중단 가능)
#   1 — active (작업 있음 → 정상 진행)
#   2 — 사용법 오류
#
# Output:
#   JSON: {"idle": true/false, "ready": N, "wip": N, "prs": N}

set -euo pipefail

PREFIX="${1:-autopilot:}"

command -v gh &>/dev/null || { echo '{"error": "gh CLI not found"}' >&2; exit 2; }
command -v jq &>/dev/null || { echo '{"error": "jq not found"}' >&2; exit 2; }

READY=$(gh issue list --label "${PREFIX}ready" --state open --json number --jq 'length' 2>/dev/null || echo "0")
WIP=$(gh issue list --label "${PREFIX}wip" --state open --json number --jq 'length' 2>/dev/null || echo "0")
PRS=$(gh pr list --label "${PREFIX}auto" --state open --json number --jq 'length' 2>/dev/null || echo "0")

TOTAL=$((READY + WIP + PRS))

if [[ "$TOTAL" -eq 0 ]]; then
  echo "{\"idle\": true, \"ready\": ${READY}, \"wip\": ${WIP}, \"prs\": ${PRS}}"
  exit 0
else
  echo "{\"idle\": false, \"ready\": ${READY}, \"wip\": ${WIP}, \"prs\": ${PRS}}"
  exit 1
fi
