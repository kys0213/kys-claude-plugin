#!/usr/bin/env bash
# check-cli-version.sh — PreToolUse hook
# 세션당 1회, autopilot CLI 버전이 plugin.json과 일치하는지 확인합니다.
# 과거 버전이면 업데이트 안내를 출력합니다.
#
# 트리거: Bash (모든 Bash 호출)
# 동작:
#   - 이미 이번 세션에서 확인 완료 → exit 0 (skip)
#   - CLI 미설치 또는 과거 버전 → 경고 출력 후 exit 0 (차단하지 않음)
#   - 최신 버전 → exit 0

set -euo pipefail

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-.}"
CONFIG_FILE="${PROJECT_DIR}/github-autopilot.local.md"

# autopilot 프로젝트가 아니면 skip
if [[ ! -f "$CONFIG_FILE" ]]; then
  exit 0
fi

# stdin 소비 (PreToolUse hook은 tool_input을 stdin으로 받음)
cat > /dev/null

# --- 세션 중복 체크 (CLAUDE_SESSION_ID 기반) ---
REPO_NAME=$(basename "$(git rev-parse --show-toplevel 2>/dev/null || echo unknown)")
MARKER_DIR="/tmp/autopilot-${REPO_NAME}"
mkdir -p "$MARKER_DIR"
MARKER_FILE="${MARKER_DIR}/version-checked.${CLAUDE_SESSION_ID:-$$}"

if [[ -f "$MARKER_FILE" ]]; then
  exit 0
fi

# 마커 생성 (이후 호출에서 skip)
touch "$MARKER_FILE"

# --- plugin.json에서 버전 읽기 ---
PLUGIN_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PLUGIN_JSON="${PLUGIN_ROOT}/.claude-plugin/plugin.json"

if [[ ! -f "$PLUGIN_JSON" ]]; then
  exit 0
fi

PLUGIN_VERSION=$(grep -o '"version"[[:space:]]*:[[:space:]]*"[^"]*"' "$PLUGIN_JSON" | head -1 | sed 's/.*"\([^"]*\)"$/\1/')

if [[ -z "$PLUGIN_VERSION" ]]; then
  exit 0
fi

# --- 설치된 CLI 버전 확인 ---
if ! command -v autopilot &> /dev/null; then
  echo "⚠ autopilot CLI가 설치되어 있지 않습니다." >&2
  echo "  /github-autopilot:setup 을 실행하여 설치하세요." >&2
  exit 0
fi

INSTALLED_VERSION=$(autopilot --version 2>/dev/null | awk '{print $NF}' || echo "")

if [[ -z "$INSTALLED_VERSION" ]]; then
  exit 0
fi

# --- SemVer 비교 ---
compare_semver() {
  local a="${1#v}" b="${2#v}"
  local a_core="${a%%-*}" b_core="${b%%-*}"
  IFS='.' read -r a_major a_minor a_patch <<< "$a_core"
  IFS='.' read -r b_major b_minor b_patch <<< "$b_core"

  a_major=${a_major:-0}; a_minor=${a_minor:-0}; a_patch=${a_patch:-0}
  b_major=${b_major:-0}; b_minor=${b_minor:-0}; b_patch=${b_patch:-0}

  for field in major minor patch; do
    local va="a_$field" vb="b_$field"
    if (( ${!va} < ${!vb} )); then return 2; fi
    if (( ${!va} > ${!vb} )); then return 1; fi
  done
  return 0
}

set +e
compare_semver "$INSTALLED_VERSION" "$PLUGIN_VERSION"
CMP=$?
set -e

if [[ "$CMP" -eq 2 ]]; then
  echo "⚠ autopilot CLI 업데이트 필요: v${INSTALLED_VERSION} → v${PLUGIN_VERSION}" >&2
  echo "  /github-autopilot:setup 을 실행하거나 아래 명령어를 실행하세요:" >&2
  echo "  bash ${PLUGIN_ROOT}/scripts/ensure-binary.sh" >&2
fi

exit 0
