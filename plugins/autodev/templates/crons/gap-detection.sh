#!/bin/bash
# Built-in cron: gap-detection (per-repo)
# 주기: 1시간 | Guard: active spec이 있고 git 변경이 있을 때
#
# 스펙-코드 갭 탐지 — 스펙과 코드의 불일치를 발견하여 이슈를 생성합니다.
# 중복 실행 방지는 daemon cron engine이 내부 상태로 보장합니다.

set -euo pipefail

# Guard: active spec이 존재하는지 확인
SPEC_COUNT=$(autodev spec list --json --repo "$AUTODEV_REPO_NAME" | jq '[.[] | select(.status == "Active")] | length')

if [ "$SPEC_COUNT" = "0" ]; then
  echo "skip: $AUTODEV_REPO_NAME active spec 없음"
  exit 0
fi

# Guard: 최근 git 변경이 있는지 확인 (마지막 1시간)
CHANGES=$(git -C "$AUTODEV_REPO_ROOT" log --since="1 hour ago" --oneline 2>/dev/null | wc -l | tr -d ' ')

if [ "$CHANGES" = "0" ]; then
  echo "skip: $AUTODEV_REPO_NAME 최근 1시간 git 변경 없음"
  exit 0
fi

echo "gap-detect: $AUTODEV_REPO_NAME (specs=$SPEC_COUNT, changes=$CHANGES)"

# Phase 1: Verify acceptance criteria for each active spec
SPEC_IDS=$(autodev spec list --json --repo "$AUTODEV_REPO_NAME" | jq -r '.[] | select(.status == "Active") | .id')

for SPEC_ID in $SPEC_IDS; do
  echo "verify: $SPEC_ID"
  autodev spec verify "$SPEC_ID" --create-issues || true
done

# Phase 2: Generic gap detection via agent
autodev agent --repo "$AUTODEV_REPO_NAME" -p "gap-detect 스킬을 사용하여 스펙-코드 갭을 탐지해줘"
