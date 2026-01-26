#!/bin/bash
# Team Claude - Configuration Management
# 설정 파일 관리 스크립트

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"

# ============================================================================
# 사용법
# ============================================================================
usage() {
  cat << 'EOF'
Team Claude Config - 설정 관리

사용법:
  tc-config <command> [options]

Commands:
  init                    기본 설정 파일 생성
  get <path>              YAML 경로 값 읽기
  set <path> <value>      YAML 경로 값 쓰기
  show                    전체 설정 출력
  path                    설정 파일 경로 출력
  verify                  환경 설정 검증

Examples:
  tc-config init
  tc-config get project.name
  tc-config set feedback_loop.mode auto
  tc-config show
  tc-config verify
EOF
}

# ============================================================================
# init - 기본 설정 파일 생성
# ============================================================================
cmd_init() {
  require_yq
  local root
  root=$(find_git_root)
  local config_path="${root}/${CONFIG_FILE}"

  if [[ -f "$config_path" ]]; then
    err "설정 파일이 이미 존재합니다: ${config_path}"
    err "덮어쓰려면 파일을 삭제 후 다시 실행하세요."
    exit 1
  fi

  # .claude 디렉토리 생성
  ensure_dir "$(dirname "$config_path")"

  # 프로젝트 이름 추출 (디렉토리 이름)
  local project_name
  project_name=$(basename "$root")

  # 기본 설정 파일 생성
  cat > "$config_path" << EOF
version: "1.0"

project:
  name: "${project_name}"
  language: ""
  framework: ""
  domain: ""
  test_command: ""
  build_command: ""
  lint_command: ""

feedback_loop:
  mode: auto
  max_iterations: 5
  auto_retry_delay: 5000

validation:
  method: test
  timeout: 120000

notification:
  method: system
  slack:
    webhook_url: ""
    channel: ""

server:
  port: 7890
  executor: iterm

agents:
  enabled:
    - spec_validator
    - test_oracle
    - impl_reviewer
  custom: []
  overrides: {}
EOF

  ok "설정 파일 생성됨: ${config_path}"

  # .team-claude 디렉토리 구조 생성
  ensure_dir "${root}/.team-claude/sessions"
  ensure_dir "${root}/.team-claude/state"
  ensure_dir "${root}/.team-claude/hooks"
  ensure_dir "${root}/.team-claude/templates"
  ensure_dir "${root}/.team-claude/agents"

  ok ".team-claude 디렉토리 구조 생성됨"

  # hooks 스크립트 복사
  local plugin_hooks_dir="${SCRIPT_DIR}/../hooks/scripts"
  if [[ -d "$plugin_hooks_dir" ]]; then
    cp -r "${plugin_hooks_dir}/"* "${root}/.team-claude/hooks/" 2>/dev/null || true
    chmod +x "${root}/.team-claude/hooks/"*.sh 2>/dev/null || true
    ok "Hook 스크립트 복사됨"
  else
    warn "Hook 스크립트 소스 디렉토리를 찾을 수 없습니다: ${plugin_hooks_dir}"
  fi

  # 환경 검증 실행
  cmd_verify || true
}

# ============================================================================
# get - YAML 경로 값 읽기
# ============================================================================
cmd_get() {
  require_yq
  local path="${1:-}"

  if [[ -z "$path" ]]; then
    err "경로를 지정하세요."
    err "사용법: tc-config get <path>"
    exit 1
  fi

  local root
  root=$(find_git_root)
  local config_path="${root}/${CONFIG_FILE}"

  if [[ ! -f "$config_path" ]]; then
    err "설정 파일이 없습니다: ${config_path}"
    err "먼저 'tc-config init'을 실행하세요."
    exit 1
  fi

  # yq로 값 읽기 (. 구분자를 사용)
  local value
  value=$(yq eval ".${path}" "$config_path")

  if [[ "$value" == "null" ]]; then
    err "경로를 찾을 수 없습니다: ${path}"
    exit 1
  fi

  echo "$value"
}

# ============================================================================
# set - YAML 경로 값 쓰기
# ============================================================================
cmd_set() {
  require_yq
  local path="${1:-}"
  local value="${2:-}"

  if [[ -z "$path" || -z "$value" ]]; then
    err "경로와 값을 지정하세요."
    err "사용법: tc-config set <path> <value>"
    exit 1
  fi

  local root
  root=$(find_git_root)
  local config_path="${root}/${CONFIG_FILE}"

  if [[ ! -f "$config_path" ]]; then
    err "설정 파일이 없습니다: ${config_path}"
    err "먼저 'tc-config init'을 실행하세요."
    exit 1
  fi

  # yq로 값 쓰기 (in-place)
  yq eval -i ".${path} = \"${value}\"" "$config_path"

  ok "설정 변경됨: ${path} = ${value}"
}

# ============================================================================
# show - 전체 설정 출력
# ============================================================================
cmd_show() {
  require_yq
  local root
  root=$(find_git_root)
  local config_path="${root}/${CONFIG_FILE}"

  if [[ ! -f "$config_path" ]]; then
    err "설정 파일이 없습니다: ${config_path}"
    err "먼저 'tc-config init'을 실행하세요."
    exit 1
  fi

  cat "$config_path"
}

# ============================================================================
# path - 설정 파일 경로 출력
# ============================================================================
cmd_path() {
  local root
  root=$(find_git_root)
  echo "${root}/${CONFIG_FILE}"
}

# ============================================================================
# verify - 환경 설정 검증
# ============================================================================
cmd_verify() {
  local root
  root=$(find_git_root)
  local errors=0
  local warnings=0

  echo ""
  echo "━━━ Team Claude 환경 검증 ━━━"
  echo ""

  # --- 1. 설정 파일 검증 ---
  echo "📁 설정 파일"
  if [[ -f "${root}/${CONFIG_FILE}" ]]; then
    echo -e "  \033[0;32m✓\033[0m ${CONFIG_FILE}"
  else
    echo -e "  \033[0;31m✗\033[0m ${CONFIG_FILE} (누락)"
    ((errors++))
  fi
  echo ""

  # --- 2. 디렉토리 구조 검증 ---
  echo "📂 디렉토리 구조"
  local dirs=("sessions" "state" "hooks" "templates" "agents")
  for dir in "${dirs[@]}"; do
    if [[ -d "${root}/.team-claude/${dir}" ]]; then
      echo -e "  \033[0;32m✓\033[0m .team-claude/${dir}"
    else
      echo -e "  \033[0;31m✗\033[0m .team-claude/${dir} (누락)"
      ((errors++))
    fi
  done
  echo ""

  # --- 3. Hook 스크립트 검증 ---
  echo "🪝 Hook 스크립트"
  local hooks=("on-worker-complete.sh" "on-validation-complete.sh" "on-worker-question.sh" "on-worker-idle.sh")
  for hook in "${hooks[@]}"; do
    local hook_path="${root}/.team-claude/hooks/${hook}"
    if [[ -f "$hook_path" ]]; then
      if [[ -x "$hook_path" ]]; then
        echo -e "  \033[0;32m✓\033[0m ${hook}"
      else
        echo -e "  \033[0;33m⚠\033[0m ${hook} (실행 권한 없음)"
        ((warnings++))
      fi
    else
      echo -e "  \033[0;31m✗\033[0m ${hook} (누락)"
      ((errors++))
    fi
  done
  echo ""

  # --- 4. 의존성 검증 ---
  echo "🔧 의존성"
  local deps=("yq" "jq" "git" "bun")
  for dep in "${deps[@]}"; do
    if command -v "$dep" &>/dev/null; then
      local version
      case "$dep" in
        yq)  version=$(yq --version 2>/dev/null | head -1) ;;
        jq)  version=$(jq --version 2>/dev/null) ;;
        git) version=$(git --version 2>/dev/null | sed 's/git version //') ;;
        bun) version=$(bun --version 2>/dev/null) ;;
      esac
      echo -e "  \033[0;32m✓\033[0m ${dep} (${version})"
    else
      if [[ "$dep" == "bun" ]]; then
        echo -e "  \033[0;33m⚠\033[0m ${dep} (미설치 - 서버 빌드에 필요)"
        ((warnings++))
      else
        echo -e "  \033[0;31m✗\033[0m ${dep} (미설치)"
        ((errors++))
      fi
    fi
  done
  echo ""

  # --- 5. 서버 바이너리 검증 ---
  echo "🖥️  서버"
  local server_path="${HOME}/.claude/team-claude-server"
  if [[ -f "$server_path" ]]; then
    if [[ -x "$server_path" ]]; then
      echo -e "  \033[0;32m✓\033[0m team-claude-server"
    else
      echo -e "  \033[0;33m⚠\033[0m team-claude-server (실행 권한 없음)"
      ((warnings++))
    fi
  else
    echo -e "  \033[0;33m⚠\033[0m team-claude-server (미설치 - tc-server.sh install 실행 필요)"
    ((warnings++))
  fi
  echo ""

  # --- 결과 요약 ---
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  if [[ $errors -eq 0 && $warnings -eq 0 ]]; then
    echo -e "\033[0;32m✓ 모든 검증 통과\033[0m"
  elif [[ $errors -eq 0 ]]; then
    echo -e "\033[0;33m⚠ 경고 ${warnings}개 (선택적 항목)\033[0m"
  else
    echo -e "\033[0;31m✗ 오류 ${errors}개, 경고 ${warnings}개\033[0m"
  fi
  echo ""

  # 에러가 있으면 exit 1, 경고만 있으면 exit 0
  if [[ $errors -gt 0 ]]; then
    return 1
  fi
  return 0
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
    get)
      cmd_get "$@"
      ;;
    set)
      cmd_set "$@"
      ;;
    show)
      cmd_show "$@"
      ;;
    path)
      cmd_path "$@"
      ;;
    verify)
      cmd_verify "$@"
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
