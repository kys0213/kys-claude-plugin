#!/bin/bash
# Built-in cron: hitl-timeout (global)
# 주기: 5분 | Guard: 미응답 HITL이 존재할 때
#
# HITL 타임아웃 처리 — 타임아웃 초과 HITL을 만료(expired) 상태로 변경합니다.
# 결정적 로직만 수행하며 LLM(autodev agent)을 호출하지 않습니다.
# 중복 실행 방지는 daemon cron engine이 내부 상태로 보장합니다.

set -euo pipefail

# Guard: 미응답 HITL이 있는지 확인
PENDING_HITL=$(autodev hitl list --json | jq 'length')

if [ "$PENDING_HITL" = "0" ]; then
  echo "skip: 미응답 HITL 없음"
  exit 0
fi

echo "hitl-timeout: 미응답 HITL ${PENDING_HITL}건 확인, 타임아웃 체크"

# 결정적 처리: 타임아웃 초과 HITL을 expired로 전이
autodev hitl timeout
