#!/bin/bash
# Team Claude - Session Management
# 세션 관리 스크립트

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"

# ============================================================================
# 사용법
# ============================================================================
usage() {
  cat << 'EOF'
Team Claude Session - 세션 관리

사용법:
  tc-session <command> [options]

Commands:
  create <title>          새 세션 생성, session-id 반환
  list                    세션 목록 조회
  show <id>               세션 상세 정보
  delete <id>             세션 삭제
  update <id> <key> <val> 세션 메타데이터 업데이트

Examples:
  tc-session create "쿠폰 할인 기능"
  tc-session list
  tc-session show abc12345
  tc-session delete abc12345
  tc-session update abc12345 status designing
EOF
}

# ============================================================================
# create - 새 세션 생성
# ============================================================================
cmd_create() {
  require_jq
  local title="${1:-}"

  if [[ -z "$title" ]]; then
    err "세션 제목을 지정하세요."
    err "사용법: tc-session create <title>"
    exit 1
  fi

  local sessions_dir
  sessions_dir=$(get_sessions_dir)
  local index_path="${sessions_dir}/index.json"

  # sessions 디렉토리 생성
  ensure_dir "$sessions_dir"

  # 세션 ID 생성
  local session_id
  session_id=$(generate_id)

  local session_dir="${sessions_dir}/${session_id}"

  # 세션 디렉토리 구조 생성
  mkdir -p "${session_dir}/specs"
  mkdir -p "${session_dir}/checkpoints"
  mkdir -p "${session_dir}/contracts"
  mkdir -p "${session_dir}/delegations"

  # meta.json 생성
  local now
  now=$(timestamp)

  cat > "${session_dir}/meta.json" << EOF
{
  "sessionId": "${session_id}",
  "title": "${title}",
  "status": "designing",
  "phase": "initial",
  "createdAt": "${now}",
  "updatedAt": "${now}",
  "decisions": [],
  "checkpointsApproved": false
}
EOF

  # 빈 conversation.md 생성
  cat > "${session_dir}/conversation.md" << EOF
# 설계 대화 기록

세션: ${session_id}
제목: ${title}
시작: ${now}

---

EOF

  # index.json 업데이트
  if [[ ! -f "$index_path" ]]; then
    echo '{"sessions":[]}' > "$index_path"
  fi

  local index_entry
  index_entry=$(jq -n \
    --arg id "$session_id" \
    --arg title "$title" \
    --arg status "designing" \
    --arg createdAt "$now" \
    '{id: $id, title: $title, status: $status, createdAt: $createdAt}')

  jq --argjson entry "$index_entry" '.sessions += [$entry]' "$index_path" > "${index_path}.tmp"
  mv "${index_path}.tmp" "$index_path"

  ok "세션 생성됨: ${session_id}"
  echo "$session_id"
}

# ============================================================================
# list - 세션 목록 조회
# ============================================================================
cmd_list() {
  require_jq
  local sessions_dir
  sessions_dir=$(get_sessions_dir)
  local index_path="${sessions_dir}/index.json"

  if [[ ! -f "$index_path" ]]; then
    info "세션이 없습니다."
    echo "[]"
    return 0
  fi

  # JSON 형태로 출력
  local sessions
  sessions=$(jq '.sessions' "$index_path")

  if [[ "$sessions" == "[]" || "$sessions" == "null" ]]; then
    info "세션이 없습니다."
    echo "[]"
    return 0
  fi

  # 포맷된 출력
  echo ""
  echo "━━━ 세션 목록 ━━━"
  echo ""

  jq -r '.sessions[] | "  \(.id)  \(.status | if . == "designing" then "🔄" elif . == "delegating" then "🚀" elif . == "completed" then "✅" else "⏸️" end)  \(.title)"' "$index_path"

  echo ""

  # JSON도 출력 (파싱용)
  echo "$sessions"
}

# ============================================================================
# show - 세션 상세 정보
# ============================================================================
cmd_show() {
  require_jq
  local session_id="${1:-}"

  if [[ -z "$session_id" ]]; then
    err "세션 ID를 지정하세요."
    err "사용법: tc-session show <id>"
    exit 1
  fi

  local sessions_dir
  sessions_dir=$(get_sessions_dir)
  local session_dir="${sessions_dir}/${session_id}"
  local meta_path="${session_dir}/meta.json"

  if [[ ! -f "$meta_path" ]]; then
    err "세션을 찾을 수 없습니다: ${session_id}"
    exit 1
  fi

  # 메타 정보 출력
  echo ""
  echo "━━━ 세션: ${session_id} ━━━"
  echo ""

  local title status phase created updated
  title=$(jq -r '.title' "$meta_path")
  status=$(jq -r '.status' "$meta_path")
  phase=$(jq -r '.phase' "$meta_path")
  created=$(jq -r '.createdAt' "$meta_path")
  updated=$(jq -r '.updatedAt' "$meta_path")

  echo "  제목: ${title}"
  echo "  상태: ${status}"
  echo "  단계: ${phase}"
  echo "  생성: ${created}"
  echo "  수정: ${updated}"
  echo ""

  # 파일 구조
  echo "━━━ 파일 ━━━"
  echo ""

  if [[ -f "${session_dir}/specs/architecture.md" ]]; then
    echo "  ✅ specs/architecture.md"
  else
    echo "  ⬜ specs/architecture.md"
  fi

  if [[ -f "${session_dir}/specs/contracts.md" ]]; then
    echo "  ✅ specs/contracts.md"
  else
    echo "  ⬜ specs/contracts.md"
  fi

  if [[ -f "${session_dir}/specs/checkpoints.yaml" ]]; then
    echo "  ✅ specs/checkpoints.yaml"
  else
    echo "  ⬜ specs/checkpoints.yaml"
  fi

  echo ""

  # Checkpoints
  local checkpoints_dir="${session_dir}/checkpoints"
  if [[ -d "$checkpoints_dir" ]] && [[ -n "$(ls -A "$checkpoints_dir" 2>/dev/null)" ]]; then
    echo "━━━ Checkpoints ━━━"
    echo ""
    for f in "${checkpoints_dir}"/*.json; do
      if [[ -f "$f" ]]; then
        local cp_id cp_name cp_status
        cp_id=$(jq -r '.id' "$f")
        cp_name=$(jq -r '.name' "$f")
        echo "  - ${cp_id}: ${cp_name}"
      fi
    done
    echo ""
  fi

  # JSON 출력
  cat "$meta_path"
}

# ============================================================================
# delete - 세션 삭제
# ============================================================================
cmd_delete() {
  require_jq
  local session_id="${1:-}"

  if [[ -z "$session_id" ]]; then
    err "세션 ID를 지정하세요."
    err "사용법: tc-session delete <id>"
    exit 1
  fi

  local sessions_dir
  sessions_dir=$(get_sessions_dir)
  local session_dir="${sessions_dir}/${session_id}"
  local index_path="${sessions_dir}/index.json"

  if [[ ! -d "$session_dir" ]]; then
    err "세션을 찾을 수 없습니다: ${session_id}"
    exit 1
  fi

  # 세션 디렉토리 삭제
  rm -rf "$session_dir"

  # index.json에서 제거
  if [[ -f "$index_path" ]]; then
    jq --arg id "$session_id" '.sessions |= map(select(.id != $id))' "$index_path" > "${index_path}.tmp"
    mv "${index_path}.tmp" "$index_path"
  fi

  ok "세션 삭제됨: ${session_id}"
}

# ============================================================================
# update - 세션 메타데이터 업데이트
# ============================================================================
cmd_update() {
  require_jq
  local session_id="${1:-}"
  local key="${2:-}"
  local value="${3:-}"

  if [[ -z "$session_id" || -z "$key" || -z "$value" ]]; then
    err "세션 ID, 키, 값을 모두 지정하세요."
    err "사용법: tc-session update <id> <key> <value>"
    exit 1
  fi

  local sessions_dir
  sessions_dir=$(get_sessions_dir)
  local meta_path="${sessions_dir}/${session_id}/meta.json"

  if [[ ! -f "$meta_path" ]]; then
    err "세션을 찾을 수 없습니다: ${session_id}"
    exit 1
  fi

  local now
  now=$(timestamp)

  # 메타 정보 업데이트
  jq --arg key "$key" --arg value "$value" --arg now "$now" \
    '.[$key] = $value | .updatedAt = $now' "$meta_path" > "${meta_path}.tmp"
  mv "${meta_path}.tmp" "$meta_path"

  # index.json에서도 status 업데이트 (status 변경 시)
  if [[ "$key" == "status" ]]; then
    local index_path="${sessions_dir}/index.json"
    if [[ -f "$index_path" ]]; then
      jq --arg id "$session_id" --arg status "$value" \
        '(.sessions[] | select(.id == $id)).status = $status' "$index_path" > "${index_path}.tmp"
      mv "${index_path}.tmp" "$index_path"
    fi
  fi

  ok "세션 업데이트됨: ${session_id}.${key} = ${value}"
}

# ============================================================================
# 메인
# ============================================================================
main() {
  local command="${1:-}"
  shift || true

  case "$command" in
    create)
      cmd_create "$@"
      ;;
    list)
      cmd_list "$@"
      ;;
    show)
      cmd_show "$@"
      ;;
    delete)
      cmd_delete "$@"
      ;;
    update)
      cmd_update "$@"
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
