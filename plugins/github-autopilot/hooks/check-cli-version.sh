#!/usr/bin/env bash
# check-cli-version.sh — SessionStart hook
# 세션 시작 시 autopilot CLI 버전이 plugin.json과 일치하는지 확인합니다.
# 과거 버전이면 업데이트 안내를 출력합니다.
#
# 트리거: SessionStart
# 동작:
#   - autopilot 프로젝트가 아니면 → exit 0 (skip)
#   - CLI 미설치 또는 과거 버전 → 안내 출력 후 exit 0
#   - 최신 버전 → exit 0

set -euo pipefail

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-.}"
CONFIG_FILE="${PROJECT_DIR}/github-autopilot.local.md"

# autopilot 프로젝트가 아니면 skip
if [[ ! -f "$CONFIG_FILE" ]]; then
  exit 0
fi

# stdin 소비 (SessionStart hook은 session info를 stdin으로 받음)
cat > /dev/null

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
  echo "autopilot CLI가 설치되어 있지 않습니다. /github-autopilot:setup 을 실행하여 설치하세요."
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
  echo "autopilot CLI 업데이트 필요: v${INSTALLED_VERSION} → v${PLUGIN_VERSION}. /github-autopilot:setup 또는 bash ${PLUGIN_ROOT}/scripts/ensure-binary.sh 를 실행하세요."
fi

exit 0
