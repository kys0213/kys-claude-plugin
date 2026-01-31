#!/bin/bash
# on-worker-idle.sh
# Worker 대기 상태 알림 - tc hook으로 위임
#
# 이 스크립트는 tc CLI의 래퍼입니다.
# 실제 로직은 cli/src/commands/hook.ts에서 처리됩니다.

set -e

# Context 사용률 (환경 변수 또는 기본값)
CONTEXT_PERCENT="${CONTEXT_PERCENT:-50}"

# tc CLI 경로 확인
TC_CLI=""
if command -v tc &>/dev/null; then
  TC_CLI="tc"
elif command -v bun &>/dev/null; then
  SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  TC_CLI="bun run ${SCRIPT_DIR}/../../cli/src/index.ts"
else
  echo "Error: tc CLI or bun not found"
  exit 1
fi

# tc hook worker-idle 호출
exec $TC_CLI hook worker-idle --percent "$CONTEXT_PERCENT"
