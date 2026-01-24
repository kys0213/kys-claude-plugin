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
