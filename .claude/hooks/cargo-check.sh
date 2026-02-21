#!/bin/bash
# PostToolUse hook: .rs 파일 수정 시 cargo check 실행
# exit 0 = 성공 (피드백 없음)
# exit 2 = 실패 (stderr가 Claude에게 피드백됨)

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

# .rs 파일이 아니면 패스
if [[ "$FILE_PATH" != *.rs ]]; then
  exit 0
fi

# 편집된 파일에서 가장 가까운 Cargo.toml 탐색
DIR=$(dirname "$FILE_PATH")
CARGO_DIR=""
while [[ "$DIR" != "/" ]]; do
  if [[ -f "$DIR/Cargo.toml" ]]; then
    CARGO_DIR="$DIR"
    break
  fi
  DIR=$(dirname "$DIR")
done

if [[ -z "$CARGO_DIR" ]]; then
  exit 0
fi

# cargo check 실행 (incremental이라 2회차부터 빠름)
OUTPUT=$(cd "$CARGO_DIR" && cargo check 2>&1)
EXIT_CODE=$?

if [[ $EXIT_CODE -ne 0 ]]; then
  # error 라인만 추출하여 피드백
  ERRORS=$(echo "$OUTPUT" | grep -E "^error" | head -20)
  echo "cargo check failed in $CARGO_DIR:" >&2
  echo "$ERRORS" >&2
  exit 2
fi

exit 0
