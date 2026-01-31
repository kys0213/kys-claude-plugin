#!/bin/bash
# Team Claude - Flow Orchestrator
# 통합 워크플로우 오케스트레이션

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"

# ============================================================================
# 사용법
# ============================================================================
usage() {
  cat << 'EOF'
Team Claude Flow - 통합 워크플로우 오케스트레이터

사용법:
  tc-flow <command> [options]

Commands:
  start <requirement>         새 워크플로우 시작
  resume <session-id>         기존 워크플로우 재개
  status [session-id]         워크플로우 상태 확인
  parse-keyword <message>     Magic Keyword 파싱

Options:
  --mode <mode>              실행 모드 (autopilot|assisted|manual)
  --phase <phase>            특정 단계만 (spec|impl|merge)
  --dry-run                  시뮬레이션만

Modes:
  autopilot   전체 자동화 (에스컬레이션 시에만 HITL)
  assisted    단계별 확인 (각 단계 완료 시 HITL)
  manual      기존 방식 (모든 결정에 HITL)

Examples:
  tc-flow start "쿠폰 기능 추가" --mode autopilot
  tc-flow resume abc12345
  tc-flow status abc12345
  tc-flow parse-keyword "autopilot: 쿠폰 기능"
EOF
}

# ============================================================================
# Magic Keywords
# ============================================================================

# Magic Keyword 목록
declare -A MAGIC_KEYWORDS=(
  ["autopilot"]="autopilot"
  ["auto"]="autopilot"
  ["ap"]="autopilot"
  ["spec"]="spec"
  ["sp"]="spec"
  ["impl"]="impl"
  ["im"]="impl"
  ["review"]="review"
  ["rv"]="review"
  ["parallel"]="parallel"
  ["pl"]="parallel"
  ["ralph"]="ralph"
  ["rl"]="ralph"
)

# Magic Keyword 파싱
parse_magic_keyword() {
  local message="$1"

  # 메시지 시작 부분에서 keyword: 패턴 찾기
  if [[ "$message" =~ ^([a-zA-Z]+): ]]; then
    local keyword="${BASH_REMATCH[1]}"
    keyword=$(echo "$keyword" | tr '[:upper:]' '[:lower:]')

    if [[ -n "${MAGIC_KEYWORDS[$keyword]:-}" ]]; then
      echo "${MAGIC_KEYWORDS[$keyword]}"
      return 0
    fi
  fi

  echo ""
  return 1
}

# Magic Keyword 제거 후 메시지 추출
extract_message() {
  local message="$1"

  if [[ "$message" =~ ^[a-zA-Z]+:[[:space:]]*(.*) ]]; then
    echo "${BASH_REMATCH[1]}"
  else
    echo "$message"
  fi
}

# ============================================================================
# Flow State 관리
# ============================================================================

# Flow 상태 파일 경로
get_flow_state_path() {
  local session_id="$1"
  echo "$(get_sessions_dir)/${session_id}/flow-state.json"
}

# Flow 상태 초기화
init_flow_state() {
  local session_id="$1"
  local mode="${2:-assisted}"
  local requirement="$3"

  require_jq

  local flow_path
  flow_path=$(get_flow_state_path "$session_id")

  ensure_dir "$(dirname "$flow_path")"

  cat > "$flow_path" << EOF
{
  "sessionId": "${session_id}",
  "mode": "${mode}",
  "requirement": $(echo "$requirement" | jq -R .),
  "status": "started",
  "currentPhase": "spec",
  "phases": {
    "spec": {
      "status": "pending",
      "iterations": 0,
      "startedAt": null,
      "completedAt": null
    },
    "impl": {
      "status": "pending",
      "iterations": 0,
      "startedAt": null,
      "completedAt": null
    },
    "merge": {
      "status": "pending",
      "startedAt": null,
      "completedAt": null
    }
  },
  "escalations": [],
  "createdAt": "$(timestamp)",
  "updatedAt": "$(timestamp)"
}
EOF

  echo "$flow_path"
}

# Flow 상태 업데이트
update_flow_state() {
  local session_id="$1"
  local field="$2"
  local value="$3"

  require_jq

  local flow_path
  flow_path=$(get_flow_state_path "$session_id")

  if [[ ! -f "$flow_path" ]]; then
    err "Flow 상태 파일이 없습니다: $session_id"
    return 1
  fi

  local ts
  ts=$(timestamp)

  local tmp
  tmp=$(mktemp)

  jq --arg field "$field" \
     --arg value "$value" \
     --arg ts "$ts" \
     '.[$field] = $value | .updatedAt = $ts' \
     "$flow_path" > "$tmp" && mv "$tmp" "$flow_path"
}

# Phase 상태 업데이트
update_phase_state() {
  local session_id="$1"
  local phase="$2"
  local field="$3"
  local value="$4"

  require_jq

  local flow_path
  flow_path=$(get_flow_state_path "$session_id")

  if [[ ! -f "$flow_path" ]]; then
    err "Flow 상태 파일이 없습니다: $session_id"
    return 1
  fi

  local ts
  ts=$(timestamp)

  local tmp
  tmp=$(mktemp)

  jq --arg phase "$phase" \
     --arg field "$field" \
     --arg value "$value" \
     --arg ts "$ts" \
     '.phases[$phase][$field] = $value | .updatedAt = $ts' \
     "$flow_path" > "$tmp" && mv "$tmp" "$flow_path"
}

# ============================================================================
# start - 새 워크플로우 시작
# ============================================================================
cmd_start() {
  local requirement=""
  local mode="assisted"
  local phase=""
  local dry_run=false

  # 인자 파싱
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --mode)
        mode="$2"
        shift 2
        ;;
      --phase)
        phase="$2"
        shift 2
        ;;
      --dry-run)
        dry_run=true
        shift
        ;;
      -*)
        err "알 수 없는 옵션: $1"
        exit 1
        ;;
      *)
        if [[ -z "$requirement" ]]; then
          requirement="$1"
        else
          requirement="$requirement $1"
        fi
        shift
        ;;
    esac
  done

  # Magic Keyword 처리
  local keyword
  keyword=$(parse_magic_keyword "$requirement")

  if [[ -n "$keyword" ]]; then
    mode="$keyword"
    requirement=$(extract_message "$requirement")
    info "Magic Keyword 감지: $keyword"
  fi

  if [[ -z "$requirement" ]]; then
    err "요구사항을 입력하세요."
    err "사용법: tc-flow start \"요구사항\" --mode <mode>"
    exit 1
  fi

  # 모드 검증
  case "$mode" in
    autopilot|assisted|manual|spec|impl|review|parallel|ralph)
      ;;
    *)
      err "유효하지 않은 모드: $mode"
      err "사용 가능: autopilot, assisted, manual, spec, impl, review, parallel, ralph"
      exit 1
      ;;
  esac

  echo ""
  echo "🚀 Automated Workflow 시작"
  echo ""
  echo "  모드: ${mode}"
  echo "  요구사항: ${requirement}"
  if [[ -n "$phase" ]]; then
    echo "  단계: ${phase}"
  fi
  if [[ "$dry_run" == "true" ]]; then
    echo "  (Dry Run - 시뮬레이션만)"
  fi
  echo ""

  if [[ "$dry_run" == "true" ]]; then
    info "Dry run 모드입니다. 실제 실행하지 않습니다."
    return 0
  fi

  # 세션 생성
  local session_id
  session_id=$("${SCRIPT_DIR}/tc-session.sh" create "$requirement" 2>/dev/null | tail -1)

  if [[ -z "$session_id" ]]; then
    err "세션 생성 실패"
    exit 1
  fi

  ok "세션 생성됨: ${session_id}"

  # Flow 상태 초기화
  init_flow_state "$session_id" "$mode" "$requirement"

  # 워크플로우 상태 업데이트
  "${SCRIPT_DIR}/tc-state.sh" transition flow_started 2>/dev/null || true
  "${SCRIPT_DIR}/tc-state.sh" set-session "$session_id" 2>/dev/null || true

  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""

  # 모드에 따른 안내
  case "$mode" in
    autopilot)
      echo "📋 AUTOPILOT 모드: 전체 자동화"
      echo ""
      echo "  1. 스펙 자동 설계 + 자동 리뷰"
      echo "  2. 자동 구현 (RALPH loop)"
      echo "  3. 자동 코드 리뷰"
      echo "  4. 자동 머지"
      echo ""
      echo "  에스컬레이션 시에만 사용자 개입을 요청합니다."
      ;;
    assisted)
      echo "📋 ASSISTED 모드: 단계별 확인"
      echo ""
      echo "  1. 스펙 자동 설계 + 자동 리뷰 → 승인 요청"
      echo "  2. 자동 구현 + 자동 리뷰 → 승인 요청"
      echo "  3. 머지 → 확인 요청"
      ;;
    spec)
      echo "📋 SPEC 모드: 스펙 설계만"
      echo ""
      echo "  스펙 설계 + 자동 리뷰까지 진행합니다."
      ;;
    impl)
      echo "📋 IMPL 모드: 구현만"
      echo ""
      echo "  기존 스펙을 기반으로 구현을 진행합니다."
      ;;
    *)
      echo "📋 ${mode^^} 모드"
      ;;
  esac

  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""

  # 결과 출력
  echo "  세션 ID: ${session_id}"
  echo ""
  echo "  다음 단계:"
  echo "    /team-claude:architect \"${requirement}\""
  echo ""
  echo "  또는 flow 재개:"
  echo "    tc-flow resume ${session_id}"
  echo ""

  # JSON 출력 (프로그래밍 용)
  echo "---"
  cat << EOF
{
  "sessionId": "${session_id}",
  "mode": "${mode}",
  "status": "started"
}
EOF
}

# ============================================================================
# resume - 워크플로우 재개
# ============================================================================
cmd_resume() {
  local session_id="${1:-}"

  if [[ -z "$session_id" ]]; then
    err "세션 ID를 지정하세요."
    err "사용법: tc-flow resume <session-id>"
    exit 1
  fi

  local flow_path
  flow_path=$(get_flow_state_path "$session_id")

  if [[ ! -f "$flow_path" ]]; then
    err "Flow 상태 파일이 없습니다: $session_id"
    exit 1
  fi

  require_jq

  local mode current_phase status
  mode=$(jq -r '.mode' "$flow_path")
  current_phase=$(jq -r '.currentPhase' "$flow_path")
  status=$(jq -r '.status' "$flow_path")

  echo ""
  ok "워크플로우 재개: ${session_id}"
  echo ""
  echo "  모드: ${mode}"
  echo "  현재 단계: ${current_phase}"
  echo "  상태: ${status}"
  echo ""

  # 단계별 안내
  case "$current_phase" in
    spec)
      echo "  다음 단계:"
      echo "    /team-claude:architect --resume ${session_id}"
      ;;
    impl)
      echo "  다음 단계:"
      echo "    /team-claude:delegate --session ${session_id} --all"
      ;;
    merge)
      echo "  다음 단계:"
      echo "    /team-claude:merge --session ${session_id}"
      ;;
  esac

  echo ""
}

# ============================================================================
# status - 상태 확인
# ============================================================================
cmd_status() {
  local session_id="${1:-}"

  if [[ -z "$session_id" ]]; then
    # 현재 활성 세션 상태
    local state_dir
    state_dir=$(get_state_dir)

    if [[ -f "${state_dir}/workflow.json" ]]; then
      session_id=$(jq -r '.currentSession // empty' "${state_dir}/workflow.json")
    fi

    if [[ -z "$session_id" ]]; then
      err "활성 세션이 없습니다."
      exit 1
    fi
  fi

  local flow_path
  flow_path=$(get_flow_state_path "$session_id")

  if [[ ! -f "$flow_path" ]]; then
    err "Flow 상태 파일이 없습니다: $session_id"
    exit 1
  fi

  require_jq

  echo ""
  echo "━━━ Flow Status: ${session_id} ━━━"
  echo ""

  local mode status current_phase requirement
  mode=$(jq -r '.mode' "$flow_path")
  status=$(jq -r '.status' "$flow_path")
  current_phase=$(jq -r '.currentPhase' "$flow_path")
  requirement=$(jq -r '.requirement' "$flow_path")

  echo "  모드: ${mode}"
  echo "  상태: ${status}"
  echo "  현재 단계: ${current_phase}"
  echo "  요구사항: ${requirement}"
  echo ""

  echo "━━━ Phases ━━━"
  echo ""

  # 각 단계 상태
  for phase in spec impl merge; do
    local phase_status iterations
    phase_status=$(jq -r ".phases.${phase}.status" "$flow_path")
    iterations=$(jq -r ".phases.${phase}.iterations // 0" "$flow_path")

    local icon
    case "$phase_status" in
      complete)    icon="✅" ;;
      in_progress) icon="🔄" ;;
      pending)     icon="⏸️" ;;
      error)       icon="❌" ;;
      *)           icon="❓" ;;
    esac

    echo "  ${icon} ${phase}: ${phase_status}"
    if [[ "$iterations" -gt 0 ]]; then
      echo "      반복: ${iterations}회"
    fi
  done

  echo ""

  # 에스컬레이션 정보
  local escalations
  escalations=$(jq -r '.escalations | length' "$flow_path")

  if [[ "$escalations" -gt 0 ]]; then
    echo "━━━ Escalations ━━━"
    echo ""
    jq -r '.escalations[] | "  ⚠️ \(.phase): \(.reason)"' "$flow_path"
    echo ""
  fi
}

# ============================================================================
# parse-keyword - Magic Keyword 파싱
# ============================================================================
cmd_parse_keyword() {
  local message="$*"

  if [[ -z "$message" ]]; then
    err "메시지를 입력하세요."
    exit 1
  fi

  local keyword
  keyword=$(parse_magic_keyword "$message")

  local extracted
  extracted=$(extract_message "$message")

  if [[ -n "$keyword" ]]; then
    echo "keyword=${keyword}"
    echo "message=${extracted}"
    echo "matched=true"
  else
    echo "keyword="
    echo "message=${message}"
    echo "matched=false"
  fi
}

# ============================================================================
# 메인
# ============================================================================
main() {
  local command="${1:-}"
  shift || true

  case "$command" in
    start)
      cmd_start "$@"
      ;;
    resume)
      cmd_resume "$@"
      ;;
    status)
      cmd_status "$@"
      ;;
    parse-keyword)
      cmd_parse_keyword "$@"
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
