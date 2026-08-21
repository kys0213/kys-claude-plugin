#!/usr/bin/env bash
# sync-artifact.sh — setup 이 복사한 산출물 1건을 플러그인 원본으로 갱신 (결정적 쓰기)
#
# check-drift.sh 가 판정(DRIFTED)을, 이 스크립트가 갱신(쓰기)을 담당한다.
# 어떤 산출물을 갱신할지, 사용자 확인을 받을지는 호출자(/atelier:update 명세)가 결정한다.
# 신규 설치는 하지 않는다 — 대상이 없거나 손상이면 exit 2 (설치는 /atelier:setup 담당).
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: sync-artifact.sh --target <claude-md|rules> [--project-dir <dir>]

Targets:
  claude-md  ~/.claude/CLAUDE.md 의 [coding-style] 마커 구간을 템플릿으로 교체
             (마커 밖 사용자 내용은 보존)
  rules      <project>/.claude/rules/agent-design-principles.md 를 플러그인 원본으로 교체

Options:
  --project-dir <dir>  rules 대상 프로젝트 루트 (기본: $CLAUDE_PROJECT_DIR, 없으면 .)
  -h, --help           도움말 출력

동작:
  - 쓰기 전 대상 파일을 <file>.bak-<timestamp> 로 백업한다
  - 대상 미설치(파일/마커 없음) 또는 마커가 한쪽만 남은 손상 상태면 거부한다

Exit codes:
  0  갱신 완료
  2  인자 오류, 대상 미설치/손상, 또는 플러그인 원본 누락
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
TARGET=""

while [ $# -gt 0 ]; do
  case "$1" in
    --target)
      if [ $# -lt 2 ]; then
        echo "ERROR: --target requires a value (claude-md|rules)" >&2
        exit 2
      fi
      TARGET="$2"
      shift 2
      ;;
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

if [ "$TARGET" != "claude-md" ] && [ "$TARGET" != "rules" ]; then
  echo "ERROR: --target must be claude-md or rules (got: '${TARGET}')" >&2
  usage >&2
  exit 2
fi

# --- Helper: 쓰기 전 백업 ---
# Prints: 생성된 백업 파일 경로
backup_file() {
  local target="$1" backup
  backup="${target}.bak-$(date +%Y%m%d-%H%M%S)"
  cp "$target" "$backup"
  echo "$backup"
}

# --- claude-md: 마커 구간을 템플릿으로 교체 ---
sync_claude_md() {
  if [ ! -f "$TEMPLATE_CLAUDE_MD" ]; then
    echo "ERROR: plugin source file not found: $TEMPLATE_CLAUDE_MD" >&2
    exit 2
  fi
  if [ ! -f "$USER_CLAUDE_MD" ]; then
    echo "ERROR: not installed: $USER_CLAUDE_MD — run /atelier:setup" >&2
    exit 2
  fi

  local has_begin=0 has_end=0
  if grep -qxF "$BEGIN_MARKER" "$USER_CLAUDE_MD"; then has_begin=1; fi
  if grep -qxF "$END_MARKER" "$USER_CLAUDE_MD"; then has_end=1; fi

  if [ "$has_begin" -eq 0 ] && [ "$has_end" -eq 0 ]; then
    echo "ERROR: coding-style block not installed in $USER_CLAUDE_MD — run /atelier:setup" >&2
    exit 2
  fi
  if [ "$has_begin" -eq 0 ] || [ "$has_end" -eq 0 ]; then
    echo "ERROR: broken coding-style block in $USER_CLAUDE_MD (one marker missing) — run /atelier:setup to reinstall" >&2
    exit 2
  fi

  # 마커가 정확히 1쌍이고 begin 이 end 보다 앞일 때만 쓴다 — 중복·역순 상태에서
  # 구간 교체를 강행하면 마커 밖 사용자 내용까지 파괴될 수 있다.
  local begin_count end_count begin_line end_line
  begin_count="$(grep -cxF "$BEGIN_MARKER" "$USER_CLAUDE_MD")"
  end_count="$(grep -cxF "$END_MARKER" "$USER_CLAUDE_MD")"
  if [ "$begin_count" -ne 1 ] || [ "$end_count" -ne 1 ]; then
    echo "ERROR: broken coding-style block in $USER_CLAUDE_MD (markers duplicated: begin=${begin_count}, end=${end_count}) — run /atelier:setup to reinstall" >&2
    exit 2
  fi
  begin_line="$(grep -nxF "$BEGIN_MARKER" "$USER_CLAUDE_MD" | cut -d: -f1)"
  end_line="$(grep -nxF "$END_MARKER" "$USER_CLAUDE_MD" | cut -d: -f1)"
  if [ "$begin_line" -ge "$end_line" ]; then
    echo "ERROR: broken coding-style block in $USER_CLAUDE_MD (markers out of order) — run /atelier:setup to reinstall" >&2
    exit 2
  fi

  local backup tmp
  backup="$(backup_file "$USER_CLAUDE_MD")"
  tmp="$(mktemp)"
  # 마커 라인은 전체 라인 일치로만 인식한다 (check-drift.sh 와 동일 기준).
  # 템플릿 자체가 begin/end 마커를 포함하므로 구간 전체를 템플릿으로 갈아끼운다.
  awk -v begin="$BEGIN_MARKER" -v end="$END_MARKER" -v tpl="$TEMPLATE_CLAUDE_MD" '
    $0 == begin { skip = 1; while ((getline line < tpl) > 0) print line; close(tpl); next }
    $0 == end { skip = 0; next }
    skip != 1 { print }
  ' "$USER_CLAUDE_MD" > "$tmp"
  # 대상 파일의 권한·inode 를 보존하기 위해 mv 대신 내용만 덮어쓴다
  cat "$tmp" > "$USER_CLAUDE_MD"
  rm -f "$tmp"
  echo "synced: coding-style block in $USER_CLAUDE_MD (backup: $backup)"
}

# --- rules: 복사본을 플러그인 원본으로 교체 ---
sync_rules() {
  local target="$PROJECT_DIR/$RULES_COPY_REL"
  if [ ! -f "$TEMPLATE_RULES" ]; then
    echo "ERROR: plugin source file not found: $TEMPLATE_RULES" >&2
    exit 2
  fi
  if [ ! -f "$target" ]; then
    echo "ERROR: not installed: $target — run /atelier:setup" >&2
    exit 2
  fi

  local backup
  backup="$(backup_file "$target")"
  cp "$TEMPLATE_RULES" "$target"
  echo "synced: $target (backup: $backup)"
}

case "$TARGET" in
  claude-md) sync_claude_md ;;
  rules) sync_rules ;;
esac
