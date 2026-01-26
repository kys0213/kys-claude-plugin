#!/bin/bash
# Team Claude - Server Lifecycle Management
# 글로벌 서버 라이프사이클 관리 + Health 모니터링

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"

# ============================================================================
# 상수
# ============================================================================
# 서버 바이너리 위치 (글로벌)
SERVER_BINARY="${HOME}/.claude/team-claude-server"
# 서버 소스 위치 (플러그인 내)
SERVER_SOURCE_DIR="${SCRIPT_DIR}/../server"
# 서버 PID 파일
PID_FILE="${HOME}/.claude/team-claude-server.pid"
# 서버 로그 파일
LOG_FILE="${HOME}/.claude/team-claude-server.log"
# 기본 포트
DEFAULT_PORT=7890

# ============================================================================
# 사용법
# ============================================================================
usage() {
  cat << 'EOF'
Team Claude Server - 서버 라이프사이클 관리

사용법:
  tc-server <command> [options]

Commands:
  status              서버 실행 중이고 healthy한지 확인
  start               미실행 시 서버 백그라운드 실행
  stop                서버 중지
  restart             서버 재시작
  ensure              미실행 시 시작, health 검증 (delegate에서 사용)
  build               서버 빌드 후 ~/.claude/team-claude-server에 설치
  install             의존성 설치 + 빌드 + 설치 (setup에서 호출)
  logs                서버 로그 확인

Examples:
  tc-server status
  tc-server start
  tc-server stop
  tc-server ensure
  tc-server build
  tc-server install
  tc-server logs -f
EOF
}

# ============================================================================
# 유틸리티 함수
# ============================================================================

# 서버 포트 확인
get_port() {
  local root
  if root=$(find_git_root 2>/dev/null); then
    local port
    port=$("${SCRIPT_DIR}/tc-config.sh" get server.port 2>/dev/null || echo "")
    if [[ -n "$port" && "$port" != "null" ]]; then
      echo "$port"
      return
    fi
  fi
  echo "$DEFAULT_PORT"
}

# PID 파일에서 PID 읽기
get_pid() {
  if [[ -f "$PID_FILE" ]]; then
    cat "$PID_FILE"
  else
    echo ""
  fi
}

# 프로세스가 실행 중인지 확인
is_running() {
  local pid
  pid=$(get_pid)
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    return 0
  fi
  return 1
}

# Health check
is_healthy() {
  local port
  port=$(get_port)
  local response
  response=$(curl -s -o /dev/null -w '%{http_code}' "http://localhost:${port}/health" 2>/dev/null)
  [[ "$response" == "200" ]]
}

# bun 설치 확인
require_bun() {
  if ! command -v bun &>/dev/null; then
    err "bun이 설치되어 있지 않습니다."
    err "설치: curl -fsSL https://bun.sh/install | bash"
    exit 1
  fi
}

# ============================================================================
# status - 서버 상태 확인
# ============================================================================
cmd_status() {
  local port
  port=$(get_port)

  echo ""
  echo "━━━ Team Claude Server Status ━━━"
  echo ""
  echo "  Binary: ${SERVER_BINARY}"
  echo "  Port: ${port}"
  echo ""

  # 바이너리 존재 확인
  if [[ -f "$SERVER_BINARY" ]]; then
    ok "Binary: 설치됨"
  else
    err "Binary: 미설치 (tc-server install 실행 필요)"
  fi

  # 프로세스 확인
  if is_running; then
    local pid
    pid=$(get_pid)
    ok "Process: 실행 중 (PID: ${pid})"
  else
    err "Process: 중지됨"
  fi

  # Health check
  if is_healthy; then
    ok "Health: OK"
  else
    err "Health: 응답 없음"
  fi

  echo ""

  # 종합 상태 반환
  if is_running && is_healthy; then
    echo "running"
    return 0
  else
    echo "stopped"
    return 1
  fi
}

# ============================================================================
# start - 서버 시작
# ============================================================================
cmd_start() {
  local port
  port=$(get_port)

  # 이미 실행 중인지 확인
  if is_running && is_healthy; then
    info "서버가 이미 실행 중입니다."
    echo "already_running"
    return 0
  fi

  # 바이너리 확인
  if [[ ! -f "$SERVER_BINARY" ]]; then
    err "서버 바이너리가 없습니다: ${SERVER_BINARY}"
    err "'tc-server install'을 먼저 실행하세요."
    exit 1
  fi

  # .claude 디렉토리 확인
  ensure_dir "${HOME}/.claude"

  info "서버 시작 중... (port: ${port})"

  # 백그라운드 실행
  TEAM_CLAUDE_PORT="$port" nohup "$SERVER_BINARY" >> "$LOG_FILE" 2>&1 &
  local pid=$!
  echo "$pid" > "$PID_FILE"

  # 시작 대기 (최대 10초)
  local attempts=0
  while [[ $attempts -lt 20 ]]; do
    if is_healthy; then
      ok "서버 시작됨 (PID: ${pid}, Port: ${port})"
      echo "started"
      return 0
    fi
    sleep 0.5
    ((attempts++))
  done

  err "서버 시작 실패 (timeout)"
  err "로그 확인: tc-server logs"
  exit 1
}

# ============================================================================
# stop - 서버 중지
# ============================================================================
cmd_stop() {
  if ! is_running; then
    info "서버가 실행 중이지 않습니다."
    return 0
  fi

  local pid
  pid=$(get_pid)

  info "서버 중지 중... (PID: ${pid})"

  # SIGTERM 전송
  kill "$pid" 2>/dev/null

  # 종료 대기 (최대 5초)
  local attempts=0
  while [[ $attempts -lt 10 ]]; do
    if ! kill -0 "$pid" 2>/dev/null; then
      rm -f "$PID_FILE"
      ok "서버 중지됨"
      return 0
    fi
    sleep 0.5
    ((attempts++))
  done

  # 강제 종료
  warn "SIGKILL 전송..."
  kill -9 "$pid" 2>/dev/null
  rm -f "$PID_FILE"
  ok "서버 강제 중지됨"
}

# ============================================================================
# restart - 서버 재시작
# ============================================================================
cmd_restart() {
  cmd_stop
  sleep 1
  cmd_start
}

# ============================================================================
# ensure - 미실행 시 시작, health 검증
# ============================================================================
cmd_ensure() {
  # 이미 healthy하면 바로 반환
  if is_healthy; then
    info "서버가 이미 healthy합니다."
    echo "already_running"

    # 상태 파일 업데이트 시도
    if command -v "${SCRIPT_DIR}/tc-state.sh" &>/dev/null; then
      "${SCRIPT_DIR}/tc-state.sh" set-server true 2>/dev/null || true
    fi

    return 0
  fi

  # 서버 시작
  local result
  result=$(cmd_start)

  # 상태 파일 업데이트
  if [[ "$result" == "started" || "$result" == "already_running" ]]; then
    if command -v "${SCRIPT_DIR}/tc-state.sh" &>/dev/null; then
      "${SCRIPT_DIR}/tc-state.sh" set-server true 2>/dev/null || true
    fi
    echo "$result"
    return 0
  fi

  exit 1
}

# ============================================================================
# build - 서버 빌드
# ============================================================================
cmd_build() {
  require_bun

  if [[ ! -d "$SERVER_SOURCE_DIR" ]]; then
    err "서버 소스를 찾을 수 없습니다: ${SERVER_SOURCE_DIR}"
    exit 1
  fi

  info "서버 빌드 중..."

  # .claude 디렉토리 확인
  ensure_dir "${HOME}/.claude"

  # 빌드
  cd "$SERVER_SOURCE_DIR"
  bun build src/index.ts --compile --outfile "$SERVER_BINARY"

  if [[ $? -eq 0 ]]; then
    ok "서버 빌드 완료: ${SERVER_BINARY}"
    chmod +x "$SERVER_BINARY"
  else
    err "빌드 실패"
    exit 1
  fi
}

# ============================================================================
# install - 전체 설치 (의존성 + 빌드)
# ============================================================================
cmd_install() {
  require_bun

  if [[ ! -d "$SERVER_SOURCE_DIR" ]]; then
    err "서버 소스를 찾을 수 없습니다: ${SERVER_SOURCE_DIR}"
    exit 1
  fi

  info "서버 설치 중..."

  # 의존성 설치
  cd "$SERVER_SOURCE_DIR"

  if [[ ! -d "node_modules" ]]; then
    info "의존성 설치 중..."
    bun install
  fi

  # 빌드
  cmd_build

  ok "서버 설치 완료"
  echo ""
  echo "  바이너리: ${SERVER_BINARY}"
  echo "  시작: tc-server start"
  echo ""
}

# ============================================================================
# logs - 서버 로그 확인
# ============================================================================
cmd_logs() {
  local follow="${1:-}"

  if [[ ! -f "$LOG_FILE" ]]; then
    info "로그 파일이 없습니다."
    return 0
  fi

  if [[ "$follow" == "-f" || "$follow" == "--follow" ]]; then
    tail -f "$LOG_FILE"
  else
    tail -100 "$LOG_FILE"
  fi
}

# ============================================================================
# 메인
# ============================================================================
main() {
  local command="${1:-}"
  shift || true

  case "$command" in
    status)
      cmd_status "$@"
      ;;
    start)
      cmd_start "$@"
      ;;
    stop)
      cmd_stop "$@"
      ;;
    restart)
      cmd_restart "$@"
      ;;
    ensure)
      cmd_ensure "$@"
      ;;
    build)
      cmd_build "$@"
      ;;
    install)
      cmd_install "$@"
      ;;
    logs)
      cmd_logs "$@"
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
