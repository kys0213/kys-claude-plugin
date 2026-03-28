#!/usr/bin/env bash
# run-loop.sh — autopilot 범용 루프 러너
#
# Usage:
#   run-loop.sh <command> <interval> [label_prefix] [idle_check] [log_dir]
#
# Arguments:
#   command      — slash command or prompt (e.g., "/github-autopilot:gap-watch")
#   interval     — loop interval (e.g., "30m", "1h", "300s")
#   label_prefix — label prefix for idle check (default: "autopilot:")
#   idle_check   — "true" or "false" (default: "true")
#   log_dir      — directory for tick logs (optional, no logging if omitted)
#
# Features:
#   - PID file based duplicate prevention (per repo + command)
#   - Signal trap: SIGTERM/SIGINT kills running claude child process
#   - Automatic PID file cleanup on exit
#
# Exit:
#   0 — idle / duplicate detected / signal received
#   Runs indefinitely otherwise until killed.

set -euo pipefail

COMMAND="${1:?Usage: run-loop.sh <command> <interval> [label_prefix] [idle_check] [log_dir]}"
INTERVAL="${2:?Missing interval}"
LABEL_PREFIX="${3:-autopilot:}"
IDLE_CHECK="${4:-true}"
LOG_DIR="${5:-}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

parse_interval() {
  local input="$1"
  local num="${input%[smhd]}"
  local unit="${input: -1}"

  case "$unit" in
    s) echo "$num" ;;
    m) echo $((num * 60)) ;;
    h) echo $((num * 3600)) ;;
    d) echo $((num * 86400)) ;;
    *) echo "$num" ;;
  esac
}

# --- command name sanitization (used for log + pid filenames) ---
sanitize_command_name() {
  local cmd="$1"
  local name="${cmd##*:}"
  local base="${name%% *}"
  if [[ "$cmd" == *" "* ]]; then
    local args="${cmd#* }"
    echo "${base}-${args// /-}"
  else
    echo "$base"
  fi
}

LOOP_NAME=$(sanitize_command_name "$COMMAND")

# --- logging ---
LOG_FILE=""
if [[ -n "$LOG_DIR" ]]; then
  LOG_FILE="${LOG_DIR}/${LOOP_NAME}.log"
fi

log() {
  local level="$1" msg="$2"
  local ts
  ts="$(date -u '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null || date '+%Y-%m-%dT%H:%M:%S')"
  local line="[${ts}] ${level}  ${msg}"
  echo "$line"
  [[ -n "$LOG_FILE" ]] && echo "$line" >> "$LOG_FILE"
}

# --- PID file: duplicate prevention + cleanup ---
REPO_NAME=$(basename "$(git rev-parse --show-toplevel 2>/dev/null || echo unknown)")
PID_DIR="/tmp/autopilot-${REPO_NAME}/pids"
mkdir -p "$PID_DIR"
PID_FILE="${PID_DIR}/${LOOP_NAME}.pid"

if [[ -f "$PID_FILE" ]]; then
  EXISTING_PID=$(cat "$PID_FILE")
  if kill -0 "$EXISTING_PID" 2>/dev/null; then
    log "DUP" "already running (pid=${EXISTING_PID}), skipping"
    exit 0
  fi
  rm -f "$PID_FILE"
fi

echo $$ > "$PID_FILE"

# --- signal trap: kill child process + cleanup PID file ---
CHILD_PID=""
cleanup() {
  if [[ -n "$CHILD_PID" ]] && kill -0 "$CHILD_PID" 2>/dev/null; then
    log "KILL" "terminating child process (pid=${CHILD_PID})"
    kill "$CHILD_PID" 2>/dev/null
    wait "$CHILD_PID" 2>/dev/null || true
  fi
  rm -f "$PID_FILE"
  log "EXIT" "loop terminated"
}
trap cleanup EXIT

# --- interval ---
SLEEP_SECONDS=$(parse_interval "$INTERVAL")
if [[ "$SLEEP_SECONDS" -lt 60 ]]; then
  log "WARN" "interval too short (${SLEEP_SECONDS}s), using 60s minimum"
  SLEEP_SECONDS=60
fi

log "INIT" "command=${COMMAND} interval=${INTERVAL} (${SLEEP_SECONDS}s) idle_check=${IDLE_CHECK} pid=$$"

TICK=0

while true; do
  TICK=$((TICK + 1))

  # idle check (built-in loops only)
  if [[ "$IDLE_CHECK" == "true" ]]; then
    idle_exit=0
    "${SCRIPT_DIR}/check-idle.sh" "$LABEL_PREFIX" >/dev/null 2>&1 || idle_exit=$?

    if [[ $idle_exit -eq 0 ]]; then
      log "IDLE" "pipeline idle, stopping (tick=${TICK})"
      exit 0
    elif [[ $idle_exit -eq 2 ]]; then
      log "SKIP" "idle check error, skipping tick=${TICK}"
      sleep "$SLEEP_SECONDS"
      continue
    fi
    # exit 1 = active, proceed
  fi

  # execute command (background + wait for signal-safe child tracking)
  log "START" "tick=${TICK}"
  TICK_START=$SECONDS

  cmd_exit=0
  claude -p "$COMMAND" < /dev/null &
  CHILD_PID=$!
  wait "$CHILD_PID" || cmd_exit=$?
  CHILD_PID=""

  DURATION=$((SECONDS - TICK_START))
  log "END" "tick=${TICK} exit=${cmd_exit} duration=${DURATION}s"

  # sleep until next cycle
  sleep "$SLEEP_SECONDS"
done
