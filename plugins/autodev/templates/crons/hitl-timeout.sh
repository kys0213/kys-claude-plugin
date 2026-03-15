#!/bin/bash
# Built-in cron: hitl-timeout (global)
# 주기: 5분 | Guard: 미응답 HITL이 존재할 때
#
# HITL 타임아웃 처리 — 미응답 HITL에 대해 리마인드 또는 자동 처리합니다.
# 중복 실행 방지는 daemon cron engine이 내부 상태로 보장합니다.

set -euo pipefail

# Guard: 미응답 HITL이 있는지 확인
PENDING_HITL=$(autodev hitl list --json | jq 'length')

if [ "$PENDING_HITL" = "0" ]; then
  echo "skip: 미응답 HITL 없음"
  exit 0
fi

echo "hitl-timeout: 미응답 HITL ${PENDING_HITL}건 확인"

autodev agent -p "미응답 HITL ${PENDING_HITL}건의 타임아웃 여부를 확인하고, 타임아웃된 항목에 리마인드를 발송해줘"
