#!/bin/bash
# Team Claude - Configuration Management
# 설정 파일 관리 스크립트

# ============================================================================
# DEPRECATED: This script is deprecated and will be removed in v1.0.0
# Use the tc CLI instead:
#   tc-config.sh init    →  tc config init / tc setup
#   tc-config.sh get     →  tc config get
#   tc-config.sh set     →  tc config set
#   tc-config.sh show    →  tc config show
#   tc-config.sh path    →  tc config path
#   tc-config.sh verify  →  tc config verify
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"

warn_deprecated() {
  echo "[DEPRECATED] ${1:-This script is deprecated}. Use 'tc ${2:-<command>}' instead." >&2
}

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
# setup_local_hooks - .claude/settings.local.json에 hooks 설정 추가
# ============================================================================
setup_local_hooks() {
  require_jq
  local root="$1"
  local settings_file="${root}/.claude/settings.local.json"

  # Team Claude hooks 정의
  local tc_hooks
  tc_hooks=$(cat << 'HOOKS_EOF'
{
  "hooks": {
    "Stop": [
      {
        "matcher": "",
        "description": "Worker 완료 시 자동 검증 트리거",
        "hooks": [
          {
            "type": "command",
            "command": "tc hook worker-complete",
            "timeout": 30
          }
        ]
      }
    ],
    "PreToolUse": [
      {
        "matcher": "Task",
        "description": "Worker 질문 시 에스컬레이션 (Task 도구 사용 시)",
        "hooks": [
          {
            "type": "command",
            "command": "tc hook worker-question",
            "timeout": 10
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Bash",
        "description": "Bash 실행 후 결과 분석 (test 명령어는 내부에서 필터링)",
        "hooks": [
          {
            "type": "command",
            "command": "tc hook validation-complete",
            "timeout": 60
          }
        ]
      }
    ],
    "Notification": [
      {
        "matcher": "idle_prompt",
        "description": "Worker 대기 상태 감지",
        "hooks": [
          {
            "type": "command",
            "command": "tc hook worker-idle",
            "timeout": 5
          }
        ]
      }
    ]
  }
}
HOOKS_EOF
)

  if [[ -f "$settings_file" ]]; then
    # 기존 settings.local.json이 있으면 hooks 병합
    local existing
    existing=$(cat "$settings_file")

    # 기존에 hooks가 있는지 확인
    if echo "$existing" | jq -e '.hooks' > /dev/null 2>&1; then
      # hooks가 있으면 병합 (기존 hooks 유지 + tc_hooks 추가)
      local merged
      merged=$(echo "$existing" | jq --argjson tc_hooks "$tc_hooks" '
        .hooks.Stop = ((.hooks.Stop // []) + $tc_hooks.hooks.Stop) |
        .hooks.PreToolUse = ((.hooks.PreToolUse // []) + $tc_hooks.hooks.PreToolUse) |
        .hooks.Notification = ((.hooks.Notification // []) + $tc_hooks.hooks.Notification)
      ')
      echo "$merged" > "$settings_file"
      ok "기존 settings.local.json에 hooks 병합됨"
    else
      # hooks가 없으면 추가
      local merged
      merged=$(echo "$existing" | jq --argjson tc_hooks "$tc_hooks" '. + $tc_hooks')
      echo "$merged" > "$settings_file"
      ok "settings.local.json에 hooks 추가됨"
    fi
  else
    # settings.local.json이 없으면 새로 생성
    echo "$tc_hooks" | jq '.' > "$settings_file"
    ok "settings.local.json 생성됨 (hooks 포함)"
  fi
}

# ============================================================================
# init - 기본 설정 파일 생성
# ============================================================================
cmd_init() {
  require_yq
  local root
  root=$(find_git_root)

  # 프로젝트 데이터 디렉토리 (~/.team-claude/{hash}/)
  local data_dir
  data_dir=$(get_project_data_dir)
  local config_path="${data_dir}/team-claude.yaml"
  local project_hash
  project_hash=$(get_project_hash)

  if [[ -f "$config_path" ]]; then
    err "설정 파일이 이미 존재합니다: ${config_path}"
    err "덮어쓰려면 파일을 삭제 후 다시 실행하세요."
    exit 1
  fi

  # ~/.team-claude/{hash}/ 디렉토리 구조 생성
  ensure_dir "${data_dir}"
  ensure_dir "${data_dir}/sessions"
  ensure_dir "${data_dir}/state"
  ensure_dir "${data_dir}/worktrees"

  # 프로젝트 이름 추출 (디렉토리 이름)
  local project_name
  project_name=$(basename "$root")

  # 기본 설정 파일 생성
  cat > "$config_path" << EOF
version: "1.0"

# 프로젝트 메타 (자동 생성)
_meta:
  project_root: "${root}"
  project_hash: "${project_hash}"

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
  info "프로젝트 해시: ${project_hash}"

  # .claude/agents 디렉토리 생성 (프로젝트 에이전트 정의)
  ensure_dir "${root}/.claude/agents"
  ok ".claude/agents 디렉토리 생성됨"

  # ============================================================================
  # Flow/PSM/HUD 초기화 (v0.5.0+)
  # ============================================================================

  # Flow 상태 초기화
  local state_dir="${data_dir}/state"
  local workflow_state="${state_dir}/workflow.json"
  if [[ ! -f "$workflow_state" ]]; then
    cat > "$workflow_state" << 'STATE_EOF'
{
  "currentSession": null,
  "lastUpdated": null,
  "status": "idle"
}
STATE_EOF
    ok "Flow 상태 초기화됨: ${workflow_state}"
  fi

  # PSM 인덱스 초기화
  local psm_index="${data_dir}/psm-index.json"
  if [[ ! -f "$psm_index" ]]; then
    cat > "$psm_index" << 'PSM_EOF'
{
  "sessions": [],
  "createdAt": null,
  "lastUpdated": null
}
PSM_EOF
    ok "PSM 인덱스 초기화됨: ${psm_index}"
  fi

  # Flow/PSM/Keywords 설정 추가 (team-claude.yaml에)
  if command -v yq &>/dev/null; then
    # flow 설정이 없으면 추가
    if [[ "$(yq eval '.flow' "$config_path")" == "null" ]]; then
      yq eval -i '.flow.defaultMode = "assisted"' "$config_path"
      yq eval -i '.flow.autoReview.enabled = true' "$config_path"
      yq eval -i '.flow.autoReview.maxIterations = 5' "$config_path"
      yq eval -i '.flow.escalation.onMaxIterations = true' "$config_path"
      yq eval -i '.flow.escalation.onConflict = true' "$config_path"
      ok "Flow 설정 추가됨"
    fi

    # psm 설정이 없으면 추가
    if [[ "$(yq eval '.psm' "$config_path")" == "null" ]]; then
      yq eval -i '.psm.parallelLimit = 4' "$config_path"
      yq eval -i '.psm.autoCleanup = true' "$config_path"
      yq eval -i '.psm.conflictCheck.enabled = true' "$config_path"
      yq eval -i '.psm.conflictCheck.action = "warn"' "$config_path"
      ok "PSM 설정 추가됨"
    fi

    # keywords 설정이 없으면 추가
    if [[ "$(yq eval '.keywords' "$config_path")" == "null" ]]; then
      yq eval -i '.keywords.enabled = true' "$config_path"
      yq eval -i '.keywords.aliases.auto = "autopilot"' "$config_path"
      yq eval -i '.keywords.aliases.ap = "autopilot"' "$config_path"
      yq eval -i '.keywords.aliases.sp = "spec"' "$config_path"
      yq eval -i '.keywords.aliases.im = "impl"' "$config_path"
      ok "Magic Keywords 설정 추가됨"
    fi

    # swarm 설정이 없으면 추가
    if [[ "$(yq eval '.swarm' "$config_path")" == "null" ]]; then
      yq eval -i '.swarm.enabled = true' "$config_path"
      yq eval -i '.swarm.maxParallel = 4' "$config_path"
      yq eval -i '.swarm.conflictCheck.enabled = true' "$config_path"
      yq eval -i '.swarm.conflictCheck.action = "warn"' "$config_path"
      ok "Swarm 설정 추가됨"
    fi
  fi

  # tc CLI 사용 안내 (더 이상 .sh 파일 복사하지 않음)
  info "Hook은 tc CLI를 통해 실행됩니다: tc hook <subcommand>"
  info "사용 가능: worker-complete, worker-idle, worker-question, validation-complete"

  # .claude/settings.local.json에 hooks 설정 추가
  setup_local_hooks "$root"

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

  local config_path
  config_path=$(get_config_path)

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

  local config_path
  config_path=$(get_config_path)

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
  local config_path
  config_path=$(get_config_path)

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
  get_config_path
}

# ============================================================================
# verify - 환경 설정 검증
# ============================================================================
cmd_verify() {
  local root
  root=$(find_git_root)
  local data_dir
  data_dir=$(get_project_data_dir)
  local config_path
  config_path=$(get_config_path)
  local project_hash
  project_hash=$(get_project_hash)
  local errors=0
  local warnings=0

  echo ""
  echo "━━━ Team Claude 환경 검증 ━━━"
  echo ""
  info "프로젝트: ${root}"
  info "해시: ${project_hash}"
  info "데이터: ${data_dir}"
  echo ""

  # --- 1. 설정 파일 검증 ---
  echo "📁 설정 파일"
  if [[ -f "$config_path" ]]; then
    echo -e "  \033[0;32m✓\033[0m ~/.team-claude/${project_hash}/team-claude.yaml"
  else
    echo -e "  \033[0;31m✗\033[0m ~/.team-claude/${project_hash}/team-claude.yaml (누락)"
    ((errors++))
  fi
  echo ""

  # --- 2. 전역 데이터 디렉토리 검증 (~/.team-claude/{hash}/) ---
  echo "📂 전역 데이터 (~/.team-claude/${project_hash}/)"
  local global_dirs=("sessions" "state" "worktrees")
  for dir in "${global_dirs[@]}"; do
    if [[ -d "${data_dir}/${dir}" ]]; then
      echo -e "  \033[0;32m✓\033[0m ${dir}"
    else
      echo -e "  \033[0;31m✗\033[0m ${dir} (누락)"
      ((errors++))
    fi
  done
  echo ""

  # --- 3. 프로젝트 디렉토리 검증 (.claude/) ---
  echo "📂 프로젝트 디렉토리 (.claude/)"

  if [[ -d "${root}/.claude/agents" ]]; then
    echo -e "  \033[0;32m✓\033[0m agents"
  else
    echo -e "  \033[0;33m⚠\033[0m agents (선택 - tc-agent init으로 생성)"
    ((warnings++))
  fi

  if [[ -d "${root}/.claude/hooks" ]]; then
    echo -e "  \033[0;32m✓\033[0m hooks"
  else
    echo -e "  \033[0;31m✗\033[0m hooks (누락)"
    ((errors++))
  fi
  echo ""

  # --- 4. tc CLI 검증 ---
  echo "🪝 tc hook CLI"
  if command -v tc &>/dev/null; then
    echo -e "  \033[0;32m✓\033[0m tc CLI 사용 가능"

    # tc hook 서브커맨드 확인
    local hook_cmds=("worker-complete" "worker-idle" "worker-question" "validation-complete")
    for cmd in "${hook_cmds[@]}"; do
      echo -e "  \033[0;32m✓\033[0m tc hook ${cmd}"
    done
  else
    echo -e "  \033[0;31m✗\033[0m tc CLI 미설치"
    echo -e "  \033[0;33m→\033[0m tc CLI 빌드: cd plugins/team-claude/cli && bun run build"
    ((errors++))
  fi

  # 레거시 .sh 스크립트 경고 (있으면)
  if [[ -d "${root}/.claude/hooks" ]]; then
    local legacy_hooks=("on-worker-complete.sh" "on-validation-complete.sh" "on-worker-question.sh" "on-worker-idle.sh")
    local found_legacy=false
    for hook in "${legacy_hooks[@]}"; do
      if [[ -f "${root}/.claude/hooks/${hook}" ]]; then
        if [[ "$found_legacy" == false ]]; then
          echo ""
          echo -e "  \033[0;33m⚠\033[0m 레거시 .sh 스크립트 발견 (제거 권장):"
          found_legacy=true
        fi
        echo -e "    - ${hook}"
        ((warnings++))
      fi
    done
  fi
  echo ""

  # --- 5. 의존성 검증 ---
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

  # --- 6. 서버 바이너리 검증 ---
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
  warn_deprecated "tc-config.sh is deprecated" "config"

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
