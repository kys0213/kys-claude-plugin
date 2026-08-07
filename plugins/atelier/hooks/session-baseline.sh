#!/usr/bin/env bash
# session-baseline.sh — SessionStart hook shim
# 세션 시작 시점의 저장소 상태를 기록해, Stop hook 이 "이 세션이 만든 변경"을
# 기존 dirty 상태와 구분할 수 있게 합니다 (없을 때만 기록 — resume/compact 안전).
#
# 판정·기록은 전부 CLI 에 있습니다 (`.claude/rules/tool-layer-boundary.md`).
# 이 shim 의 책임은 부트스트랩뿐입니다: atelier 미설치면 무음 종료.
#
# check-cli-version.sh 와 분리한 이유: 책임이 다르고, 그 스크립트의
# `set -euo pipefail` 아래에서 스냅샷 실패가 버전 경고를 삼킵니다.
# stdin 은 hook 마다 별도로 전달되므로 분리해도 손해가 없습니다.

command -v atelier >/dev/null 2>&1 || exit 0

exec atelier session baseline --project-dir "${CLAUDE_PROJECT_DIR:-.}"
