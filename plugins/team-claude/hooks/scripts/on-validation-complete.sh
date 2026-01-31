#!/bin/bash
# on-validation-complete.sh
# 검증 완료 후 결과 처리 - tc hook으로 위임
#
# 이 스크립트는 tc CLI의 래퍼입니다.
# 실제 로직은 cli/src/commands/hook.ts에서 처리됩니다.

set -e

# stdin으로 검증 결과 받기
VALIDATION_OUTPUT=$(cat)
EXIT_CODE=${PIPESTATUS[0]:-0}

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

# 출력을 임시 파일에 저장 (긴 출력 처리)
OUTPUT_FILE=$(mktemp)
echo "$VALIDATION_OUTPUT" > "$OUTPUT_FILE"

# tc hook validation-complete 호출
exec $TC_CLI hook validation-complete \
  --exit-code "$EXIT_CODE" \
  --output "$(cat "$OUTPUT_FILE")"

# 임시 파일 정리 (exec으로 인해 실행되지 않음, 시스템이 정리)
