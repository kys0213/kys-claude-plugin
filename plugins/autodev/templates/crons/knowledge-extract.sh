#!/bin/bash
# Built-in cron: knowledge-extract (per-repo)
# 주기: 1시간 | Guard: 미추출 merged PR이 존재할 때
#
# 지식 추출 — merged PR에서 코드 패턴, 결정 사항, 교훈을 추출합니다.
# 중복 실행 방지는 daemon cron engine이 내부 상태로 보장합니다.

set -euo pipefail

# Guard: 미추출 merged PR이 있는지 확인
UNEXTRACTED=$(autodev queue list --json --repo "$AUTODEV_REPO_NAME" --unextracted \
  | jq 'length')

if [ "$UNEXTRACTED" = "0" ]; then
  echo "skip: $AUTODEV_REPO_NAME 미추출 merged PR 없음"
  exit 0
fi

echo "knowledge-extract: $AUTODEV_REPO_NAME (unextracted=$UNEXTRACTED)"

autodev agent --repo "$AUTODEV_REPO_NAME" -p "완료된 작업에서 지식을 추출해줘"
