#!/bin/bash
# Team Claude - Worktree Management
# Git Worktree 관리 스크립트

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"

# ============================================================================
# 사용법
# ============================================================================
usage() {
  cat << 'EOF'
Team Claude Worktree - Git Worktree 관리

사용법:
  tc-worktree <command> [options]

Commands:
  create <checkpoint-id>  Worktree + 브랜치 생성
  list                    Worktree 목록
  delete <checkpoint-id>  Worktree 삭제
  cleanup                 모든 team-claude worktree 정리
  path <checkpoint-id>    Worktree 경로 출력

Examples:
  tc-worktree create coupon-service
  tc-worktree list
  tc-worktree delete coupon-service
  tc-worktree cleanup
EOF
}

# ============================================================================
# create - Worktree + 브랜치 생성
# ============================================================================
cmd_create() {
  require_git
  local checkpoint_id="${1:-}"

  if [[ -z "$checkpoint_id" ]]; then
    err "Checkpoint ID를 지정하세요."
    err "사용법: tc-worktree create <checkpoint-id>"
    exit 1
  fi

  local root
  root=$(find_git_root)
  local worktrees_dir="${root}/${WORKTREES_DIR}"
  local worktree_path="${worktrees_dir}/${checkpoint_id}"
  local branch_name="team-claude/${checkpoint_id}"

  # worktrees 디렉토리 생성
  ensure_dir "$worktrees_dir"

  # 이미 존재하는지 확인
  if [[ -d "$worktree_path" ]]; then
    warn "Worktree가 이미 존재합니다: ${worktree_path}"
    echo "$worktree_path"
    return 0
  fi

  # 현재 브랜치 저장
  local current_branch
  current_branch=$(git -C "$root" rev-parse --abbrev-ref HEAD)

  # 브랜치가 이미 존재하는지 확인
  if git -C "$root" show-ref --verify --quiet "refs/heads/${branch_name}"; then
    info "브랜치가 이미 존재함: ${branch_name}"
    # 기존 브랜치로 worktree 생성
    git -C "$root" worktree add "$worktree_path" "$branch_name" 2>/dev/null || {
      err "Worktree 생성 실패: ${worktree_path}"
      exit 1
    }
  else
    # 새 브랜치와 함께 worktree 생성
    git -C "$root" worktree add -b "$branch_name" "$worktree_path" 2>/dev/null || {
      err "Worktree 생성 실패: ${worktree_path}"
      exit 1
    }
  fi

  ok "Worktree 생성됨: ${worktree_path}"
  ok "브랜치: ${branch_name}"

  echo "$worktree_path"
}

# ============================================================================
# list - Worktree 목록
# ============================================================================
cmd_list() {
  require_git
  local root
  root=$(find_git_root)

  echo ""
  echo "━━━ Team Claude Worktrees ━━━"
  echo ""

  # git worktree list에서 team-claude 관련만 필터
  local has_worktrees=false

  while IFS= read -r line; do
    if [[ "$line" == *".team-claude/worktrees"* ]]; then
      has_worktrees=true
      local path branch
      path=$(echo "$line" | awk '{print $1}')
      branch=$(echo "$line" | grep -o '\[.*\]' | tr -d '[]')
      local checkpoint_id
      checkpoint_id=$(basename "$path")

      echo "  ${checkpoint_id}"
      echo "    경로: ${path}"
      echo "    브랜치: ${branch}"
      echo ""
    fi
  done < <(git -C "$root" worktree list)

  if [[ "$has_worktrees" == "false" ]]; then
    info "Team Claude worktree가 없습니다."
  fi

  # JSON 출력 (파싱용)
  echo "---"
  local json_output="["
  local first=true

  while IFS= read -r line; do
    if [[ "$line" == *".team-claude/worktrees"* ]]; then
      local path branch
      path=$(echo "$line" | awk '{print $1}')
      branch=$(echo "$line" | grep -o '\[.*\]' | tr -d '[]')
      local checkpoint_id
      checkpoint_id=$(basename "$path")

      if [[ "$first" == "true" ]]; then
        first=false
      else
        json_output+=","
      fi
      json_output+="{\"checkpointId\":\"${checkpoint_id}\",\"path\":\"${path}\",\"branch\":\"${branch}\"}"
    fi
  done < <(git -C "$root" worktree list)

  json_output+="]"
  echo "$json_output"
}

# ============================================================================
# delete - Worktree 삭제
# ============================================================================
cmd_delete() {
  require_git
  local checkpoint_id="${1:-}"

  if [[ -z "$checkpoint_id" ]]; then
    err "Checkpoint ID를 지정하세요."
    err "사용법: tc-worktree delete <checkpoint-id>"
    exit 1
  fi

  local root
  root=$(find_git_root)
  local worktree_path="${root}/${WORKTREES_DIR}/${checkpoint_id}"
  local branch_name="team-claude/${checkpoint_id}"

  if [[ ! -d "$worktree_path" ]]; then
    err "Worktree를 찾을 수 없습니다: ${worktree_path}"
    exit 1
  fi

  # Worktree 제거
  git -C "$root" worktree remove "$worktree_path" --force 2>/dev/null || {
    warn "git worktree remove 실패, 수동 삭제 시도..."
    rm -rf "$worktree_path"
    git -C "$root" worktree prune
  }

  ok "Worktree 삭제됨: ${worktree_path}"

  # 브랜치 삭제 여부 확인 (선택적)
  info "브랜치 '${branch_name}'는 유지됩니다."
  info "브랜치 삭제: git branch -D ${branch_name}"
}

# ============================================================================
# cleanup - 모든 team-claude worktree 정리
# ============================================================================
cmd_cleanup() {
  require_git
  local root
  root=$(find_git_root)

  echo ""
  echo "━━━ Team Claude Worktree 정리 ━━━"
  echo ""

  local cleaned=0

  while IFS= read -r line; do
    if [[ "$line" == *".team-claude/worktrees"* ]]; then
      local path
      path=$(echo "$line" | awk '{print $1}')
      local checkpoint_id
      checkpoint_id=$(basename "$path")

      info "삭제 중: ${checkpoint_id}"

      git -C "$root" worktree remove "$path" --force 2>/dev/null || {
        warn "git worktree remove 실패, 수동 삭제..."
        rm -rf "$path"
      }

      ((cleaned++))
    fi
  done < <(git -C "$root" worktree list)

  # prune 실행
  git -C "$root" worktree prune

  if [[ $cleaned -eq 0 ]]; then
    info "정리할 worktree가 없습니다."
  else
    ok "${cleaned}개의 worktree 정리됨"
  fi

  # 디렉토리가 비어있으면 삭제
  local worktrees_dir="${root}/${WORKTREES_DIR}"
  if [[ -d "$worktrees_dir" ]] && [[ -z "$(ls -A "$worktrees_dir" 2>/dev/null)" ]]; then
    rmdir "$worktrees_dir" 2>/dev/null || true
  fi
}

# ============================================================================
# path - Worktree 경로 출력
# ============================================================================
cmd_path() {
  local checkpoint_id="${1:-}"

  if [[ -z "$checkpoint_id" ]]; then
    err "Checkpoint ID를 지정하세요."
    err "사용법: tc-worktree path <checkpoint-id>"
    exit 1
  fi

  local root
  root=$(find_git_root)
  local worktree_path="${root}/${WORKTREES_DIR}/${checkpoint_id}"

  if [[ ! -d "$worktree_path" ]]; then
    err "Worktree를 찾을 수 없습니다: ${worktree_path}"
    exit 1
  fi

  echo "$worktree_path"
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
    delete)
      cmd_delete "$@"
      ;;
    cleanup)
      cmd_cleanup "$@"
      ;;
    path)
      cmd_path "$@"
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
