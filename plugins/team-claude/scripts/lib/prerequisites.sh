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

# ============================================================================
# 전체 인프라 진단 (Setup용)
# ============================================================================

# 전체 인프라 상태를 JSON으로 반환
check_infrastructure() {
  local result=()
  local all_ok=true

  # 1. CLI 도구 의존성
  local missing_deps=()
  command -v yq &>/dev/null || missing_deps+=("yq")
  command -v jq &>/dev/null || missing_deps+=("jq")
  command -v git &>/dev/null || missing_deps+=("git")
  command -v curl &>/dev/null || missing_deps+=("curl")
  command -v bun &>/dev/null || missing_deps+=("bun")

  if [[ ${#missing_deps[@]} -eq 0 ]]; then
    result+=("\"dependencies\": {\"status\": \"ok\", \"missing\": []}")
  else
    result+=("\"dependencies\": {\"status\": \"missing\", \"missing\": [$(printf '"%s",' "${missing_deps[@]}" | sed 's/,$//')]}")
    all_ok=false
  fi

  # 2. 서버 바이너리
  if prereq_server_binary_exists; then
    result+=("\"server_binary\": {\"status\": \"ok\", \"path\": \"${HOME}/.claude/team-claude-server\"}")
  else
    result+=("\"server_binary\": {\"status\": \"missing\", \"path\": \"${HOME}/.claude/team-claude-server\"}")
    all_ok=false
  fi

  # 3. 서버 실행 상태
  local port
  port=$(get_server_port 2>/dev/null || echo "7890")
  if prereq_server_healthy "$port"; then
    result+=("\"server_running\": {\"status\": \"ok\", \"port\": ${port}}")
  else
    result+=("\"server_running\": {\"status\": \"stopped\", \"port\": ${port}}")
    # 서버가 꺼져있어도 일단 warning 수준 (자동 시작 가능)
  fi

  # 4. 플랫폼 체크 (iTerm2 또는 headless)
  local platform_info
  if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS - iTerm2 확인
    if [[ -d "/Applications/iTerm.app" ]]; then
      result+=("\"platform\": {\"os\": \"macos\", \"terminal\": \"iterm\", \"status\": \"ok\"}")
    else
      result+=("\"platform\": {\"os\": \"macos\", \"terminal\": \"headless\", \"status\": \"fallback\", \"note\": \"iTerm2 not found, using headless mode\"}")
    fi
  else
    # Linux/Other
    result+=("\"platform\": {\"os\": \"linux\", \"terminal\": \"headless\", \"status\": \"ok\"}")
  fi

  # 5. 설정 파일
  if prereq_config_exists; then
    result+=("\"config\": {\"status\": \"ok\"}")
  else
    result+=("\"config\": {\"status\": \"missing\"}")
    # 설정은 setup에서 생성하면 됨
  fi

  # 6. 상태 파일
  if prereq_state_exists; then
    result+=("\"state\": {\"status\": \"ok\"}")
  else
    result+=("\"state\": {\"status\": \"missing\"}")
    # 상태는 setup에서 생성하면 됨
  fi

  # 전체 상태
  local overall="ok"
  if [[ "$all_ok" == "false" ]]; then
    overall="needs_setup"
  fi

  # JSON 출력
  echo "{"
  echo "  \"overall\": \"${overall}\","
  echo "  $(printf '%s,\n  ' "${result[@]}" | sed '$ s/,$//')"
  echo "}"
}

# 인프라 상태를 사람이 읽을 수 있는 형태로 출력
print_infrastructure_status() {
  echo ""
  echo "╔═══════════════════════════════════════════════════════════════╗"
  echo "║              Team Claude Infrastructure Check                   ║"
  echo "╚═══════════════════════════════════════════════════════════════╝"
  echo ""

  # 1. CLI 도구 의존성
  echo "━━━ 1. CLI Dependencies ━━━"
  local deps=("yq" "jq" "git" "curl" "bun")
  local all_deps_ok=true
  for dep in "${deps[@]}"; do
    if command -v "$dep" &>/dev/null; then
      local version
      case "$dep" in
        yq)   version=$(yq --version 2>/dev/null | head -1) ;;
        jq)   version=$(jq --version 2>/dev/null) ;;
        git)  version=$(git --version 2>/dev/null) ;;
        curl) version=$(curl --version 2>/dev/null | head -1) ;;
        bun)  version=$(bun --version 2>/dev/null) ;;
      esac
      ok "  ${dep}: ${version}"
    else
      err "  ${dep}: 미설치"
      all_deps_ok=false
    fi
  done
  echo ""

  # 2. 서버 바이너리
  echo "━━━ 2. Server Binary ━━━"
  if prereq_server_binary_exists; then
    ok "  Binary: ~/.claude/team-claude-server"
  else
    err "  Binary: 미설치"
    info "  → 해결: tc-server install"
  fi
  echo ""

  # 3. 서버 상태
  echo "━━━ 3. Server Status ━━━"
  local port
  port=$(get_server_port 2>/dev/null || echo "7890")
  if prereq_server_healthy "$port"; then
    ok "  Server: http://localhost:${port} (healthy)"
  else
    warn "  Server: 중지됨 (port ${port})"
    info "  → 해결: tc-server start"
  fi
  echo ""

  # 4. 플랫폼
  echo "━━━ 4. Platform & Terminal ━━━"
  if [[ "$OSTYPE" == "darwin"* ]]; then
    ok "  OS: macOS"
    if [[ -d "/Applications/iTerm.app" ]]; then
      ok "  Terminal: iTerm2 (recommended)"
    else
      warn "  Terminal: iTerm2 미설치 (headless 모드로 동작)"
      info "  → 권장: brew install --cask iterm2"
    fi
  else
    ok "  OS: Linux"
    ok "  Terminal: Headless mode"
  fi
  echo ""

  # 5. 설정 상태
  echo "━━━ 5. Configuration ━━━"
  if prereq_config_exists; then
    ok "  Config: .claude/team-claude.yaml"
  else
    warn "  Config: 미생성"
    info "  → 해결: /team-claude:setup"
  fi

  if prereq_state_exists; then
    ok "  State: .team-claude/state/workflow.json"
  else
    warn "  State: 미생성"
    info "  → 해결: /team-claude:setup"
  fi
  echo ""

  # 종합 결과
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  if [[ "$all_deps_ok" == "true" ]] && prereq_server_binary_exists; then
    ok "✅ 인프라 준비 완료"
    echo ""
    echo "다음 단계:"
    if ! prereq_server_healthy "$port"; then
      echo "  1. tc-server start     # 서버 시작"
    fi
    if ! prereq_config_exists; then
      echo "  2. /team-claude:setup  # 설정 초기화"
    fi
  else
    err "❌ 인프라 설정 필요"
    echo ""
    echo "누락된 항목을 먼저 설치하세요."
  fi
  echo ""
}

# 빠른 인프라 체크 (delegate 전 호출용)
quick_infrastructure_check() {
  local errors=()

  # 필수 의존성만 체크
  command -v jq &>/dev/null || errors+=("jq 미설치")
  command -v git &>/dev/null || errors+=("git 미설치")

  # 서버 바이너리
  prereq_server_binary_exists || errors+=("서버 미설치 (tc-server install)")

  if [[ ${#errors[@]} -gt 0 ]]; then
    err "인프라 체크 실패:"
    for e in "${errors[@]}"; do
      err "  • $e"
    done
    return 1
  fi

  return 0
}
