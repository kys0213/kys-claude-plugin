#!/usr/bin/env bash
# check-drift.sh — setup 이 복사한 산출물(복사형)이 플러그인 원본과 어긋났는지 판정
#
# 대상 (참조형 산출물은 ${CLAUDE_PLUGIN_ROOT} 해석으로 자동 최신이므로 제외):
#   1. ~/.claude/CLAUDE.md 의 [coding-style:begin]~[end] 블록 ↔ templates/claude-md/CLAUDE.md
#   2. <project>/.claude/rules/agent-design-principles.md     ↔ rules/agent-design-principles.md
#
# read-only — 차이를 보고만 하고 어떤 파일도 수정하지 않는다. 갱신은 /atelier:setup 담당.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: check-drift.sh [--project-dir <dir>]

Options:
  --project-dir <dir>  rules 복사본을 찾을 프로젝트 루트 (기본: $CLAUDE_PROJECT_DIR, 없으면 .)
  -h, --help           도움말 출력

Output format:
  <check>=<STATUS> [detail]
    STATUS: OK | DRIFTED | NOT_INSTALLED
  마지막 줄 요약: → N checked, N drifted, N missing

Exit codes:
  0  드리프트 없음 (OK / NOT_INSTALLED 만)
  1  드리프트 1건 이상 발견
  2  인자 오류 또는 플러그인 원본 파일 누락
EOF
}

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PLUGIN_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=drift-common.sh
. "$SCRIPT_DIR/drift-common.sh"
TEMPLATE_CLAUDE_MD="$PLUGIN_DIR/$TEMPLATE_CLAUDE_MD_REL"
TEMPLATE_RULES="$PLUGIN_DIR/$TEMPLATE_RULES_REL"
USER_CLAUDE_MD="$HOME/.claude/CLAUDE.md"
PROJECT_DIR="${CLAUDE_PROJECT_DIR:-.}"

while [ $# -gt 0 ]; do
  case "$1" in
    --project-dir)
      if [ $# -lt 2 ]; then
        echo "ERROR: --project-dir requires a value" >&2
        exit 2
      fi
      PROJECT_DIR="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "ERROR: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

# --- 1. 플러그인 원본 존재 확인 (없으면 판정 불가 — 보고가 아니라 오류) ---
for template in "$TEMPLATE_CLAUDE_MD" "$TEMPLATE_RULES"; do
  if [ ! -f "$template" ]; then
    echo "ERROR: plugin source file not found: $template" >&2
    exit 2
  fi
done

CHECKED=0
DRIFTED=0
MISSING=0

# --- Helper: 판정 1건 보고 및 집계 ---
report() {
  local name="$1" status="$2" detail="${3:-}"
  CHECKED=$((CHECKED + 1))
  case "$status" in
    DRIFTED) DRIFTED=$((DRIFTED + 1)) ;;
    NOT_INSTALLED) MISSING=$((MISSING + 1)) ;;
  esac
  if [ -n "$detail" ]; then
    echo "${name}=${status} ${detail}"
  else
    echo "${name}=${status}"
  fi
}

# --- 2. CLAUDE.md [coding-style] 블록 판정 ---
# 템플릿 파일 자체가 begin/end 마커를 포함하므로, 사용자 CLAUDE.md 에서
# 마커 구간(마커 라인 포함)을 추출해 템플릿 전문과 그대로 diff 한다.
CLAUDE_MD_CHECK="claude-md-coding-style-block"

has_marker() {
  grep -qxF "$1" "$USER_CLAUDE_MD"
}

# 마커 라인(전체 라인 일치) 포함 구간 추출 — sync-artifact.sh 와 동일 기준
extract_block() {
  awk -v begin="$BEGIN_MARKER" -v end="$END_MARKER" '
    $0 == begin { inblock = 1 }
    inblock { print }
    $0 == end { inblock = 0 }
  ' "$USER_CLAUDE_MD"
}

if [ ! -f "$USER_CLAUDE_MD" ]; then
  report "$CLAUDE_MD_CHECK" NOT_INSTALLED "($USER_CLAUDE_MD)"
elif ! has_marker "$BEGIN_MARKER" && ! has_marker "$END_MARKER"; then
  report "$CLAUDE_MD_CHECK" NOT_INSTALLED "($USER_CLAUDE_MD)"
elif ! has_marker "$BEGIN_MARKER"; then
  report "$CLAUDE_MD_CHECK" DRIFTED "(begin marker missing)"
elif ! has_marker "$END_MARKER"; then
  report "$CLAUDE_MD_CHECK" DRIFTED "(end marker missing)"
elif diff -q <(extract_block) "$TEMPLATE_CLAUDE_MD" > /dev/null; then
  report "$CLAUDE_MD_CHECK" OK
else
  report "$CLAUDE_MD_CHECK" DRIFTED "($USER_CLAUDE_MD)"
fi

# --- 3. .claude/rules 복사본 판정 ---
# setup Step 2b 는 "내용 수정 없이 그대로" 복사하므로 byte-identical 이 계약이다.
RULES_CHECK="rules/agent-design-principles.md"
RULES_COPY="$PROJECT_DIR/$RULES_COPY_REL"
if [ ! -f "$RULES_COPY" ]; then
  report "$RULES_CHECK" NOT_INSTALLED "($RULES_COPY)"
elif diff -q "$RULES_COPY" "$TEMPLATE_RULES" > /dev/null; then
  report "$RULES_CHECK" OK
else
  report "$RULES_CHECK" DRIFTED "($RULES_COPY)"
fi

# --- 4. 요약 및 exit code ---
echo "→ ${CHECKED} checked, ${DRIFTED} drifted, ${MISSING} missing"

if [ "$DRIFTED" -gt 0 ]; then
  exit 1
fi
exit 0
