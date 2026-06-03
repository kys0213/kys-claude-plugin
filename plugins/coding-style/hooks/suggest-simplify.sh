#!/usr/bin/env bash
# Stop hook: 코드/문서 변경이 있으면 /simplify 검토를 모델 컨텍스트에 주입합니다.
#
# Claude Code Stop hook 사양:
#   - plain stdout은 사용자 화면(UI)에만 표시되고 모델에는 전달되지 않습니다.
#   - 모델의 다음 턴(system-reminder)에 도달하려면
#     hookSpecificOutput.additionalContext JSON으로 출력해야 합니다.
#
# 동작:
#   1. git status --porcelain으로 작업 트리 변경사항 확인
#   2. 변경된 파일이 있으면 additionalContext JSON 출력 (→ 다음 턴 모델 주입)
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

FILE_LIST=$(echo "$CHANGED" | head -10 | sed 's/^/- /')
OVERFLOW=""
if [ "$FILE_COUNT" -gt 10 ]; then
  OVERFLOW="
- ... 외 $((FILE_COUNT - 10))개"
fi

CTX="이번 세션에서 ${FILE_COUNT}개 파일이 변경되었습니다. 응답을 마치기 전에 /simplify 슬래시 커맨드를 호출해 코드 재사용성·품질·효율성을 검토하세요.

변경 파일:
${FILE_LIST}${OVERFLOW}"

# additionalContext JSON으로 출력 → 모델의 다음 턴 system-reminder로 주입됩니다.
if command -v jq >/dev/null 2>&1; then
  jq -n --arg ctx "$CTX" '{
    hookSpecificOutput: {
      hookEventName: "Stop",
      additionalContext: $ctx
    }
  }'
else
  # jq 부재 시: 백슬래시·따옴표를 이스케이프하고 개행을 \n으로 치환한 수동 JSON 인코딩
  ESCAPED=$(printf '%s' "$CTX" \
    | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' \
    | awk 'BEGIN { ORS = "" } { print (NR == 1 ? "" : "\\n") $0 }')
  printf '{"hookSpecificOutput":{"hookEventName":"Stop","additionalContext":"%s"}}\n' "$ESCAPED"
fi
