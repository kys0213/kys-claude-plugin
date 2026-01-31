#!/bin/bash
# Team Claude - PSM (Parallel Session Manager)
# git worktree 기반 병렬 세션 관리

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"

# ============================================================================
# 사용법
# ============================================================================
usage() {
  cat << 'EOF'
Team Claude PSM - Parallel Session Manager

사용법:
  tc-psm <command> [options]

Commands:
  new <name> [--from <session>]     새 세션 생성
  list [--status <status>]          세션 목록
  status [session-name]             세션 상태 확인
  switch <session-name>             세션 전환
  parallel <session1> <session2>... 병렬 실행
  cleanup [session-name] [--all]    세션 정리

Status:
  active      진행 중인 세션
  paused      일시 중지된 세션
  complete    완료된 세션
  error       오류 상태 세션

Examples:
  tc-psm new coupon-feature
  tc-psm list --status active
  tc-psm parallel auth payment notification
  tc-psm cleanup --all
EOF
}

# ============================================================================
# PSM 인덱스 관리
# ============================================================================

# PSM 인덱스 경로
get_psm_index_path() {
  echo "$(get_project_data_dir)/psm-index.json"
}

# PSM 인덱스 초기화
init_psm_index() {
  local index_path
  index_path=$(get_psm_index_path)

  if [[ ! -f "$index_path" ]]; then
    ensure_dir "$(dirname "$index_path")"
    cat > "$index_path" << 'EOF'
{
  "sessions": [],
  "settings": {
    "parallelLimit": 4,
    "autoCleanup": true
  },
  "createdAt": ""
}
EOF
    # 타임스탬프 추가
    local ts
    ts=$(timestamp)
    local tmp
    tmp=$(mktemp)
    jq --arg ts "$ts" '.createdAt = $ts' "$index_path" > "$tmp" && mv "$tmp" "$index_path"
  fi
}

# 세션을 인덱스에 추가
add_session_to_index() {
  local name="$1"
  local status="${2:-active}"
  local progress="${3:-0/0}"
  local worktree_path="$4"
  local branch="$5"

  require_jq
  init_psm_index

  local index_path
  index_path=$(get_psm_index_path)

  local ts
  ts=$(timestamp)

  local tmp
  tmp=$(mktemp)

  jq --arg name "$name" \
     --arg status "$status" \
     --arg progress "$progress" \
     --arg worktree "$worktree_path" \
     --arg branch "$branch" \
     --arg ts "$ts" \
     '.sessions += [{
       "name": $name,
       "status": $status,
       "progress": $progress,
       "worktreePath": $worktree,
       "branch": $branch,
       "createdAt": $ts,
       "updatedAt": $ts
     }]' "$index_path" > "$tmp" && mv "$tmp" "$index_path"
}

# 세션 상태 업데이트
update_session_in_index() {
  local name="$1"
  local field="$2"
  local value="$3"

  require_jq

  local index_path
  index_path=$(get_psm_index_path)

  if [[ ! -f "$index_path" ]]; then
    err "PSM 인덱스가 없습니다."
    return 1
  fi

  local ts
  ts=$(timestamp)

  local tmp
  tmp=$(mktemp)

  jq --arg name "$name" \
     --arg field "$field" \
     --arg value "$value" \
     --arg ts "$ts" \
     '(.sessions[] | select(.name == $name)) |= (.[$field] = $value | .updatedAt = $ts)' \
     "$index_path" > "$tmp" && mv "$tmp" "$index_path"
}

# 세션 제거
remove_session_from_index() {
  local name="$1"

  require_jq

  local index_path
  index_path=$(get_psm_index_path)

  if [[ ! -f "$index_path" ]]; then
    return 0
  fi

  local tmp
  tmp=$(mktemp)

  jq --arg name "$name" '.sessions |= map(select(.name != $name))' \
     "$index_path" > "$tmp" && mv "$tmp" "$index_path"
}

# 세션 정보 조회
get_session_info() {
  local name="$1"

  require_jq

  local index_path
  index_path=$(get_psm_index_path)

  if [[ ! -f "$index_path" ]]; then
    return 1
  fi

  jq -r --arg name "$name" '.sessions[] | select(.name == $name)' "$index_path"
}

# ============================================================================
# new - 새 세션 생성
# ============================================================================
cmd_new() {
  require_git
  require_jq

  local session_name=""
  local from_session=""

  # 인자 파싱
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --from)
        from_session="$2"
        shift 2
        ;;
      -*)
        err "알 수 없는 옵션: $1"
        exit 1
        ;;
      *)
        if [[ -z "$session_name" ]]; then
          session_name="$1"
        fi
        shift
        ;;
    esac
  done

  if [[ -z "$session_name" ]]; then
    err "세션 이름을 지정하세요."
    err "사용법: tc-psm new <session-name>"
    exit 1
  fi

  # 유효한 세션 이름인지 확인 (영문, 숫자, 하이픈만)
  if [[ ! "$session_name" =~ ^[a-zA-Z][a-zA-Z0-9-]*$ ]]; then
    err "유효하지 않은 세션 이름: $session_name"
    err "영문자로 시작하고, 영문자/숫자/하이픈만 사용 가능합니다."
    exit 1
  fi

  local root
  root=$(find_git_root)
  local worktrees_dir
  worktrees_dir=$(get_worktrees_dir)
  local worktree_path="${worktrees_dir}/${session_name}"
  local branch_name="team-claude/${session_name}"

  # 이미 존재하는지 확인
  if [[ -d "$worktree_path" ]]; then
    warn "세션이 이미 존재합니다: $session_name"
    echo "$worktree_path"
    return 0
  fi

  # worktrees 디렉토리 생성
  ensure_dir "$worktrees_dir"

  # 기준 브랜치 결정
  local base_branch
  if [[ -n "$from_session" ]]; then
    base_branch="team-claude/${from_session}"
    if ! git -C "$root" show-ref --verify --quiet "refs/heads/${base_branch}"; then
      err "소스 세션 브랜치가 없습니다: $base_branch"
      exit 1
    fi
  else
    base_branch=$(git -C "$root" rev-parse --abbrev-ref HEAD)
  fi

  # 브랜치가 이미 존재하는지 확인
  if git -C "$root" show-ref --verify --quiet "refs/heads/${branch_name}"; then
    info "브랜치가 이미 존재함: ${branch_name}"
    git -C "$root" worktree add "$worktree_path" "$branch_name" 2>/dev/null || {
      err "Worktree 생성 실패: ${worktree_path}"
      exit 1
    }
  else
    # 새 브랜치와 함께 worktree 생성
    git -C "$root" worktree add -b "$branch_name" "$worktree_path" "$base_branch" 2>/dev/null || {
      err "Worktree 생성 실패: ${worktree_path}"
      exit 1
    }
  fi

  # 세션 메타데이터 생성
  local session_meta_dir="${worktree_path}/.team-claude-session"
  ensure_dir "$session_meta_dir"

  cat > "${session_meta_dir}/meta.json" << EOF
{
  "name": "${session_name}",
  "status": "active",
  "worktreePath": "${worktree_path}",
  "branch": "${branch_name}",
  "baseBranch": "${base_branch}",
  "fromSession": "${from_session}",
  "createdAt": "$(timestamp)",
  "updatedAt": "$(timestamp)",
  "progress": {
    "total": 0,
    "completed": 0,
    "inProgress": 0,
    "pending": 0
  },
  "checkpoints": []
}
EOF

  # CLAUDE.md 템플릿 생성
  cat > "${worktree_path}/CLAUDE.md" << EOF
# Session: ${session_name}

## Overview
이 세션은 PSM(Parallel Session Manager)에 의해 생성되었습니다.

## Branch
\`${branch_name}\`

## Instructions
1. 이 worktree에서 독립적으로 작업합니다.
2. 작업 완료 후 PR을 생성합니다.
3. 다른 세션과의 충돌에 주의하세요.

## Context
- 생성일: $(date "+%Y-%m-%d %H:%M:%S")
- 기준 브랜치: ${base_branch}
$(if [[ -n "$from_session" ]]; then echo "- 소스 세션: ${from_session}"; fi)
EOF

  # PSM 인덱스에 추가
  add_session_to_index "$session_name" "active" "0/0" "$worktree_path" "$branch_name"

  echo ""
  ok "새 세션 생성: ${session_name}"
  echo ""
  echo "  Worktree: ${worktree_path}"
  echo "  브랜치: ${branch_name}"
  echo "  상태: initialized"
  echo ""
  echo "  다음 단계:"
  echo "    cd ${worktree_path}"
  echo "    또는"
  echo "    /team-claude:psm switch ${session_name}"
  echo ""

  echo "$worktree_path"
}

# ============================================================================
# list - 세션 목록
# ============================================================================
cmd_list() {
  require_jq
  init_psm_index

  local filter_status=""

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --status)
        filter_status="$2"
        shift 2
        ;;
      *)
        shift
        ;;
    esac
  done

  local index_path
  index_path=$(get_psm_index_path)

  echo ""
  echo "━━━ PSM Sessions ━━━"
  echo ""

  local sessions
  if [[ -n "$filter_status" ]]; then
    sessions=$(jq -r --arg status "$filter_status" \
      '.sessions[] | select(.status == $status) | @json' "$index_path")
  else
    sessions=$(jq -r '.sessions[] | @json' "$index_path")
  fi

  if [[ -z "$sessions" ]]; then
    info "세션이 없습니다."
    echo ""
    return 0
  fi

  # 헤더
  printf "  %-20s %-12s %-35s %-12s\n" "NAME" "STATUS" "BRANCH" "PROGRESS"
  echo "  ───────────────────────────────────────────────────────────────────────────"

  # 상태 아이콘
  get_status_icon() {
    case "$1" in
      active)   echo "🔄" ;;
      paused)   echo "⏸️" ;;
      complete) echo "✅" ;;
      error)    echo "❌" ;;
      *)        echo "❓" ;;
    esac
  }

  # 세션 목록 출력
  local active=0 paused=0 complete=0 error=0

  echo "$sessions" | while read -r session; do
    local name status branch progress
    name=$(echo "$session" | jq -r '.name')
    status=$(echo "$session" | jq -r '.status')
    branch=$(echo "$session" | jq -r '.branch')
    progress=$(echo "$session" | jq -r '.progress // "0/0"')

    local icon
    icon=$(get_status_icon "$status")

    printf "  %-20s %s %-10s %-35s %-12s\n" "$name" "$icon" "$status" "$branch" "$progress"
  done

  echo ""

  # 통계
  local stats
  stats=$(jq -r '
    .sessions | group_by(.status) |
    map({key: .[0].status, count: length}) |
    from_entries
  ' "$index_path")

  local total active paused complete
  total=$(jq -r '.sessions | length' "$index_path")
  active=$(echo "$stats" | jq -r '.active // 0')
  paused=$(echo "$stats" | jq -r '.paused // 0')
  complete=$(echo "$stats" | jq -r '.complete // 0')

  echo "  Total: ${total} sessions (${active} active, ${paused} paused, ${complete} complete)"
  echo ""
}

# ============================================================================
# status - 상태 확인
# ============================================================================
cmd_status() {
  require_jq
  init_psm_index

  local session_name="${1:-}"
  local index_path
  index_path=$(get_psm_index_path)

  if [[ -n "$session_name" ]]; then
    # 특정 세션 상태
    local session
    session=$(get_session_info "$session_name")

    if [[ -z "$session" || "$session" == "null" ]]; then
      err "세션을 찾을 수 없습니다: $session_name"
      exit 1
    fi

    local status branch worktree progress
    status=$(echo "$session" | jq -r '.status')
    branch=$(echo "$session" | jq -r '.branch')
    worktree=$(echo "$session" | jq -r '.worktreePath')
    progress=$(echo "$session" | jq -r '.progress // "0/0"')

    local icon
    case "$status" in
      active)   icon="🔄" ;;
      paused)   icon="⏸️" ;;
      complete) icon="✅" ;;
      error)    icon="❌" ;;
      *)        icon="❓" ;;
    esac

    echo ""
    echo "━━━ Session: ${session_name} ━━━"
    echo ""
    echo "  상태: ${icon} ${status}"
    echo "  브랜치: ${branch}"
    echo "  Worktree: ${worktree}"
    echo "  진행률: ${progress}"
    echo ""

    # 세션 메타 파일이 있으면 상세 정보
    local meta_file="${worktree}/.team-claude-session/meta.json"
    if [[ -f "$meta_file" ]]; then
      local checkpoints
      checkpoints=$(jq -r '.checkpoints[]?' "$meta_file" 2>/dev/null)

      if [[ -n "$checkpoints" ]]; then
        echo "━━━ Checkpoints ━━━"
        echo ""

        jq -r '.checkpoints[] | "  \(.status | if . == "complete" then "✅" elif . == "in_progress" then "🔄" elif . == "pending" then "⏸️" else "❌" end) \(.id) \(if .attempts > 0 then "(\(.attempts)회 시도)" else "" end)"' "$meta_file" 2>/dev/null || true
        echo ""
      fi
    fi

  else
    # 전체 상태
    echo ""
    echo "━━━ PSM Status ━━━"
    echo ""

    local stats
    stats=$(jq -r '
      .sessions | group_by(.status) |
      map({key: .[0].status, count: length}) |
      from_entries
    ' "$index_path")

    echo "  Active Sessions: $(echo "$stats" | jq -r '.active // 0')"
    echo "  Paused Sessions: $(echo "$stats" | jq -r '.paused // 0')"
    echo "  Complete Sessions: $(echo "$stats" | jq -r '.complete // 0')"
    echo ""

    echo "━━━ Resource Usage ━━━"
    echo ""

    local worktrees_dir
    worktrees_dir=$(get_worktrees_dir)
    local worktree_count=0
    local disk_usage="0"

    if [[ -d "$worktrees_dir" ]]; then
      worktree_count=$(find "$worktrees_dir" -maxdepth 1 -type d | wc -l)
      worktree_count=$((worktree_count - 1))  # 자기 자신 제외
      disk_usage=$(du -sh "$worktrees_dir" 2>/dev/null | cut -f1 || echo "0")
    fi

    echo "  Worktrees: ${worktree_count}"
    echo "  Disk Usage: ${disk_usage}"
    echo ""
  fi
}

# ============================================================================
# switch - 세션 전환
# ============================================================================
cmd_switch() {
  require_jq

  local session_name="${1:-}"

  if [[ -z "$session_name" ]]; then
    err "세션 이름을 지정하세요."
    err "사용법: tc-psm switch <session-name>"
    exit 1
  fi

  local session
  session=$(get_session_info "$session_name")

  if [[ -z "$session" || "$session" == "null" ]]; then
    err "세션을 찾을 수 없습니다: $session_name"
    exit 1
  fi

  local worktree status progress
  worktree=$(echo "$session" | jq -r '.worktreePath')
  status=$(echo "$session" | jq -r '.status')
  progress=$(echo "$session" | jq -r '.progress // "0/0"')

  if [[ ! -d "$worktree" ]]; then
    err "Worktree 디렉토리가 없습니다: $worktree"
    err "세션을 정리하고 다시 생성하세요."
    exit 1
  fi

  echo ""
  ok "세션 전환: ${session_name}"
  echo ""
  echo "  Worktree: ${worktree}"
  echo "  상태: ${status}"
  echo "  진행률: ${progress}"
  echo ""
  echo "  실행:"
  echo "    cd ${worktree}"
  echo ""

  # 환경 변수로 경로 출력 (호출자가 사용)
  echo "WORKTREE_PATH=${worktree}"
}

# ============================================================================
# parallel - 병렬 실행
# ============================================================================
cmd_parallel() {
  require_jq

  local sessions=("$@")

  if [[ ${#sessions[@]} -lt 2 ]]; then
    err "최소 2개의 세션을 지정하세요."
    err "사용법: tc-psm parallel <session1> <session2> [session3...]"
    exit 1
  fi

  echo ""
  echo "🚀 병렬 실행 준비"
  echo ""
  echo "  Sessions: ${#sessions[@]}"
  echo ""

  # 세션 검증
  echo "━━━ 세션 검증 ━━━"
  echo ""

  local valid_sessions=()
  for session in "${sessions[@]}"; do
    local info
    info=$(get_session_info "$session")

    if [[ -z "$info" || "$info" == "null" ]]; then
      warn "세션을 찾을 수 없음: $session (건너뜀)"
      continue
    fi

    local status worktree
    status=$(echo "$info" | jq -r '.status')
    worktree=$(echo "$info" | jq -r '.worktreePath')

    if [[ "$status" == "complete" ]]; then
      info "이미 완료됨: $session (건너뜀)"
      continue
    fi

    if [[ ! -d "$worktree" ]]; then
      warn "Worktree 없음: $session (건너뜀)"
      continue
    fi

    valid_sessions+=("$session")
    ok "준비됨: $session"
  done

  echo ""

  if [[ ${#valid_sessions[@]} -eq 0 ]]; then
    err "실행할 세션이 없습니다."
    exit 1
  fi

  # 실행 계획
  echo "━━━ 실행 계획 ━━━"
  echo ""
  printf "  %-20s %-15s %-10s\n" "Session" "Status" "Workers"
  echo "  ─────────────────────────────────────────────────"

  for session in "${valid_sessions[@]}"; do
    printf "  %-20s %-15s %-10s\n" "$session" "ready" "1"
  done

  echo ""
  echo "  총 Workers: ${#valid_sessions[@]}"
  echo ""

  # 상태 업데이트
  for session in "${valid_sessions[@]}"; do
    update_session_in_index "$session" "status" "active"
  done

  info "병렬 실행을 시작하려면 각 세션의 worktree에서 Claude를 실행하세요."
  echo ""

  for session in "${valid_sessions[@]}"; do
    local info
    info=$(get_session_info "$session")
    local worktree
    worktree=$(echo "$info" | jq -r '.worktreePath')
    echo "  ${session}: cd ${worktree} && claude"
  done

  echo ""
}

# ============================================================================
# cleanup - 정리
# ============================================================================
cmd_cleanup() {
  require_git
  require_jq

  local session_name=""
  local clean_all=false
  local force=false

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --all)
        clean_all=true
        shift
        ;;
      --force)
        force=true
        shift
        ;;
      -*)
        err "알 수 없는 옵션: $1"
        exit 1
        ;;
      *)
        if [[ -z "$session_name" ]]; then
          session_name="$1"
        fi
        shift
        ;;
    esac
  done

  local root
  root=$(find_git_root)
  local index_path
  index_path=$(get_psm_index_path)

  echo ""
  echo "🧹 세션 정리"
  echo ""

  local cleaned=0
  local skipped=0

  if [[ -n "$session_name" ]]; then
    # 특정 세션 정리
    local info
    info=$(get_session_info "$session_name")

    if [[ -z "$info" || "$info" == "null" ]]; then
      err "세션을 찾을 수 없습니다: $session_name"
      exit 1
    fi

    local status worktree branch
    status=$(echo "$info" | jq -r '.status')
    worktree=$(echo "$info" | jq -r '.worktreePath')
    branch=$(echo "$info" | jq -r '.branch')

    if [[ "$status" != "complete" && "$force" != "true" ]]; then
      warn "세션이 완료되지 않았습니다: $session_name (status: $status)"
      warn "--force 옵션으로 강제 정리할 수 있습니다."
      exit 1
    fi

    # Worktree 삭제
    if [[ -d "$worktree" ]]; then
      git -C "$root" worktree remove "$worktree" --force 2>/dev/null || {
        warn "git worktree remove 실패, 수동 삭제..."
        rm -rf "$worktree"
        git -C "$root" worktree prune
      }
    fi

    # 인덱스에서 제거
    remove_session_from_index "$session_name"

    ok "정리 완료: $session_name"
    cleaned=1

  elif [[ "$clean_all" == "true" ]]; then
    # 모든 세션 정리
    local sessions
    sessions=$(jq -r '.sessions[].name' "$index_path" 2>/dev/null)

    for session in $sessions; do
      local info
      info=$(get_session_info "$session")
      local status worktree
      status=$(echo "$info" | jq -r '.status')
      worktree=$(echo "$info" | jq -r '.worktreePath')

      if [[ "$status" != "complete" && "$force" != "true" ]]; then
        warn "건너뜀 (미완료): $session"
        ((skipped++))
        continue
      fi

      if [[ -d "$worktree" ]]; then
        git -C "$root" worktree remove "$worktree" --force 2>/dev/null || {
          rm -rf "$worktree"
        }
      fi

      remove_session_from_index "$session"
      info "정리됨: $session"
      ((cleaned++))
    done

    git -C "$root" worktree prune

  else
    # 완료된 세션만 정리
    local sessions
    sessions=$(jq -r '.sessions[] | select(.status == "complete") | .name' "$index_path" 2>/dev/null)

    if [[ -z "$sessions" ]]; then
      info "정리할 완료된 세션이 없습니다."
      return 0
    fi

    for session in $sessions; do
      local info
      info=$(get_session_info "$session")
      local worktree
      worktree=$(echo "$info" | jq -r '.worktreePath')

      if [[ -d "$worktree" ]]; then
        git -C "$root" worktree remove "$worktree" --force 2>/dev/null || {
          rm -rf "$worktree"
        }
      fi

      remove_session_from_index "$session"
      info "정리됨: $session"
      ((cleaned++))
    done

    git -C "$root" worktree prune
  fi

  echo ""
  echo "  정리 완료: ${cleaned} 세션"
  if [[ $skipped -gt 0 ]]; then
    echo "  건너뜀: ${skipped} 세션"
  fi
  echo ""
}

# ============================================================================
# 메인
# ============================================================================
main() {
  local command="${1:-}"
  shift || true

  case "$command" in
    new)
      cmd_new "$@"
      ;;
    list)
      cmd_list "$@"
      ;;
    status)
      cmd_status "$@"
      ;;
    switch)
      cmd_switch "$@"
      ;;
    parallel)
      cmd_parallel "$@"
      ;;
    cleanup)
      cmd_cleanup "$@"
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
