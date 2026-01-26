#!/bin/bash
# Team Claude - Common Library
# 공통 상수 및 유틸리티 함수

set -euo pipefail

# ============================================================================
# 경로 상수
# ============================================================================
CONFIG_FILE=".claude/team-claude.yaml"
SESSIONS_DIR=".team-claude/sessions"
WORKTREES_DIR=".team-claude/worktrees"
SESSIONS_INDEX="${SESSIONS_DIR}/index.json"

# ============================================================================
# 서버 상수
# ============================================================================
TC_SERVER_DEFAULT_PORT=7890
TC_SERVER_BINARY="${HOME}/.claude/team-claude-server"
TC_SERVER_PID_FILE="${HOME}/.claude/team-claude-server.pid"
TC_SERVER_LOG_FILE="${HOME}/.claude/team-claude-server.log"

# 서버 포트 가져오기 (설정 파일에서 또는 기본값)
get_server_port() {
  # 설정 파일에서 포트 읽기 시도
  if [[ -f "$CONFIG_FILE" ]] && command -v yq &>/dev/null; then
    local port
    port=$(yq -r '.server.port // empty' "$CONFIG_FILE" 2>/dev/null || echo "")
    if [[ -n "$port" && "$port" != "null" ]]; then
      echo "$port"
      return
    fi
  fi
  echo "$TC_SERVER_DEFAULT_PORT"
}

# 서버 URL 가져오기
get_server_url() {
  local port
  port=$(get_server_port)
  echo "http://localhost:${port}"
}

# ============================================================================
# 색상 출력
# ============================================================================
info()  { echo -e "\033[0;34m[INFO]\033[0m $*"; }
ok()    { echo -e "\033[0;32m[OK]\033[0m $*"; }
warn()  { echo -e "\033[0;33m[WARN]\033[0m $*"; }
err()   { echo -e "\033[0;31m[ERR]\033[0m $*" >&2; }

# ============================================================================
# 의존성 확인
# ============================================================================
require_yq() {
  if ! command -v yq &>/dev/null; then
    err "yq가 설치되어 있지 않습니다."
    err "설치: brew install yq"
    exit 1
  fi
}

require_jq() {
  if ! command -v jq &>/dev/null; then
    err "jq가 설치되어 있지 않습니다."
    err "설치: brew install jq"
    exit 1
  fi
}

require_git() {
  if ! command -v git &>/dev/null; then
    err "git이 설치되어 있지 않습니다."
    exit 1
  fi
}

# ============================================================================
# 의존성 확인 및 설치 (setup용)
# ============================================================================

# 의존성 상태 확인 (exit 없이)
# 반환: 0=모두 설치됨, 1=일부 미설치
check_dependencies() {
  local missing=()

  command -v yq &>/dev/null || missing+=("yq")
  command -v jq &>/dev/null || missing+=("jq")
  command -v git &>/dev/null || missing+=("git")

  if [[ ${#missing[@]} -eq 0 ]]; then
    return 0
  else
    echo "${missing[*]}"
    return 1
  fi
}

# 의존성 설치 (brew 사용)
install_dependency() {
  local dep="$1"

  if ! command -v brew &>/dev/null; then
    err "Homebrew가 설치되어 있지 않습니다."
    err "https://brew.sh 에서 설치 후 다시 시도하세요."
    return 1
  fi

  info "${dep} 설치 중..."
  if brew install "$dep"; then
    ok "${dep} 설치 완료"
    return 0
  else
    err "${dep} 설치 실패"
    return 1
  fi
}

# 모든 누락된 의존성 설치
install_all_dependencies() {
  local missing
  if missing=$(check_dependencies); then
    ok "모든 의존성이 설치되어 있습니다."
    return 0
  fi

  for dep in $missing; do
    if [[ "$dep" == "git" ]]; then
      err "git은 수동으로 설치해야 합니다."
      err "Xcode Command Line Tools: xcode-select --install"
      continue
    fi
    install_dependency "$dep" || return 1
  done

  return 0
}

# 의존성 상태 출력 (human readable)
print_dependency_status() {
  echo "━━━ 의존성 상태 ━━━"
  echo

  if command -v yq &>/dev/null; then
    ok "yq: $(yq --version 2>/dev/null | head -1)"
  else
    err "yq: 미설치"
  fi

  if command -v jq &>/dev/null; then
    ok "jq: $(jq --version 2>/dev/null)"
  else
    err "jq: 미설치"
  fi

  if command -v git &>/dev/null; then
    ok "git: $(git --version 2>/dev/null)"
  else
    err "git: 미설치"
  fi

  echo
}

# ============================================================================
# 유틸리티 함수
# ============================================================================

# Git 루트 디렉토리 찾기
find_git_root() {
  git rev-parse --show-toplevel 2>/dev/null || {
    err "Git 저장소가 아닙니다."
    exit 1
  }
}

# 8자리 랜덤 ID 생성
generate_id() {
  LC_ALL=C tr -dc 'a-z0-9' < /dev/urandom | head -c 8 || true
}

# ISO 8601 타임스탬프 생성
timestamp() {
  date -u +"%Y-%m-%dT%H:%M:%SZ"
}

# 설정 파일 존재 확인
config_exists() {
  local root
  root=$(find_git_root)
  [[ -f "${root}/${CONFIG_FILE}" ]]
}

# 세션 디렉토리 존재 확인
session_exists() {
  local session_id="$1"
  local root
  root=$(find_git_root)
  [[ -d "${root}/${SESSIONS_DIR}/${session_id}" ]]
}

# 디렉토리 안전 생성
ensure_dir() {
  local dir="$1"
  if [[ ! -d "$dir" ]]; then
    mkdir -p "$dir"
  fi
}

# JSON 값 안전하게 출력 (null 처리)
json_value_or_default() {
  local value="$1"
  local default="${2:-}"
  if [[ "$value" == "null" || -z "$value" ]]; then
    echo "$default"
  else
    echo "$value"
  fi
}
