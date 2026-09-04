#!/usr/bin/env bash
# push-check.sh — Stop hook shim
# 열린 PR 이 있는 브랜치에 push 되지 않은 커밋이 남아 있으면 Stop 을 block 합니다.
#
# 판정·출력은 전부 CLI 에 있습니다 (`.claude/rules/tool-layer-boundary.md`).
# 이 shim 의 책임은 부트스트랩뿐입니다: atelier 미설치면 무음 종료.
# 항상 exit 0 (block 신호는 stdout JSON) 은 session 서브시스템 경계가 보장합니다.
#
# 트리거: Stop (글로벌 등록)

command -v atelier >/dev/null 2>&1 || exit 0

exec atelier session push-check --project-dir "${CLAUDE_PROJECT_DIR:-.}"
