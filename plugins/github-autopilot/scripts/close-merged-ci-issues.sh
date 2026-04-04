#!/usr/bin/env bash
# close-merged-ci-issues.sh — CI failure 이슈 중 관련 PR이 머지된 이슈를 자동 close
#
# Usage:
#   close-merged-ci-issues.sh [label_prefix]
#
# Arguments:
#   label_prefix — 라벨 접두사 (기본값: "autopilot:")
#
# Exit codes:
#   0 — 정상 완료
#   2 — 사용법 오류
#
# Output:
#   JSON: {"closed": [...], "still_open": [...]}

set -euo pipefail

LABEL_PREFIX="${1:-autopilot:}"
CI_FAILURE_LABEL="${LABEL_PREFIX}ci-failure"

command -v gh &>/dev/null || { echo '{"error": "gh CLI not found"}' >&2; exit 2; }
command -v jq &>/dev/null || { echo '{"error": "jq not found"}' >&2; exit 2; }

# 1. autopilot:ci-failure 라벨이 있는 open 이슈 조회
ISSUES=$(gh issue list --state open --label "$CI_FAILURE_LABEL" --json number,title --limit 50 2>/dev/null || echo "[]")

COUNT=$(echo "$ISSUES" | jq 'length')
if [[ "$COUNT" -eq 0 ]]; then
  echo '{"closed": [], "still_open": []}'
  exit 0
fi

CLOSED="[]"
STILL_OPEN="[]"

for row in $(echo "$ISSUES" | jq -r '.[] | @base64'); do
  _jq() { echo "$row" | base64 --decode | jq -r "${1}"; }

  ISSUE_NUMBER=$(_jq '.number')
  ISSUE_TITLE=$(_jq '.title')

  # 2. 이슈 제목에서 브랜치명 추출
  #    ci-watch 형식: "fix: CI failure in {workflow} on {branch}"
  BRANCH=$(echo "$ISSUE_TITLE" | sed -n 's/.*CI failure in .* on \(.*\)/\1/p')

  if [[ -z "$BRANCH" ]]; then
    STILL_OPEN=$(echo "$STILL_OPEN" | jq --argjson num "$ISSUE_NUMBER" --arg title "$ISSUE_TITLE" --arg reason "branch not parseable" \
      '. + [{"number": $num, "title": $title, "reason": $reason}]')
    continue
  fi

  # 3. 해당 브랜치의 PR이 MERGED 상태인지 확인
  MERGED_PR=$(gh pr list --head "$BRANCH" --state merged --json number --limit 1 2>/dev/null || echo "[]")
  MERGED_COUNT=$(echo "$MERGED_PR" | jq 'length')

  if [[ "$MERGED_COUNT" -gt 0 ]]; then
    # 4. MERGED면 이슈를 close + 코멘트
    gh issue close "$ISSUE_NUMBER" --comment "Autopilot: 관련 브랜치(\`$BRANCH\`)의 PR이 머지되어 자동 close합니다." 2>/dev/null
    CLOSED=$(echo "$CLOSED" | jq --argjson num "$ISSUE_NUMBER" --arg title "$ISSUE_TITLE" --arg branch "$BRANCH" \
      '. + [{"number": $num, "title": $title, "branch": $branch}]')
  else
    STILL_OPEN=$(echo "$STILL_OPEN" | jq --argjson num "$ISSUE_NUMBER" --arg title "$ISSUE_TITLE" --arg reason "PR not merged" \
      '. + [{"number": $num, "title": $title, "reason": $reason}]')
  fi
done

jq -n --argjson closed "$CLOSED" --argjson still_open "$STILL_OPEN" \
  '{closed: $closed, still_open: $still_open}'
