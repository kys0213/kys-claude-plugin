#!/usr/bin/env bash
# Stop hook: 코드/문서 변경이 있으면 /simplify 검토를 제안합니다.
#
# 동작:
#   1. git status --porcelain으로 작업 트리 변경사항 확인
#   2. 변경된 파일이 있으면 /simplify 추천 메시지 출력
#   3. 변경이 없으면 무출력 (exit 0)

# git repo가 아니면 종료
git rev-parse --is-inside-work-tree >/dev/null 2>&1 || exit 0

# 변경된 파일 수집 (staged + unstaged + untracked, 단일 명령)
CHANGED=$(git status --porcelain 2>/dev/null | cut -c4-)

if [ -z "$CHANGED" ]; then
  exit 0
fi

FILE_COUNT=$(echo "$CHANGED" | wc -l)
FILE_COUNT=${FILE_COUNT// /}

FILE_LIST=$(echo "$CHANGED" | head -10 | sed 's/^/    /')
OVERFLOW=""
if [ "$FILE_COUNT" -gt 10 ]; then
  OVERFLOW="    ... 외 $((FILE_COUNT - 10))개"
fi

cat <<MSG

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
[coding-style] /simplify 검토 제안
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

이번 세션에서 ${FILE_COUNT}개 파일이 변경되었습니다.
작업을 마무리하기 전에 /simplify 를 실행하여
코드 재사용성, 품질, 효율성을 검토해 보세요.

  변경 파일:
${FILE_LIST}${OVERFLOW:+
${OVERFLOW}}

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
MSG
