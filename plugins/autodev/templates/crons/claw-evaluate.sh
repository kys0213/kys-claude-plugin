#!/bin/bash
# Built-in cron: claw-evaluate (per-repo)
# 주기: 60초 | Guard: 큐에 pending 아이템이 있거나 HITL이 존재할 때
#
# Claw headless 큐 평가 — 큐 상태를 분석하고 다음 작업을 결정합니다.
# 중복 실행 방지는 daemon cron engine이 내부 상태로 보장합니다.

set -euo pipefail

# Guard: 큐에 pending 아이템이 있거나 HITL이 존재할 때만 실행
PENDING=$(autodev queue list --repo "$AUTODEV_REPO_NAME" --json | jq 'length')
HITL=$(autodev hitl list --repo "$AUTODEV_REPO_NAME" --json | jq 'length')

if [ "$PENDING" = "0" ] && [ "$HITL" = "0" ]; then
  echo "skip: $AUTODEV_REPO_NAME 큐 비어있고 HITL 없음"
  exit 0
fi

echo "evaluate: $AUTODEV_REPO_NAME (pending=$PENDING, hitl=$HITL)"

autodev agent --repo "$AUTODEV_REPO_NAME" -p "큐를 평가하고 다음 작업을 결정해줘"
