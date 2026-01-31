#!/bin/bash
# Team Claude - Server Lifecycle Management
# tc server 커맨드로 위임하는 래퍼 스크립트
#
# 이 스크립트는 tc CLI의 래퍼입니다.
# 실제 로직은 cli/src/commands/server.ts에서 처리됩니다.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# tc CLI 경로 확인
TC_CLI=""
if command -v tc &>/dev/null; then
  TC_CLI="tc"
elif command -v bun &>/dev/null; then
  TC_CLI="bun run ${SCRIPT_DIR}/../cli/src/index.ts"
else
  echo "[ERR] tc CLI or bun not found"
  echo "[ERR] 설치: curl -fsSL https://bun.sh/install | bash"
  exit 1
fi

# tc server 커맨드로 위임
exec $TC_CLI server "$@"
