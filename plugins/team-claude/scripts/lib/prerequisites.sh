#!/bin/bash
# Team Claude - Prerequisites Check Library
# 전제조건 체크 함수들

# 이 스크립트는 source로 불러와야 합니다.
# source ./lib/prerequisites.sh

# common.sh가 이미 로드되지 않았다면 로드
if [[ -z "${COMMON_SH_LOADED:-}" ]]; then
  SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  source "${SCRIPT_DIR}/common.sh"
fi

# ============================================================================
# 전제조건 체크 함수들
# ============================================================================

# 설정 파일 존재 확인
prereq_config_exists() {
  local root
  root=$(find_git_root 2>/dev/null) || return 1
  [[ -f "${root}/${CONFIG_FILE}" ]]
}

# 상태 파일 존재 확인
prereq_state_exists() {
  local root
  root=$(find_git_root 2>/dev/null) || return 1
  [[ -f "${root}/.team-claude/state/workflow.json" ]]
}

# 서버가 healthy한지 확인
prereq_server_healthy() {
  local port="${1:-7890}"
  local response
  response=$(curl -s -o /dev/null -w '%{http_code}' "http://localhost:${port}/health" 2>/dev/null)
  [[ "$response" == "200" ]]
}

# 세션이 존재하는지 확인
prereq_session_exists() {
  local session_id="${1:-}"
  [[ -z "$session_id" ]] && return 1

  local root
  root=$(find_git_root 2>/dev/null) || return 1
  [[ -d "${root}/.team-claude/sessions/${session_id}" ]]
}

# Checkpoint가 승인되었는지 확인
prereq_checkpoints_approved() {
  local session_id="${1:-}"
  [[ -z "$session_id" ]] && return 1

  local root
  root=$(find_git_root 2>/dev/null) || return 1
  local meta_path="${root}/.team-claude/sessions/${session_id}/meta.json"

  [[ -f "$meta_path" ]] || return 1

  local approved
  approved=$(jq -r '.checkpointsApproved // false' "$meta_path" 2>/dev/null)
  [[ "$approved" == "true" ]]
}

# 현재 phase가 특정 phase 이상인지 확인
prereq_phase_at_least() {
  local required_phase="${1:-}"
  local script_dir="${2:-./plugins/team-claude/scripts}"

  "${script_dir}/tc-state.sh" require "$required_phase" 2>/dev/null
}

# bun이 설치되어 있는지 확인
prereq_bun_installed() {
  command -v bun &>/dev/null
}

# 서버 바이너리가 존재하는지 확인
prereq_server_binary_exists() {
  [[ -f "${HOME}/.claude/team-claude-server" ]]
}

# ============================================================================
# 복합 전제조건 체크
# ============================================================================

# delegate 실행 전 모든 전제조건 확인
check_delegate_prerequisites() {
  local session_id="${1:-}"
  local script_dir="${2:-./plugins/team-claude/scripts}"
  local errors=()

  # 1. 설정 파일 존재
  if ! prereq_config_exists; then
    errors+=("설정 파일이 없습니다. '/team-claude:setup'을 실행하세요.")
  fi

  # 2. 상태 파일 존재
  if ! prereq_state_exists; then
    errors+=("상태 파일이 없습니다. '/team-claude:setup'을 실행하세요.")
  fi

  # 3. 세션 존재
  if [[ -n "$session_id" ]] && ! prereq_session_exists "$session_id"; then
    errors+=("세션을 찾을 수 없습니다: ${session_id}")
  fi

  # 4. Checkpoint 승인
  if [[ -n "$session_id" ]] && ! prereq_checkpoints_approved "$session_id"; then
    errors+=("Checkpoint가 승인되지 않았습니다. '/team-claude:architect --resume ${session_id}'에서 승인하세요.")
  fi

  # 5. 서버 healthy
  if ! prereq_server_healthy; then
    errors+=("서버가 실행 중이지 않습니다. 'tc-server ensure'를 실행합니다...")
  fi

  # 에러가 있으면 출력
  if [[ ${#errors[@]} -gt 0 ]]; then
    echo ""
    err "━━━ Prerequisites 검사 실패 ━━━"
    echo ""
    for e in "${errors[@]}"; do
      err "  • $e"
    done
    echo ""
    return 1
  fi

  return 0
}

# architect 실행 전 전제조건 확인
check_architect_prerequisites() {
  local script_dir="${1:-./plugins/team-claude/scripts}"
  local errors=()

  # 1. 설정 파일 존재
  if ! prereq_config_exists; then
    errors+=("설정 파일이 없습니다. '/team-claude:setup'을 실행하세요.")
  fi

  # 2. 상태 파일 존재
  if ! prereq_state_exists; then
    errors+=("상태 파일이 없습니다. '/team-claude:setup'을 실행하세요.")
  fi

  if [[ ${#errors[@]} -gt 0 ]]; then
    echo ""
    err "━━━ Prerequisites 검사 실패 ━━━"
    echo ""
    for e in "${errors[@]}"; do
      err "  • $e"
    done
    echo ""
    return 1
  fi

  return 0
}

# merge 실행 전 전제조건 확인
check_merge_prerequisites() {
  local session_id="${1:-}"
  local script_dir="${2:-./plugins/team-claude/scripts}"
  local errors=()

  # 1. 설정 파일 존재
  if ! prereq_config_exists; then
    errors+=("설정 파일이 없습니다.")
  fi

  # 2. 세션 존재
  if [[ -n "$session_id" ]] && ! prereq_session_exists "$session_id"; then
    errors+=("세션을 찾을 수 없습니다: ${session_id}")
  fi

  if [[ ${#errors[@]} -gt 0 ]]; then
    echo ""
    err "━━━ Prerequisites 검사 실패 ━━━"
    echo ""
    for e in "${errors[@]}"; do
      err "  • $e"
    done
    echo ""
    return 1
  fi

  return 0
}

# ============================================================================
# 전제조건 상태 출력
# ============================================================================

print_prerequisites_status() {
  local session_id="${1:-}"

  echo ""
  echo "━━━ Prerequisites Status ━━━"
  echo ""

  # 설정 파일
  if prereq_config_exists; then
    ok "  Config: .claude/team-claude.yaml"
  else
    err "  Config: 미생성"
  fi

  # 상태 파일
  if prereq_state_exists; then
    ok "  State: .team-claude/state/workflow.json"
  else
    err "  State: 미생성"
  fi

  # 서버 바이너리
  if prereq_server_binary_exists; then
    ok "  Server Binary: ~/.claude/team-claude-server"
  else
    err "  Server Binary: 미설치"
  fi

  # 서버 상태
  if prereq_server_healthy; then
    ok "  Server: Running (healthy)"
  else
    err "  Server: Not running"
  fi

  # 세션 (있는 경우)
  if [[ -n "$session_id" ]]; then
    if prereq_session_exists "$session_id"; then
      ok "  Session: ${session_id}"

      if prereq_checkpoints_approved "$session_id"; then
        ok "  Checkpoints: Approved"
      else
        err "  Checkpoints: Not approved"
      fi
    else
      err "  Session: Not found (${session_id})"
    fi
  fi

  echo ""
}
