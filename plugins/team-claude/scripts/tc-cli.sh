#!/bin/bash
# Team Claude - CLI Management
# tc CLI 바이너리 빌드/설치 스크립트

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"

# ============================================================================
# 경로 상수
# ============================================================================
CLI_SRC_DIR="${SCRIPT_DIR}/../cli"
CLI_BINARY="${HOME}/.claude/tc"

# ============================================================================
# 사용법
# ============================================================================
usage() {
  cat << 'EOF'
Team Claude CLI - 빌드 및 관리

사용법:
  tc-cli <command>

Commands:
  build                   CLI 바이너리 빌드
  install                 빌드 + ~/.claude/tc에 설치
  uninstall               CLI 바이너리 삭제
  status                  설치 상태 확인
  dev                     개발 모드 실행 (bun run)

Examples:
  tc-cli install          # 빌드 후 설치
  tc-cli status           # 상태 확인
  tc-cli dev test unit    # 개발 모드로 테스트 실행
EOF
}

# ============================================================================
# build - CLI 빌드
# ============================================================================
cmd_build() {
  if ! command -v bun &>/dev/null; then
    err "bun이 설치되어 있지 않습니다."
    err "설치: curl -fsSL https://bun.sh/install | bash"
    exit 1
  fi

  info "CLI 빌드 중..."

  # 의존성 설치
  if [[ ! -d "${CLI_SRC_DIR}/node_modules" ]]; then
    info "의존성 설치 중..."
    (cd "$CLI_SRC_DIR" && bun install)
  fi

  # 빌드
  (cd "$CLI_SRC_DIR" && bun build src/index.ts --compile --outfile dist/tc)

  if [[ -f "${CLI_SRC_DIR}/dist/tc" ]]; then
    ok "빌드 완료: ${CLI_SRC_DIR}/dist/tc"
  else
    err "빌드 실패"
    exit 1
  fi
}

# ============================================================================
# install - 빌드 + 설치
# ============================================================================
cmd_install() {
  cmd_build

  info "CLI 설치 중..."

  # ~/.claude 디렉토리 생성
  ensure_dir "$(dirname "$CLI_BINARY")"

  # 복사
  cp "${CLI_SRC_DIR}/dist/tc" "$CLI_BINARY"
  chmod +x "$CLI_BINARY"

  ok "CLI 설치됨: ${CLI_BINARY}"
  echo ""
  info "사용법: ${CLI_BINARY} --help"
  info "또는 PATH에 추가: export PATH=\"\$HOME/.claude:\$PATH\""
}

# ============================================================================
# uninstall - CLI 삭제
# ============================================================================
cmd_uninstall() {
  if [[ -f "$CLI_BINARY" ]]; then
    rm "$CLI_BINARY"
    ok "CLI 삭제됨: ${CLI_BINARY}"
  else
    info "CLI가 설치되어 있지 않습니다."
  fi
}

# ============================================================================
# status - 상태 확인
# ============================================================================
cmd_status() {
  echo ""
  echo "━━━ Team Claude CLI 상태 ━━━"
  echo ""

  if [[ -f "$CLI_BINARY" ]]; then
    echo -e "  \033[0;32m✓\033[0m 설치됨: ${CLI_BINARY}"

    # 버전 확인
    local version
    version=$("$CLI_BINARY" --version 2>/dev/null || echo "unknown")
    echo -e "  \033[0;32m✓\033[0m 버전: ${version}"
  else
    echo -e "  \033[0;31m✗\033[0m 미설치"
    echo ""
    info "설치: tc-cli install"
  fi

  echo ""

  # bun 확인
  if command -v bun &>/dev/null; then
    echo -e "  \033[0;32m✓\033[0m bun: $(bun --version)"
  else
    echo -e "  \033[0;31m✗\033[0m bun: 미설치"
  fi

  echo ""
}

# ============================================================================
# dev - 개발 모드 실행
# ============================================================================
cmd_dev() {
  if ! command -v bun &>/dev/null; then
    err "bun이 설치되어 있지 않습니다."
    exit 1
  fi

  # 의존성 설치
  if [[ ! -d "${CLI_SRC_DIR}/node_modules" ]]; then
    info "의존성 설치 중..."
    (cd "$CLI_SRC_DIR" && bun install)
  fi

  # 실행
  (cd "$CLI_SRC_DIR" && bun run src/index.ts "$@")
}

# ============================================================================
# 메인
# ============================================================================
main() {
  local command="${1:-}"
  shift || true

  case "$command" in
    build)
      cmd_build "$@"
      ;;
    install)
      cmd_install "$@"
      ;;
    uninstall)
      cmd_uninstall "$@"
      ;;
    status)
      cmd_status "$@"
      ;;
    dev)
      cmd_dev "$@"
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
