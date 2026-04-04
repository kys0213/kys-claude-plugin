#!/usr/bin/env bash
# DEPRECATED: Use `autopilot pipeline idle --label-prefix <prefix>` instead.
# This script is kept for backward compatibility.
#
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
#   2 — 실행 환경 오류 (gh CLI 없음, API 실패 등)
#
# Output:
#   JSON: {"idle": true/false, "ready": N, "wip": N, "prs": N}

set -euo pipefail

PREFIX="${1:-autopilot:}"

command -v gh &>/dev/null || { echo '{"error": "gh CLI not found"}' >&2; exit 2; }

# 병렬 실행으로 API 응답 대기 시간 단축
READY_FILE=$(mktemp)
WIP_FILE=$(mktemp)
PRS_FILE=$(mktemp)
trap 'rm -f "$READY_FILE" "$WIP_FILE" "$PRS_FILE"' EXIT

gh issue list --label "${PREFIX}ready" --state open --json number --jq 'length' > "$READY_FILE" 2>&1 &
gh issue list --label "${PREFIX}wip" --state open --json number --jq 'length' > "$WIP_FILE" 2>&1 &
gh pr list --label "${PREFIX}auto" --state open --json number --jq 'length' > "$PRS_FILE" 2>&1 &
wait

READY=$(cat "$READY_FILE")
WIP=$(cat "$WIP_FILE")
PRS=$(cat "$PRS_FILE")

# 숫자가 아닌 응답이면 API 오류로 판단
if ! [[ "$READY" =~ ^[0-9]+$ ]] || ! [[ "$WIP" =~ ^[0-9]+$ ]] || ! [[ "$PRS" =~ ^[0-9]+$ ]]; then
  echo "{\"error\": \"gh API failure\", \"ready\": \"${READY}\", \"wip\": \"${WIP}\", \"prs\": \"${PRS}\"}" >&2
  exit 2
fi

TOTAL=$((READY + WIP + PRS))

if [[ "$TOTAL" -eq 0 ]]; then
  echo "{\"idle\": true, \"ready\": ${READY}, \"wip\": ${WIP}, \"prs\": ${PRS}}"
  exit 0
else
  echo "{\"idle\": false, \"ready\": ${READY}, \"wip\": ${WIP}, \"prs\": ${PRS}}"
  exit 1
fi
