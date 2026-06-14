#!/usr/bin/env bash
# check-cli-version.sh — SessionStart hook
# 세션 시작 시 설치된 atelier CLI 버전이 활성 플러그인 버전과 일치하는지 확인합니다.
#
# 트리거: SessionStart (글로벌 등록 — atelier 설치 여부로 스스로 판단)
# 동작:
#   - atelier CLI 미설치 → exit 0 (무음 — atelier 미사용 환경 배려)
#   - 설치 버전 < 플러그인 버전 → 업데이트 안내 한 줄 출력 후 exit 0
#   - 최신/상위 버전 → exit 0 (무음)

set -euo pipefail

# atelier 미설치 환경은 가장 싼 builtin 검사로 먼저 가른다 — 이 hook 은 모든 세션에서
# 실행되므로(미설치가 다수), plugin.json 파싱·stdin 처리 전에 즉시 종료한다.
command -v atelier &> /dev/null || exit 0

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
INSTALLED_VERSION=$(atelier --version 2>/dev/null | awk '{print $NF}' || echo "")

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
  echo "atelier CLI 업데이트 필요: v${INSTALLED_VERSION} → v${PLUGIN_VERSION}. /atelier:setup 또는 bash ${PLUGIN_ROOT}/scripts/ensure-binary.sh 를 실행하세요."
fi

exit 0
