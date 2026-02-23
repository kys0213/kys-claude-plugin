#!/bin/bash
# PostToolUse hook: .rs 파일 수정 시 CI와 동일한 검증 체인 실행
# CI(validate.yml) 순서: cargo fmt --check → cargo clippy → cargo check → cargo test
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

cd "$CARGO_DIR" || exit 0

# 1. Format check (CI: cargo fmt --check)
FMT_OUTPUT=$(cargo fmt --check 2>&1)
if [[ $? -ne 0 ]]; then
  echo "cargo fmt --check failed in $CARGO_DIR:" >&2
  echo "$FMT_OUTPUT" | head -20 >&2
  exit 2
fi

# 2. Clippy lint (CI: cargo clippy -- -D warnings)
CLIPPY_OUTPUT=$(cargo clippy -- -D warnings 2>&1)
if [[ $? -ne 0 ]]; then
  ERRORS=$(echo "$CLIPPY_OUTPUT" | grep -E "^error" | head -20)
  echo "cargo clippy failed in $CARGO_DIR:" >&2
  echo "$ERRORS" >&2
  exit 2
fi

# 3. Type check (CI: cargo check)
CHECK_OUTPUT=$(cargo check 2>&1)
if [[ $? -ne 0 ]]; then
  ERRORS=$(echo "$CHECK_OUTPUT" | grep -E "^error" | head -20)
  echo "cargo check failed in $CARGO_DIR:" >&2
  echo "$ERRORS" >&2
  exit 2
fi

# 4. Tests (CI: cargo test)
TEST_OUTPUT=$(cargo test 2>&1)
if [[ $? -ne 0 ]]; then
  ERRORS=$(echo "$TEST_OUTPUT" | grep -E "(^test .* FAILED|^error)" | head -20)
  echo "cargo test failed in $CARGO_DIR:" >&2
  echo "$ERRORS" >&2
  exit 2
fi

exit 0
