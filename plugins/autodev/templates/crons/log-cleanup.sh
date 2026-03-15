#!/bin/bash
# Built-in cron: log-cleanup (global)
# 주기: 매일 00시 | Guard: 보관 기간 초과 로그가 존재할 때
#
# 로그 정리 — 보관 기간이 지난 로그 파일을 정리합니다.
# 중복 실행 방지는 daemon cron engine이 내부 상태로 보장합니다.

set -euo pipefail

LOG_DIR="${AUTODEV_HOME}/logs"
RETENTION_DAYS=30

# Guard: 로그 디렉토리가 존재하는지 확인
if [ ! -d "$LOG_DIR" ]; then
  echo "skip: 로그 디렉토리 없음"
  exit 0
fi

# 단일 패스: 삭제하면서 카운트
DELETED=$(find "$LOG_DIR" -name "*.log" -mtime +"$RETENTION_DAYS" -delete -print 2>/dev/null | wc -l | tr -d ' ')

if [ "$DELETED" = "0" ]; then
  echo "skip: 보관 기간 초과 로그 없음"
  exit 0
fi

# 빈 디렉토리 정리
find "$LOG_DIR" -type d -empty -delete 2>/dev/null || true

echo "log-cleanup: ${DELETED}개 로그 파일 정리 완료 (${RETENTION_DAYS}일 초과)"
