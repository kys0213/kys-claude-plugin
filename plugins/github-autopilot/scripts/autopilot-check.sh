#!/usr/bin/env bash
# autopilot-check.sh — pre-check and state management for autopilot loops
#
# Subcommands:
#   diff <loop-name> [spec_paths...]
#     Check what changed since last analysis.
#     Exit codes:
#       0 = no changes (skip)
#       1 = spec changed (full analysis needed)
#       2 = code only changed (lightweight re-verification)
#       3 = first run (no previous state)
#     Output: JSON with changed files categorized
#
#   mark <loop-name>
#     Record current HEAD as the last analyzed commit.
#
#   status
#     Show state of all loops (loop name, last hash, timestamp, age).

set -euo pipefail

# --- state directory ---
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo unknown)"
REPO_NAME="$(basename "$REPO_ROOT")"
STATE_DIR="/tmp/autopilot-${REPO_NAME}/state"
mkdir -p "$STATE_DIR"

# --- helpers ---
json_escape() {
  local s="$1"
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  printf '%s' "$s"
}

iso_timestamp() {
  date -u '+%Y-%m-%dT%H:%M:%S' 2>/dev/null || date '+%Y-%m-%dT%H:%M:%S'
}

# --- diff subcommand ---
cmd_diff() {
  local loop_name="${1:?Usage: autopilot-check.sh diff <loop-name> [spec_paths...]}"
  shift
  local spec_paths=("$@")

  local state_file="${STATE_DIR}/${loop_name}.state"

  # first run check
  if [[ ! -f "$state_file" ]]; then
    echo '{"status":"first_run","changed_files":[],"spec_files":[],"code_files":[]}'
    exit 3
  fi

  # read last analyzed hash
  local last_hash
  last_hash="$(grep -o '"hash":"[^"]*"' "$state_file" | head -1 | cut -d'"' -f4)"

  if [[ -z "$last_hash" ]]; then
    echo '{"status":"first_run","changed_files":[],"spec_files":[],"code_files":[]}'
    exit 3
  fi

  # verify the commit still exists
  if ! git cat-file -e "$last_hash" 2>/dev/null; then
    echo '{"status":"first_run","changed_files":[],"spec_files":[],"code_files":[],"reason":"last_hash_not_found"}'
    exit 3
  fi

  local current_hash
  current_hash="$(git rev-parse HEAD)"

  # same commit = no changes
  if [[ "$last_hash" == "$current_hash" ]]; then
    echo '{"status":"no_changes","changed_files":[],"spec_files":[],"code_files":[]}'
    exit 0
  fi

  # get changed files
  local changed_files
  changed_files="$(git diff "${last_hash}..HEAD" --name-only 2>/dev/null || true)"

  if [[ -z "$changed_files" ]]; then
    echo '{"status":"no_changes","changed_files":[],"spec_files":[],"code_files":[]}'
    exit 0
  fi

  # classify files: spec vs code
  local spec_list=()
  local code_list=()
  local has_spec=false

  while IFS= read -r file; do
    [[ -z "$file" ]] && continue
    local is_spec=false

    # match against spec_paths patterns
    if [[ ${#spec_paths[@]} -gt 0 ]]; then
      for pattern in "${spec_paths[@]}"; do
        # strip trailing slash for matching
        pattern="${pattern%/}"
        if [[ "$file" == ${pattern}* ]] || [[ "$file" == ${pattern}/* ]]; then
          is_spec=true
          break
        fi
      done
    fi

    if [[ "$is_spec" == "true" ]]; then
      spec_list+=("$file")
      has_spec=true
    else
      code_list+=("$file")
    fi
  done <<< "$changed_files"

  # build JSON arrays
  local spec_json="["
  local first=true
  for f in "${spec_list[@]+"${spec_list[@]}"}"; do
    [[ -z "$f" ]] && continue
    if [[ "$first" == "true" ]]; then first=false; else spec_json+=","; fi
    spec_json+="\"$(json_escape "$f")\""
  done
  spec_json+="]"

  local code_json="["
  first=true
  for f in "${code_list[@]+"${code_list[@]}"}"; do
    [[ -z "$f" ]] && continue
    if [[ "$first" == "true" ]]; then first=false; else code_json+=","; fi
    code_json+="\"$(json_escape "$f")\""
  done
  code_json+="]"

  local all_json="["
  first=true
  while IFS= read -r file; do
    [[ -z "$file" ]] && continue
    if [[ "$first" == "true" ]]; then first=false; else all_json+=","; fi
    all_json+="\"$(json_escape "$file")\""
  done <<< "$changed_files"
  all_json+="]"

  # determine status and exit code
  if [[ "$has_spec" == "true" ]]; then
    echo "{\"status\":\"spec_changed\",\"changed_files\":${all_json},\"spec_files\":${spec_json},\"code_files\":${code_json}}"
    exit 1
  elif [[ ${#code_list[@]} -gt 0 ]]; then
    echo "{\"status\":\"code_changed\",\"changed_files\":${all_json},\"spec_files\":${spec_json},\"code_files\":${code_json}}"
    exit 2
  else
    echo "{\"status\":\"no_changes\",\"changed_files\":${all_json},\"spec_files\":${spec_json},\"code_files\":${code_json}}"
    exit 0
  fi
}

# --- mark subcommand ---
cmd_mark() {
  local loop_name="${1:?Usage: autopilot-check.sh mark <loop-name>}"
  local state_file="${STATE_DIR}/${loop_name}.state"

  local current_hash
  current_hash="$(git rev-parse HEAD)"
  local ts
  ts="$(iso_timestamp)"

  echo "{\"hash\":\"${current_hash}\",\"timestamp\":\"${ts}\"}" > "$state_file"
  echo "marked ${loop_name}: ${current_hash} at ${ts}"
}

# --- status subcommand ---
cmd_status() {
  local found=false

  echo "Loop States (${STATE_DIR}):"
  echo "---"

  for state_file in "${STATE_DIR}"/*.state; do
    [[ ! -f "$state_file" ]] && continue
    found=true

    local loop_name
    loop_name="$(basename "$state_file" .state)"
    local content
    content="$(cat "$state_file")"

    local hash
    hash="$(echo "$content" | grep -o '"hash":"[^"]*"' | head -1 | cut -d'"' -f4)"
    local timestamp
    timestamp="$(echo "$content" | grep -o '"timestamp":"[^"]*"' | head -1 | cut -d'"' -f4)"

    # calculate age
    local age="unknown"
    if [[ -n "$timestamp" ]]; then
      local now_epoch
      local ts_epoch
      now_epoch="$(date +%s)"
      # try GNU date first, then BSD date
      ts_epoch="$(date -d "$timestamp" +%s 2>/dev/null || date -j -f '%Y-%m-%dT%H:%M:%S' "$timestamp" +%s 2>/dev/null || echo "")"

      if [[ -n "$ts_epoch" ]]; then
        local diff_secs=$((now_epoch - ts_epoch))
        if [[ $diff_secs -lt 60 ]]; then
          age="${diff_secs}s ago"
        elif [[ $diff_secs -lt 3600 ]]; then
          age="$((diff_secs / 60))m ago"
        elif [[ $diff_secs -lt 86400 ]]; then
          age="$((diff_secs / 3600))h ago"
        else
          age="$((diff_secs / 86400))d ago"
        fi
      fi
    fi

    local short_hash="${hash:0:7}"
    printf "  %-20s  %s  %s  (%s)\n" "$loop_name" "$short_hash" "$timestamp" "$age"
  done

  if [[ "$found" == "false" ]]; then
    echo "  (no loop states found)"
  fi
}

# --- main dispatch ---
SUBCOMMAND="${1:?Usage: autopilot-check.sh <diff|mark|status> [args...]}"
shift

case "$SUBCOMMAND" in
  diff)   cmd_diff "$@" ;;
  mark)   cmd_mark "$@" ;;
  status) cmd_status "$@" ;;
  *)
    echo "Unknown subcommand: $SUBCOMMAND" >&2
    echo "Usage: autopilot-check.sh <diff|mark|status> [args...]" >&2
    exit 1
    ;;
esac
