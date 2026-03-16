#!/bin/bash
# Built-in cron: daily-report (global)
# 주기: 매일 06시 | Guard: 24시간 내 활동이 있을 때
#
# 일일 리포트 — daemon 내장 DailyReporter가 직접 처리합니다.
# 이 스크립트는 DailyReporter 트리거 역할만 수행하며 LLM(autodev agent)을 호출하지 않습니다.
# 실제 로직: daemon 로그 파싱 → 통계 집계 → GitHub 이슈 게시
# 중복 실행 방지는 daemon cron engine이 내부 상태로 보장합니다.

set -euo pipefail

YESTERDAY=$(date -d "yesterday" +%Y-%m-%d 2>/dev/null || date -v-1d +%Y-%m-%d)
LOG_FILE="${AUTODEV_HOME}/logs/daemon.${YESTERDAY}.log"

# Guard: 어제 daemon 로그가 존재하는지 확인
if [ ! -f "$LOG_FILE" ]; then
  echo "skip: ${YESTERDAY} 로그 파일 없음"
  exit 0
fi

# Guard: 로그에 실제 활동(task 실행)이 있는지 확인
if ! grep -q "task_" "$LOG_FILE" 2>/dev/null; then
  echo "skip: ${YESTERDAY} 활동 이력 없음"
  exit 0
fi

echo "daily-report: ${YESTERDAY} 리포트 생성 트리거"

# 결정적 처리: daemon 내장 DailyReporter에 위임
autodev report daily --date "$YESTERDAY"
