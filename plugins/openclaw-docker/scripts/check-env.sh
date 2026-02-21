#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
  cat <<EOF
Usage: $(basename "$0") [project-dir]

OpenClaw Docker 환경을 검증합니다.

Arguments:
  project-dir  OpenClaw 프로젝트 루트 (기본: ~/Documents/openclaw)

Output format:
  CHECK=STATUS [detail]

Exit codes:
  0  모든 필수 항목 통과
  1  하나 이상의 필수 항목 실패
EOF
  exit 1
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
fi

PROJECT_DIR="${1:-$HOME/Documents/openclaw}"
HAS_FAIL=0

check() {
  local name="$1" status="$2" detail="${3:-}"
  if [[ "$status" == "FAIL" ]]; then
    HAS_FAIL=1
  fi
  if [[ -n "$detail" ]]; then
    echo "$name=$status $detail"
  else
    echo "$name=$status"
  fi
}

# 1. Docker installed
if command -v docker &>/dev/null; then
  docker_ver="$(docker --version 2>/dev/null | sed 's/Docker version //' | cut -d',' -f1)"
  check "DOCKER_INSTALLED" "OK" "docker $docker_ver"
else
  check "DOCKER_INSTALLED" "FAIL" "docker not found"
fi

# 2. Docker running
if docker info &>/dev/null; then
  check "DOCKER_RUNNING" "OK"
else
  check "DOCKER_RUNNING" "FAIL" "docker daemon not responding"
fi

# 3. run.sh exists and executable
if [[ -f "$PROJECT_DIR/run.sh" ]]; then
  if [[ -x "$PROJECT_DIR/run.sh" ]]; then
    check "RUN_SH" "OK" "executable"
  else
    check "RUN_SH" "FAIL" "not executable (chmod +x run.sh)"
  fi
else
  check "RUN_SH" "FAIL" "not found at $PROJECT_DIR/run.sh"
fi

# 4. .env.local exists
if [[ -f "$PROJECT_DIR/.env.local" ]]; then
  check "ENV_LOCAL" "OK"
else
  check "ENV_LOCAL" "FAIL" "not found at $PROJECT_DIR/.env.local"
fi

# 5. Required variables
REQUIRED_VARS=(OPENCLAW_CONFIG_DIR OPENCLAW_WORKSPACE_DIR OPENCLAW_GATEWAY_TOKEN OPENCLAW_IMAGE)
found=0
missing=()

if [[ -f "$PROJECT_DIR/.env.local" ]]; then
  for var in "${REQUIRED_VARS[@]}"; do
    if grep -q "^${var}=" "$PROJECT_DIR/.env.local"; then
      found=$((found + 1))
    else
      missing+=("$var")
    fi
  done
else
  missing=("${REQUIRED_VARS[@]}")
fi

total=${#REQUIRED_VARS[@]}
if [[ $found -eq $total ]]; then
  check "REQUIRED_VARS" "OK" "$found/$total"
else
  check "REQUIRED_VARS" "FAIL" "$found/$total missing: ${missing[*]:-}"
fi

# 6. docker-compose.override.yml (optional)
if [[ -f "$PROJECT_DIR/docker-compose.override.yml" ]]; then
  check "OVERRIDE_YML" "OK" "found"
else
  check "OVERRIDE_YML" "WARN" "not found (optional)"
fi

exit $HAS_FAIL
