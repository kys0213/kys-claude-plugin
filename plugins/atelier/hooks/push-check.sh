#!/usr/bin/env bash
# push-check.sh — Stop hook shim
# 열린 PR 이 있는 브랜치에 push 되지 않은 커밋이 남아 있으면 Stop 을 block 해서
# 에이전트가 push 한 뒤 종료하도록 합니다. push 는 이 hook 이 하지 않습니다 —
# 감지·안내만 하고, 실제 push 는 에이전트가 git skill 정책대로 수행합니다.
#
# 판정·출력은 전부 CLI 에 있습니다 (`.claude/rules/tool-layer-boundary.md`).
# 이 shim 의 책임은 부트스트랩뿐입니다: atelier 미설치면 무음 종료.
#
# 트리거: Stop (글로벌 등록)
#
# `exec` 를 쓰지 않고 반드시 exit 0 으로 끝내는 이유:
# Stop hook 의 exit 2 는 "stderr 를 이유로 block" 이라는 신호입니다. 설치된
# atelier 가 구버전이라 `session push-check` 를 모르면 clap 이 exit 2 로 죽고,
# 그러면 모든 세션 종료가 clap 사용법 에러로 막힙니다. block 신호는 stdout 의
# JSON 이 전달하므로 exit code 는 항상 0 이면 충분합니다.

command -v atelier >/dev/null 2>&1 || exit 0

atelier session push-check --project-dir "${CLAUDE_PROJECT_DIR:-.}" 2>/dev/null
exit 0
