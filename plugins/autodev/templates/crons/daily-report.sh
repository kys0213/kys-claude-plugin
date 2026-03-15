#!/bin/bash
# Built-in cron: daily-report (global)
# 주기: 매일 06시 | Guard: 24시간 내 활동이 있을 때
#
# 일일 리포트 — 전체 레포의 24시간 활동을 요약하여 리포트를 생성합니다.
# 중복 실행 방지는 daemon cron engine이 내부 상태로 보장합니다.

set -euo pipefail

# Guard: 최근 판단 이력이 존재하는지 확인
RECENT=$(autodev spec decisions --json -n 1 | jq 'length')

if [ "$RECENT" = "0" ]; then
  echo "skip: 판단 이력 없음"
  exit 0
fi

echo "daily-report: 리포트 생성 시작"

autodev agent -p "지난 24시간의 autodev 활동을 요약하여 일일 리포트를 생성해줘. 각 레포별 진행 상황, 완료된 이슈, 실패한 작업, HITL 현황을 포함해줘."
