#!/usr/bin/env bash
# guard-pr-base.sh — thin shim (`.claude/rules/tool-layer-boundary.md`).
# 로직은 `atelier autopilot hook guard-pr-base` CLI 서브커맨드에 있습니다 (#776).
# CLI 이전 전에 등록된 settings.json entry 호환용으로만 유지합니다 —
# 신규 setup 은 CLI 커맨드를 직접 등록합니다.
#
# 트리거: PreToolUse (mcp__github__create_pull_request, Bash)
# 동작: stdin 페이로드를 CLI 로 위임. 바이너리 부재 시 exit 0 (차단 금지).

command -v atelier >/dev/null 2>&1 || exit 0
exec atelier autopilot hook guard-pr-base --project-dir "${CLAUDE_PROJECT_DIR:-.}"
