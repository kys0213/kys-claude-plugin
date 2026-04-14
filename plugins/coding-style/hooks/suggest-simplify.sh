#!/usr/bin/env bash
# Stop hook: 코드/문서 변경이 있으면 /simplify 검토를 제안합니다.
#
# 동작:
#   1. git diff로 작업 트리 변경사항 확인 (staged + unstaged)
#   2. 변경된 파일이 있으면 /simplify 추천 메시지 출력
#   3. 변경이 없으면 무출력 (exit 0)

# git repo가 아니면 종료
git rev-parse --is-inside-work-tree >/dev/null 2>&1 || exit 0

# 변경된 파일 수집 (staged + unstaged + untracked)
CHANGED=$(git diff --name-only HEAD 2>/dev/null; git diff --name-only --cached 2>/dev/null; git ls-files --others --exclude-standard 2>/dev/null)
CHANGED=$(echo "$CHANGED" | sort -u | grep -v '^$')

if [ -z "$CHANGED" ]; then
  exit 0
fi

FILE_COUNT=$(echo "$CHANGED" | wc -l | tr -d ' ')

cat <<MSG

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
[workflow-guide] /simplify 검토 제안
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

이번 세션에서 ${FILE_COUNT}개 파일이 변경되었습니다.
작업을 마무리하기 전에 /simplify 를 실행하여
코드 재사용성, 품질, 효율성을 검토해 보세요.

  변경 파일:
$(echo "$CHANGED" | head -10 | sed 's/^/    /')
$([ "$FILE_COUNT" -gt 10 ] && echo "    ... 외 $((FILE_COUNT - 10))개")

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
MSG
