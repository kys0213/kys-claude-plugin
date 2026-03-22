#!/bin/bash
# Built-in cron: claw-evaluate (per-repo)
# 주기: 60초 | Guard: 큐에 pending 아이템이 있거나 HITL이 존재하거나 분해 대기 스펙이 있을 때
#
# Claw headless 큐 평가 — 큐 상태를 분석하고 다음 작업을 결정합니다.
# 새로 등록된 스펙이 있으면 decompose skill로 이슈를 분해합니다.
# 중복 실행 방지는 daemon cron engine이 내부 상태로 보장합니다.

set -euo pipefail

# Guard: 큐에 pending 아이템이 있거나 HITL이 존재하거나 분해 대기 스펙이 있을 때만 실행
PENDING=$(autodev queue list --repo "$AUTODEV_REPO_NAME" --json | jq 'length')
HITL=$(autodev hitl list --repo "$AUTODEV_REPO_NAME" --json | jq 'length')
UNDECOMPOSED=$(autodev spec list-undecomposed --repo "$AUTODEV_REPO_NAME" --json | jq 'length')

if [ "$PENDING" = "0" ] && [ "$HITL" = "0" ] && [ "$UNDECOMPOSED" = "0" ]; then
  echo "skip: $AUTODEV_REPO_NAME 큐 비어있고 HITL 없고 분해 대기 스펙 없음"
  exit 0
fi

echo "evaluate: $AUTODEV_REPO_NAME (pending=$PENDING, hitl=$HITL, undecomposed=$UNDECOMPOSED)"

# Build prompt based on what needs attention
PROMPT=""

if [ "$UNDECOMPOSED" -gt "0" ]; then
  SPEC_IDS=$(autodev spec list-undecomposed --repo "$AUTODEV_REPO_NAME" --json | jq -r '.[].id')
  PROMPT="새로 등록된 스펙이 ${UNDECOMPOSED}개 있습니다. decompose skill을 사용하여 각 스펙을 구현 가능한 단위의 GitHub 이슈로 분해해주세요. 각 이슈에는 autodev:analyze 라벨을 부여해야 합니다. 분해 대상 스펙 ID: ${SPEC_IDS}."
fi

if [ "$PENDING" -gt "0" ] || [ "$HITL" -gt "0" ]; then
  if [ -n "$PROMPT" ]; then
    PROMPT="${PROMPT} 그리고 큐를 평가하고 다음 작업을 결정해주세요."
  else
    PROMPT="큐를 평가하고 다음 작업을 결정해줘"
  fi
fi

autodev agent --repo "$AUTODEV_REPO_NAME" -p "$PROMPT"
