#!/bin/bash
# Team Claude - HUD (Heads-Up Display)
# 워크플로우 상태를 statusline에 표시

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# common.sh가 있으면 로드, 없으면 기본 함수 정의
if [[ -f "${SCRIPT_DIR}/lib/common.sh" ]]; then
  source "${SCRIPT_DIR}/lib/common.sh"
else
  # 최소 필수 함수들
  TC_DATA_ROOT="${HOME}/.team-claude"

  get_project_hash() {
    local root
    root=$(git rev-parse --show-toplevel 2>/dev/null || echo "$PWD")
    echo -n "$root" | md5sum 2>/dev/null | cut -c1-12 || echo "default"
  }

  get_project_data_dir() {
    echo "${TC_DATA_ROOT}/$(get_project_hash)"
  }
fi

# ============================================================================
# 설정
# ============================================================================

# HUD 설정 파일
HUD_CONFIG="${HOME}/.claude/team-claude-hud.yaml"

# 기본 설정
DEFAULT_PRESET="essential"
DEFAULT_SEPARATOR=" │ "

# 아이콘 (기본값)
declare -A ICONS=(
  ["autopilot"]="🚀"
  ["assisted"]="👤"
  ["manual"]="✋"
  ["spec"]="📋"
  ["impl"]="🔧"
  ["merge"]="🔀"
  ["psm"]="🌳"
  ["swarm"]="🐝"
  ["review"]="🔍"
  ["ralph"]="🔄"
  ["pass"]="✅"
  ["fail"]="❌"
  ["progress"]="🔄"
  ["pending"]="⏸️"
  ["time"]="⏱️"
)

# 진행률 바 설정
PROGRESS_WIDTH=10
PROGRESS_FILLED="█"
PROGRESS_EMPTY="░"

# ============================================================================
# 유틸리티 함수
# ============================================================================

# JSON 값 읽기 (jq 없어도 동작)
json_get() {
  local file="$1"
  local key="$2"

  if command -v jq &>/dev/null; then
    jq -r "$key // empty" "$file" 2>/dev/null
  else
    # 간단한 grep 기반 파싱 (fallback)
    grep -o "\"${key}\"[[:space:]]*:[[:space:]]*\"[^\"]*\"" "$file" 2>/dev/null | \
      sed 's/.*: *"\([^"]*\)".*/\1/'
  fi
}

# 진행률 바 생성
progress_bar() {
  local percent="${1:-0}"
  local width="${2:-$PROGRESS_WIDTH}"

  # 숫자 검증
  if ! [[ "$percent" =~ ^[0-9]+$ ]]; then
    percent=0
  fi

  local filled=$((percent * width / 100))
  local empty=$((width - filled))

  local bar=""
  for ((i=0; i<filled; i++)); do
    bar+="$PROGRESS_FILLED"
  done
  for ((i=0; i<empty; i++)); do
    bar+="$PROGRESS_EMPTY"
  done

  echo "$bar"
}

# 시간 포맷 (초 → Xm Xs)
format_duration() {
  local seconds="$1"

  if ! [[ "$seconds" =~ ^[0-9]+$ ]]; then
    echo "0s"
    return
  fi

  if [[ $seconds -lt 60 ]]; then
    echo "${seconds}s"
  elif [[ $seconds -lt 3600 ]]; then
    local m=$((seconds / 60))
    local s=$((seconds % 60))
    echo "${m}m${s}s"
  else
    local h=$((seconds / 3600))
    local m=$(((seconds % 3600) / 60))
    echo "${h}h${m}m"
  fi
}

# ============================================================================
# 상태 읽기
# ============================================================================

# Flow 상태 읽기
get_flow_state() {
  local data_dir
  data_dir=$(get_project_data_dir)

  local state_file="${data_dir}/state/workflow.json"

  if [[ ! -f "$state_file" ]]; then
    echo ""
    return
  fi

  local session_id
  session_id=$(json_get "$state_file" ".currentSession")

  if [[ -z "$session_id" ]]; then
    echo ""
    return
  fi

  local flow_file="${data_dir}/sessions/${session_id}/flow-state.json"

  if [[ ! -f "$flow_file" ]]; then
    echo ""
    return
  fi

  echo "$flow_file"
}

# PSM 상태 읽기
get_psm_state() {
  local data_dir
  data_dir=$(get_project_data_dir)

  local psm_file="${data_dir}/psm-index.json"

  if [[ -f "$psm_file" ]]; then
    echo "$psm_file"
  else
    echo ""
  fi
}

# ============================================================================
# HUD 출력 생성
# ============================================================================

# 모드 출력
render_mode() {
  local flow_file="$1"

  if [[ -z "$flow_file" ]]; then
    return
  fi

  local mode
  mode=$(json_get "$flow_file" ".mode")

  case "$mode" in
    autopilot)
      echo "${ICONS[autopilot]} auto"
      ;;
    assisted)
      echo "${ICONS[assisted]} asst"
      ;;
    manual)
      echo "${ICONS[manual]} man"
      ;;
    *)
      echo ""
      ;;
  esac
}

# 단계 출력
render_phase() {
  local flow_file="$1"

  if [[ -z "$flow_file" ]]; then
    return
  fi

  local phase
  phase=$(json_get "$flow_file" ".currentPhase")

  local icon=""
  case "$phase" in
    spec)  icon="${ICONS[spec]}" ;;
    impl)  icon="${ICONS[impl]}" ;;
    merge) icon="${ICONS[merge]}" ;;
    *)     return ;;
  esac

  # 진행률 계산 (간단한 예시)
  local status
  status=$(json_get "$flow_file" ".phases.${phase}.status")

  local percent=0
  case "$status" in
    pending)     percent=0 ;;
    in_progress) percent=50 ;;
    complete)    percent=100 ;;
  esac

  local bar
  bar=$(progress_bar "$percent" 8)

  echo "${icon} ${phase} ${bar} ${percent}%"
}

# 구현 전략 출력
render_impl_strategy() {
  local flow_file="$1"

  if [[ -z "$flow_file" ]]; then
    return
  fi

  local strategy
  strategy=$(json_get "$flow_file" ".implStrategy")

  case "$strategy" in
    psm)
      echo "${ICONS[psm]}"
      ;;
    swarm)
      echo "${ICONS[swarm]}"
      ;;
    sequential)
      echo "seq"
      ;;
    *)
      echo ""
      ;;
  esac
}

# PSM 세션 상태 출력
render_psm() {
  local psm_file="$1"

  if [[ -z "$psm_file" || ! -f "$psm_file" ]]; then
    return
  fi

  if ! command -v jq &>/dev/null; then
    return
  fi

  local total active complete
  total=$(jq -r '.sessions | length' "$psm_file" 2>/dev/null || echo 0)
  active=$(jq -r '[.sessions[] | select(.status == "active")] | length' "$psm_file" 2>/dev/null || echo 0)
  complete=$(jq -r '[.sessions[] | select(.status == "complete")] | length' "$psm_file" 2>/dev/null || echo 0)

  if [[ "$total" -gt 0 ]]; then
    echo "${ICONS[psm]} ${complete}/${total}"
  fi
}

# 리뷰 상태 출력
render_review() {
  local flow_file="$1"

  if [[ -z "$flow_file" ]]; then
    return
  fi

  # 간단한 리뷰 상태 (실제로는 리뷰 파일에서 읽어야 함)
  local phase
  phase=$(json_get "$flow_file" ".currentPhase")

  local iterations
  iterations=$(json_get "$flow_file" ".phases.${phase}.iterations")

  if [[ -n "$iterations" && "$iterations" != "null" && "$iterations" -gt 0 ]]; then
    echo "${ICONS[review]} ${iterations}/5"
  fi
}

# 경과 시간 출력
render_duration() {
  local flow_file="$1"

  if [[ -z "$flow_file" ]]; then
    return
  fi

  local created_at
  created_at=$(json_get "$flow_file" ".createdAt")

  if [[ -z "$created_at" || "$created_at" == "null" ]]; then
    return
  fi

  # ISO 8601 → Unix timestamp
  local start_ts
  if command -v date &>/dev/null; then
    start_ts=$(date -d "$created_at" +%s 2>/dev/null || echo "")
  fi

  if [[ -z "$start_ts" ]]; then
    return
  fi

  local now_ts
  now_ts=$(date +%s)

  local elapsed=$((now_ts - start_ts))
  local formatted
  formatted=$(format_duration "$elapsed")

  echo "${ICONS[time]} ${formatted}"
}

# ============================================================================
# 메인 출력
# ============================================================================

main() {
  local parts=()

  # Flow 상태 파일
  local flow_file
  flow_file=$(get_flow_state)

  # PSM 상태 파일
  local psm_file
  psm_file=$(get_psm_state)

  # Flow가 없으면 빈 출력
  if [[ -z "$flow_file" && -z "$psm_file" ]]; then
    # Team Claude가 활성화되지 않음
    exit 0
  fi

  # 모드
  local mode_output
  mode_output=$(render_mode "$flow_file")
  if [[ -n "$mode_output" ]]; then
    parts+=("$mode_output")
  fi

  # 단계
  local phase_output
  phase_output=$(render_phase "$flow_file")
  if [[ -n "$phase_output" ]]; then
    parts+=("$phase_output")
  fi

  # 구현 전략
  local strategy_output
  strategy_output=$(render_impl_strategy "$flow_file")
  if [[ -n "$strategy_output" ]]; then
    parts+=("$strategy_output")
  fi

  # PSM 상태
  local psm_output
  psm_output=$(render_psm "$psm_file")
  if [[ -n "$psm_output" ]]; then
    parts+=("$psm_output")
  fi

  # 리뷰 상태
  local review_output
  review_output=$(render_review "$flow_file")
  if [[ -n "$review_output" ]]; then
    parts+=("$review_output")
  fi

  # 경과 시간
  local duration_output
  duration_output=$(render_duration "$flow_file")
  if [[ -n "$duration_output" ]]; then
    parts+=("$duration_output")
  fi

  # 결합하여 출력
  if [[ ${#parts[@]} -gt 0 ]]; then
    local IFS="$DEFAULT_SEPARATOR"
    echo "${parts[*]}"
  fi
}

# stdin에서 Claude Code 컨텍스트 읽기 (무시 - 우리는 파일 기반)
if [[ ! -t 0 ]]; then
  cat > /dev/null
fi

main "$@"
