#!/bin/bash
# Team Claude - Workflow State Management
# 프로젝트별 워크플로우 상태 관리 스크립트

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"

# ============================================================================
# 상수
# ============================================================================
STATE_DIR=".team-claude/state"
STATE_FILE="${STATE_DIR}/workflow.json"

# 유효한 phase 목록
VALID_PHASES=("idle" "setup" "designing" "checkpoints_approved" "delegating" "merging" "completed")

# ============================================================================
# 사용법
# ============================================================================
usage() {
  cat << 'EOF'
Team Claude State - 워크플로우 상태 관리

사용법:
  tc-state <command> [options]

Commands:
  init                    상태 파일 초기화
  check                   현재 워크플로우 상태 표시
  get <key>               특정 값 조회 (phase, serverRunning 등)
  require <phase>         필요한 단계가 아니면 실패 (exit 1)
  transition <to>         상태 전이 (검증 포함)
  set-session <id>        현재 세션 ID 설정
  set-server <true|false> 서버 실행 상태 설정
  reset                   워크플로우 상태 초기화

Examples:
  tc-state init
  tc-state check
  tc-state require checkpoints_approved
  tc-state transition designing
  tc-state set-session abc12345
  tc-state set-server true
  tc-state reset
EOF
}

# ============================================================================
# 상태 파일 경로
# ============================================================================
get_state_path() {
  local root
  root=$(find_git_root)
  echo "${root}/${STATE_FILE}"
}

get_state_dir() {
  local root
  root=$(find_git_root)
  echo "${root}/${STATE_DIR}"
}

# ============================================================================
# init - 상태 파일 초기화
# ============================================================================
cmd_init() {
  require_jq

  local state_dir state_path
  state_dir=$(get_state_dir)
  state_path=$(get_state_path)

  ensure_dir "$state_dir"

  if [[ -f "$state_path" ]]; then
    warn "상태 파일이 이미 존재합니다: ${state_path}"
    warn "덮어쓰려면 'tc-state reset'을 먼저 실행하세요."
    return 0
  fi

  local now
  now=$(timestamp)

  cat > "$state_path" << EOF
{
  "phase": "idle",
  "serverRunning": false,
  "currentSessionId": null,
  "prerequisites": {
    "setup": false,
    "architect": false,
    "checkpointsApproved": false,
    "serverHealthy": false
  },
  "createdAt": "${now}",
  "updatedAt": "${now}"
}
EOF

  ok "상태 파일 초기화됨: ${state_path}"
}

# ============================================================================
# check - 현재 상태 표시
# ============================================================================
cmd_check() {
  require_jq

  local state_path
  state_path=$(get_state_path)

  if [[ ! -f "$state_path" ]]; then
    err "상태 파일이 없습니다."
    err "'tc-state init'을 먼저 실행하세요."
    exit 1
  fi

  echo ""
  echo "━━━ Team Claude Workflow State ━━━"
  echo ""

  local phase session server_running
  phase=$(jq -r '.phase' "$state_path")
  session=$(jq -r '.currentSessionId // "없음"' "$state_path")
  server_running=$(jq -r '.serverRunning' "$state_path")

  # Phase 표시 (이모지 포함)
  local phase_icon
  case "$phase" in
    idle) phase_icon="⏸️" ;;
    setup) phase_icon="🔧" ;;
    designing) phase_icon="🏗️" ;;
    checkpoints_approved) phase_icon="✅" ;;
    delegating) phase_icon="🚀" ;;
    merging) phase_icon="🔀" ;;
    completed) phase_icon="🎉" ;;
    *) phase_icon="❓" ;;
  esac

  echo "  Phase: ${phase_icon} ${phase}"
  echo "  Session: ${session}"
  echo "  Server: $([ "$server_running" = "true" ] && echo "🟢 실행 중" || echo "🔴 중지")"
  echo ""

  # Prerequisites 상태
  echo "━━━ Prerequisites ━━━"
  echo ""

  local prereqs
  prereqs=$(jq '.prerequisites' "$state_path")

  local setup architect checkpoints server_healthy
  setup=$(echo "$prereqs" | jq -r '.setup')
  architect=$(echo "$prereqs" | jq -r '.architect')
  checkpoints=$(echo "$prereqs" | jq -r '.checkpointsApproved')
  server_healthy=$(echo "$prereqs" | jq -r '.serverHealthy')

  echo "  $([ "$setup" = "true" ] && echo "✅" || echo "⬜") setup"
  echo "  $([ "$architect" = "true" ] && echo "✅" || echo "⬜") architect"
  echo "  $([ "$checkpoints" = "true" ] && echo "✅" || echo "⬜") checkpointsApproved"
  echo "  $([ "$server_healthy" = "true" ] && echo "✅" || echo "⬜") serverHealthy"
  echo ""

  # JSON도 출력 (파싱용)
  cat "$state_path"
}

# ============================================================================
# get - 특정 값 조회
# ============================================================================
cmd_get() {
  require_jq

  local key="${1:-}"

  if [[ -z "$key" ]]; then
    err "키를 지정하세요."
    err "사용법: tc-state get <key>"
    err "예: tc-state get phase"
    exit 1
  fi

  local state_path
  state_path=$(get_state_path)

  if [[ ! -f "$state_path" ]]; then
    err "상태 파일이 없습니다."
    exit 1
  fi

  local value
  value=$(jq -r ".${key}" "$state_path")

  if [[ "$value" == "null" ]]; then
    err "키를 찾을 수 없습니다: ${key}"
    exit 1
  fi

  echo "$value"
}

# ============================================================================
# require - 필요한 단계가 아니면 실패
# ============================================================================
cmd_require() {
  require_jq

  local required_phase="${1:-}"

  if [[ -z "$required_phase" ]]; then
    err "필요한 phase를 지정하세요."
    err "사용법: tc-state require <phase>"
    exit 1
  fi

  local state_path
  state_path=$(get_state_path)

  if [[ ! -f "$state_path" ]]; then
    err "상태 파일이 없습니다."
    err "'/team-claude:setup'을 먼저 실행하세요."
    exit 1
  fi

  local current_phase
  current_phase=$(jq -r '.phase' "$state_path")

  # phase 순서 매핑
  local -A phase_order
  phase_order=([idle]=0 [setup]=1 [designing]=2 [checkpoints_approved]=3 [delegating]=4 [merging]=5 [completed]=6)

  local required_order current_order
  required_order=${phase_order[$required_phase]:-999}
  current_order=${phase_order[$current_phase]:-0}

  if [[ $current_order -lt $required_order ]]; then
    err "필요한 단계: ${required_phase}"
    err "현재 단계: ${current_phase}"
    echo ""

    # 어떤 단계가 필요한지 안내
    case "$required_phase" in
      setup)
        err "'/team-claude:setup'을 먼저 실행하세요."
        ;;
      designing)
        err "'/team-claude:architect'로 설계를 시작하세요."
        ;;
      checkpoints_approved)
        err "'/team-claude:architect'에서 Checkpoint를 승인하세요."
        ;;
      delegating)
        err "'/team-claude:delegate'로 구현을 위임하세요."
        ;;
      merging)
        err "'/team-claude:merge'로 병합을 시작하세요."
        ;;
    esac

    exit 1
  fi

  ok "Phase 확인됨: ${current_phase} >= ${required_phase}"
}

# ============================================================================
# transition - 상태 전이
# ============================================================================
cmd_transition() {
  require_jq

  local to_phase="${1:-}"

  if [[ -z "$to_phase" ]]; then
    err "전이할 phase를 지정하세요."
    err "사용법: tc-state transition <phase>"
    err "유효한 phases: ${VALID_PHASES[*]}"
    exit 1
  fi

  # 유효한 phase인지 확인
  local valid=false
  for p in "${VALID_PHASES[@]}"; do
    if [[ "$p" == "$to_phase" ]]; then
      valid=true
      break
    fi
  done

  if [[ "$valid" != "true" ]]; then
    err "유효하지 않은 phase: ${to_phase}"
    err "유효한 phases: ${VALID_PHASES[*]}"
    exit 1
  fi

  local state_path
  state_path=$(get_state_path)

  if [[ ! -f "$state_path" ]]; then
    err "상태 파일이 없습니다."
    err "'tc-state init'을 먼저 실행하세요."
    exit 1
  fi

  local from_phase now
  from_phase=$(jq -r '.phase' "$state_path")
  now=$(timestamp)

  # 상태 업데이트
  jq --arg phase "$to_phase" --arg now "$now" \
    '.phase = $phase | .updatedAt = $now' "$state_path" > "${state_path}.tmp"
  mv "${state_path}.tmp" "$state_path"

  # prerequisites 업데이트
  case "$to_phase" in
    setup)
      jq '.prerequisites.setup = true' "$state_path" > "${state_path}.tmp"
      mv "${state_path}.tmp" "$state_path"
      ;;
    designing)
      jq '.prerequisites.architect = true' "$state_path" > "${state_path}.tmp"
      mv "${state_path}.tmp" "$state_path"
      ;;
    checkpoints_approved)
      jq '.prerequisites.checkpointsApproved = true' "$state_path" > "${state_path}.tmp"
      mv "${state_path}.tmp" "$state_path"
      ;;
  esac

  ok "상태 전이: ${from_phase} → ${to_phase}"
}

# ============================================================================
# set-session - 현재 세션 ID 설정
# ============================================================================
cmd_set_session() {
  require_jq

  local session_id="${1:-}"

  if [[ -z "$session_id" ]]; then
    err "세션 ID를 지정하세요."
    exit 1
  fi

  local state_path now
  state_path=$(get_state_path)
  now=$(timestamp)

  if [[ ! -f "$state_path" ]]; then
    err "상태 파일이 없습니다."
    exit 1
  fi

  jq --arg id "$session_id" --arg now "$now" \
    '.currentSessionId = $id | .updatedAt = $now' "$state_path" > "${state_path}.tmp"
  mv "${state_path}.tmp" "$state_path"

  ok "현재 세션 설정됨: ${session_id}"
}

# ============================================================================
# set-server - 서버 실행 상태 설정
# ============================================================================
cmd_set_server() {
  require_jq

  local running="${1:-}"

  if [[ "$running" != "true" && "$running" != "false" ]]; then
    err "true 또는 false를 지정하세요."
    exit 1
  fi

  local state_path now
  state_path=$(get_state_path)
  now=$(timestamp)

  if [[ ! -f "$state_path" ]]; then
    err "상태 파일이 없습니다."
    exit 1
  fi

  local bool_val
  bool_val=$([ "$running" = "true" ] && echo "true" || echo "false")

  jq --argjson running "$bool_val" --arg now "$now" \
    '.serverRunning = $running | .prerequisites.serverHealthy = $running | .updatedAt = $now' \
    "$state_path" > "${state_path}.tmp"
  mv "${state_path}.tmp" "$state_path"

  ok "서버 상태 설정됨: ${running}"
}

# ============================================================================
# reset - 상태 초기화
# ============================================================================
cmd_reset() {
  require_jq

  local state_path
  state_path=$(get_state_path)

  if [[ -f "$state_path" ]]; then
    rm -f "$state_path"
    ok "상태 파일 삭제됨"
  fi

  cmd_init
}

# ============================================================================
# 메인
# ============================================================================
main() {
  local command="${1:-}"
  shift || true

  case "$command" in
    init)
      cmd_init "$@"
      ;;
    check)
      cmd_check "$@"
      ;;
    get)
      cmd_get "$@"
      ;;
    require)
      cmd_require "$@"
      ;;
    transition)
      cmd_transition "$@"
      ;;
    set-session)
      cmd_set_session "$@"
      ;;
    set-server)
      cmd_set_server "$@"
      ;;
    reset)
      cmd_reset "$@"
      ;;
    -h|--help|help|"")
      usage
      ;;
    *)
      err "알 수 없는 명령어: ${command}"
      usage
      exit 1
      ;;
  esac
}

main "$@"
