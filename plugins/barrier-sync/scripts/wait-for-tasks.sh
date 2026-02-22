#!/usr/bin/env bash
#
# wait-for-tasks.sh — FIFO-based barrier for parallel background Task synchronization
#
# Usage:
#   BARRIER_ID="my-barrier" bash /path/to/wait-for-tasks.sh <expected_count> [timeout_sec]
#
# - expected_count: number of Tasks to wait for
# - timeout_sec: optional timeout in seconds (default: 300)
# - BARRIER_ID: optional env var for barrier isolation (default: barrier-$$)
#
# Output (stdout):
#   When all tasks complete or timeout occurs, prints a summary with each agent's result.

set -euo pipefail

# --- Args ---
EXPECTED=${1:?Usage: wait-for-tasks.sh <expected_count> [timeout_sec]}
TIMEOUT=${2:-300}
BARRIER_ID="${BARRIER_ID:-barrier-$$}"

# --- Paths ---
BARRIER_DIR="/tmp/claude-barriers/${BARRIER_ID}"
FIFO_PATH="${BARRIER_DIR}/pipe"
META_PATH="${BARRIER_DIR}/meta.json"
RESULTS_DIR="${BARRIER_DIR}/results"

# --- Cleanup on exit ---
cleanup() {
  # Kill timeout watcher if alive
  if [[ -n "${TIMEOUT_PID:-}" ]] && kill -0 "$TIMEOUT_PID" 2>/dev/null; then
    kill "$TIMEOUT_PID" 2>/dev/null || true
  fi
  rm -rf "$BARRIER_DIR"
}
trap cleanup EXIT

# --- Setup ---
mkdir -p "$RESULTS_DIR"
mkfifo "$FIFO_PATH"

# Write meta for signal-done.cjs to discover
cat > "$META_PATH" <<METAEOF
{"pid":$$,"fifo":"${FIFO_PATH}","expected":${EXPECTED},"barrier_id":"${BARRIER_ID}"}
METAEOF

# --- Timeout watcher ---
(
  sleep "$TIMEOUT"
  # Write a timeout sentinel to unblock the read loop
  echo "__TIMEOUT__" > "$FIFO_PATH" 2>/dev/null || true
) &
TIMEOUT_PID=$!

# --- Barrier loop ---
COMPLETED=0
AGENTS=""
TIMED_OUT=false

while (( COMPLETED < EXPECTED )); do
  # Blocking read from FIFO — zero CPU, zero LLM turns
  if read -r AGENT_ID < "$FIFO_PATH"; then
    # Check for timeout sentinel
    if [[ "$AGENT_ID" == "__TIMEOUT__" ]]; then
      TIMED_OUT=true
      break
    fi
    COMPLETED=$((COMPLETED + 1))
    AGENTS="${AGENTS} ${AGENT_ID}"
  fi
done

# --- Output results ---
if [[ "$TIMED_OUT" == "true" ]]; then
  echo "--- BARRIER TIMEOUT (${TIMEOUT}s) ${COMPLETED}/${EXPECTED} completed ---"
else
  echo "--- BARRIER COMPLETE (${COMPLETED}/${EXPECTED}) ---"
fi

echo "agents:${AGENTS}"
echo ""

# Print each agent's captured result
for AGENT_ID in $AGENTS; do
  RESULT_FILE="${RESULTS_DIR}/${AGENT_ID}.txt"
  echo "=== ${AGENT_ID} ==="
  if [[ -f "$RESULT_FILE" ]]; then
    cat "$RESULT_FILE"
  else
    echo "(no result captured)"
  fi
  echo ""
done

# Exit code: 0 = all done, 1 = timeout
if [[ "$TIMED_OUT" == "true" ]]; then
  exit 1
fi
