#!/bin/bash
# on-worker-complete.sh
# Worker 완료 시 검증 트리거 - tc hook으로 위임
#
# 이 스크립트는 tc CLI의 래퍼입니다.
# 실제 로직은 cli/src/commands/hook.ts에서 처리됩니다.

set -e

# 환경 변수에서 task ID 가져오기 (있는 경우)
TASK_ID="${AFL_TASK_ID:-task-$(date +%s)}"

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

# tc hook worker-complete 호출
exec $TC_CLI hook worker-complete --task-id "$TASK_ID"
