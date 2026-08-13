#!/usr/bin/env bash
# suggest-simplify.sh — Stop hook shim
# 이 세션이 코드를 변경했을 때만 /simplify 검토를 제안합니다.
#
# 판정·집계·출력은 전부 CLI 에 있습니다 (`.claude/rules/tool-layer-boundary.md`).
# 이 shim 의 책임은 부트스트랩뿐입니다: atelier 미설치면 무음 종료.
#
# 트리거: Stop (글로벌 등록 — 비차단 advisory)
# 세션 베이스라인은 session-baseline.sh (SessionStart) 가 기록합니다.

command -v atelier >/dev/null 2>&1 || exit 0

exec atelier session simplify-check --project-dir "${CLAUDE_PROJECT_DIR:-.}"
