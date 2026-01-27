#!/bin/bash
# Team Claude - Agent Management
# 에이전트 관리 스크립트

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"

# ============================================================================
# 경로 상수
# ============================================================================
PROJECT_AGENTS_DIR=".claude/agents"
PLUGIN_AGENTS_DIR="${HOME}/.claude/plugins/team-claude/agents"

# ============================================================================
# 사용법
# ============================================================================
usage() {
  cat << 'EOF'
Team Claude Agent - 에이전트 관리

사용법:
  tc-agent <command> [options]

Commands:
  list                    모든 에이전트 목록 조회 (프로젝트 + 플러그인)
  validate                에이전트 이름 충돌 검사
  info <name>             에이전트 상세 정보
  init                    .claude/agents 디렉토리 생성

Examples:
  tc-agent list
  tc-agent validate
  tc-agent info payment-expert
  tc-agent init
EOF
}

# ============================================================================
# list - 에이전트 목록 조회
# ============================================================================
cmd_list() {
  require_yq
  local root
  root=$(find_git_root)

  echo ""
  echo "━━━ Team Claude 에이전트 목록 ━━━"
  echo ""

  # --- 프로젝트 로컬 에이전트 ---
  echo "📁 프로젝트 에이전트 (.claude/agents/)"
  local project_agents_path="${root}/${PROJECT_AGENTS_DIR}"

  if [[ -d "$project_agents_path" ]]; then
    local count=0
    for agent_file in "${project_agents_path}"/*.md; do
      if [[ -f "$agent_file" ]]; then
        local name description
        name=$(yq -f=extract '.name // empty' "$agent_file" 2>/dev/null || basename "$agent_file" .md)
        description=$(yq -f=extract '.description // empty' "$agent_file" 2>/dev/null || "")

        if [[ -z "$name" || "$name" == "null" ]]; then
          name=$(basename "$agent_file" .md)
        fi
        if [[ -z "$description" || "$description" == "null" ]]; then
          description="(설명 없음)"
        fi

        echo -e "  \033[0;32m●\033[0m ${name}"
        echo "     ${description}"
        ((count++))
      fi
    done

    if [[ $count -eq 0 ]]; then
      echo "  (에이전트 없음)"
    fi
  else
    echo "  (디렉토리 없음 - tc-agent init으로 생성)"
  fi

  echo ""

  # --- 플러그인 기본 에이전트 ---
  echo "📦 플러그인 에이전트 (~/.claude/plugins/team-claude/agents/)"

  if [[ -d "$PLUGIN_AGENTS_DIR" ]]; then
    for agent_file in "${PLUGIN_AGENTS_DIR}"/*.md; do
      if [[ -f "$agent_file" ]]; then
        local name description
        name=$(yq -f=extract '.name // empty' "$agent_file" 2>/dev/null || basename "$agent_file" .md)
        description=$(yq -f=extract '.description // empty' "$agent_file" 2>/dev/null || "")

        if [[ -z "$name" || "$name" == "null" ]]; then
          name=$(basename "$agent_file" .md)
        fi
        if [[ -z "$description" || "$description" == "null" ]]; then
          description="(설명 없음)"
        fi

        echo -e "  \033[0;34m●\033[0m ${name}"
        echo "     ${description}"
      fi
    done
  else
    echo "  (플러그인 에이전트 디렉토리 없음)"
  fi

  echo ""
}

# ============================================================================
# validate - 이름 충돌 검사
# ============================================================================
cmd_validate() {
  require_yq
  local root
  root=$(find_git_root)

  echo ""
  echo "━━━ 에이전트 이름 충돌 검사 ━━━"
  echo ""

  local project_agents_path="${root}/${PROJECT_AGENTS_DIR}"
  local conflicts=0
  local warnings=0

  if [[ ! -d "$project_agents_path" ]]; then
    info "프로젝트 에이전트가 없습니다. (.claude/agents/)"
    echo ""
    return 0
  fi

  # 플러그인 에이전트 이름 수집
  declare -A plugin_agents
  if [[ -d "$PLUGIN_AGENTS_DIR" ]]; then
    for agent_file in "${PLUGIN_AGENTS_DIR}"/*.md; do
      if [[ -f "$agent_file" ]]; then
        local name
        name=$(yq -f=extract '.name // empty' "$agent_file" 2>/dev/null || basename "$agent_file" .md)
        if [[ -z "$name" || "$name" == "null" ]]; then
          name=$(basename "$agent_file" .md)
        fi
        plugin_agents["$name"]="$agent_file"
      fi
    done
  fi

  # 프로젝트 에이전트 검사
  for agent_file in "${project_agents_path}"/*.md; do
    if [[ -f "$agent_file" ]]; then
      local name
      name=$(yq -f=extract '.name // empty' "$agent_file" 2>/dev/null || basename "$agent_file" .md)
      if [[ -z "$name" || "$name" == "null" ]]; then
        name=$(basename "$agent_file" .md)
      fi

      # 플러그인과 충돌 검사
      if [[ -n "${plugin_agents[$name]:-}" ]]; then
        echo -e "  \033[0;33m⚠\033[0m ${name}"
        echo "     프로젝트: ${agent_file}"
        echo "     플러그인: ${plugin_agents[$name]}"
        echo "     → 프로젝트 에이전트가 플러그인을 오버라이드합니다"
        echo ""
        ((warnings++))
      else
        echo -e "  \033[0;32m✓\033[0m ${name}"
      fi
    fi
  done

  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

  if [[ $warnings -eq 0 ]]; then
    echo -e "\033[0;32m✓ 충돌 없음\033[0m"
  else
    echo -e "\033[0;33m⚠ 오버라이드 ${warnings}개 (의도된 경우 무시 가능)\033[0m"
  fi
  echo ""

  return 0
}

# ============================================================================
# info - 에이전트 상세 정보
# ============================================================================
cmd_info() {
  require_yq
  local name="${1:-}"

  if [[ -z "$name" ]]; then
    err "에이전트 이름을 지정하세요."
    err "사용법: tc-agent info <name>"
    exit 1
  fi

  local root
  root=$(find_git_root)

  # 에이전트 파일 찾기 (프로젝트 우선)
  local agent_file=""
  local source=""

  local project_file="${root}/${PROJECT_AGENTS_DIR}/${name}.md"
  local plugin_file="${PLUGIN_AGENTS_DIR}/${name}.md"

  if [[ -f "$project_file" ]]; then
    agent_file="$project_file"
    source="프로젝트"
  elif [[ -f "$plugin_file" ]]; then
    agent_file="$plugin_file"
    source="플러그인"
  else
    err "에이전트를 찾을 수 없습니다: ${name}"
    err "확인할 위치:"
    err "  - ${project_file}"
    err "  - ${plugin_file}"
    exit 1
  fi

  echo ""
  echo "━━━ 에이전트 상세: ${name} ━━━"
  echo ""

  # YAML 프론트매터 파싱
  local description model tools
  description=$(yq -f=extract '.description // "(없음)"' "$agent_file" 2>/dev/null || echo "(파싱 실패)")
  model=$(yq -f=extract '.model // "sonnet"' "$agent_file" 2>/dev/null || echo "sonnet")
  tools=$(yq -f=extract '.tools // []' "$agent_file" 2>/dev/null || echo "[]")

  echo "  소스: ${source}"
  echo "  파일: ${agent_file}"
  echo ""
  echo "  설명: ${description}"
  echo "  모델: ${model}"
  echo "  도구: ${tools}"
  echo ""

  # 마크다운 본문 미리보기 (첫 10줄)
  echo "━━━ 본문 미리보기 ━━━"
  echo ""
  # 프론트매터(---)를 건너뛴 후 본문 출력
  awk '/^---$/{c++; next} c>=2' "$agent_file" | head -15
  echo ""
  echo "(전체 보기: cat ${agent_file})"
  echo ""
}

# ============================================================================
# init - 에이전트 디렉토리 초기화
# ============================================================================
cmd_init() {
  local root
  root=$(find_git_root)

  local agents_dir="${root}/${PROJECT_AGENTS_DIR}"

  if [[ -d "$agents_dir" ]]; then
    info "에이전트 디렉토리가 이미 존재합니다: ${agents_dir}"
    return 0
  fi

  ensure_dir "$agents_dir"
  ok "에이전트 디렉토리 생성됨: ${agents_dir}"

  # 예제 템플릿 생성
  local template_file="${agents_dir}/.example-agent.md"
  cat > "$template_file" << 'EOF'
---
name: example-agent
description: 예제 에이전트 - 이 파일을 복사하여 커스텀 에이전트를 만드세요
model: sonnet
tools: ["Read", "Glob", "Grep"]
---

# Example Agent

이 파일은 에이전트 템플릿 예제입니다.

## 역할

- 역할 1 설명
- 역할 2 설명

## 리뷰 체크리스트

- [ ] 체크 항목 1
- [ ] 체크 항목 2

## 프로젝트 컨텍스트

(선택) 이 프로젝트에 특화된 지침을 여기에 작성하세요.
EOF

  info "예제 템플릿 생성됨: ${template_file}"
  echo ""
  echo "다음 단계:"
  echo "  1. .example-agent.md를 복사하여 새 에이전트 생성"
  echo "  2. tc-agent list 로 에이전트 확인"
  echo "  3. tc-agent validate 로 충돌 검사"
  echo ""
}

# ============================================================================
# 메인
# ============================================================================
main() {
  local command="${1:-}"
  shift || true

  case "$command" in
    list)
      cmd_list "$@"
      ;;
    validate)
      cmd_validate "$@"
      ;;
    info)
      cmd_info "$@"
      ;;
    init)
      cmd_init "$@"
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
